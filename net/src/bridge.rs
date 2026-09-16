use crate::proto::{
    decode_client, decode_host, encode_client, encode_host, ClientMsg, HostMsg, WIRE_VERSION,
};
use crate::table;
use crate::table::{HostEvent, JoinEvent, TableHost};
use parking_lot::Mutex;
use spirit_node::gossip::TableAdvert;
use spirit_node::iroh::{Endpoint, EndpointAddr, EndpointId};
use spirit_node::mesh::Mesh;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub const HOST_WATCHDOG_PERIOD: Duration = Duration::from_secs(5);

#[cfg(not(target_arch = "wasm32"))]
pub type HostFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

#[cfg(target_arch = "wasm32")]
pub type HostFuture = Pin<Box<dyn Future<Output = ()>>>;

static NET_EVENTS: Mutex<Vec<NetToGame>> = Mutex::new(Vec::new());
static HOST_TX: Mutex<Option<UnboundedSender<(u64, HostMsg)>>> = Mutex::new(None);
static CLIENT_TX: Mutex<Option<UnboundedSender<ClientMsg>>> = Mutex::new(None);
static JOIN_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub enum NetToGame {
    HostReady,
    HostFailed { error: String },
    HostClosed,
    HostLost { reason: String },
    PeerJoined { conn: u64, peer: String },
    PeerFrame { conn: u64, msg: ClientMsg },
    PeerLeft { conn: u64, reason: String },
    Connected,
    FromHost { msg: HostMsg },
    Dropped { reason: String },
}

pub fn push(event: NetToGame) {
    NET_EVENTS.lock().push(event);
}

pub fn drain_events() -> Vec<NetToGame> {
    std::mem::take(&mut *NET_EVENTS.lock())
}

pub fn request_join(host_id: String) {
    JOIN_REQUESTS.lock().push(host_id);
}

pub fn take_join_requests() -> Vec<String> {
    std::mem::take(&mut *JOIN_REQUESTS.lock())
}

fn unreachable_reason(error: &str) -> String {
    if error.contains("timed out") {
        format!("host unreachable ({error}) — the host device may be asleep, offline, or its app suspended")
    } else {
        format!("could not reach host: {error}")
    }
}

pub fn start_host(
    mesh: Arc<Mesh>,
    table: &table::TableProtocol,
    endpoint: Endpoint,
    name: String,
    spawn: impl FnOnce(HostFuture),
) {
    let host = table.open();
    mesh.set_table(Some(TableAdvert { name }));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<(u64, HostMsg)>();
    *HOST_TX.lock() = Some(tx);
    spawn(Box::pin(run_host(host, rx, endpoint, mesh)));
    push(NetToGame::HostReady);
}

fn relay(event: HostEvent) {
    match event {
        HostEvent::Joined(conn, peer) => push(NetToGame::PeerJoined { conn, peer }),
        HostEvent::Frame(conn, bytes) => {
            if let Some(msg) = decode_client(&bytes) {
                push(NetToGame::PeerFrame { conn, msg });
            }
        }
        HostEvent::Left(conn, reason) => push(NetToGame::PeerLeft { conn, reason }),
    }
}

async fn run_host(
    mut host: TableHost,
    mut rx: UnboundedReceiver<(u64, HostMsg)>,
    endpoint: Endpoint,
    mesh: Arc<Mesh>,
) {
    let mut watchdog = n0_future::time::interval(HOST_WATCHDOG_PERIOD);
    let mut lost = false;
    loop {
        tokio::select! {
            event = host.next() => match event {
                Some(event) => relay(event),
                None => return,
            },
            outbound = rx.recv() => match outbound {
                Some((conn, msg)) => host.send(conn, encode_host(&msg)),
                None => break,
            },
            _ = watchdog.tick() => {
                if endpoint.is_closed() {
                    mesh.set_table(None);
                    *HOST_TX.lock() = None;
                    push(NetToGame::HostLost {
                        reason: "endpoint stopped — table withdrawn".into(),
                    });
                    lost = true;
                    break;
                }
            },
        }
    }
    if lost {
        host.close();
        return;
    }
    while let Some(event) = host.next().await {
        relay(event);
    }
}

pub fn close_host(open: Option<(&Mesh, &table::TableProtocol)>) {
    if let Some((mesh, table)) = open {
        table.close();
        mesh.set_table(None);
    }
    *HOST_TX.lock() = None;
    push(NetToGame::HostClosed);
}

pub fn host_addr(mesh: &Mesh, host_id: &str) -> Option<EndpointAddr> {
    mesh.addr_of(host_id)
        .or_else(|| host_id.parse::<EndpointId>().ok().map(EndpointAddr::new))
}

pub async fn run_join(
    endpoint: spirit_node::iroh::Endpoint,
    mesh: Arc<Mesh>,
    addr: EndpointAddr,
    host_id: String,
    name: String,
) {
    let mut link = match table::join_via(&endpoint, addr).await {
        Ok(link) => link,
        Err(error) => {
            mesh.forget_table(&host_id);
            push(NetToGame::Dropped {
                reason: unreachable_reason(&error.to_string()),
            });
            return;
        }
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ClientMsg>();
    *CLIENT_TX.lock() = Some(tx);
    link.send(encode_client(&ClientMsg::Join {
        name,
        version: WIRE_VERSION,
    }));
    push(NetToGame::Connected);
    let mut seated = false;
    loop {
        tokio::select! {
            event = link.next() => match event {
                Some(JoinEvent::Frame(bytes)) => {
                    seated = true;
                    if let Some(msg) = decode_host(&bytes) {
                        push(NetToGame::FromHost { msg });
                    }
                }
                Some(JoinEvent::Closed(reason)) => {
                    if !seated {
                        mesh.forget_table(&host_id);
                    }
                    push(NetToGame::Dropped { reason });
                    break;
                }
                None => {
                    push(NetToGame::Dropped { reason: "link closed".into() });
                    break;
                }
            },
            outbound = rx.recv() => match outbound {
                Some(msg) => link.send(encode_client(&msg)),
                None => break,
            },
        }
    }
    *CLIENT_TX.lock() = None;
}

pub fn send_to(conn: u64, msg: HostMsg) {
    if let Some(tx) = HOST_TX.lock().as_ref() {
        let _ = tx.send((conn, msg));
    }
}

pub fn send_to_host(msg: ClientMsg) {
    if let Some(tx) = CLIENT_TX.lock().as_ref() {
        let _ = tx.send(msg);
    }
}

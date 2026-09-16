use n0_future::task::JoinHandle;
use parking_lot::Mutex;
use spirit_node::iroh::endpoint::{Connection, ConnectionError, RecvStream, SendStream};
use spirit_node::iroh::protocol::{AcceptError, ProtocolHandler};
use spirit_node::iroh::{Endpoint, EndpointAddr};
use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub const ALPN: &[u8] = b"spirit-table/1";

pub const MAX_FRAME_BYTES: usize = 8 << 20;

pub const DIAL_TIMEOUT: Duration = Duration::from_secs(15);

pub const REFUSED_NO_TABLE: u32 = 1;

#[derive(Debug)]
pub enum HostEvent {
    Joined(u64, String),
    Frame(u64, Vec<u8>),
    Left(u64, String),
}

#[derive(Debug)]
pub enum JoinEvent {
    Frame(Vec<u8>),
    Closed(String),
}

async fn read_frame(recv: &mut RecvStream) -> Result<Option<Vec<u8>>, String> {
    let mut len_bytes = [0u8; 4];
    if recv.read_exact(&mut len_bytes).await.is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(format!("frame of {len} bytes exceeds {MAX_FRAME_BYTES}"));
    }
    let mut bytes = vec![0u8; len];
    recv.read_exact(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(bytes))
}

async fn write_frame(send: &mut SendStream, bytes: &[u8]) -> Result<(), String> {
    let len = u32::try_from(bytes.len())
        .ok()
        .filter(|len| *len as usize <= MAX_FRAME_BYTES)
        .ok_or_else(|| format!("frame of {} bytes exceeds {MAX_FRAME_BYTES}", bytes.len()))?;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|error| error.to_string())?;
    send.write_all(bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

type ConnMap = Arc<Mutex<HashMap<u64, (UnboundedSender<Vec<u8>>, Connection)>>>;

#[derive(Clone)]
struct ActiveTable {
    conns: ConnMap,
    events: UnboundedSender<HostEvent>,
}

#[derive(Clone, Default)]
pub struct TableProtocol {
    active: Arc<Mutex<Option<ActiveTable>>>,
    next_conn: Arc<AtomicU64>,
}

impl std::fmt::Debug for TableProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active = self.active.lock();
        let conns = active
            .as_ref()
            .map(|table| table.conns.lock().len())
            .unwrap_or(0);
        f.debug_struct("TableProtocol")
            .field("open", &active.is_some())
            .field("conns", &conns)
            .field("next_conn", &self.next_conn.load(Ordering::Relaxed))
            .finish()
    }
}

impl TableProtocol {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.active.lock().is_some()
    }

    pub fn open(&self) -> TableHost {
        let (events_tx, events_rx) = unbounded_channel();
        let conns: ConnMap = Arc::new(Mutex::new(HashMap::new()));
        let table = ActiveTable {
            conns: conns.clone(),
            events: events_tx,
        };
        if let Some(previous) = self.active.lock().replace(table) {
            close_all(&previous.conns);
        }
        TableHost {
            conns,
            events: events_rx,
            protocol: self.clone(),
        }
    }

    pub fn close(&self) {
        if let Some(table) = self.active.lock().take() {
            close_all(&table.conns);
        }
    }
}

fn close_all(conns: &ConnMap) {
    for (_, connection) in conns.lock().drain().map(|(_, pair)| pair) {
        connection.close(0u32.into(), b"table closed");
    }
}

impl ProtocolHandler for TableProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let Some(table) = self.active.lock().clone() else {
            connection.close(REFUSED_NO_TABLE.into(), b"no open table");
            return Ok(());
        };
        let (mut send, mut recv) = connection.accept_bi().await?;
        let conn = self.next_conn.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = unbounded_channel::<Vec<u8>>();
        table.conns.lock().insert(conn, (tx, connection.clone()));
        let _ = table
            .events
            .send(HostEvent::Joined(conn, connection.remote_id().to_string()));
        let writer = n0_future::task::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if write_frame(&mut send, &bytes).await.is_err() {
                    break;
                }
            }
        });
        let reason = loop {
            match read_frame(&mut recv).await {
                Ok(Some(bytes)) => {
                    let _ = table.events.send(HostEvent::Frame(conn, bytes));
                }
                Ok(None) => break close_label(&connection),
                Err(error) => break error,
            }
        };
        table.conns.lock().remove(&conn);
        let _ = table.events.send(HostEvent::Left(conn, reason));
        writer.abort();
        connection.close(0u32.into(), b"done");
        Ok(())
    }
}

pub struct TableHost {
    conns: ConnMap,
    events: UnboundedReceiver<HostEvent>,
    protocol: TableProtocol,
}

impl TableHost {
    pub async fn next(&mut self) -> Option<HostEvent> {
        self.events.recv().await
    }

    pub fn send(&self, conn: u64, bytes: Vec<u8>) {
        if let Some((tx, _)) = self.conns.lock().get(&conn) {
            let _ = tx.send(bytes);
        }
    }

    pub fn close(self) {
        self.protocol.close();
    }
}

pub struct TableJoin {
    pub host_id: String,
    events: UnboundedReceiver<JoinEvent>,
    out: UnboundedSender<Vec<u8>>,
    writer: JoinHandle<()>,
    reader: JoinHandle<()>,
}

impl TableJoin {
    pub async fn next(&mut self) -> Option<JoinEvent> {
        self.events.recv().await
    }

    pub fn send(&self, bytes: Vec<u8>) {
        let _ = self.out.send(bytes);
    }
}

impl Drop for TableJoin {
    fn drop(&mut self) {
        self.writer.abort();
        self.reader.abort();
    }
}

fn close_label(connection: &Connection) -> String {
    match connection.close_reason() {
        Some(ConnectionError::ApplicationClosed(close)) => {
            let reason = String::from_utf8_lossy(&close.reason).into_owned();
            let refused = close.error_code == REFUSED_NO_TABLE.into();
            match (refused, reason.is_empty()) {
                (true, false) => format!("host refused: {reason}"),
                (true, true) => "host refused the connection".into(),
                (false, false) => format!("peer closed: {reason}"),
                (false, true) => "peer closed the connection".into(),
            }
        }
        Some(other) => other.to_string(),
        None => "connection closed".into(),
    }
}

pub async fn join_via(
    endpoint: &Endpoint,
    addr: EndpointAddr,
) -> Result<TableJoin, Box<dyn Error + Send + Sync>> {
    let host_id = addr.id.to_string();
    let connection = n0_future::time::timeout(DIAL_TIMEOUT, endpoint.connect(addr, ALPN))
        .await
        .map_err(|_| format!("dial timed out after {}s", DIAL_TIMEOUT.as_secs()))??;
    let (mut send, mut recv) = connection.open_bi().await?;
    let (out_tx, mut out_rx) = unbounded_channel::<Vec<u8>>();
    let (events_tx, events_rx) = unbounded_channel();
    let writer_events = events_tx.clone();
    let writer = n0_future::task::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if let Err(error) = write_frame(&mut send, &bytes).await {
                let _ = writer_events.send(JoinEvent::Closed(error));
                break;
            }
        }
    });
    let reader_connection = connection.clone();
    let reader = n0_future::task::spawn(async move {
        loop {
            match read_frame(&mut recv).await {
                Ok(Some(bytes)) => {
                    let _ = events_tx.send(JoinEvent::Frame(bytes));
                }
                Ok(None) => {
                    let _ = events_tx.send(JoinEvent::Closed(close_label(&reader_connection)));
                    break;
                }
                Err(error) => {
                    let _ = events_tx.send(JoinEvent::Closed(error));
                    break;
                }
            }
        }
        reader_connection.close(0u32.into(), b"done");
    });
    Ok(TableJoin {
        host_id,
        events: events_rx,
        out: out_tx,
        writer,
        reader,
    })
}

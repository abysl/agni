#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]

use agni_core::{CardFace, PlayerId, Table, Zone};
use agni_net::bridge::{self, NetToGame};
use agni_net::session::{
    ClientMsg, ClientSession, HostMsg, HostSession, WireIntent, WireZone, WIRE_VERSION,
};
use agni_net::table::{TableProtocol, ALPN};
use agni_sim::log::encode_log;
use spirit_node::iroh::endpoint::presets;
use spirit_node::iroh::protocol::Router;
use spirit_node::iroh::{Endpoint, RelayMode};
use spirit_node::mesh::Mesh;
use std::path::Path;
use std::time::Duration;

const SETTLE: Duration = Duration::from_secs(20);

fn faces(prefix: &str, n: usize) -> Vec<CardFace> {
    (0..n)
        .map(|i| CardFace::named(format!("{prefix} {i}")))
        .collect()
}

fn area(table: &Table, seat: u8, zone: Zone) -> Vec<u32> {
    table
        .in_area(PlayerId(seat), zone)
        .map(|card| card.id.0)
        .collect()
}

async fn local_endpoint() -> Endpoint {
    let endpoint = Endpoint::builder(presets::N0)
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .expect("a relay-less endpoint binds");
    let started = std::time::Instant::now();
    while endpoint.addr().addrs.is_empty() {
        assert!(started.elapsed() < SETTLE, "no direct address appeared");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    endpoint
}

async fn next_event(pick: impl Fn(&NetToGame) -> bool) -> NetToGame {
    let started = std::time::Instant::now();
    let mut held = Vec::new();
    loop {
        let mut events = bridge::drain_events();
        if let Some(at) = events.iter().position(&pick) {
            let event = events.remove(at);
            for other in held.into_iter().chain(events) {
                bridge::push(other);
            }
            return event;
        }
        held.append(&mut events);
        assert!(
            started.elapsed() < SETTLE,
            "no matching net event within {SETTLE:?}; saw {}",
            held.iter().map(describe).collect::<Vec<_>>().join(", ")
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn describe(event: &NetToGame) -> String {
    match event {
        NetToGame::HostReady => "HostReady".into(),
        NetToGame::HostFailed { error } => format!("HostFailed({error})"),
        NetToGame::HostClosed => "HostClosed".into(),
        NetToGame::HostLost { reason } => format!("HostLost({reason})"),
        NetToGame::PeerJoined { conn, peer } => format!("PeerJoined({conn}, {peer})"),
        NetToGame::PeerFrame { conn, .. } => format!("PeerFrame({conn})"),
        NetToGame::PeerLeft { conn, reason } => format!("PeerLeft({conn}, {reason})"),
        NetToGame::Connected => "Connected".into(),
        NetToGame::FromHost { .. } => "FromHost".into(),
        NetToGame::Dropped { reason } => format!("Dropped({reason})"),
    }
}

async fn host_frame(from: u64) -> ClientMsg {
    match next_event(|event| matches!(event, NetToGame::PeerFrame { conn, .. } if *conn == from))
        .await
    {
        NetToGame::PeerFrame { msg, .. } => msg,
        _ => unreachable!(),
    }
}

async fn client_frame() -> HostMsg {
    match next_event(|event| matches!(event, NetToGame::FromHost { .. })).await {
        NetToGame::FromHost { msg } => msg,
        _ => unreachable!(),
    }
}

async fn deliver(client: &mut ClientSession, entries: &[agni_sim::log::LogEntry], conn: u64) {
    for entry in entries {
        bridge::send_to(
            conn,
            HostMsg::Entry {
                entry: entry.clone(),
            },
        );
    }
    for _ in 0..entries.len() {
        let HostMsg::Entry { entry } = client_frame().await else {
            panic!("expected an entry");
        };
        client.apply(entry).expect("replica folds");
    }
}

#[test]
fn the_bridge_hosts_a_table_seats_a_joiner_deals_plays_a_turn_and_closes() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");
    runtime.block_on(async {
        let host_endpoint = local_endpoint().await;
        let protocol = TableProtocol::new();
        let _router = Router::builder(host_endpoint.clone())
            .accept(ALPN, protocol.clone())
            .spawn();
        let host_mesh = Mesh::new(Path::new("ephemeral-host"), host_endpoint.clone());
        host_mesh.set_replicate(false);

        let mut solo = Table::new();
        for face in faces("host", 3) {
            solo.add_face(PlayerId(0), Zone::Hand, face);
        }
        bridge::start_host(
            host_mesh.clone(),
            &protocol,
            host_endpoint.clone(),
            "rae's table".into(),
            |future| {
                tokio::spawn(future);
            },
        );
        assert!(protocol.is_open());
        assert!(matches!(
            next_event(|event| matches!(event, NetToGame::HostReady)).await,
            NetToGame::HostReady
        ));
        let mut host = HostSession::host_from("rae", &solo);

        let client_endpoint = local_endpoint().await;
        let client_mesh = Mesh::new(Path::new("ephemeral-client"), client_endpoint.clone());
        client_mesh.set_replicate(false);
        let host_id = host_endpoint.id().to_string();
        tokio::spawn(bridge::run_join(
            client_endpoint.clone(),
            client_mesh.clone(),
            host_endpoint.addr(),
            host_id.clone(),
            "ada".into(),
        ));

        let conn = match next_event(|event| matches!(event, NetToGame::PeerJoined { .. })).await {
            NetToGame::PeerJoined { conn, peer } => {
                assert_eq!(peer, client_endpoint.id().to_string());
                conn
            }
            _ => unreachable!(),
        };
        assert!(matches!(
            next_event(|event| matches!(event, NetToGame::Connected)).await,
            NetToGame::Connected
        ));
        let ClientMsg::Join { name, version } = host_frame(conn).await else {
            panic!("expected a join frame");
        };
        assert_eq!(version, WIRE_VERSION);
        let (seat, _) = host.join(&name).expect("joiner seats");
        assert_eq!(seat, 1);
        let (_, dealt) = host.deal(seat, faces("ada", 3)).expect("joiner deal");
        bridge::send_to(
            conn,
            HostMsg::Welcome {
                version: WIRE_VERSION,
                seat,
                roster: host.roster(),
                log: host.log().to_vec(),
            },
        );
        bridge::send_to(conn, HostMsg::Faces { faces: dealt });

        let HostMsg::Welcome {
            seat, roster, log, ..
        } = client_frame().await
        else {
            panic!("expected welcome");
        };
        let mut client = ClientSession::from_welcome(seat, roster, log);
        let HostMsg::Faces { faces } = client_frame().await else {
            panic!("expected faces");
        };
        client.add_faces(faces);
        assert_eq!(area(&client.table(), 1, Zone::Hand).len(), 3);

        let played = area(&client.table(), 1, Zone::Hand)[0];
        let intent = WireIntent::Move {
            card: played,
            to: WireZone::Board,
            seat: 1,
            index: 0,
        };
        client.optimistic(intent.clone());
        bridge::send_to_host(ClientMsg::Intent { intent });
        let ClientMsg::Intent { intent } = host_frame(conn).await else {
            panic!("expected an intent frame");
        };
        let entries = host.intent(seat, intent).expect("the joiner's play folds");
        deliver(&mut client, &entries, conn).await;

        let answered = area(&host.table(), 0, Zone::Hand)[0];
        let entries = host
            .intent(
                0,
                WireIntent::Move {
                    card: answered,
                    to: WireZone::Board,
                    seat: 1,
                    index: 1,
                },
            )
            .expect("the host's play folds");
        deliver(&mut client, &entries, conn).await;

        for seat in 0..2 {
            for zone in [Zone::Hand, Zone::Board] {
                assert_eq!(
                    area(&host.table(), seat, zone),
                    area(&client.table(), seat, zone)
                );
            }
        }
        assert_eq!(encode_log(host.log()), encode_log(client.log()));
        assert_eq!(
            area(&client.table(), 1, Zone::Board),
            vec![played, answered]
        );

        bridge::close_host(Some((&host_mesh, &protocol)));
        assert!(!protocol.is_open());
        assert!(matches!(
            next_event(|event| matches!(event, NetToGame::HostClosed)).await,
            NetToGame::HostClosed
        ));
        let NetToGame::Dropped { reason } =
            next_event(|event| matches!(event, NetToGame::Dropped { .. })).await
        else {
            unreachable!()
        };
        assert!(
            reason.contains("table closed"),
            "the joiner learns the table closed: {reason}"
        );
        let NetToGame::PeerLeft { conn: left, .. } =
            next_event(|event| matches!(event, NetToGame::PeerLeft { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(left, conn);
    });
}

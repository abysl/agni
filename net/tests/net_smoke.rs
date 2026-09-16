#![cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]

use agni_core::{CardFace, PlayerId, Table, Zone};
use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, ClientMsg, ClientSession, HostMsg,
    HostSession, WireIntent, WireZone,
};
use agni_net::table::{join_via, HostEvent, JoinEvent, TableProtocol, ALPN};
use agni_sim::log::{encode_log, LogAction};
use spirit_node::iroh::endpoint::presets;
use spirit_node::iroh::protocol::Router;
use spirit_node::iroh::Endpoint;

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

async fn next_frame(link: &mut agni_net::table::TableJoin) -> HostMsg {
    match link.next().await.expect("link stays open") {
        JoinEvent::Frame(bytes) => decode_host(&bytes).expect("host frame decodes"),
        JoinEvent::Closed(reason) => panic!("link closed early: {reason}"),
    }
}

#[test]
#[ignore]
fn host_and_client_converge_over_iroh() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime builds");
    runtime.block_on(async {
        let host_endpoint = Endpoint::bind(presets::N0).await.expect("host endpoint");
        let protocol = TableProtocol::new();
        let _router = Router::builder(host_endpoint.clone())
            .accept(ALPN, protocol.clone())
            .spawn();
        let mut table_host = protocol.open();

        let mut solo = Table::new();
        for face in faces("host", 3) {
            solo.add_face(PlayerId(0), Zone::Hand, face);
        }
        let mut host = HostSession::host_from("host", &solo);

        let client_endpoint = Endpoint::bind(presets::N0).await.expect("client endpoint");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let mut link = join_via(&client_endpoint, host_endpoint.addr())
            .await
            .expect("client dials host");
        link.send(encode_client(&ClientMsg::Join {
            name: "ada".into(),
            version: agni_net::session::WIRE_VERSION,
        }));

        let conn = loop {
            match table_host.next().await.expect("host event") {
                HostEvent::Joined(conn, _) => break conn,
                _ => continue,
            }
        };
        let join_frame = loop {
            match table_host.next().await.expect("host event") {
                HostEvent::Frame(from, bytes) if from == conn => break bytes,
                _ => continue,
            }
        };
        let Some(ClientMsg::Join { name, .. }) = decode_client(&join_frame) else {
            panic!("expected a join frame");
        };
        let (seat, _) = host.join(&name).expect("joiner seats");
        let (_, wire_faces) = host.deal(seat, faces("ada", 3)).expect("joiner deal");
        table_host.send(
            conn,
            encode_host(&HostMsg::Welcome {
                version: agni_net::session::WIRE_VERSION,
                seat,
                roster: host.roster(),
                log: host.log().to_vec(),
            }),
        );
        table_host.send(conn, encode_host(&HostMsg::Faces { faces: wire_faces }));

        let HostMsg::Welcome {
            seat, roster, log, ..
        } = next_frame(&mut link).await
        else {
            panic!("expected welcome");
        };
        let mut client = ClientSession::from_welcome(seat, roster, log);
        let HostMsg::Faces { faces } = next_frame(&mut link).await else {
            panic!("expected faces");
        };
        client.add_faces(faces);
        assert_eq!(seat, 1);
        assert_eq!(area(&client.table(), 1, Zone::Hand).len(), 3);
        assert!(client
            .table()
            .in_area(PlayerId(0), Zone::Hand)
            .all(|card| card.face.name.is_empty()));

        let my_card = area(&client.table(), 1, Zone::Hand)[0];
        let intent = WireIntent::Move {
            card: my_card,
            to: WireZone::Board,
            seat: 1,
            index: 0,
        };
        client.optimistic(intent.clone());
        link.send(encode_client(&ClientMsg::Intent { intent }));
        let intent_frame = loop {
            match table_host.next().await.expect("host event") {
                HostEvent::Frame(from, bytes) if from == conn => break bytes,
                _ => continue,
            }
        };
        let Some(ClientMsg::Intent { intent }) = decode_client(&intent_frame) else {
            panic!("expected an intent frame");
        };
        let echoed = host.intent(seat, intent).expect("client intent is valid");
        assert!(matches!(echoed[1].action, LogAction::Reveal { .. }));
        for entry in &echoed {
            table_host.send(
                conn,
                encode_host(&HostMsg::Entry {
                    entry: entry.clone(),
                }),
            );
        }
        for _ in 0..echoed.len() {
            let HostMsg::Entry { entry } = next_frame(&mut link).await else {
                panic!("expected an entry");
            };
            client.apply(entry).expect("replica folds");
        }

        let host_card = area(&host.table(), 0, Zone::Hand)[0];
        let entries = host
            .intent(
                0,
                WireIntent::Move {
                    card: host_card,
                    to: WireZone::Board,
                    seat: 1,
                    index: 1,
                },
            )
            .expect("host intent is valid");
        for entry in &entries {
            table_host.send(
                conn,
                encode_host(&HostMsg::Entry {
                    entry: entry.clone(),
                }),
            );
        }
        for _ in 0..entries.len() {
            let HostMsg::Entry { entry } = next_frame(&mut link).await else {
                panic!("expected an entry");
            };
            client.apply(entry).expect("replica folds");
        }

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
            vec![my_card, host_card]
        );
        assert_eq!(
            client
                .table()
                .get(agni_core::CardId(host_card))
                .unwrap()
                .face
                .name,
            "host 0"
        );
    });
}

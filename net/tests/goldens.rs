use agni_core::CardFace;
use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, ClientMsg, HostMsg, SeatInfo,
    WireIntent, WireZone, WIRE_VERSION,
};
use agni_sim::log::{LogAction, LogEntry};
use agni_sim::wire::{CounterTarget, DealGroup, DealTarget};
use serde_bytes::ByteBuf;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn check_golden(name: &str, bytes: &[u8]) {
    let path = fixture(name);
    if std::env::var_os("AGNI_GOLDEN_WRITE").is_some() {
        std::fs::write(&path, format!("{}\n", hex(bytes))).unwrap();
    }
    let pinned = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(
        hex(bytes),
        pinned.trim(),
        "{name} drifted from its golden vector; if the wire format really changed, bump WIRE_VERSION and regenerate with AGNI_GOLDEN_WRITE=1"
    );
}

fn client_messages() -> Vec<ClientMsg> {
    vec![
        ClientMsg::Join {
            name: "ada".into(),
            version: WIRE_VERSION,
        },
        ClientMsg::Intent {
            intent: WireIntent::Move {
                card: 7,
                to: WireZone::Board,
                seat: 1,
                index: 2,
            },
        },
        ClientMsg::Intent {
            intent: WireIntent::Annotate {
                card: 7,
                key: "exhausted".into(),
                value: None,
            },
        },
        ClientMsg::Intent {
            intent: WireIntent::Game {
                data: ByteBuf::from(vec![4, 2]),
            },
        },
        ClientMsg::Intent {
            intent: WireIntent::Counter {
                target: CounterTarget::Seat(1),
                counter: 0,
                delta: -3,
            },
        },
        ClientMsg::Intent {
            intent: WireIntent::Counter {
                target: CounterTarget::Card(7),
                counter: 2,
                delta: 1,
            },
        },
        ClientMsg::DealDeck {
            groups: vec![DealGroup {
                target: DealTarget::Zone("library".into()),
                faces: vec![CardFace::named("Island"), CardFace::hidden()],
                shuffle: true,
                draw: 7,
            }],
        },
        ClientMsg::ReloadDeck {
            groups: vec![DealGroup {
                target: DealTarget::Spread("battlefields".into()),
                faces: vec![CardFace {
                    name: "Bandle Tree".into(),
                    tint: [1, 2, 3],
                    foil: false,
                    kind: None,
                    energy: None,
                    power: None,
                    might: None,
                    domain: Vec::new(),
                }],
                shuffle: false,
                draw: 0,
            }],
        },
        ClientMsg::PickColor { color: 3 },
        ClientMsg::NeedModule { hash: [0xc5; 32] },
    ]
}

fn host_messages() -> Vec<HostMsg> {
    let roster = vec![
        SeatInfo {
            seat: 0,
            name: "rae".into(),
            host: true,
            connected: true,
            color: 0,
            playmat: None,
        },
        SeatInfo {
            seat: 1,
            name: "ada".into(),
            host: false,
            connected: false,
            color: 4,
            playmat: None,
        },
    ];
    let entry = LogEntry::new(1, 1, LogAction::Join { name: "ada".into() });
    let counter = LogEntry::new(
        2,
        1,
        LogAction::Counter {
            target: CounterTarget::Seat(1),
            counter: 0,
            delta: -3,
        },
    );
    vec![
        HostMsg::Welcome {
            version: WIRE_VERSION,
            seat: 1,
            roster: roster.clone(),
            log: vec![
                LogEntry::new(0, 0, LogAction::genesis("rae")),
                entry.clone(),
            ],
        },
        HostMsg::Roster { roster },
        HostMsg::Entry { entry },
        HostMsg::Entry { entry: counter },
        HostMsg::Faces {
            faces: vec![(3, CardFace::named("Ambush"))],
        },
        HostMsg::End {
            reason: "closed".into(),
        },
        HostMsg::Module {
            hash: [0xc5; 32],
            total: 6,
            offset: 4,
            bytes: vec![0xde, 0xad],
        },
        HostMsg::NoModule {
            hash: [0xc6; 32],
            reason: "this table pinned no module".into(),
        },
    ]
}

#[test]
fn the_client_wire_encoding_is_pinned() {
    let messages = client_messages();
    let bytes: Vec<u8> = messages.iter().flat_map(encode_client).collect();
    check_golden("client_v6.hex", &bytes);
    for msg in &messages {
        assert_eq!(decode_client(&encode_client(msg)).unwrap(), *msg);
    }
}

#[test]
fn the_host_wire_encoding_is_pinned() {
    let messages = host_messages();
    let bytes: Vec<u8> = messages.iter().flat_map(encode_host).collect();
    check_golden("host_v6.hex", &bytes);
    for msg in &messages {
        assert_eq!(decode_host(&encode_host(msg)).unwrap(), *msg);
    }
}

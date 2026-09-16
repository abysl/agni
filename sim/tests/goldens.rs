use agni_core::{CardFace, Zone};
use agni_sim::abi::{decode, encode, FoldRequest, ViewRequest};
use agni_sim::engine::FoldMode;
use agni_sim::log::{decode_log, encode_log, Effect, LogAction, LogEntry, TableConfig, Verdict};
use agni_sim::wire::{
    ZoneDecl, ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneSpec, ZoneVisibility,
};
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

fn unhex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text.trim().bytes().collect();
    digits
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
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
        "{name} drifted from its golden vector; if the wire format really changed, bump the version and regenerate with AGNI_GOLDEN_WRITE=1"
    );
}

const HAND: ZoneSpec = ZoneSpec {
    id: 1,
    name: "hand",
    label: "Hand",
    kind: ZoneKind::Hand,
    owner: ZoneOwner::PerSeat,
    visibility: ZoneVisibility::Owner,
    layout: ZoneLayout::Fan,
    place: ZonePlace::Fan,
    span: 3,
};

const BOARD: ZoneSpec = ZoneSpec {
    id: 2,
    name: "board",
    label: "Board",
    kind: ZoneKind::Battlefield,
    owner: ZoneOwner::Shared,
    visibility: ZoneVisibility::All,
    layout: ZoneLayout::Grid,
    place: ZonePlace::Center,
    span: 6,
};

fn scripted_log() -> Vec<LogEntry> {
    let config = TableConfig {
        engine: Some("blob:00ff".into()),
        plugin: None,
        zones: vec![ZoneDecl::from(&HAND), ZoneDecl::from(&BOARD)],
        options: Some(ByteBuf::from(vec![1, 2, 3])),
        counters: Vec::new(),
        despawn_any: false,
    };
    vec![
        LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config,
            },
        ),
        LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
        LogEntry::new(
            2,
            0,
            LogAction::Deal {
                cards: vec![0, 1, 2],
                to: Zone::Plugin(1),
            },
        ),
        LogEntry::new(
            3,
            0,
            LogAction::Move {
                card: 1,
                to: Zone::Plugin(2),
                seat: 0,
                index: 4,
                hidden: false,
            },
        ),
        LogEntry::new(
            4,
            0,
            LogAction::Reveal {
                card: 1,
                face: CardFace {
                    name: "Ambush".into(),
                    tint: [10, 20, 30],
                    foil: true,
                    kind: None,
                    energy: None,
                    power: None,
                    might: None,
                    domain: Vec::new(),
                },
            },
        ),
        LogEntry::new(
            5,
            1,
            LogAction::Annotate {
                card: 1,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![1])),
            },
        ),
        LogEntry::new(
            6,
            1,
            LogAction::Game {
                data: ByteBuf::from(vec![9, 9]),
            },
        ),
        LogEntry::new(7, 0, LogAction::Reset),
        LogEntry::new(8, 0, LogAction::Clear { seat: 1 }),
        LogEntry::new(
            9,
            0,
            LogAction::Reveal {
                card: 2,
                face: CardFace::hidden(),
            },
        ),
    ]
}

#[test]
fn the_log_encoding_is_pinned() {
    let log = scripted_log();
    let bytes = encode_log(&log);
    check_golden("log_v1.hex", &bytes);
    assert_eq!(decode_log(&bytes).unwrap(), log);
}

#[test]
fn the_pinned_log_still_decodes() {
    let bytes = unhex(&std::fs::read_to_string(fixture("log_v1.hex")).unwrap());
    assert_eq!(decode_log(&bytes).unwrap(), scripted_log());
}

#[test]
fn the_abi_request_encoding_is_pinned() {
    let fold = FoldRequest {
        entry: scripted_log().remove(3),
        verdict: Some(Verdict {
            accept: true,
            plugin_state: Some(ByteBuf::from(vec![4])),
            effects: vec![
                Effect::Spawn {
                    face: CardFace::named("Sprite")
                        .with_kind("Unit")
                        .with_might(Some(3)),
                    to: Zone::Plugin(2),
                    seat: 1,
                    owner: Some(1),
                },
                Effect::Despawn { card: 1 },
            ],
            reason: None,
        }),
        mode: FoldMode::Admission,
        viewer: 1,
    };
    let bytes = encode(&fold);
    check_golden("fold_request_v1.hex", &bytes);
    assert_eq!(decode::<FoldRequest>(&bytes).unwrap(), fold);
    let refused = FoldRequest {
        verdict: Some(Verdict::refuse("the chain resolves itself")),
        ..fold
    };
    let bytes = encode(&refused);
    check_golden("fold_refusal_v1.hex", &bytes);
    assert_eq!(decode::<FoldRequest>(&bytes).unwrap(), refused);
    let view = ViewRequest { viewer: 2 };
    check_golden("view_request_v0.hex", &encode(&view));
}

#[test]
fn a_bare_verdict_and_an_ownerless_spawn_still_encode_as_before() {
    let bare = FoldRequest {
        entry: scripted_log().remove(3),
        verdict: Some(Verdict::reject()),
        mode: FoldMode::Admission,
        viewer: 1,
    };
    let bytes = encode(&bare);
    assert_eq!(
        hex(&bytes),
        std::fs::read_to_string(fixture("fold_request_v0.hex"))
            .unwrap()
            .trim()
    );
    let spawn = Effect::Spawn {
        face: CardFace::named("Sprite"),
        to: Zone::Plugin(2),
        seat: 1,
        owner: None,
    };
    let bytes = encode(&spawn);
    assert!(!bytes.windows(5).any(|window| window == b"owner"));
    assert_eq!(decode::<Effect>(&bytes).unwrap(), spawn);
}

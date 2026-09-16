use agni_core::{CardFace, CardId, PlayerId, Zone};
use agni_engine_host::{load_engine, load_plugin, WasmEngine, WasmPlugin};
use agni_harden::{harden, HardenConfig, HardenedModule};
use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, engine_blob_ref, genesis_engine_pin,
    genesis_plugin_pin, module_matches_pin, pin_hash, roster_color, verify_engine_pin,
    verify_plugin_pin, ClientMsg, ClientSession, HostMsg, HostSession, WireIntent, WireZone,
    WIRE_VERSION,
};
use agni_riftbound::{
    deal_plan, zone_table, DeckFaces, ZONE_BASE, ZONE_BATTLEFIELD_FIRST, ZONE_CHAMPION, ZONE_HAND,
    ZONE_LEGEND, ZONE_MAIN_DECK, ZONE_RUNE_DECK, ZONE_RUNE_POOL, ZONE_SIDEBOARD,
};
use agni_sim::log::{encode_log, encode_state, LogEntry, TableConfig};
use agni_sim::wire::decode_plugin_manifest;
use serde_bytes::ByteBuf;
use spirit_node::spirit_core::{identity, BlobHash, BlobStore};
use spirit_sdk::modules::{self, Module, Role};
use spirit_sdk::record::Tdr;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const ENGINE_BUDGET: u64 = 10_000_000_000;
const PLUGIN_BUDGET: u64 = 100_000_000;
const MAIN: usize = 40;
const RUNES: usize = 12;
const FIELDS: usize = 3;
const OPENING: usize = agni_riftbound::OPENING_HAND_SIZE as usize;

fn built_module(env_var: &str, package: &str, artifact: &str) -> PathBuf {
    if let Ok(path) = std::env::var(env_var) {
        return PathBuf::from(path);
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("workspace root resolves");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--jobs",
            &test_build_jobs(),
        ])
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "building {package} for wasm32 failed — set {env_var} to a prebuilt module"
    );
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));
    target.join(format!("wasm32-unknown-unknown/release/{artifact}"))
}

fn hardened_engine() -> &'static HardenedModule {
    static HARDENED: OnceLock<HardenedModule> = OnceLock::new();
    HARDENED.get_or_init(|| {
        let raw = std::fs::read(built_module(
            "AGNI_ENGINE_WASM",
            "agni-engine-wasm",
            "agni_engine_wasm.wasm",
        ))
        .expect("engine module reads");
        harden(&raw, &HardenConfig::engine()).expect("engine survives the pipeline")
    })
}

fn hardened_plugin() -> &'static HardenedModule {
    static HARDENED: OnceLock<HardenedModule> = OnceLock::new();
    HARDENED.get_or_init(|| {
        let raw = std::fs::read(built_module(
            "AGNI_RIFTBOUND_WASM",
            "agni-riftbound-plugin",
            "riftbound_plugin.wasm",
        ))
        .expect("plugin module reads");
        let first = harden(&raw, &HardenConfig::default()).expect("plugin survives the pipeline");
        let again = harden(&raw, &HardenConfig::default()).expect("plugin hardens twice");
        assert_eq!(first.hash, again.hash);
        first
    })
}

fn wasm_engine() -> Box<WasmEngine> {
    Box::new(load_engine(&hardened_engine().bytes, ENGINE_BUDGET).unwrap())
}

fn wasm_plugin(bytes: &[u8]) -> Box<WasmPlugin> {
    Box::new(load_plugin(bytes, PLUGIN_BUDGET).unwrap())
}

fn face(name: String) -> agni_core::CardFace {
    CardFace::named(name)
}

fn synthetic_deck(prefix: &str) -> DeckFaces {
    DeckFaces {
        legend: Some(face(format!("{prefix} Chronicle Warden"))),
        chosen_champion: Some(face(format!("{prefix} Emberwing Vanguard"))),
        main_deck: (0..MAIN)
            .map(|i| face(format!("{prefix} Main {i:02}")))
            .collect(),
        runes: (0..RUNES)
            .map(|i| face(format!("{prefix} Rune {i:02}")))
            .collect(),
        battlefields: (0..FIELDS)
            .map(|i| face(format!("{prefix} Bastion {i}")))
            .collect(),
        sideboard: vec![
            face(format!("{prefix} Spare Blade")),
            face(format!("{prefix} Spare Sigil")),
        ],
    }
}

fn scratch_store() -> BlobStore {
    let dir = std::env::temp_dir().join(format!("agni-riftbound-mvp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    BlobStore::open(dir).unwrap()
}

fn relay(client: &mut ClientSession, entries: &[LogEntry]) {
    for entry in entries {
        let framed = encode_host(&HostMsg::Entry {
            entry: entry.clone(),
        });
        let HostMsg::Entry { entry } = decode_host(&framed).unwrap() else {
            panic!("expected an entry frame");
        };
        assert!(client.apply(entry).unwrap());
    }
}

fn relay_private_faces(host: &HostSession, client: &mut ClientSession, entries: &[LogEntry]) {
    let owed: Vec<_> = host
        .private_faces(entries)
        .into_iter()
        .filter(|(seat, _)| *seat == client.seat())
        .map(|(_, face)| face)
        .collect();
    if owed.is_empty() {
        return;
    }
    let framed = encode_host(&HostMsg::Faces { faces: owed });
    let HostMsg::Faces { faces } = decode_host(&framed).unwrap() else {
        panic!("expected a faces frame");
    };
    client.add_faces(faces);
}

fn client_intent(
    host: &mut HostSession,
    client: &mut ClientSession,
    seat: u8,
    intent: WireIntent,
) -> Vec<LogEntry> {
    let framed = encode_client(&ClientMsg::Intent { intent });
    let ClientMsg::Intent { intent } = decode_client(&framed).unwrap() else {
        panic!("expected an intent frame");
    };
    let entries = host.intent(seat, intent).expect("intent is valid");
    relay(client, &entries);
    relay_private_faces(host, client, &entries);
    entries
}

fn host_intent(
    host: &mut HostSession,
    client: &mut ClientSession,
    intent: WireIntent,
) -> Vec<LogEntry> {
    let entries = host.intent(0, intent).expect("intent is valid");
    relay(client, &entries);
    relay_private_faces(host, client, &entries);
    entries
}

fn ids_in(table: &agni_core::Table, seat: u8, zone: u16) -> Vec<u32> {
    table
        .in_area(PlayerId(seat), Zone::Plugin(zone))
        .map(|card| card.id.0)
        .collect()
}

fn name_of(table: &agni_core::Table, card: u32) -> String {
    table.get(CardId(card)).unwrap().face.name.clone()
}

fn rotated(view: &agni_sim::view::TableView, card: u32) -> bool {
    view.cards
        .iter()
        .find(|entry| entry.id == card)
        .map(|entry| entry.badge("exhausted").is_some())
        .unwrap_or(false)
}

fn exhaust_card(card: u32, on: bool) -> WireIntent {
    WireIntent::Annotate {
        card,
        key: "exhausted".into(),
        value: on.then(|| ByteBuf::from(vec![0xf5])),
    }
}

fn move_card(card: u32, zone: u16, seat: u8, index: u32) -> WireIntent {
    WireIntent::Move {
        card,
        to: WireZone::Plugin(zone),
        seat,
        index,
    }
}

fn assert_dealt_shape(table: &agni_core::Table, seat: u8, prefix: &str, drawn: usize) {
    let legend = ids_in(table, seat, ZONE_LEGEND);
    assert_eq!(legend.len(), 1);
    assert_eq!(
        name_of(table, legend[0]),
        format!("{prefix} Chronicle Warden")
    );
    let champion = ids_in(table, seat, ZONE_CHAMPION);
    assert_eq!(champion.len(), 1);
    assert_eq!(
        name_of(table, champion[0]),
        format!("{prefix} Emberwing Vanguard")
    );
    let runes = ids_in(table, seat, ZONE_RUNE_DECK);
    assert_eq!(runes.len(), RUNES);
    for card in runes {
        assert_eq!(name_of(table, card), "");
    }
    assert_eq!(
        ids_in(table, seat, ZONE_MAIN_DECK).len(),
        MAIN - OPENING - drawn
    );
    assert_eq!(ids_in(table, seat, ZONE_SIDEBOARD).len(), 2);
    assert!(ids_in(table, seat, ZONE_BASE).is_empty());
    assert!(ids_in(table, seat, ZONE_RUNE_POOL).is_empty());
}

fn assert_opening_hand(table: &agni_core::Table, seat: u8, visible_prefix: Option<&str>) {
    let hand = ids_in(table, seat, ZONE_HAND);
    assert_eq!(hand.len(), OPENING);
    for card in hand {
        match visible_prefix {
            Some(prefix) => {
                assert!(name_of(table, card).starts_with(&format!("{prefix} Main")))
            }
            None => assert_eq!(name_of(table, card), ""),
        }
    }
}

#[test]
fn two_seats_play_the_riftbound_mvp_through_the_hardened_plugin() {
    let plugin = hardened_plugin();
    let engine = hardened_engine();

    let mut probe = wasm_plugin(&plugin.bytes);
    let manifest_bytes = probe.manifest_bytes().unwrap();
    let manifest = decode_plugin_manifest(&manifest_bytes).unwrap();
    assert_eq!(manifest.name, "riftbound");
    assert_eq!(manifest.zones, zone_table());
    assert!(!manifest.hotkeys.is_empty());

    let store = scratch_store();
    let identity = identity::load_or_create(store.root()).unwrap();
    modules::publish(
        &store,
        &identity,
        &Module::new("riftbound", Role::Plugin, &manifest.version, 0),
        &Tdr::new("wasm-harden", &("test",)).unwrap(),
        &plugin.bytes,
    )
    .unwrap();

    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: manifest.zones.clone(),
            options: None,
            counters: Vec::new(),
            despawn_any: false,
        },
        wasm_engine(),
        Some(wasm_plugin(&plugin.bytes)),
    )
    .unwrap();
    assert_eq!(host.state().zones, zone_table());

    let engine_pin = genesis_engine_pin(host.log()).unwrap();
    assert_eq!(engine_pin, engine_blob_ref(engine.hash));
    let plugin_pin = genesis_plugin_pin(host.log()).unwrap();
    assert_eq!(plugin_pin, engine_blob_ref(plugin.hash));

    let (ada, _) = host.join("ada").unwrap();
    let framed = encode_host(&HostMsg::Welcome {
        version: WIRE_VERSION,
        seat: ada,
        roster: host.roster(),
        log: host.log().to_vec(),
    });
    let HostMsg::Welcome {
        seat, roster, log, ..
    } = decode_host(&framed).unwrap()
    else {
        panic!("expected a welcome frame");
    };

    let fetched_pin = genesis_plugin_pin(&log).unwrap();
    let fetched = store
        .get(BlobHash::from_bytes(pin_hash(&fetched_pin).unwrap()))
        .unwrap();
    assert!(module_matches_pin(&fetched_pin, &fetched));
    let joiner_plugin = wasm_plugin(&fetched);
    verify_engine_pin(&log, Some(&engine_blob_ref(engine.hash))).unwrap();
    verify_plugin_pin(&log, Some(&engine_blob_ref(joiner_plugin.hash()))).unwrap();
    verify_plugin_pin(&log, None).unwrap_err();
    verify_plugin_pin(&log, Some(&engine_blob_ref([0x11; 32]))).unwrap_err();

    let mut client =
        ClientSession::from_welcome_with(seat, roster, log, wasm_engine(), Some(joiner_plugin))
            .unwrap();

    let (entries, host_faces) = host
        .deal_groups(0, deal_plan(&synthetic_deck("rae")))
        .unwrap();
    relay(&mut client, &entries);
    assert_eq!(
        host_faces
            .iter()
            .filter(|(_, face)| face.name.starts_with("rae Main"))
            .count(),
        OPENING
    );

    let deal_msg = encode_client(&ClientMsg::DealDeck {
        groups: deal_plan(&synthetic_deck("ada")),
    });
    let ClientMsg::DealDeck { groups } = decode_client(&deal_msg).unwrap() else {
        panic!("expected a deal frame");
    };
    let (entries, owner_faces) = host.deal_groups(ada, groups).unwrap();
    relay(&mut client, &entries);
    assert_eq!(
        owner_faces
            .iter()
            .filter(|(_, face)| face.name.starts_with("ada Main"))
            .count(),
        OPENING
    );
    let framed = encode_host(&HostMsg::Faces { faces: owner_faces });
    let HostMsg::Faces { faces } = decode_host(&framed).unwrap() else {
        panic!("expected a faces frame");
    };
    client.add_faces(faces);

    for table in [host.table(), client.table()] {
        assert_dealt_shape(&table, 0, "rae", 0);
        assert_dealt_shape(&table, ada, "ada", 0);
        for slot in 0..FIELDS as u16 {
            let field = ids_in(&table, 0, ZONE_BATTLEFIELD_FIRST + slot);
            assert_eq!(field.len(), 2);
            for card in field {
                assert!(name_of(&table, card).contains("Bastion"));
            }
        }
    }
    assert_opening_hand(&host.table(), 0, Some("rae"));
    assert_opening_hand(&host.table(), ada, None);
    assert_opening_hand(&client.table(), ada, Some("ada"));
    assert_opening_hand(&client.table(), 0, None);
    let own = client.table();
    for card in ids_in(&own, ada, ZONE_SIDEBOARD) {
        assert!(name_of(&own, card).starts_with("ada Spare"));
    }
    for card in ids_in(&own, 0, ZONE_SIDEBOARD) {
        assert_eq!(name_of(&own, card), "");
    }

    for _ in 0..2 {
        let deck = ids_in(&client.table(), ada, ZONE_MAIN_DECK);
        let top = *deck.last().unwrap();
        let hand = ids_in(&client.table(), ada, ZONE_HAND).len() as u32;
        client_intent(
            &mut host,
            &mut client,
            ada,
            move_card(top, ZONE_HAND, ada, hand),
        );
        assert!(name_of(&client.table(), top).starts_with("ada Main"));
        assert_eq!(name_of(&host.state().table, top), "");
    }
    for _ in 0..2 {
        let deck = ids_in(&host.table(), 0, ZONE_MAIN_DECK);
        let top = *deck.last().unwrap();
        let hand = ids_in(&host.table(), 0, ZONE_HAND).len() as u32;
        host_intent(&mut host, &mut client, move_card(top, ZONE_HAND, 0, hand));
        assert!(name_of(&host.table(), top).starts_with("rae Main"));
        assert_eq!(name_of(&client.table(), top), "");
    }
    assert_dealt_shape(&host.table(), 0, "rae", 2);
    assert_dealt_shape(&client.table(), ada, "ada", 2);

    let ada_hand = ids_in(&client.table(), ada, ZONE_HAND);
    let played = ada_hand[0];
    let entries = client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(played, ZONE_BATTLEFIELD_FIRST, ada, 2),
    );
    assert_eq!(entries.len(), 2);
    assert!(name_of(&host.table(), played).starts_with("ada Main"));

    client_intent(&mut host, &mut client, ada, exhaust_card(played, true));
    assert!(rotated(host.view(), played));
    assert!(rotated(client.view(), played));
    host_intent(&mut host, &mut client, exhaust_card(played, false));
    assert!(!rotated(host.view(), played));
    assert!(!rotated(client.view(), played));

    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(played, ZONE_HAND, ada, 0),
    );
    assert!(name_of(&client.table(), played).starts_with("ada Main"));

    let rae_hand = ids_in(&host.table(), 0, ZONE_HAND);
    let rae_played = rae_hand[0];
    host_intent(
        &mut host,
        &mut client,
        move_card(rae_played, ZONE_BATTLEFIELD_FIRST + 1, 0, 2),
    );
    host_intent(&mut host, &mut client, exhaust_card(rae_played, true));
    assert!(rotated(client.view(), rae_played));
    host_intent(&mut host, &mut client, exhaust_card(rae_played, false));
    host_intent(
        &mut host,
        &mut client,
        move_card(rae_played, ZONE_HAND, 0, 0),
    );

    for (seat, drive_client) in [(0u8, false), (ada, true)] {
        for zone in [ZONE_CHAMPION, ZONE_LEGEND] {
            let piece = ids_in(&host.table(), seat, zone)[0];
            let out = move_card(piece, ZONE_BATTLEFIELD_FIRST + 2, seat, 9);
            let back = move_card(piece, zone, seat, 0);
            if drive_client {
                client_intent(&mut host, &mut client, seat, out);
                client_intent(&mut host, &mut client, seat, exhaust_card(piece, true));
                client_intent(&mut host, &mut client, seat, exhaust_card(piece, false));
                client_intent(&mut host, &mut client, seat, back);
            } else {
                host_intent(&mut host, &mut client, out);
                host_intent(&mut host, &mut client, exhaust_card(piece, true));
                host_intent(&mut host, &mut client, exhaust_card(piece, false));
                host_intent(&mut host, &mut client, back);
            }
            assert_eq!(ids_in(&client.table(), seat, zone).len(), 1);
        }
    }

    let channelled = ids_in(&host.table(), 0, ZONE_RUNE_DECK)[0];
    host_intent(
        &mut host,
        &mut client,
        move_card(channelled, ZONE_RUNE_POOL, 0, 0),
    );
    assert_eq!(ids_in(&client.table(), 0, ZONE_RUNE_POOL), [channelled]);
    let channelled_name = name_of(&client.table(), channelled);
    assert!(channelled_name.starts_with("rae Rune"));
    host_intent(&mut host, &mut client, exhaust_card(channelled, true));
    assert!(rotated(client.view(), channelled));
    host_intent(&mut host, &mut client, exhaust_card(channelled, false));

    let summoned = ids_in(&host.table(), 0, ZONE_HAND)[0];
    host_intent(&mut host, &mut client, move_card(summoned, ZONE_BASE, 0, 0));
    assert_eq!(ids_in(&client.table(), 0, ZONE_BASE), [summoned]);
    host_intent(
        &mut host,
        &mut client,
        move_card(summoned, ZONE_BATTLEFIELD_FIRST, 0, 2),
    );
    assert!(ids_in(&client.table(), 0, ZONE_BASE).is_empty());

    let benched = ids_in(&client.table(), ada, ZONE_SIDEBOARD)[0];
    let deck = ids_in(&client.table(), ada, ZONE_MAIN_DECK).len() as u32;
    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(benched, ZONE_MAIN_DECK, ada, deck),
    );
    assert_eq!(ids_in(&client.table(), ada, ZONE_SIDEBOARD).len(), 1);
    assert!(ids_in(&client.table(), ada, ZONE_MAIN_DECK).contains(&benched));
    assert_eq!(name_of(&host.state().table, benched), "");
    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(benched, ZONE_SIDEBOARD, ada, 1),
    );
    assert_eq!(ids_in(&client.table(), ada, ZONE_SIDEBOARD).len(), 2);
    assert!(name_of(&client.table(), benched).starts_with("ada Spare"));

    let foreign = ids_in(&client.table(), ada, ZONE_HAND)[0];
    assert!(host
        .intent(0, move_card(foreign, ZONE_BATTLEFIELD_FIRST, 0, 0))
        .is_err());

    let revealed_names: BTreeSet<String> = host
        .state()
        .revealed
        .iter()
        .filter_map(|card| host.face_of(*card))
        .map(|(_, face)| face.name)
        .collect();
    let log_bytes = encode_log(host.log());
    let leaked = |needle: &str| {
        log_bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    for prefix in ["rae", "ada"] {
        for i in 0..MAIN {
            let name = format!("{prefix} Main {i:02}");
            if !revealed_names.contains(&name) {
                assert!(!leaked(&name), "{name} leaked into the shared log");
            }
        }
        for i in 0..RUNES {
            let name = format!("{prefix} Rune {i:02}");
            if name != channelled_name {
                assert!(!leaked(&name), "{name} leaked into the shared log");
            }
        }
        assert!(!leaked(&format!("{prefix} Spare")));
        assert!(leaked(&format!("{prefix} Chronicle Warden")));
        assert!(leaked(&format!("{prefix} Bastion")));
    }
    assert!(leaked(&channelled_name));
    assert!(revealed_names.contains(&channelled_name));

    assert_eq!(encode_log(host.log()), encode_log(client.log()));
    assert_eq!(encode_state(host.state()), encode_state(client.state()));
    assert_eq!(host.view().cards.len(), client.view().cards.len());

    assert_eq!(host.roster()[0].color, 0);
    assert_eq!(host.roster()[1].color, 1);
    let pick_msg = encode_client(&ClientMsg::PickColor { color: 3 });
    let ClientMsg::PickColor { color } = decode_client(&pick_msg).unwrap() else {
        panic!("expected a colour frame");
    };
    assert!(host.pick_color(ada, color));
    assert!(!host.pick_color(0, color));
    let framed = encode_host(&HostMsg::Roster {
        roster: host.roster(),
    });
    let HostMsg::Roster { roster } = decode_host(&framed).unwrap() else {
        panic!("expected a roster frame");
    };
    client.set_roster(roster);
    assert_eq!(roster_color(client.roster(), ada), 3);
    assert_ne!(roster_color(client.roster(), 0), 3);

    let mut sideboarded = synthetic_deck("ada");
    sideboarded.sideboard.push(face("ada Spare Ward".into()));
    sideboarded.main_deck.truncate(MAIN - 1);
    let reload_msg = encode_client(&ClientMsg::ReloadDeck {
        groups: deal_plan(&sideboarded),
    });
    let ClientMsg::ReloadDeck { groups } = decode_client(&reload_msg).unwrap() else {
        panic!("expected a reload frame");
    };
    let rae_deck = ids_in(&host.table(), 0, ZONE_MAIN_DECK);
    let rae_hand = ids_in(&host.table(), 0, ZONE_HAND);
    let (entries, owner_faces) = host.reload_groups(ada, groups).unwrap();
    assert!(matches!(
        entries.first().map(|entry| &entry.action),
        Some(agni_sim::log::LogAction::Clear { seat }) if *seat == ada
    ));
    relay(&mut client, &entries);
    let framed = encode_host(&HostMsg::Faces { faces: owner_faces });
    let HostMsg::Faces { faces } = decode_host(&framed).unwrap() else {
        panic!("expected a faces frame");
    };
    client.add_faces(faces);
    for table in [host.table(), client.table()] {
        assert_eq!(ids_in(&table, 0, ZONE_MAIN_DECK), rae_deck);
        assert_eq!(ids_in(&table, 0, ZONE_HAND), rae_hand);
        assert_eq!(
            ids_in(&table, ada, ZONE_MAIN_DECK).len(),
            MAIN - 1 - OPENING
        );
        assert_eq!(ids_in(&table, ada, ZONE_SIDEBOARD).len(), 3);
        assert_eq!(ids_in(&table, ada, ZONE_HAND).len(), OPENING);
        assert_eq!(ids_in(&table, ada, ZONE_LEGEND).len(), 1);
        assert_eq!(ids_in(&table, ada, ZONE_RUNE_DECK).len(), RUNES);
        for slot in 0..FIELDS as u16 {
            let ada_fields = ids_in(&table, 0, ZONE_BATTLEFIELD_FIRST + slot)
                .into_iter()
                .filter(|card| name_of(&table, *card).starts_with("ada"))
                .count();
            assert_eq!(ada_fields, 1);
        }
    }
    assert_opening_hand(&client.table(), ada, Some("ada"));
    assert_opening_hand(&host.table(), ada, None);
    assert_eq!(encode_log(host.log()), encode_log(client.log()));
    assert_eq!(encode_state(host.state()), encode_state(client.state()));
    assert_eq!(host.view().cards.len(), client.view().cards.len());

    let _ = std::fs::remove_dir_all(store.root());
}

fn test_build_jobs() -> String {
    std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "4".into())
}

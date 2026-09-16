use agni_core::{CardFace, CardId, PlayerId, Zone};
use agni_engine_host::{load_engine, load_plugin, WasmEngine, WasmPlugin};
use agni_harden::{harden, HardenConfig, HardenedModule};
use agni_mtg::{
    deal_plan, zone_table, DeckFaces, MIN_MAIN_DECK, ZONE_BATTLEFIELD, ZONE_COMMAND, ZONE_EXILE,
    ZONE_GRAVEYARD, ZONE_HAND, ZONE_LIBRARY,
};
use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, engine_blob_ref, genesis_engine_pin,
    genesis_plugin_pin, module_matches_pin, pin_hash, verify_engine_pin, verify_plugin_pin,
    ClientMsg, ClientSession, HostMsg, HostSession, WireIntent, WireZone, WIRE_VERSION,
};
use agni_sim::log::{encode_log, encode_state, LogEntry, TableConfig};
use agni_sim::wire::decode_plugin_manifest;
use spirit_node::spirit_core::{identity, BlobHash, BlobStore};
use spirit_sdk::modules::{self, Module, Role};
use spirit_sdk::record::Tdr;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const ENGINE_BUDGET: u64 = 10_000_000_000;
const PLUGIN_BUDGET: u64 = 100_000_000;
const OPENING: usize = agni_mtg::OPENING_HAND_SIZE as usize;

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
            "AGNI_MTG_WASM",
            "agni-mtg-plugin",
            "mtg_plugin.wasm",
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
        commander: Some(face(format!("{prefix} Serelith, Tidebound Oracle"))),
        library: (0..MIN_MAIN_DECK)
            .map(|i| face(format!("{prefix} Library {i:02}")))
            .collect(),
    }
}

fn scratch_store() -> BlobStore {
    let dir = std::env::temp_dir().join(format!("agni-mtg-mvp-{}", std::process::id()));
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

fn revealed_names(host: &HostSession) -> BTreeSet<String> {
    host.state()
        .revealed
        .iter()
        .filter_map(|card| host.face_of(*card))
        .map(|(_, face)| face.name)
        .collect()
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
    let command = ids_in(table, seat, ZONE_COMMAND);
    assert_eq!(command.len(), 1);
    assert_eq!(
        name_of(table, command[0]),
        format!("{prefix} Serelith, Tidebound Oracle")
    );
    assert_eq!(
        ids_in(table, seat, ZONE_LIBRARY).len(),
        MIN_MAIN_DECK - OPENING - drawn
    );
}

fn assert_opening_hand(table: &agni_core::Table, seat: u8, visible_prefix: Option<&str>) {
    let hand = ids_in(table, seat, ZONE_HAND);
    assert_eq!(hand.len(), OPENING);
    for card in hand {
        match visible_prefix {
            Some(prefix) => {
                assert!(name_of(table, card).starts_with(&format!("{prefix} Library")))
            }
            None => assert_eq!(name_of(table, card), ""),
        }
    }
}

#[test]
fn two_seats_play_an_mtg_table_through_the_hardened_plugin() {
    let plugin = hardened_plugin();
    let engine = hardened_engine();

    let mut probe = wasm_plugin(&plugin.bytes);
    let manifest_bytes = probe.manifest_bytes().unwrap();
    let manifest = decode_plugin_manifest(&manifest_bytes).unwrap();
    assert_eq!(manifest.name, "mtg");
    assert_eq!(manifest.zones, zone_table());
    assert!(!manifest.hotkeys.is_empty());

    let store = scratch_store();
    let identity = identity::load_or_create(store.root()).unwrap();
    modules::publish(
        &store,
        &identity,
        &Module::new("mtg", Role::Plugin, &manifest.version, 0),
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
            .filter(|(_, face)| face.name.starts_with("rae Library"))
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
            .filter(|(_, face)| face.name.starts_with("ada Library"))
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
    }
    assert_opening_hand(&host.table(), 0, Some("rae"));
    assert_opening_hand(&host.table(), ada, None);
    assert_opening_hand(&client.table(), ada, Some("ada"));
    assert_opening_hand(&client.table(), 0, None);

    for _ in 0..2 {
        let library = ids_in(&client.table(), ada, ZONE_LIBRARY);
        let top = *library.last().unwrap();
        let hand = ids_in(&client.table(), ada, ZONE_HAND).len() as u32;
        client_intent(
            &mut host,
            &mut client,
            ada,
            move_card(top, ZONE_HAND, ada, hand),
        );
        assert!(name_of(&client.table(), top).starts_with("ada Library"));
        assert_eq!(name_of(&host.state().table, top), "");
    }
    for _ in 0..2 {
        let library = ids_in(&host.table(), 0, ZONE_LIBRARY);
        let top = *library.last().unwrap();
        let hand = ids_in(&host.table(), 0, ZONE_HAND).len() as u32;
        host_intent(&mut host, &mut client, move_card(top, ZONE_HAND, 0, hand));
        assert!(name_of(&host.table(), top).starts_with("rae Library"));
        assert_eq!(name_of(&client.table(), top), "");
    }
    assert_dealt_shape(&host.table(), 0, "rae", 2);
    assert_dealt_shape(&client.table(), ada, "ada", 2);

    let mut public: BTreeSet<String> = revealed_names(&host);

    let ada_spell = ids_in(&client.table(), ada, ZONE_HAND)[0];
    let entries = client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(ada_spell, ZONE_BATTLEFIELD, ada, 0),
    );
    assert_eq!(entries.len(), 2);
    assert!(name_of(&host.table(), ada_spell).starts_with("ada Library"));
    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(ada_spell, ZONE_GRAVEYARD, ada, 0),
    );
    assert_eq!(ids_in(&host.table(), ada, ZONE_GRAVEYARD), vec![ada_spell]);
    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(ada_spell, ZONE_EXILE, ada, 0),
    );
    assert_eq!(ids_in(&client.table(), ada, ZONE_EXILE), vec![ada_spell]);
    assert!(ids_in(&host.table(), ada, ZONE_GRAVEYARD).is_empty());
    public.extend(revealed_names(&host));
    client_intent(
        &mut host,
        &mut client,
        ada,
        move_card(ada_spell, ZONE_LIBRARY, ada, 0),
    );
    assert_eq!(
        ids_in(&host.table(), ada, ZONE_LIBRARY).len(),
        MIN_MAIN_DECK - OPENING - 1
    );

    let rae_spell = ids_in(&host.table(), 0, ZONE_HAND)[0];
    host_intent(
        &mut host,
        &mut client,
        move_card(rae_spell, ZONE_BATTLEFIELD, 0, 0),
    );
    host_intent(
        &mut host,
        &mut client,
        move_card(rae_spell, ZONE_GRAVEYARD, 0, 0),
    );
    public.extend(revealed_names(&host));
    host_intent(
        &mut host,
        &mut client,
        move_card(rae_spell, ZONE_HAND, 0, 0),
    );
    assert!(name_of(&host.table(), rae_spell).starts_with("rae Library"));

    for (seat, drive_client) in [(0u8, false), (ada, true)] {
        let commander = ids_in(&host.table(), seat, ZONE_COMMAND)[0];
        let out = move_card(commander, ZONE_BATTLEFIELD, seat, 1);
        let back = move_card(commander, ZONE_COMMAND, seat, 0);
        if drive_client {
            client_intent(&mut host, &mut client, seat, out);
            client_intent(&mut host, &mut client, seat, back);
        } else {
            host_intent(&mut host, &mut client, out);
            host_intent(&mut host, &mut client, back);
        }
        assert_eq!(ids_in(&client.table(), seat, ZONE_COMMAND).len(), 1);
    }

    let foreign = ids_in(&client.table(), ada, ZONE_HAND)[0];
    assert!(host
        .intent(0, move_card(foreign, ZONE_BATTLEFIELD, 0, 0))
        .is_err());

    public.extend(revealed_names(&host));
    let log_bytes = encode_log(host.log());
    let leaked = |needle: &str| {
        log_bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    };
    for prefix in ["rae", "ada"] {
        for i in 0..MIN_MAIN_DECK {
            let name = format!("{prefix} Library {i:02}");
            if !public.contains(&name) {
                assert!(!leaked(&name), "{name} leaked into the shared log");
            }
        }
        assert!(leaked(&format!("{prefix} Serelith, Tidebound Oracle")));
    }

    assert_eq!(encode_log(host.log()), encode_log(client.log()));
    assert_eq!(encode_state(host.state()), encode_state(client.state()));
    assert_eq!(host.view().cards.len(), client.view().cards.len());

    let _ = std::fs::remove_dir_all(store.root());
}

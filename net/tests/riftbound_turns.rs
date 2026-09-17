use agni_core::{CardFace, CardId, PlayerId, Zone};
use agni_engine_host::{load_engine, load_plugin, WasmEngine, WasmPlugin};
use agni_harden::{harden, HardenConfig, HardenedModule};
use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, genesis_engine_pin, genesis_plugin_pin,
    module_hash, pin_hash, ClientMsg, ClientSession, HostMsg, HostSession, ModuleInbox,
    SessionError, WireIntent, WireZone, MODULE_CHUNK_BYTES, WIRE_VERSION,
};
use agni_plugin_sdk::decide::{BOTTOM, TOP};
use agni_plugin_sdk::dice::{commitment, Outcome};
use agni_plugin_sdk::prompt::Pick;
use agni_riftbound::{
    counter_table, zone_table, TableOptions, COUNTER_MIGHT, ZONE_BANISHMENT, ZONE_BASE,
    ZONE_BATTLEFIELD_FIRST, ZONE_CHAIN, ZONE_HAND, ZONE_LEGEND, ZONE_MAIN_DECK, ZONE_RUNE_DECK,
    ZONE_RUNE_POOL, ZONE_TRASH,
};
use agni_riftbound_turns::engine::ctx::COUNTER_BUFFED;
use agni_riftbound_turns::engine::legal::Reason;
use agni_riftbound_turns::state::{
    CostedGrant, CostedKind, Expiry, ItemKind, ItemStatus, PromptWhy, TargetRef,
    SLOT_PROMISED_REPEAT,
};
use agni_riftbound_turns::{GameBlob, Mode, Phase, Refusal, TurnEvent};
use agni_sim::abi::{decode, encode, PluginViewRequest};
use agni_sim::engine::{
    native_decide_request, Engine, EngineFault, FoldMode, NativeEngine, PluginModule,
};
use agni_sim::log::{
    decode_state, encode_state, FoldError, LogAction, LogEntry, TableConfig, Verdict,
};
use agni_sim::wire::{
    AffordanceKind, Arrow, ArrowKind, CounterTarget, LegalKind, Origin as ArrowFrom, PluginView,
    TargetRef as Aim,
};
use serde_bytes::ByteBuf;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const ENGINE_BUDGET: u64 = 10_000_000_000;
const PLUGIN_BUDGET: u64 = 100_000_000;

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
    assert!(status.success(), "building {package} for wasm32 failed");
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));
    target.join(format!("wasm32-unknown-unknown/release/{artifact}"))
}

fn hardened(env_var: &str, package: &str, artifact: &str, config: HardenConfig) -> HardenedModule {
    let raw = std::fs::read(built_module(env_var, package, artifact)).expect("module reads");
    harden(&raw, &config).expect("module survives the pipeline")
}

fn engine_module() -> &'static HardenedModule {
    static HARDENED: OnceLock<HardenedModule> = OnceLock::new();
    HARDENED.get_or_init(|| {
        hardened(
            "AGNI_ENGINE_WASM",
            "agni-engine-wasm",
            "agni_engine_wasm.wasm",
            HardenConfig::engine(),
        )
    })
}

fn plugin_module() -> &'static HardenedModule {
    static HARDENED: OnceLock<HardenedModule> = OnceLock::new();
    HARDENED.get_or_init(|| {
        hardened(
            "AGNI_RIFTBOUND_WASM",
            "agni-riftbound-plugin",
            "riftbound_plugin.wasm",
            HardenConfig::default(),
        )
    })
}

fn wasm_engine() -> Box<WasmEngine> {
    Box::new(load_engine(&engine_module().bytes, ENGINE_BUDGET).unwrap())
}

fn wasm_plugin() -> Box<WasmPlugin> {
    Box::new(load_plugin(&plugin_module().bytes, PLUGIN_BUDGET).unwrap())
}

struct NativeRiftbound;

impl PluginModule for NativeRiftbound {
    fn decide(&mut self, request: &[u8]) -> Result<Verdict, EngineFault> {
        decode(&agni_riftbound_turns::decide_bytes(request))
            .ok_or_else(|| EngineFault("native Riftbound verdict does not decode".into()))
    }

    fn view(&mut self, request: &[u8]) -> Result<PluginView, EngineFault> {
        decode(&agni_riftbound_turns::present::present_bytes(request))
            .ok_or_else(|| EngineFault("native Riftbound view does not decode".into()))
    }
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

fn game(event: TurnEvent) -> WireIntent {
    WireIntent::Game {
        data: ByteBuf::from(event.encode()),
    }
}

fn from_host(host: &mut HostSession, client: &mut ClientSession, event: TurnEvent) {
    let entries = host
        .intent(0, game(event))
        .expect("the host's turn action is accepted");
    relay(client, &entries);
}

fn from_client(host: &mut HostSession, client: &mut ClientSession, seat: u8, event: TurnEvent) {
    let framed = encode_client(&ClientMsg::Intent {
        intent: game(event),
    });
    let ClientMsg::Intent { intent } = decode_client(&framed).unwrap() else {
        panic!("expected an intent frame");
    };
    let entries = host
        .intent(seat, intent)
        .expect("the joiner's turn action is accepted");
    relay(client, &entries);
}

fn refused(host: &mut HostSession, seat: u8, event: TurnEvent, why: Refusal) {
    let error = host
        .intent(seat, game(event))
        .expect_err("the plugin refuses it");
    assert!(error.is_refusal(), "{event:?}: {error}");
    assert_eq!(error.reason(), Some(why.label().as_str()), "{event:?}");
    assert_eq!(error.to_string(), why.label());
    assert!(matches!(error, SessionError::Refused(_)));
}

fn blob_of(host: &HostSession, client: &ClientSession) -> GameBlob {
    assert_eq!(
        host.view().plugin_state,
        client.view().plugin_state,
        "both replicas fold byte-identical blobs"
    );
    let mine = GameBlob::decode(&host.view().plugin_state).expect("the host view carries a blob");
    let theirs =
        GameBlob::decode(&client.view().plugin_state).expect("the joiner view carries a blob");
    assert_eq!(mine, theirs);
    mine
}

fn assert_plugin_replay_parity(entries: &[LogEntry], expected: &[u8], label: &str) {
    let mut native_engine = NativeEngine::new();
    let mut wasm_engine = wasm_engine();
    let mut native_plugin = NativeRiftbound;
    let mut wasm_plugin = wasm_plugin();

    for entry in entries {
        let native_request = native_engine
            .decide_request(entry)
            .unwrap_or_else(|error| panic!("{label} seq {} native request: {error}", entry.seq));
        let wasm_request = wasm_engine
            .decide_request(entry)
            .unwrap_or_else(|error| panic!("{label} seq {} wasm request: {error}", entry.seq));
        assert_eq!(
            native_request, wasm_request,
            "{label} seq {} request",
            entry.seq
        );

        let native_verdict = native_request.as_deref().map(|request| {
            native_plugin
                .decide(request)
                .unwrap_or_else(|error| panic!("{label} seq {} native verdict: {error}", entry.seq))
        });
        let wasm_verdict = wasm_request.as_deref().map(|request| {
            wasm_plugin
                .decide(request)
                .unwrap_or_else(|error| panic!("{label} seq {} wasm verdict: {error}", entry.seq))
        });
        assert_eq!(
            native_verdict, wasm_verdict,
            "{label} seq {} verdict",
            entry.seq
        );

        let native_outcome = native_engine
            .fold_entry(entry, native_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("{label} seq {} native fold: {error}", entry.seq));
        let wasm_outcome = wasm_engine
            .fold_entry(entry, wasm_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("{label} seq {} wasm fold: {error}", entry.seq));
        assert_eq!(
            native_outcome.result, wasm_outcome.result,
            "{label} seq {} fold result",
            entry.seq
        );
        assert_eq!(
            native_outcome.deltas, wasm_outcome.deltas,
            "{label} seq {} fold effects",
            entry.seq
        );
        assert!(
            native_outcome.result.is_ok(),
            "{label} seq {} originating log entry is admitted: {:?}",
            entry.seq,
            native_outcome.result
        );

        let native_snapshot = native_engine
            .snapshot()
            .unwrap_or_else(|error| panic!("{label} seq {} native snapshot: {error}", entry.seq));
        let wasm_snapshot = wasm_engine
            .snapshot()
            .unwrap_or_else(|error| panic!("{label} seq {} wasm snapshot: {error}", entry.seq));
        assert_eq!(
            native_snapshot, wasm_snapshot,
            "{label} seq {} snapshot",
            entry.seq
        );
        let native_state = decode_state(&native_snapshot).expect("native snapshot decodes");
        let wasm_state = decode_state(&wasm_snapshot).expect("wasm snapshot decodes");
        assert_eq!(native_state, wasm_state, "{label} seq {} state", entry.seq);

        for viewer in 0..2 {
            assert_eq!(
                native_engine.view(viewer).unwrap(),
                wasm_engine.view(viewer).unwrap(),
                "{label} seq {} engine view seat {viewer}",
                entry.seq
            );
            let request = encode(&PluginViewRequest::of(&native_state, viewer));
            assert_eq!(
                native_plugin.view(&request).unwrap(),
                wasm_plugin.view(&request).unwrap(),
                "{label} seq {} plugin view seat {viewer}",
                entry.seq
            );
        }

        native_engine
            .restore(&native_snapshot)
            .unwrap_or_else(|error| panic!("{label} seq {} native restore: {error}", entry.seq));
        wasm_engine
            .restore(&wasm_snapshot)
            .unwrap_or_else(|error| panic!("{label} seq {} wasm restore: {error}", entry.seq));
    }

    assert_eq!(
        native_engine.snapshot().unwrap(),
        expected,
        "{label} final state matches the originating host"
    );
    assert_eq!(
        wasm_engine.snapshot().unwrap(),
        expected,
        "{label} final wasm state matches the originating host"
    );
}

fn damage_checkpoint(entries: &[LogEntry]) -> (Vec<u8>, usize) {
    let mut warm_engine = NativeEngine::new();
    let mut warm_plugin = NativeRiftbound;
    let mut checkpoint = None;
    let mut suffix_at = None;
    for (index, entry) in entries.iter().enumerate() {
        let request = warm_engine
            .decide_request(entry)
            .unwrap_or_else(|error| panic!("damage warm seq {} request: {error}", entry.seq));
        let verdict = request.as_deref().map(|request| {
            warm_plugin
                .decide(request)
                .unwrap_or_else(|error| panic!("damage warm seq {} verdict: {error}", entry.seq))
        });
        let outcome = warm_engine
            .fold_entry(entry, verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("damage warm seq {} fold: {error}", entry.seq));
        assert!(outcome.result.is_ok());
        let snapshot = warm_engine
            .snapshot()
            .unwrap_or_else(|error| panic!("damage warm seq {} snapshot: {error}", entry.seq));
        let state = decode_state(&snapshot).expect("damage warm snapshot decodes");
        if state.plugin_state.is_empty() {
            continue;
        }
        let blob = GameBlob::decode(&state.plugin_state).expect("damage warm blob decodes");
        let has_damage = state.table.cards().iter().any(|card| {
            state.counter(
                CounterTarget::Card(card.id.0),
                agni_riftbound::COUNTER_DAMAGE,
            ) == Some(2)
        });
        if has_damage && matches!(blob.why, Some(PromptWhy::Resume { .. })) {
            checkpoint = Some(snapshot);
            suffix_at = Some(index + 1);
            break;
        }
    }
    (
        checkpoint.expect("damage corpus reaches a nonzero saved Resume"),
        suffix_at.expect("damage suffix starts after the saved Resume"),
    )
}

fn assert_damage_cold_suffix(entries: &[LogEntry], expected: &[u8]) {
    let (checkpoint, suffix_at) = damage_checkpoint(entries);

    let mut native_engine = NativeEngine::new();
    let mut wasm_engine = wasm_engine();
    native_engine
        .restore(&checkpoint)
        .expect("native cold restore");
    wasm_engine.restore(&checkpoint).expect("wasm cold restore");
    let mut native_plugin = NativeRiftbound;
    let mut wasm_plugin = wasm_plugin();
    for entry in &entries[suffix_at..] {
        let native_request = native_engine.decide_request(entry).unwrap_or_else(|error| {
            panic!("damage cold seq {} native request: {error}", entry.seq)
        });
        let wasm_request = wasm_engine
            .decide_request(entry)
            .unwrap_or_else(|error| panic!("damage cold seq {} wasm request: {error}", entry.seq));
        assert_eq!(
            native_request, wasm_request,
            "damage cold seq {} request",
            entry.seq
        );
        let native_verdict = native_request.as_deref().map(|request| {
            native_plugin.decide(request).unwrap_or_else(|error| {
                panic!("damage cold seq {} native verdict: {error}", entry.seq)
            })
        });
        let wasm_verdict = wasm_request.as_deref().map(|request| {
            wasm_plugin.decide(request).unwrap_or_else(|error| {
                panic!("damage cold seq {} wasm verdict: {error}", entry.seq)
            })
        });
        assert_eq!(
            native_verdict, wasm_verdict,
            "damage cold seq {} verdict",
            entry.seq
        );
        let native_outcome = native_engine
            .fold_entry(entry, native_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("damage cold seq {} native fold: {error}", entry.seq));
        let wasm_outcome = wasm_engine
            .fold_entry(entry, wasm_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("damage cold seq {} wasm fold: {error}", entry.seq));
        assert_eq!(
            native_outcome.result, wasm_outcome.result,
            "damage cold seq {} result",
            entry.seq
        );
        assert_eq!(
            native_outcome.deltas, wasm_outcome.deltas,
            "damage cold seq {} deltas",
            entry.seq
        );
        assert!(native_outcome.result.is_ok());
        let native_snapshot = native_engine.snapshot().expect("native cold snapshot");
        let wasm_snapshot = wasm_engine.snapshot().expect("wasm cold snapshot");
        assert_eq!(
            native_snapshot, wasm_snapshot,
            "damage cold seq {} snapshot",
            entry.seq
        );
        let state = decode_state(&native_snapshot).expect("damage cold state decodes");
        for viewer in 0..2 {
            assert_eq!(
                native_engine.view(viewer).unwrap(),
                wasm_engine.view(viewer).unwrap(),
                "damage cold seq {} engine view seat {viewer}",
                entry.seq
            );
            let request = encode(&PluginViewRequest::of(&state, viewer));
            assert_eq!(
                native_plugin.view(&request).unwrap(),
                wasm_plugin.view(&request).unwrap(),
                "damage cold seq {} plugin view seat {viewer}",
                entry.seq
            );
        }
    }
    assert_eq!(
        native_engine.snapshot().unwrap(),
        expected,
        "damage cold native final state"
    );
    assert_eq!(
        wasm_engine.snapshot().unwrap(),
        expected,
        "damage cold wasm final state"
    );
}

fn assert_registered_cold_suffix(
    entries: &[LogEntry],
    suffix_at: usize,
    expected: &[u8],
    lent: bool,
) {
    let mut warm_engine = NativeEngine::new();
    let mut warm_plugin = NativeRiftbound;
    for entry in &entries[..suffix_at] {
        let request = warm_engine
            .decide_request(entry)
            .unwrap_or_else(|error| panic!("registered warm seq {} request: {error}", entry.seq));
        let verdict = request.as_deref().map(|request| {
            warm_plugin.decide(request).unwrap_or_else(|error| {
                panic!("registered warm seq {} verdict: {error}", entry.seq)
            })
        });
        let outcome = warm_engine
            .fold_entry(entry, verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("registered warm seq {} fold: {error}", entry.seq));
        assert!(outcome.result.is_ok());
    }
    let checkpoint = warm_engine.snapshot().expect("registered warm snapshot");
    let checkpoint_state = decode_state(&checkpoint).expect("registered checkpoint decodes");
    let checkpoint_blob = GameBlob::decode(&checkpoint_state.plugin_state)
        .expect("registered checkpoint blob decodes");
    assert!(checkpoint_blob.chain.iter().any(|item| match item.kind {
        ItemKind::Granted { .. } => !lent,
        ItemKind::Lent { .. } => lent,
        _ => false,
    }));

    let mut native_engine = NativeEngine::new();
    let mut wasm_engine = wasm_engine();
    native_engine
        .restore(&checkpoint)
        .expect("registered native cold restore");
    wasm_engine
        .restore(&checkpoint)
        .expect("registered wasm cold restore");
    let mut native_plugin = NativeRiftbound;
    let mut wasm_plugin = wasm_plugin();
    for entry in &entries[suffix_at..] {
        let native_request = native_engine.decide_request(entry).unwrap_or_else(|error| {
            panic!("registered cold seq {} native request: {error}", entry.seq)
        });
        let wasm_request = wasm_engine.decide_request(entry).unwrap_or_else(|error| {
            panic!("registered cold seq {} wasm request: {error}", entry.seq)
        });
        assert_eq!(
            native_request, wasm_request,
            "registered cold seq {} request",
            entry.seq
        );
        let native_verdict = native_request.as_deref().map(|request| {
            native_plugin.decide(request).unwrap_or_else(|error| {
                panic!("registered cold seq {} native verdict: {error}", entry.seq)
            })
        });
        let wasm_verdict = wasm_request.as_deref().map(|request| {
            wasm_plugin.decide(request).unwrap_or_else(|error| {
                panic!("registered cold seq {} wasm verdict: {error}", entry.seq)
            })
        });
        assert_eq!(
            native_verdict, wasm_verdict,
            "registered cold seq {} verdict",
            entry.seq
        );
        let native_outcome = native_engine
            .fold_entry(entry, native_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| {
                panic!("registered cold seq {} native fold: {error}", entry.seq)
            });
        let wasm_outcome = wasm_engine
            .fold_entry(entry, wasm_verdict, FoldMode::Admission, 0)
            .unwrap_or_else(|error| panic!("registered cold seq {} wasm fold: {error}", entry.seq));
        assert_eq!(
            native_outcome.result, wasm_outcome.result,
            "registered cold seq {} result",
            entry.seq
        );
        assert_eq!(
            native_outcome.deltas, wasm_outcome.deltas,
            "registered cold seq {} deltas",
            entry.seq
        );
        assert!(native_outcome.result.is_ok());
        assert_eq!(
            native_engine
                .snapshot()
                .expect("registered native cold snapshot"),
            wasm_engine
                .snapshot()
                .expect("registered wasm cold snapshot"),
            "registered cold seq {} snapshot",
            entry.seq
        );
    }
    assert_eq!(
        native_engine.snapshot().unwrap(),
        expected,
        "registered cold native final state"
    );
    assert_eq!(
        wasm_engine.snapshot().unwrap(),
        expected,
        "registered cold wasm final state"
    );
}

fn labels(view: &PluginView) -> Vec<String> {
    view.shown()
        .map(|(_, affordance)| affordance.label.clone())
        .collect()
}

fn hidden_labels(view: &PluginView) -> Vec<String> {
    view.hidden
        .iter()
        .map(|index| view.affordances[usize::from(*index)].label.clone())
        .filter(|label| label != agni_plugin_sdk::manual::DISABLE)
        .collect()
}

fn open_table() -> (HostSession, ClientSession, u8) {
    open_table_with(None)
}

fn open_native_table() -> (HostSession, ClientSession, u8) {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: zone_table(),
            options: None,
            counters: counter_table(),
            despawn_any: false,
        },
        Box::new(NativeEngine::new()),
        Some(Box::new(NativeRiftbound)),
    )
    .unwrap();
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
    let client = ClientSession::from_welcome_with(
        seat,
        roster,
        log,
        Box::new(NativeEngine::new()),
        Some(Box::new(NativeRiftbound)),
    )
    .unwrap();
    (host, client, ada)
}

#[test]
fn manual_recovery_search_shuffle_and_edits_replay_without_leaking_deck_faces() {
    use agni_plugin_sdk::manual::Command as Manual;
    let (mut host, mut client, ada) = open_native_table();
    let deck = deal(
        &mut host,
        &mut client,
        ada,
        vec![
            unit("one", 3, 9, "Fury", 2),
            unit("two", 4, 9, "Fury", 3),
            unit("three", 5, 9, "Fury", 4),
        ],
        ZONE_MAIN_DECK,
    );
    let before = host.state().table.clone();
    let entries = host
        .intent(
            ada,
            WireIntent::Game {
                data: Manual::Disable.encode().into(),
            },
        )
        .unwrap();
    relay(&mut client, &entries);
    assert!(blob_of(&host, &client).manual);
    assert_eq!(host.state().table, before);
    let entries = host
        .intent(
            ada,
            WireIntent::Game {
                data: Manual::Look {
                    zone: ZONE_MAIN_DECK,
                    count: u32::MAX,
                }
                .encode()
                .into(),
            },
        )
        .unwrap();
    relay(&mut client, &entries);
    let owed = host.owed_faces();
    assert_eq!(owed.len(), 3);
    assert!(owed.iter().all(|(seat, _)| *seat == ada));
    client.add_faces(owed.into_iter().map(|(_, face)| face).collect());
    for card in &deck {
        assert!(joiner_face(&client, *card).is_some());
        assert!(face_for(&host, 0, *card).is_none());
    }
    let revealed = deck[1];
    let entries = host
        .intent(
            ada,
            WireIntent::Game {
                data: Manual::Reveal { card: revealed }.encode().into(),
            },
        )
        .unwrap();
    relay(&mut client, &entries);
    assert_eq!(face_for(&host, 0, revealed).as_deref(), Some("two"));
    assert_eq!(joiner_face(&client, revealed).as_deref(), Some("two"));
    assert_eq!(
        host.state()
            .table
            .get(agni_core::CardId(revealed))
            .unwrap()
            .zone,
        Zone::Plugin(ZONE_MAIN_DECK)
    );
    let entries = host
        .intent(
            ada,
            WireIntent::Game {
                data: Manual::Conceal { card: revealed }.encode().into(),
            },
        )
        .unwrap();
    relay(&mut client, &entries);
    assert!(face_for(&host, 0, revealed).is_none());
    assert!(joiner_face(&client, revealed).is_none());
    let entries = host
        .intent(
            ada,
            WireIntent::Game {
                data: Manual::Shuffle {
                    zone: ZONE_MAIN_DECK,
                    seed: 42,
                }
                .encode()
                .into(),
            },
        )
        .unwrap();
    relay(&mut client, &entries);
    assert!(host.state().peeks.is_empty());
    assert!(client.view().peeked.is_empty());
    for card in &deck {
        assert!(joiner_face(&client, *card).is_none());
    }
    for intent in [
        WireIntent::Move {
            card: deck[0],
            to: Zone::Plugin(ZONE_BATTLEFIELD_FIRST),
            seat: ada,
            index: TOP,
        },
        WireIntent::Counter {
            target: CounterTarget::Seat(ada),
            counter: agni_riftbound::COUNTER_POINTS,
            delta: 4,
        },
        WireIntent::Annotate {
            card: deck[0],
            key: "exhausted".into(),
            value: Some(vec![1].into()),
        },
        game(TurnEvent::EndTurn),
    ] {
        let entries = host.intent(ada, intent).unwrap();
        relay(&mut client, &entries);
    }
    assert_eq!(cards_in(&host, ada, ZONE_MAIN_DECK).len(), 2);
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
    let restored = ClientSession::from_welcome_with(
        ada,
        host.roster(),
        host.log().to_vec(),
        Box::new(NativeEngine::new()),
        Some(Box::new(NativeRiftbound)),
    )
    .unwrap();
    assert_eq!(restored.state(), client.state());
    assert!(host
        .intent(
            0,
            WireIntent::Game {
                data: Manual::Reveal { card: deck[1] }.encode().into()
            }
        )
        .is_err());
}

fn open_table_with(options: Option<TableOptions>) -> (HostSession, ClientSession, u8) {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: zone_table(),
            options: options.map(|options| ByteBuf::from(options.encode())),
            counters: counter_table(),
            despawn_any: false,
        },
        wasm_engine(),
        Some(wasm_plugin()),
    )
    .unwrap();
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
    let client =
        ClientSession::from_welcome_with(seat, roster, log, wasm_engine(), Some(wasm_plugin()))
            .unwrap();
    (host, client, ada)
}

#[test]
fn manual_recovery_reveal_conceal_and_shuffle_cross_hardened_module_abis() {
    use agni_plugin_sdk::manual::Command as Manual;
    let (mut host, mut client, ada) = open_table_with(None);
    let deck = deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("singleton", 3, 9, "Fury", 2)],
        ZONE_MAIN_DECK,
    );
    for command in [
        Manual::Disable,
        Manual::Look {
            zone: ZONE_MAIN_DECK,
            count: u32::MAX,
        },
    ] {
        let entries = host
            .intent(
                ada,
                WireIntent::Game {
                    data: command.encode().into(),
                },
            )
            .unwrap();
        relay(&mut client, &entries);
    }
    let owed = host.owed_faces();
    assert_eq!(owed.len(), 1);
    client.add_faces(owed.into_iter().map(|(_, face)| face).collect());
    for command in [
        Manual::Reveal { card: deck[0] },
        Manual::Conceal { card: deck[0] },
        Manual::Shuffle {
            zone: ZONE_MAIN_DECK,
            seed: 7,
        },
    ] {
        let entries = host
            .intent(
                ada,
                WireIntent::Game {
                    data: command.encode().into(),
                },
            )
            .unwrap();
        relay(&mut client, &entries);
    }
    assert!(face_for(&host, 0, deck[0]).is_none());
    assert!(joiner_face(&client, deck[0]).is_none());
    let restored = ClientSession::from_welcome_with(
        ada,
        host.roster(),
        host.log().to_vec(),
        wasm_engine(),
        Some(wasm_plugin()),
    )
    .unwrap();
    assert_eq!(restored.state(), client.state());
}

#[test]
fn reflection_copies_a_public_face_might_and_script_across_hardened_host_and_joiner_replay() {
    let (mut host, mut client, ada) = open_table();
    let source = deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Covert Informant", 3, 1, "Mind", 4)],
        ZONE_BASE,
    )[0];
    let mut mirror = spell("Mirror Image", 3, 2, "Mind");
    mirror.domain = vec!["Mind".into(), "Order".into()];
    let mirror = deal(&mut host, &mut client, 0, vec![mirror], ZONE_HAND)[0];
    let entries = host
        .intent(
            0,
            WireIntent::Move {
                card: mirror,
                to: WireZone::Plugin(ZONE_CHAIN),
                seat: 0,
                index: TOP,
            },
        )
        .expect("Mirror Image is playable on the free table");
    relay(&mut client, &entries);
    let source_option = option_index(&host.plugin_view(0), &card_option(source));
    pick(&mut host, &mut client, 0, source_option);
    from_host(&mut host, &mut client, TurnEvent::Pass);
    from_client(&mut host, &mut client, ada, TurnEvent::Pass);

    let reflection = *host
        .state()
        .tokens
        .iter()
        .next()
        .expect("Mirror Image creates a Reflection token");
    let copy_label = format!("{{card {reflection}}}: empower (3 energy)");
    assert_eq!(face_name(&host, reflection), "Covert Informant");
    assert_eq!(
        host.state()
            .table
            .get(CardId(reflection))
            .unwrap()
            .face
            .might,
        Some(4)
    );
    assert!(host.state().is_token(reflection));
    assert_eq!(
        client
            .state()
            .table
            .get(CardId(reflection))
            .unwrap()
            .face
            .name,
        "Covert Informant"
    );
    assert_eq!(
        client
            .state()
            .table
            .get(CardId(reflection))
            .unwrap()
            .face
            .might,
        Some(4)
    );
    assert!(client.state().is_token(reflection));
    assert!(labels(&host.plugin_view(0)).contains(&copy_label));
    assert!(labels(&client.plugin_view(0)).contains(&copy_label));

    let mut restored = ClientSession::from_welcome_with(
        ada,
        host.roster(),
        host.log().to_vec(),
        wasm_engine(),
        Some(wasm_plugin()),
    )
    .expect("the joiner rebuilds the transformed token from the host log");
    assert_eq!(restored.state(), client.state());
    assert_eq!(
        restored
            .state()
            .table
            .get(CardId(reflection))
            .unwrap()
            .face
            .might,
        Some(4)
    );
    assert!(restored.state().is_token(reflection));
    assert!(labels(&restored.plugin_view(0)).contains(&copy_label));
}

fn roll_for_first(host: &mut HostSession, client: &mut ClientSession, ada: u8) -> u8 {
    let mut secrets = [[1u8; 8], [2u8; 8]];
    for attempt in 0..32u8 {
        secrets[0][0] = attempt;
        from_host(
            host,
            client,
            TurnEvent::CommitRoll {
                commit: commitment(&secrets[0]),
            },
        );
        refused(
            host,
            ada,
            TurnEvent::RevealRoll { secret: secrets[1] },
            Refusal::Dice(agni_plugin_sdk::dice::DiceRefusal::NotEveryoneCommitted),
        );
        from_client(
            host,
            client,
            ada,
            TurnEvent::CommitRoll {
                commit: commitment(&secrets[1]),
            },
        );
        assert_eq!(labels(&host.plugin_view(0)), ["reveal"]);
        assert!(
            host.intent(0, game(TurnEvent::RevealRoll { secret: [9; 8] }))
                .is_err_and(|error| error.reason().is_some()),
            "a reveal that does not match its commitment is refused with a reason"
        );
        from_host(host, client, TurnEvent::RevealRoll { secret: secrets[0] });
        from_client(
            host,
            client,
            ada,
            TurnEvent::RevealRoll { secret: secrets[1] },
        );
        let lobby = blob_of(host, client);
        assert!(!lobby.is_playing());
        assert_eq!(lobby.players(), 2);
        if let Outcome::Winner(seat) = lobby.roll().expect("in the lobby").outcome() {
            return seat;
        }
    }
    panic!("a d6 decides within a few rounds")
}

fn target_prompt_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let (hand, _, _, garrison) = open_m7_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck: vec![unit("Wisp", 1, 0, "Calm", 1); 10],
            my_hand: vec![spell("Stupefy", 1, 0, "Mind")],
            their_garrison: vec![unit("Jinx", 3, 1, "Calm", 3)],
            ..M7Deal::default()
        },
    );
    let spell = hand[0];
    let asked = moved(&mut host, &mut client, 0, spell, ZONE_CHAIN, 0, TOP);
    assert!(
        asked.prompt.is_some(),
        "the replay includes an enforced target prompt"
    );
    let target = garrison[0];
    let target_option = option_index(&host.plugin_view(0), &card_option(target));
    let stacked = pick(&mut host, &mut client, 0, target_option);
    assert_eq!(stacked.chain.len(), 1);
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let resolved = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(resolved.chain.is_empty());
    assert_eq!(might(&host, target), -1);
    (host.log().to_vec(), encode_state(host.state()))
}

fn hidden_reveal_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (consult, hero) = a_held_battlefield_with_a_card_hidden_at_it(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    drain_priority(&mut host, &mut client);
    let walked = moved(&mut host, &mut client, 0, hero, ZONE_BASE, 0, TOP);
    if walked.holder(bf1).is_some() {
        drain_priority(&mut host, &mut client);
    }
    assert!(host.state().revealed.contains(&consult));
    (host.log().to_vec(), encode_state(host.state()))
}

fn mulligan_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let (hand, _, _, _) = open_m7_game_with_pick(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck: vec![unit("Wisp", 1, 0, "Calm", 1); 10],
            my_hand: vec![unit("Mulligan", 1, 0, "Mind", 1)],
            ..M7Deal::default()
        },
        Some(0),
    );
    assert_eq!(hand.len(), 1);
    (host.log().to_vec(), encode_state(host.state()))
}

fn temporal_portal_repeat_banked_pool_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_pool: vec![rune("Mind"); 7],
            my_hand: vec![spell("Rally the Troops", 2, 2, "Mind")],
            my_base: vec![
                unit("Jhin - Murderous Artist", 4, 1, "Fury", 4),
                gear("Temporal Portal", 3, "Mind"),
            ],
            their_deck: vec![unit("Wisp", 1, 0, "Calm", 1); 10],
            ..M9Deal::default()
        },
    );
    let rally = table.hand[0];
    let jhin = table.base[0];
    let portal = table.base[1];

    moved(
        &mut host,
        &mut client,
        0,
        jhin,
        ZONE_BATTLEFIELD_FIRST,
        0,
        TOP,
    );
    let banked = drain_priority(&mut host, &mut client);
    assert_eq!(banked.seat(0).pool.energy, 1);
    assert_eq!(banked.seat(0).pool.power.len(), 1);

    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: portal,
            ability: 0,
        },
    );
    let activated = drain_priority(&mut host, &mut client);
    assert!(activated.prompt.is_none());
    assert!(exhausted(&host, portal), "the Portal paid its self-cost");
    assert_eq!(activated.seat(0).promises.len(), 1);
    assert_eq!(activated.seat(0).pool.energy, 1);
    assert!(activated.seat(0).pool.power.is_empty());

    let ready_before_play = ready_runes(&host, 0);
    let rune_deck_before_play = cards_in(&host, 0, ZONE_RUNE_DECK).len();
    let hand_before_play = cards_in(&host, 0, ZONE_HAND).len();
    let asked = moved(&mut host, &mut client, 0, rally, ZONE_CHAIN, 0, TOP);
    assert!(
        asked.prompt.is_some(),
        "the promised Repeat suspends payment"
    );
    assert!(matches!(
        asked.why,
        Some(PromptWhy::OptionalCost { cost, .. })
            if cost == SLOT_PROMISED_REPEAT as u8
    ));
    assert_eq!(
        asked.seat(0).promises.len(),
        1,
        "the promise waits for payment"
    );
    assert_eq!(
        asked.seat(0).pool.energy,
        1,
        "the banked energy is quoted, not spent yet"
    );

    let suspended = host.log().to_vec();
    let suspended_state = encode_state(host.state());

    let yes = option_index(&host.plugin_view(0), "yes");
    let paid = pick(&mut host, &mut client, 0, yes);
    assert!(paid.prompt.is_none(), "Repeat payment has completed");
    assert_eq!(paid.chain.len(), 1, "the paid spell remains on the chain");
    assert_eq!(
        paid.chain[0].repeats(),
        1,
        "the paid spell has two executions"
    );
    assert!(paid.seat(0).promises.is_empty(), "the promise was consumed");
    assert_eq!(paid.seat(0).pool.energy, 0, "the banked energy paid Rally");
    assert!(paid.seat(0).pool.power.is_empty());
    assert_eq!(
        ready_before_play - ready_runes(&host, 0),
        4,
        "the base and Repeat costs consumed four ready runes"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_RUNE_DECK).len() - rune_deck_before_play,
        4,
        "the four power runes were recycled"
    );

    let settled = drain_priority(&mut host, &mut client);
    assert!(settled.chain.is_empty());
    assert_eq!(settled.seat(0).promises.len(), 0);
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_before_play - 1 + 2,
        "Rally drew once per execution after leaving the hand"
    );
    assert_eq!(
        settled
            .log
            .iter()
            .filter(|line| line.contains("repeats"))
            .count(),
        1,
        "the repeat marker records the second execution"
    );
    assert_eq!(
        settled
            .log
            .iter()
            .filter(|line| line.contains("rallies for turn"))
            .count(),
        2,
        "Rally's draw effect resolved twice"
    );
    assert_plugin_replay_parity(
        &suspended,
        &suspended_state,
        "Temporal Portal banked prompt",
    );
    (host.log().to_vec(), encode_state(host.state()))
}

fn native_lotus_alpha_strike_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_native_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_pool: vec![rune("Body"); 2],
            my_hand: vec![spell("Alpha Strike", 3, 1, "Body")],
            my_base: vec![unit("Vi", 3, 1, "Fury", 5)],
            their_pool: vec![rune("Fury"); 2],
            their_hand: vec![spell("Lotus Trap", 2, 0, "Fury")],
            their_garrison: vec![
                unit("Wisp", 1, 0, "Calm", 1),
                unit("Training Giant", 1, 0, "Calm", 12),
            ],
            ..M9Deal::default()
        },
    );
    let strike = table.hand[0];
    let vi = table.base[0];
    let trap = table.theirs[0];
    let (wisp, giant) = (table.garrison[0], table.garrison[1]);
    assert_eq!(ready_runes(&host, 0), 4);
    assert_eq!(ready_runes(&host, ada), 2);
    let my_rune_deck = cards_in(&host, 0, ZONE_RUNE_DECK).len();
    let their_rune_deck = cards_in(&host, ada, ZONE_RUNE_DECK).len();

    let asked = moved(&mut host, &mut client, 0, strike, ZONE_CHAIN, 0, TOP);
    assert_eq!(asked.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
    let vi_option = option_index(&host.plugin_view(0), &card_option(vi));
    let stacked = pick(&mut host, &mut client, 0, vi_option);
    assert_eq!(stacked.chain.len(), 1);
    assert_eq!(stacked.chain[0].controller, 0);
    assert_eq!(ready_runes(&host, 0), 1);
    assert_eq!(cards_in(&host, 0, ZONE_RUNE_DECK).len(), my_rune_deck + 1);
    let pool = cards_in(&host, 0, ZONE_RUNE_POOL);
    assert_eq!(pool.len(), 3, "one of the four paid runes was recycled");
    assert_eq!(
        pool.iter().filter(|rune| !exhausted(&host, **rune)).count(),
        1
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let trap_asked = moved(&mut host, &mut client, ada, trap, ZONE_CHAIN, ada, TOP);
    assert_eq!(
        trap_asked.prompt.as_ref().map(|prompt| prompt.seat),
        Some(ada)
    );
    let giant_option = option_index(&host.plugin_view(ada), &card_option(giant));
    pick(&mut host, &mut client, ada, giant_option);
    assert_eq!(ready_runes(&host, ada), 0);
    assert_eq!(cards_in(&host, ada, ZONE_RUNE_DECK).len(), their_rune_deck);
    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let trap_done = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(zone_of(&host, trap), Some((ada, ZONE_TRASH)));
    assert_eq!(trap_done.chain.len(), 1);
    let trap_blob = blob_of(&host, &client);
    assert_eq!(
        trap_blob
            .card_state(giant)
            .map(|row| row.damage_multiplier_this_turn),
        Some(2)
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let first_prompt = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(
        first_prompt.prompt.as_ref().map(|prompt| prompt.seat),
        Some(0)
    );
    let wisp_option = option_index(&host.plugin_view(0), &card_option(wisp));
    let after_wisp = pick(&mut host, &mut client, 0, wisp_option);
    assert!(after_wisp.prompt.is_some());
    let giant_option = option_index(&host.plugin_view(0), &card_option(giant));
    let after_giant = pick(&mut host, &mut client, 0, giant_option);
    assert!(after_giant.prompt.is_some());
    assert_eq!(damage_on(&host, giant), 2);
    assert_eq!(
        host.state()
            .counter(CounterTarget::Card(giant), agni_riftbound::COUNTER_DAMAGE),
        Some(2)
    );
    let saved = blob_of(&host, &client);
    assert!(matches!(saved.why, Some(PromptWhy::Resume { .. })));
    assert!(saved
        .chain
        .iter()
        .any(|item| item.kind.source() == strike && item.status == ItemStatus::Resolving));
    let giant_state = saved.card_state(giant).expect("saved giant state");
    assert_eq!(giant_state.damage_multiplier_this_turn, 2);
    assert_eq!(giant_state.damage_marks, [(0, 2)]);

    for _ in 0..3 {
        assert!(host.plugin_view(0).prompt.is_some());
        let giant_option = option_index(&host.plugin_view(0), &card_option(giant));
        pick(&mut host, &mut client, 0, giant_option);
    }
    let finished = drain_priority(&mut host, &mut client);
    assert!(finished.prompt.is_none() && finished.chain.is_empty());
    assert_eq!(zone_of(&host, wisp), Some((ada, ZONE_TRASH)));
    assert_eq!(zone_of(&host, giant), Some((0, bf1)));
    assert_eq!(damage_on(&host, giant), 8);
    let finished_giant = blob_of(&host, &client)
        .card_state(giant)
        .cloned()
        .expect("surviving giant state");
    assert_eq!(finished_giant.damage_multiplier_this_turn, 2);
    assert_eq!(finished_giant.damage_marks, [(0, 8)]);
    assert_eq!(zone_of(&host, strike), Some((0, ZONE_TRASH)));
    assert_eq!(xp(&host, 0), 1);

    let expired = turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    assert_eq!(expired.turn_player(), ada);
    assert_eq!(expired.holder(bf1), Some(ada));
    assert_eq!(damage_on(&host, giant), 0);
    let expired_giant = blob_of(&host, &client).card_state(giant).cloned();
    assert!(expired_giant
        .is_none_or(|row| { row.damage_multiplier_this_turn == 0 && row.damage_marks.is_empty() }));
    (host.log().to_vec(), encode_state(host.state()))
}

struct NativeHereToHelpTable {
    help: u32,
    hidden: [u32; 2],
    anchors: [u32; 2],
    jhin: u32,
    portal: u32,
}

fn native_here_to_help_entries() -> (Vec<LogEntry>, Vec<u8>) {
    let (mut host, mut client, ada) = open_native_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Scout", 1, 0, "Mind", 1); 14],
        ZONE_MAIN_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Wisp", 1, 0, "Calm", 1); 14],
        ZONE_MAIN_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Mind"); 10],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Mind"); 7],
        ZONE_RUNE_POOL,
    );
    let battlefields = [ZONE_BATTLEFIELD_FIRST, ZONE_BATTLEFIELD_FIRST + 1];
    for (name, zone) in ["Crossroads", "Sanctum"].into_iter().zip(battlefields) {
        deal(&mut host, &mut client, 0, vec![battlefield(name)], zone);
    }
    let base = deal(
        &mut host,
        &mut client,
        0,
        vec![
            unit("Jhin - Murderous Artist", 4, 1, "Fury", 4),
            gear("Temporal Portal", 3, "Mind"),
            unit("Field Anchor One", 1, 0, "Mind", 1),
            unit("Field Anchor Two", 1, 0, "Mind", 1),
        ],
        ZONE_BASE,
    );
    let hand = deal(
        &mut host,
        &mut client,
        0,
        vec![
            spell("Here to Help", 2, 1, "Mind"),
            unit("Hidden Eligible One", 4, 0, "Mind", 1),
            unit("Hidden Eligible Two", 4, 0, "Mind", 1),
        ],
        ZONE_HAND,
    );
    let table = NativeHereToHelpTable {
        help: hand[0],
        hidden: [hand[1], hand[2]],
        anchors: [base[2], base[3]],
        jhin: base[0],
        portal: base[1],
    };
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let keep = cards_in(&host, 0, ZONE_HAND).len() as u16;
    pick(&mut host, &mut client, 0, keep);
    let keep = cards_in(&host, ada, ZONE_HAND).len() as u16;
    let _started = pick(&mut host, &mut client, ada, keep);

    moved(
        &mut host,
        &mut client,
        0,
        table.anchors[0],
        battlefields[0],
        0,
        TOP,
    );
    let held_first = drain_priority(&mut host, &mut client);
    if held_first.prompt.is_some() {
        let done = option_index(&host.plugin_view(0), "done");
        pick(&mut host, &mut client, 0, done);
    }
    let held_first = drain_priority(&mut host, &mut client);
    assert!(held_first.prompt.is_none());
    assert!(
        held_first.showdown.is_none(),
        "first showdown {:?}",
        held_first.showdown
    );
    moved(
        &mut host,
        &mut client,
        0,
        table.anchors[1],
        battlefields[1],
        0,
        TOP,
    );
    let held_second = drain_priority(&mut host, &mut client);
    if held_second.prompt.is_some() {
        let done = option_index(&host.plugin_view(0), "done");
        pick(&mut host, &mut client, 0, done);
    }
    let held_second = drain_priority(&mut host, &mut client);
    assert!(held_second.prompt.is_none());
    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let held = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!(held.holder(battlefields[0]), Some(0));
    assert_eq!(held.holder(battlefields[1]), Some(0));
    moved(
        &mut host,
        &mut client,
        0,
        table.jhin,
        battlefields[0],
        0,
        TOP,
    );
    let banked = drain_priority(&mut host, &mut client);
    if banked.prompt.is_some() {
        let done = option_index(&host.plugin_view(0), "done");
        pick(&mut host, &mut client, 0, done);
    }
    let banked = drain_priority(&mut host, &mut client);
    assert!(banked.seat(0).pool.energy > 0);

    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: table.portal,
            ability: 0,
        },
    );
    let activated = drain_priority(&mut host, &mut client);
    assert_eq!(activated.seat(0).promises.len(), 1);
    let asked = moved(&mut host, &mut client, 0, table.help, ZONE_CHAIN, 0, TOP);
    let yes = option_index(&host.plugin_view(0), "yes");
    assert!(matches!(
        asked.why,
        Some(PromptWhy::OptionalCost { cost, .. }) if cost == SLOT_PROMISED_REPEAT as u8
    ));
    let paid = pick(&mut host, &mut client, 0, yes);
    assert_eq!(paid.chain.len(), 1);
    assert_eq!(paid.chain[0].repeats(), 1);
    let pool_after_parent = paid.seat(0).pool.clone();
    let ready_after_parent = ready_runes(&host, 0);
    let first_prompt = drain_priority(&mut host, &mut client);
    assert!(matches!(first_prompt.why, Some(PromptWhy::Resume { .. })));
    let first = option_index(&host.plugin_view(0), &card_option(table.hidden[0]));
    let first_reveal = pick(&mut host, &mut client, 0, first);
    assert!(
        first_reveal
            .log
            .iter()
            .any(|line| line.contains(&format!("reveals {{card {}}}", table.hidden[0]))),
        "the first hidden pick is publicly logged as a reveal"
    );
    assert_eq!(first_reveal.chain.len(), 1);
    assert_eq!(first_reveal.queue.len(), 1);
    assert_eq!(first_reveal.seat(0).pool, pool_after_parent);
    assert!(first_reveal
        .chain
        .iter()
        .any(|item| { item.kind.source() == table.help && item.status == ItemStatus::Resolving }));
    assert!(first_reveal
        .queue
        .iter()
        .any(|pending| pending.item.kind.source() == table.hidden[0]));
    assert!(!cards_in(&host, 0, ZONE_HAND).contains(&table.hidden[0]));
    let _second_prompt = drain_priority(&mut host, &mut client);
    let second = option_index(&host.plugin_view(0), &card_option(table.hidden[1]));
    let second_reveal = pick(&mut host, &mut client, 0, second);
    assert!(!second_reveal
        .chain
        .iter()
        .any(|item| item.kind.source() == table.help));
    assert_eq!(second_reveal.queue.len(), 2);
    assert_eq!(second_reveal.seat(0).pool, pool_after_parent);
    assert_eq!(ready_runes(&host, 0), ready_after_parent);
    assert!(!cards_in(&host, 0, ZONE_HAND).contains(&table.hidden[1]));

    let parent_done = second_reveal;
    assert!(parent_done.chain.is_empty());
    assert_eq!(parent_done.queue.len(), 2);
    assert!(matches!(
        parent_done.why,
        Some(PromptWhy::PlayLocation { .. })
    ));
    let first_location = option_index(
        &host.plugin_view(0),
        &format!("{{zone {}}}", battlefields[0]),
    );
    let first_location_state = pick(&mut host, &mut client, 0, first_location);
    assert_eq!(ready_runes(&host, 0), ready_after_parent - 1);
    assert!(cards_in(&host, 0, battlefields[0]).contains(&table.hidden[0]));
    let second_location = option_index(
        &host.plugin_view(0),
        &format!("{{zone {}}}", battlefields[1]),
    );
    pick(&mut host, &mut client, 0, second_location);
    let finished = drain_priority(&mut host, &mut client);
    assert!(finished.queue.is_empty());
    assert_eq!(ready_runes(&host, 0), ready_after_parent - 2);
    assert!(cards_in(&host, 0, battlefields[0]).contains(&table.anchors[0]));
    assert!(cards_in(&host, 0, battlefields[0]).contains(&table.jhin));
    assert!(cards_in(&host, 0, battlefields[0]).contains(&table.hidden[0]));
    assert!(cards_in(&host, 0, battlefields[1]).contains(&table.anchors[1]));
    assert!(cards_in(&host, 0, battlefields[1]).contains(&table.hidden[1]));
    assert!(first_location_state
        .queue
        .iter()
        .any(|pending| pending.item.kind.source() == table.hidden[1]));
    same_bytes(&host, &client, "repeated Here to Help");
    (host.log().to_vec(), encode_state(host.state()))
}

#[test]
fn native_and_hardened_riftbound_replays_match_at_every_entry() {
    let (economy, economy_state) = temporal_portal_repeat_banked_pool_entries();
    assert_plugin_replay_parity(&economy, &economy_state, "Temporal Portal banked pool");

    let (target, target_state) = target_prompt_entries();
    assert_plugin_replay_parity(&target, &target_state, "target prompt");

    let (mulligan, mulligan_state) = mulligan_entries();
    assert_plugin_replay_parity(&mulligan, &mulligan_state, "mulligan");

    let (hidden, hidden_state) = hidden_reveal_entries();
    assert_plugin_replay_parity(&hidden, &hidden_state, "hidden reveal");

    let (here_to_help, here_to_help_state) = native_here_to_help_entries();
    assert_plugin_replay_parity(
        &here_to_help,
        &here_to_help_state,
        "repeated Here to Help child queue",
    );

    let (damage, damage_state) = native_lotus_alpha_strike_entries();
    assert_plugin_replay_parity(&damage, &damage_state, "Lotus Trap / Alpha Strike v13");
    assert_damage_cold_suffix(&damage, &damage_state);
}

#[test]
fn native_repeated_here_to_help_defers_both_children_until_the_parent_finishes() {
    native_here_to_help_entries();
}

#[test]
fn native_lotus_trap_and_alpha_strike_persist_damage_multiplier_marks_and_expiry() {
    native_lotus_alpha_strike_entries();
}

#[test]
fn native_lotus_alpha_strike_exposes_a_saved_damage_checkpoint() {
    let (entries, _) = native_lotus_alpha_strike_entries();
    let (checkpoint, suffix_at) = damage_checkpoint(&entries);
    assert!(suffix_at > 0 && suffix_at < entries.len());
    let state = decode_state(&checkpoint).expect("damage checkpoint decodes");
    let blob = GameBlob::decode(&state.plugin_state).expect("damage checkpoint blob decodes");
    assert!(matches!(blob.why, Some(PromptWhy::Resume { .. })));
    assert!(state.table.cards().iter().any(|card| {
        state.counter(
            CounterTarget::Card(card.id.0),
            agni_riftbound::COUNTER_DAMAGE,
        ) == Some(2)
    }));
}

#[test]
fn hardened_plugin_costed_grant_results_do_not_depend_on_instance_history() {
    let (_entries, bytes) = target_prompt_entries();
    let mut state = decode_state(&bytes).expect("target state decodes");
    let spell = state
        .table
        .cards()
        .iter()
        .find(|card| card.face.kind.as_deref() == Some(agni_riftbound_turns::cards::KIND_SPELL))
        .map(|card| card.id.0)
        .expect("target state has a spell");
    let mut blob = GameBlob::decode(&state.plugin_state).expect("plugin state decodes");
    let turn = blob.turn();
    blob.card_state_mut(spell).granted_costed.push(CostedGrant {
        kind: CostedKind::Flow,
        energy: 2,
        power: vec![agni_riftbound_turns::cards::Power::Own],
        until: Expiry::EndOfTurn(turn),
    });
    state.plugin_state = ByteBuf::from(blob.encode());
    let view_request = encode(&PluginViewRequest::of(&state, 0));
    assert_eq!(blob.phase(), Some(Phase::Action));
    let continuation = LogEntry::new(
        state.next_seq,
        blob.turn_player(),
        LogAction::Game {
            data: ByteBuf::from(TurnEvent::EndTurn.encode()),
        },
    );
    let decide_request =
        encode(&native_decide_request(&state, &continuation).expect("valid continuation request"));

    let mut fresh_view = wasm_plugin();
    let expected_view = fresh_view.view(&view_request).expect("fresh view succeeds");
    let mut fresh_decide = wasm_plugin();
    let expected_verdict = fresh_decide
        .decide(&decide_request)
        .expect("cold decide succeeds");
    assert!(
        expected_verdict.accept,
        "a valid EndTurn continuation is accepted"
    );
    assert!(expected_verdict.reason.is_none());

    let mut native = NativeRiftbound;
    assert_eq!(native.view(&view_request).unwrap(), expected_view);
    assert_eq!(native.decide(&decide_request).unwrap(), expected_verdict);

    let mut warm = wasm_plugin();
    warm.view(&view_request)
        .expect("initial warm view succeeds");
    let mut unrelated = state.clone();
    let mut unrelated_blob = GameBlob::decode(&unrelated.plugin_state).unwrap();
    unrelated_blob
        .card_state_mut(spell)
        .granted_costed
        .push(CostedGrant {
            kind: CostedKind::Repeat,
            energy: 3,
            power: vec![agni_riftbound_turns::cards::Power::Rainbow],
            until: Expiry::Permanent,
        });
    unrelated.plugin_state = ByteBuf::from(unrelated_blob.encode());
    warm.view(&encode(&PluginViewRequest::of(&unrelated, 0)))
        .expect("unrelated costed view succeeds");
    let reset = GameBlob::start(2, 0, Mode::Free);
    let mut reset_state = state.clone();
    reset_state.plugin_state = ByteBuf::from(reset.encode());
    warm.view(&encode(&PluginViewRequest::of(&reset_state, 0)))
        .expect("new-game view succeeds");

    assert_eq!(
        warm.view(&view_request).expect("warm view succeeds"),
        expected_view
    );
    assert_eq!(
        warm.decide(&decide_request).expect("warm decide succeeds"),
        expected_verdict
    );
}

fn fetch_from_host(host: &HostSession, inbox: &mut ModuleInbox, hash: [u8; 32]) -> Vec<u8> {
    let framed = encode_client(&ClientMsg::NeedModule { hash });
    let ClientMsg::NeedModule { hash: asked } = decode_client(&framed).unwrap() else {
        panic!("a need-module frame");
    };
    let frames = host.module_frames(asked);
    let mut landed = None;
    for frame in &frames {
        let framed = encode_host(frame);
        assert!(framed.len() < agni_net::table::MAX_FRAME_BYTES);
        let msg = decode_host(&framed).unwrap();
        assert!(matches!(msg, HostMsg::Module { .. }), "{msg:?}");
        if let Some(done) = inbox.receive(&msg).unwrap() {
            landed = Some(done);
        }
    }
    assert_eq!(landed, Some(hash));
    inbox.take(hash).expect("the module assembled")
}

#[test]
fn a_joiner_without_the_pinned_modules_fetches_both_from_the_host_and_plays_a_turn() {
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig {
            engine: None,
            plugin: None,
            zones: zone_table(),
            options: None,
            counters: counter_table(),
            despawn_any: false,
        },
        wasm_engine(),
        Some(wasm_plugin()),
    )
    .unwrap();
    let engine_hash = module_hash(&engine_module().bytes);
    let plugin_hash = module_hash(&plugin_module().bytes);
    assert_eq!(
        host.serve_module(engine_module().bytes.clone()),
        Some(engine_hash)
    );
    assert_eq!(
        host.serve_module(plugin_module().bytes.clone()),
        Some(plugin_hash)
    );

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

    let store: std::collections::BTreeMap<[u8; 32], Vec<u8>> = std::collections::BTreeMap::new();
    let engine_pin = pin_hash(&genesis_engine_pin(&log).expect("genesis pins the engine")).unwrap();
    let plugin_pin = pin_hash(&genesis_plugin_pin(&log).expect("genesis pins the plugin")).unwrap();
    assert_eq!((engine_pin, plugin_pin), (engine_hash, plugin_hash));
    assert!(!store.contains_key(&engine_pin) && !store.contains_key(&plugin_pin));

    let mut inbox = ModuleInbox::new();
    let engine_bytes = fetch_from_host(&host, &mut inbox, engine_pin);
    assert!(
        engine_bytes.len() > MODULE_CHUNK_BYTES,
        "the engine crosses in more than one chunk"
    );
    let plugin_bytes = fetch_from_host(&host, &mut inbox, plugin_pin);
    assert_eq!(engine_bytes, engine_module().bytes);
    assert_eq!(plugin_bytes, plugin_module().bytes);

    let engine = Box::new(load_engine(&engine_bytes, ENGINE_BUDGET).unwrap());
    let plugin = Box::new(load_plugin(&plugin_bytes, PLUGIN_BUDGET).unwrap());
    assert_eq!(engine.engine_hash(), Some(engine_pin));
    assert_eq!(plugin.module_hash(), Some(plugin_pin));
    let mut client =
        ClientSession::from_welcome_with(seat, roster, log, engine, Some(plugin)).unwrap();

    let winner = roll_for_first(&mut host, &mut client, ada);
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let opened = blob_of(&host, &client);
    assert_eq!((opened.turn(), opened.turn_player()), (1, 0));
    from_host(&mut host, &mut client, TurnEvent::EndTurn);
    let passed = blob_of(&host, &client);
    assert_eq!((passed.turn(), passed.turn_player()), (2, ada));
    from_client(&mut host, &mut client, ada, TurnEvent::EndTurn);
    let back = blob_of(&host, &client);
    assert_eq!((back.turn(), back.turn_player()), (3, 0));
}

#[test]
fn two_seats_pick_a_mode_pass_turns_and_fight_a_showdown_through_the_hardened_plugin() {
    let (mut host, mut client, ada) = open_table();

    assert!(
        host.view().plugin_state.is_empty(),
        "an implicit lobby has no blob until someone rolls"
    );
    let lobby = host.plugin_view(0);
    assert_eq!(lobby.status[0], "roll for first player");
    assert_eq!(labels(&lobby), ["roll"]);
    assert_eq!(
        lobby.affordances[0].kind,
        AffordanceKind::Commit { roll: 1 }
    );
    refused(&mut host, 0, TurnEvent::EndTurn, Refusal::NotStarted);
    refused(
        &mut host,
        0,
        TurnEvent::StartGame { first_player: 0 },
        Refusal::RollFirst,
    );

    let winner = roll_for_first(&mut host, &mut client, ada);
    let loser = 1 - winner;
    refused(
        &mut host,
        loser,
        TurnEvent::StartGame {
            first_player: loser,
        },
        Refusal::NotTheWinner,
    );
    refused(
        &mut host,
        loser,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
        Refusal::NotTheWinner,
    );
    let choices = host.plugin_view(winner);
    assert!(labels(&choices).contains(&"go first".to_string()));
    assert!(labels(&choices).contains(&"switch to rules enforced".to_string()));
    assert!(choices.status.contains(&"mode: free table".to_string()));
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    let chosen = blob_of(&host, &client);
    assert_eq!(chosen.mode, Mode::Enforced);
    assert!(!chosen.is_playing());
    assert!(host
        .plugin_view(loser)
        .status
        .contains(&"mode: rules enforced".to_string()));
    let (dealt, _) = host
        .deal_to(0, vec![CardFace::named("Vi")], WireZone::Plugin(ZONE_BASE))
        .unwrap();
    relay(&mut client, &[dealt]);
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let opened = blob_of(&host, &client);
    assert_eq!(opened.mode, Mode::Enforced);
    assert_eq!((opened.turn(), opened.turn_player()), (1, 0));
    assert_eq!(opened.phase(), Some(agni_riftbound_turns::Phase::Action));
    assert!(opened.prompt.is_none(), "empty hands keep themselves");
    assert_eq!(opened.log.last().unwrap(), "turn 1 · {seat 0}");
    let strip = host.plugin_view(0);
    assert_eq!(labels(&strip), ["end turn", "free table"]);
    assert_eq!(hidden_labels(&strip), ["concede"]);
    assert_eq!(
        strip.status[0],
        "turn 1 · {seat 0} · action phase · rules enforced"
    );
    let turn = strip
        .turn
        .clone()
        .expect("the structured turn crosses the wasm boundary");
    assert_eq!((turn.number, turn.seat), (1, 0));
    assert_eq!(turn.phase, "action phase");
    assert_eq!(turn.mode, "rules enforced");
    assert_eq!(turn.phases.len(), 9);
    assert_eq!(turn.phases[5], "action phase");
    assert_eq!(strip.primary, Some(0), "end turn is the primary");
    assert_eq!(strip.waiting, None);
    assert_eq!(strip.seats.len(), 2);
    assert_eq!(strip.seats[0].seat, 0);
    assert_eq!(strip.seats[0].victory, 8);
    assert_eq!(strip.seats[1].seat, 1);
    let theirs = client.plugin_view(ada);
    assert!(labels(&theirs).is_empty());
    assert_eq!(hidden_labels(&theirs), ["concede"]);
    assert_eq!(theirs.primary, None);
    let waiting = theirs.waiting.clone().expect("the other seat waits");
    assert_eq!(waiting.seat, Some(0));
    assert_eq!(waiting.what, "their action phase");
    assert_eq!(theirs.seats, strip.seats, "both seats see the same counts");
    assert!(client
        .plugin_view(ada)
        .status
        .iter()
        .any(|line| line.starts_with("waiting for {seat 0}")));

    refused(&mut host, ada, TurnEvent::EndTurn, Refusal::NotYourTurn);
    refused(&mut host, 0, TurnEvent::Pass, Refusal::NoShowdown);
    refused(&mut host, ada, TurnEvent::FreeTable, Refusal::NotYourTurn);
    refused(
        &mut host,
        0,
        TurnEvent::Pick(agni_plugin_sdk::prompt::Pick {
            prompt: 1,
            option: 0,
        }),
        Refusal::NoPrompt,
    );
    assert_eq!(blob_of(&host, &client), opened);
    assert!(
        host.deal_to(
            0,
            vec![CardFace::named("Jinx")],
            WireZone::Plugin(ZONE_BASE)
        )
        .is_err(),
        "the deal is over once an enforced game starts"
    );

    let unit = host
        .state()
        .table
        .in_area(agni_core::PlayerId(0), agni_core::Zone::Plugin(ZONE_BASE))
        .map(|card| card.id.0)
        .next()
        .expect("the unit sits in the base");
    let march = host
        .intent(
            0,
            WireIntent::Move {
                card: unit,
                to: WireZone::Plugin(ZONE_BATTLEFIELD_FIRST + 1),
                seat: 0,
                index: 0,
            },
        )
        .expect("a ready unit marches");
    relay(&mut client, &march);
    let closed = blob_of(&host, &client);
    assert_eq!(
        closed.showdown, None,
        "nobody holds a card, so every focus is passed for and the showdown closes as it opens"
    );
    assert_eq!(
        host.state().annotation(unit, "exhausted"),
        Some(&[1u8][..]),
        "marching exhausts the unit"
    );
    assert_eq!(closed.holder(ZONE_BATTLEFIELD_FIRST + 1), Some(0));
    assert!(closed.scored(ZONE_BATTLEFIELD_FIRST + 1, 0));
    assert!(closed.log.contains(&format!(
        "{{seat 0}} conquers {{zone {}}}",
        ZONE_BATTLEFIELD_FIRST + 1
    )));
    refused(&mut host, 0, TurnEvent::Pass, Refusal::NoShowdown);
    refused(&mut host, ada, TurnEvent::Pass, Refusal::NoShowdown);
    let scored = host.plugin_view(ada);
    assert_eq!(scored.status[1], "points · {seat 0} 1 · {seat 1} 0");
    assert_eq!(
        scored.status[2],
        format!("{{zone {}}} held by {{seat 0}}", ZONE_BATTLEFIELD_FIRST + 1)
    );
    assert_eq!(labels(&host.plugin_view(0)), ["end turn", "free table"]);

    from_host(&mut host, &mut client, TurnEvent::FreeTable);
    assert_eq!(blob_of(&host, &client).free_table, Some(0));
    assert_eq!(
        hidden_labels(&host.plugin_view(0)),
        ["withdraw free table", "concede"]
    );
    from_host(&mut host, &mut client, TurnEvent::FreeTable);
    assert_eq!(
        blob_of(&host, &client).free_table,
        None,
        "the proposer withdraws"
    );
    assert!(host
        .plugin_view(0)
        .status
        .contains(&"{seat 0} withdraws the free table proposal".to_string()));
    from_host(&mut host, &mut client, TurnEvent::FreeTable);
    assert_eq!(blob_of(&host, &client).free_table, Some(0));
    assert!(labels(&client.plugin_view(ada)).is_empty());
    assert_eq!(
        hidden_labels(&client.plugin_view(ada)),
        ["confirm free table", "concede"],
        "the confirmation never sits on the strip or in the AI's list"
    );
    from_client(&mut host, &mut client, ada, TurnEvent::FreeTable);
    let freed = blob_of(&host, &client);
    assert_eq!(freed.mode, Mode::Free);
    assert_eq!(freed.free_table, None);
    assert!(host
        .plugin_view(0)
        .status
        .contains(&"{seat 0} and {seat 1} freed the table".to_string()));
    refused(&mut host, 0, TurnEvent::FreeTable, Refusal::AlreadyFree);

    from_host(&mut host, &mut client, TurnEvent::EndTurn);
    let handed = blob_of(&host, &client);
    assert_eq!((handed.turn(), handed.turn_player()), (2, ada));
    assert!(!handed.scored(ZONE_BATTLEFIELD_FIRST + 1, 0));
    assert_eq!(handed.holder(ZONE_BATTLEFIELD_FIRST + 1), Some(0));
    assert_eq!(labels(&client.plugin_view(ada)), ["end turn"]);
    assert_eq!(
        client.plugin_view(ada).status[0],
        "turn 2 · {seat 1} · action phase · free table"
    );

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn unit(name: &str, energy: u8, power: u8, domain: &str, might: u8) -> CardFace {
    CardFace::named(name)
        .with_kind("Unit")
        .with_cost(Some(energy), Some(power))
        .with_domain(vec![domain.to_string()])
        .with_might(Some(might))
}

fn spell(name: &str, energy: u8, power: u8, domain: &str) -> CardFace {
    CardFace::named(name)
        .with_kind("Spell")
        .with_cost(Some(energy), Some(power))
        .with_domain(vec![domain.to_string()])
}

fn rune(domain: &str) -> CardFace {
    CardFace::named(format!("{domain} Rune"))
        .with_kind("Rune")
        .with_domain(vec![domain.to_string()])
}

fn battlefield(name: &str) -> CardFace {
    CardFace::named(name).with_kind("Battlefield")
}

fn deal(
    host: &mut HostSession,
    client: &mut ClientSession,
    seat: u8,
    faces: Vec<CardFace>,
    zone: u16,
) -> Vec<u32> {
    let (entry, _) = host
        .deal_to(seat, faces, WireZone::Plugin(zone))
        .expect("the deal is accepted before the start");
    let LogAction::Deal { cards, .. } = &entry.action else {
        panic!("a deal entry");
    };
    let ids = cards.clone();
    relay(client, &[entry]);
    ids
}

fn cards_in(host: &HostSession, seat: u8, zone: u16) -> Vec<u32> {
    host.state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(zone))
        .map(|card| card.id.0)
        .collect()
}

fn units_in(host: &HostSession, seat: u8, zone: u16) -> Vec<u32> {
    cards_in(host, seat, zone)
        .into_iter()
        .filter(|card| {
            host.face_of(*card)
                .is_some_and(|(_, face)| face.kind.as_deref() == Some("Unit"))
        })
        .collect()
}

fn face_name(host: &HostSession, card: u32) -> String {
    host.face_of(card)
        .map(|(_, face)| face.name)
        .unwrap_or_default()
}

fn named(host: &HostSession, seat: u8, zone: u16, name: &str) -> u32 {
    cards_in(host, seat, zone)
        .into_iter()
        .find(|id| face_name(host, *id) == name)
        .unwrap_or_else(|| panic!("{name} sits in zone {zone} of seat {seat}"))
}

fn exhausted(host: &HostSession, card: u32) -> bool {
    host.state().annotation(card, "exhausted") == Some(&[1u8][..])
}

fn points(host: &HostSession, seat: u8) -> i32 {
    host.state()
        .counter(CounterTarget::Seat(seat), agni_riftbound::COUNTER_POINTS)
        .unwrap_or(0)
}

fn moved(
    host: &mut HostSession,
    client: &mut ClientSession,
    seat: u8,
    card: u32,
    to: u16,
    to_seat: u8,
    index: u32,
) -> GameBlob {
    let entries = host
        .intent(
            seat,
            WireIntent::Move {
                card,
                to: WireZone::Plugin(to),
                seat: to_seat,
                index,
            },
        )
        .unwrap_or_else(|error| panic!("{card} -> zone {to} is legal: {error}"));
    relay(client, &entries);
    blob_of(host, client)
}

fn move_refused(host: &mut HostSession, seat: u8, card: u32, to: u16, to_seat: u8, why: Refusal) {
    let error = host
        .intent(
            seat,
            WireIntent::Move {
                card,
                to: WireZone::Plugin(to),
                seat: to_seat,
                index: TOP,
            },
        )
        .expect_err("the plugin refuses it");
    assert!(error.is_refusal(), "{card} -> {to}: {error}");
    assert_eq!(error.reason(), Some(why.label().as_str()), "{card} -> {to}");
}

fn pick(host: &mut HostSession, client: &mut ClientSession, seat: u8, option: u16) -> GameBlob {
    let prompt = blob_of(host, client).prompt.expect("a prompt is open").id;
    let event = TurnEvent::Pick(Pick { prompt, option });
    if seat == 0 {
        from_host(host, client, event);
    } else {
        from_client(host, client, seat, event);
    }
    blob_of(host, client)
}

fn turn_event(
    host: &mut HostSession,
    client: &mut ClientSession,
    seat: u8,
    event: TurnEvent,
) -> GameBlob {
    if seat == 0 {
        from_host(host, client, event);
    } else {
        from_client(host, client, seat, event);
    }
    blob_of(host, client)
}

#[test]
fn an_enforced_game_runs_three_turns_with_charged_plays_marches_showdowns_and_scoring() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let mine: Vec<CardFace> = [
        unit("Rift Herald", 2, 0, "Fury", 3),
        unit("Vanguard", 1, 1, "Fury", 2),
        unit("Scout", 1, 0, "Fury", 1),
        unit("Sentinel", 2, 1, "Fury", 3),
        spell("Spark", 1, 0, "Fury"),
        unit("Warden", 1, 0, "Fury", 2),
        unit("Brawler", 2, 0, "Fury", 2),
        unit("Lookout", 1, 0, "Fury", 1),
        unit("Reserve", 3, 1, "Fury", 4),
    ]
    .into_iter()
    .rev()
    .collect();
    let theirs: Vec<CardFace> = [
        ("Tidecaller", 2, 1, "Calm", 3),
        ("Dreamer", 1, 0, "Calm", 1),
        ("Wisp", 1, 0, "Calm", 1),
        ("Seer", 2, 0, "Calm", 2),
        ("Gardener", 1, 1, "Calm", 2),
        ("Keeper", 2, 0, "Calm", 2),
        ("Wanderer", 1, 0, "Calm", 1),
        ("Sleeper", 3, 0, "Calm", 3),
    ]
    .iter()
    .rev()
    .map(|(name, energy, power, domain, might)| unit(name, *energy, *power, domain, *might))
    .collect();
    let my_deck = deal(&mut host, &mut client, 0, mine, ZONE_MAIN_DECK);
    let their_deck = deal(&mut host, &mut client, ada, theirs, ZONE_MAIN_DECK);
    assert_eq!((my_deck.len(), their_deck.len()), (9, 8));
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Fury"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let vi = deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Vi", 3, 1, "Fury", 6)],
        ZONE_BASE,
    )[0];
    let jinx_and_ekko = deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Jinx", 3, 1, "Calm", 3), unit("Ekko", 2, 1, "Calm", 2)],
        ZONE_BASE,
    );
    let (jinx, ekko) = (jinx_and_ekko[0], jinx_and_ekko[1]);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    assert!(
        [vi, jinx, ekko]
            .iter()
            .all(|card| host.state().revealed.contains(card)),
        "cards dealt to public zones surface before the start"
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let setup = blob_of(&host, &client);
    assert_eq!(setup.mode, Mode::Enforced);
    assert_eq!(setup.phase(), Some(Phase::Setup));
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), 4);
    assert_eq!(cards_in(&host, ada, ZONE_HAND).len(), 4);
    assert!(setup
        .log
        .contains(&"setup · mulligans in turn order".to_string()));
    let prompt = setup
        .prompt
        .clone()
        .expect("the first player mulligans first");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
    let strip = host.plugin_view(0);
    let summary = strip.prompt.clone().expect("a prompt summary");
    assert_eq!(summary.why, "set aside up to 2 cards to redraw");
    assert_eq!((summary.seat, summary.min, summary.max), (0, 0, 2));
    let offered = labels(&strip);
    assert_eq!(offered.len(), 6, "four cards, keep and the panic button");
    for (index, card) in cards_in(&host, 0, ZONE_HAND).iter().enumerate() {
        assert_eq!(offered[index], format!("set aside {{card {card}}}"));
        assert_eq!(strip.affordances[index].card, Some(*card));
    }
    assert_eq!(offered[4], "keep");
    assert_eq!(offered[5], "free table");
    let waiting = client.plugin_view(ada);
    assert!(labels(&waiting).is_empty());
    assert!(waiting
        .status
        .contains(&"waiting for {seat 0}: set aside up to 2 cards to redraw".to_string()));
    assert_eq!(
        waiting.waiting.as_ref().map(|held| held.what.as_str()),
        Some("set aside up to 2 cards to redraw")
    );
    assert_eq!(strip.primary, Some(4), "keep is the prompt's primary");

    let my_hand = cards_in(&host, 0, ZONE_HAND);
    move_refused(&mut host, 0, my_hand[0], ZONE_BASE, 0, Refusal::PromptOpen);
    refused(&mut host, 0, TurnEvent::EndTurn, Refusal::PromptOpen);
    let their_hand = cards_in(&host, ada, ZONE_HAND);
    let error = host
        .intent(
            ada,
            WireIntent::Move {
                card: their_hand[3],
                to: WireZone::Plugin(ZONE_MAIN_DECK),
                seat: ada,
                index: BOTTOM,
            },
        )
        .expect_err("only the prompted seat mulligans");
    assert_eq!(
        error.reason(),
        Some(
            Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NotYourPrompt { seat: 0 })
                .label()
                .as_str()
        )
    );
    assert_eq!(blob_of(&host, &client), setup);

    let kept = pick(&mut host, &mut client, 0, 4);
    assert_eq!(kept.prompt.as_ref().map(|prompt| prompt.seat), Some(ada));
    assert!(kept.log.contains(&"{seat 0} keeps their hand".to_string()));
    assert_eq!(cards_in(&host, 0, ZONE_HAND), my_hand);

    let set_aside = their_hand[3];
    assert_eq!(face_name(&host, set_aside), "Seer");
    let gestured = moved(
        &mut host,
        &mut client,
        ada,
        set_aside,
        ZONE_MAIN_DECK,
        ada,
        BOTTOM,
    );
    assert_eq!(
        gestured.prompt.as_ref().map(|prompt| prompt.picked.clone()),
        Some(vec![set_aside])
    );
    assert_eq!(cards_in(&host, ada, ZONE_MAIN_DECK)[0], set_aside);
    assert_eq!(
        labels(&client.plugin_view(ada)).len(),
        4,
        "three cards and keep"
    );
    let started = pick(&mut host, &mut client, ada, 3);
    assert_eq!(started.phase(), Some(Phase::Action));
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert!(started.prompt.is_none());
    assert!(started
        .log
        .contains(&"{seat 1} sets aside 1 and redraws".to_string()));
    assert_eq!(started.log.last().unwrap(), "turn 1 · {seat 0}");
    assert_eq!(cards_in(&host, ada, ZONE_HAND).len(), 4);
    assert!(!cards_in(&host, ada, ZONE_HAND).contains(&set_aside));
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), 5, "the first draw");
    assert_eq!(
        cards_in(&host, 0, ZONE_RUNE_POOL).len(),
        2,
        "two runes channeled"
    );
    assert_eq!(cards_in(&host, ada, ZONE_RUNE_POOL).len(), 0);
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.status[0],
        "turn 1 · {seat 0} · action phase · rules enforced"
    );
    assert_eq!(strip.status[1], "points · {seat 0} 0 · {seat 1} 0");
    assert_eq!(labels(&strip), ["end turn", "free table"]);
    assert!(client
        .plugin_view(ada)
        .status
        .contains(&"waiting for {seat 0}: their action phase".to_string()));

    let marched = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    let showdown = marched.showdown.clone().expect("a showdown opens at once");
    assert_eq!(
        (
            showdown.zone,
            showdown.attacker,
            showdown.defender,
            showdown.combat
        ),
        (bf1, 0, ada, false)
    );
    assert_eq!(showdown.focus(), 0);
    assert!(exhausted(&host, vi));
    assert_eq!(marched.contester(bf1), Some(0));
    let strip = host.plugin_view(0);
    assert_eq!(labels(&strip), ["pass", "free table"]);
    assert_eq!(strip.affordances[0].hotkey.as_deref(), Some("w"));
    assert!(strip.status.contains(&format!(
        "showdown at {{zone {bf1}}} · {{seat 0}} against {{seat 1}} · focus {{seat 0}}"
    )));
    assert!(client.plugin_view(ada).status.contains(&format!(
        "waiting for {{seat 0}}: pass or respond at {{zone {bf1}}}"
    )));
    refused(&mut host, ada, TurnEvent::Pass, Refusal::NotYourFocus);
    refused(&mut host, 0, TurnEvent::EndTurn, Refusal::ShowdownOpen);
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let window = passed
        .showdown
        .clone()
        .expect("the joiner still holds focus");
    assert_eq!((window.focus(), window.passes()), (ada, 1));
    assert_eq!(labels(&client.plugin_view(ada)), ["pass"]);
    let conquered = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(conquered.showdown.is_none());
    assert_eq!(conquered.holder(bf1), Some(0));
    assert!(conquered.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(
        &conquered.log[conquered.log.len() - 3..],
        [
            "{seat 0} passes",
            "{seat 1} passes",
            &format!("{{seat 0}} conquers {{zone {bf1}}}")
        ]
    );

    let vanguard = named(&host, 0, ZONE_HAND, "Vanguard");
    let runes_before = cards_in(&host, 0, ZONE_RUNE_POOL);
    let played = moved(&mut host, &mut client, 0, vanguard, ZONE_BASE, 0, TOP);
    assert_eq!(units_in(&host, 0, ZONE_BASE), [vanguard]);
    assert!(exhausted(&host, vanguard), "units enter exhausted");
    let runes_after = cards_in(&host, 0, ZONE_RUNE_POOL);
    assert_eq!(
        runes_after.len(),
        1,
        "one rune is exhausted for the energy and recycled for the power"
    );
    assert!(
        !exhausted(&host, runes_after[0]),
        "the other rune stays ready"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_RUNE_DECK)[0],
        *runes_before
            .iter()
            .find(|rune| !runes_after.contains(rune))
            .unwrap(),
        "the recycled rune sits at the bottom of the rune deck"
    );
    assert_eq!(
        played.log.last().unwrap(),
        &format!("{{seat 0}} plays {{card {vanguard}}} to their base")
    );
    let spark = named(&host, 0, ZONE_HAND, "Spark");
    assert!(!host.state().revealed.contains(&spark));
    let played = moved(&mut host, &mut client, 0, spark, ZONE_CHAIN, 0, TOP);
    assert!(
        host.state().revealed.contains(&spark),
        "the host reveals the spell before it moves"
    );
    assert_eq!(cards_in(&host, 0, ZONE_CHAIN), [spark]);
    assert!(
        exhausted(&host, runes_after[0]),
        "the spell takes the last ready rune"
    );
    assert_eq!(played.chain.len(), 1);
    assert_eq!(
        played
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((0, 0)),
        "the controller may respond to their own spell first"
    );
    assert_eq!(
        played.log.last().unwrap(),
        &format!("{{seat 0}} plays {{card {spark}}}")
    );
    let strip = host.plugin_view(0);
    assert_eq!(labels(&strip), ["pass", "free table"]);
    assert!(strip
        .status
        .contains(&format!("chain: {{card {spark}}} (top)")));
    assert!(client
        .plugin_view(ada)
        .status
        .contains(&"waiting for {seat 0}: respond or pass".to_string()));
    refused(
        &mut host,
        ada,
        TurnEvent::Pass,
        Refusal::Illegal(Reason::NotYourPriority),
    );
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(
        passed
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((ada, 1))
    );
    assert_eq!(labels(&client.plugin_view(ada)), ["pass"]);
    let resolved = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(cards_in(&host, 0, ZONE_TRASH), [spark]);
    assert!(cards_in(&host, 0, ZONE_CHAIN).is_empty());
    assert!(resolved.chain.is_empty() && resolved.priority.is_none());
    assert_eq!(
        &resolved.log[resolved.log.len() - 3..],
        [
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            format!("{{card {spark}}} resolves")
        ]
    );
    let herald = named(&host, 0, ZONE_HAND, "Rift Herald");
    move_refused(
        &mut host,
        0,
        herald,
        ZONE_BASE,
        0,
        Refusal::NotEnoughRunes {
            needed: 2,
            ready: 0,
        },
    );
    move_refused(
        &mut host,
        0,
        herald,
        ZONE_TRASH,
        0,
        Refusal::Illegal(Reason::KillsAreAutomatic),
    );
    move_refused(
        &mut host,
        0,
        vi,
        ZONE_TRASH,
        0,
        Refusal::Illegal(Reason::KillsAreAutomatic),
    );
    let their_card = cards_in(&host, ada, ZONE_HAND)[0];
    move_refused(
        &mut host,
        ada,
        their_card,
        ZONE_TRASH,
        ada,
        Refusal::Illegal(Reason::KillsAreAutomatic),
    );
    move_refused(&mut host, ada, jinx, bf1, 0, Refusal::NotYourTurn);
    let error = host
        .intent(
            0,
            WireIntent::Annotate {
                card: vi,
                key: "exhausted".into(),
                value: None,
            },
        )
        .expect_err("marks are automatic");
    assert_eq!(
        error.reason(),
        Some(
            Refusal::Illegal(Reason::AnnotationsAutomatic)
                .label()
                .as_str()
        )
    );
    assert_eq!(
        blob_of(&host, &client),
        resolved,
        "refusals leave the blob alone"
    );

    let second = turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    assert_eq!((second.turn(), second.turn_player()), (2, ada));
    assert_eq!(second.log.last().unwrap(), "turn 2 · {seat 1}");
    assert!(!second.scored(bf1, 0));
    assert_eq!(second.holder(bf1), Some(0));
    assert_eq!(
        cards_in(&host, ada, ZONE_RUNE_POOL).len(),
        3,
        "the seat going second channels three"
    );
    assert_eq!(cards_in(&host, ada, ZONE_HAND).len(), 5);
    assert_eq!(labels(&client.plugin_view(ada)), ["end turn", "free table"]);
    assert!(host
        .plugin_view(0)
        .status
        .contains(&"waiting for {seat 1}: their action phase".to_string()));

    let tidecaller = named(&host, ada, ZONE_HAND, "Tidecaller");
    moved(&mut host, &mut client, ada, tidecaller, ZONE_BASE, ada, TOP);
    assert!(exhausted(&host, tidecaller));
    let their_pool = cards_in(&host, ada, ZONE_RUNE_POOL);
    assert_eq!(their_pool.len(), 2);
    assert_eq!(
        their_pool
            .iter()
            .filter(|rune| exhausted(&host, **rune))
            .count(),
        1,
        "two runes are exhausted for the energy and one of those is recycled for the power"
    );

    let asked = moved(&mut host, &mut client, ada, jinx, bf1, 0, TOP);
    assert!(asked.showdown.is_none(), "the group move is offered first");
    let prompt = asked.prompt.clone().expect("a group-move prompt");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (ada, 0, 1));
    let strip = client.plugin_view(ada);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(&*format!("move others to {{zone {bf1}}} too?"))
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {ekko}}}"),
            "done".into(),
            "free table".into()
        ]
    );
    assert_eq!(strip.affordances[0].card, Some(ekko));
    assert!(host.plugin_view(0).status.contains(&format!(
        "waiting for {{seat 1}}: move others to {{zone {bf1}}} too?"
    )));
    refused(&mut host, ada, TurnEvent::Pass, Refusal::PromptOpen);
    let combat = pick(&mut host, &mut client, ada, 0);
    assert!(combat.prompt.is_none());
    assert_eq!(
        units_in(&host, 0, bf1),
        [vi, jinx, ekko],
        "Vi, Jinx and Ekko share the battlefield"
    );
    assert!(exhausted(&host, jinx) && exhausted(&host, ekko));
    let showdown = combat
        .showdown
        .clone()
        .expect("one combat is staged and opens");
    assert_eq!(
        (
            showdown.zone,
            showdown.attacker,
            showdown.defender,
            showdown.combat
        ),
        (bf1, ada, 0, true)
    );
    assert!(combat.staged.is_empty());
    assert_eq!(showdown.focus(), ada);
    assert!(combat.log.contains(&format!(
        "combat at {{zone {bf1}}} · {{seat 1}} against {{seat 0}}"
    )));
    assert!(host.plugin_view(0).status.contains(&format!(
        "{{zone {bf1}}} held by {{seat 0}}, contested by {{seat 1}}"
    )));
    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let assigning = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert!(
        assigning.showdown.is_some(),
        "the showdown stays open through the damage step"
    );
    let prompt = assigning
        .prompt
        .clone()
        .expect("five attacking might went to Vi on its own · six defending might is a choice");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("assign 6 damage: who takes lethal next?")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {jinx}}} (lethal 3)"),
            format!("{{card {ekko}}} (lethal 2)")
        ]
    );
    assert_eq!(strip.affordances[0].card, Some(jinx));
    assert!(strip.status.iter().any(|line| line
        == &format!(
        "attackers {{card {jinx}}}, {{card {ekko}}} (5 might) · defenders {{card {vi}}} (6 might)"
    )));
    assert!(strip
        .status
        .iter()
        .any(|line| line == "{seat 0} assigns 6 damage"));
    assert!(client
        .plugin_view(ada)
        .status
        .iter()
        .any(|line| line == "waiting for {seat 0}: assign 6 damage: who takes lethal next?"));
    refused(&mut host, 0, TurnEvent::Pass, Refusal::PromptOpen);
    let closed = pick(&mut host, &mut client, 0, 0);
    assert!(closed.showdown.is_none());
    assert_eq!(closed.holder(bf1), Some(0));
    assert_eq!(closed.contester(bf1), None);
    assert_eq!(units_in(&host, 0, bf1), [vi]);
    assert_eq!(
        cards_in(&host, ada, ZONE_BASE),
        [tidecaller],
        "lethal to Jinx, then the excess to the last unit left · both are trashed"
    );
    assert_eq!(cards_in(&host, ada, ZONE_TRASH), [jinx, ekko]);
    assert_eq!(points(&host, ada), 0);
    assert!(
        !closed.scored(bf1, 0),
        "the defender re-establishes without scoring"
    );
    assert!(closed
        .log
        .iter()
        .any(|line| line == &format!("{{seat 0}} keeps {{zone {bf1}}}")));
    assert!(closed.log.iter().any(|line| line
        == &format!("{{card {vi}}} takes 5 · {{card {jinx}}} takes 3 · {{card {ekko}}} takes 3")));

    let third = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((third.turn(), third.turn_player()), (3, 0));
    assert_eq!(
        points(&host, 0),
        2,
        "holding the battlefield at the beginning phase scores"
    );
    assert!(third.scored(bf1, 0));
    assert!(third
        .log
        .contains(&format!("{{seat 0}} holds {{zone {bf1}}}")));
    assert!(
        !exhausted(&host, vanguard),
        "the awaken step readies last turn's play"
    );
    assert_eq!(cards_in(&host, 0, ZONE_RUNE_POOL).len(), 3);
    assert_eq!(
        host.plugin_view(0).status[1],
        "points · {seat 0} 2 · {seat 1} 0"
    );

    let scout = named(&host, 0, ZONE_HAND, "Scout");
    let asked = moved(&mut host, &mut client, 0, scout, ZONE_CHAIN, 0, TOP);
    assert_eq!(asked.queue.len(), 1);
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(&*format!("where does {{card {scout}}} enter?"))
    );
    assert_eq!(
        labels(&strip),
        [
            "your base".to_string(),
            format!("{{zone {bf1}}}"),
            "cancel".into(),
            "free table".into()
        ]
    );
    assert_eq!(strip.affordances[2].hotkey.as_deref(), Some("x"));
    let taken_back = pick(&mut host, &mut client, 0, 2);
    assert!(taken_back.prompt.is_none());
    assert!(taken_back.queue.is_empty());
    assert!(cards_in(&host, 0, ZONE_HAND).contains(&scout));
    assert_eq!(cards_in(&host, 0, ZONE_RUNE_POOL).len(), 3);
    assert_eq!(
        taken_back.log.last().unwrap(),
        &format!("{{seat 0}} takes back {{card {scout}}}")
    );
    moved(&mut host, &mut client, 0, scout, ZONE_CHAIN, 0, TOP);
    let entered = pick(&mut host, &mut client, 0, 1);
    assert!(entered.prompt.is_none());
    assert_eq!(units_in(&host, 0, bf1), [vi, scout]);
    assert!(exhausted(&host, scout));
    assert!(
        entered.showdown.is_none(),
        "a play onto a held battlefield contests nothing"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_RUNE_POOL)
            .iter()
            .filter(|rune| !exhausted(&host, **rune))
            .count(),
        2
    );
    assert_eq!(
        entered.log.last().unwrap(),
        &format!("{{seat 0}} plays {{card {scout}}} to {{zone {bf1}}}")
    );

    let contested = moved(&mut host, &mut client, 0, vanguard, bf2, 0, TOP);
    assert!(
        contested.prompt.is_none(),
        "Vi cannot follow: battlefield to battlefield needs Ganking"
    );
    let showdown = contested
        .showdown
        .clone()
        .expect("the other battlefield opens a showdown");
    assert_eq!(
        (showdown.zone, showdown.combat, showdown.focus()),
        (bf2, false, 0)
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let taken = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(taken.showdown.is_none());
    assert_eq!(taken.holder(bf2), Some(0));
    assert_eq!(points(&host, 0), 3);
    assert_eq!(
        taken.log.last().unwrap(),
        &format!("{{seat 0}} conquers {{zone {bf2}}}")
    );
    let strip = host.plugin_view(0);
    assert_eq!(strip.status[1], "points · {seat 0} 3 · {seat 1} 0");
    assert_eq!(
        strip.status[2],
        format!("{{zone {bf1}}} held by {{seat 0}} · {{zone {bf2}}} held by {{seat 0}}")
    );
    assert_eq!(labels(&strip), ["end turn", "free table"]);
    refused(&mut host, 0, TurnEvent::Pass, Refusal::NoShowdown);

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn gear(name: &str, energy: u8, domain: &str) -> CardFace {
    CardFace::named(name)
        .with_kind("Gear")
        .with_cost(Some(energy), Some(0))
        .with_domain(vec![domain.to_string()])
}

fn might(host: &HostSession, card: u32) -> i32 {
    host.state()
        .counter(CounterTarget::Card(card), COUNTER_MIGHT)
        .unwrap_or(0)
}

fn ready_runes(host: &HostSession, seat: u8) -> usize {
    cards_in(host, seat, ZONE_RUNE_POOL)
        .into_iter()
        .filter(|rune| !exhausted(host, *rune))
        .count()
}

fn card_option(card: u32) -> String {
    format!("{{card {card}}}")
}

#[test]
fn a_reaction_is_countered_across_two_seats_and_frigid_jewel_fires_on_the_second_draw() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let mine: Vec<CardFace> = [
        spell("Stupefy", 1, 0, "Mind"),
        spell("Stupefy", 1, 0, "Mind"),
        spell("Discipline", 2, 0, "Calm"),
        unit("Warden", 1, 0, "Calm", 2),
        unit("Scout", 1, 0, "Calm", 1),
        unit("Lookout", 1, 0, "Calm", 1),
        unit("Brawler", 2, 0, "Calm", 2),
        unit("Reserve", 3, 1, "Calm", 4),
        unit("Sentinel", 2, 1, "Calm", 3),
    ]
    .into_iter()
    .rev()
    .collect();
    let theirs: Vec<CardFace> = [
        spell("Defy", 1, 1, "Calm"),
        spell("Stupefy", 1, 0, "Mind"),
        unit("Dreamer", 1, 0, "Calm", 1),
        unit("Wisp", 1, 0, "Calm", 1),
        unit("Seer", 2, 0, "Calm", 2),
        unit("Keeper", 2, 0, "Calm", 2),
        unit("Wanderer", 1, 0, "Calm", 1),
        unit("Sleeper", 3, 0, "Calm", 3),
    ]
    .into_iter()
    .rev()
    .collect();
    deal(&mut host, &mut client, 0, mine, ZONE_MAIN_DECK);
    deal(&mut host, &mut client, ada, theirs, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let my_base = deal(
        &mut host,
        &mut client,
        0,
        vec![
            unit("Vi", 3, 1, "Fury", 3),
            unit("Poppy", 2, 0, "Fury", 2),
            gear("Frigid Jewel", 2, "Mind"),
        ],
        ZONE_BASE,
    );
    let (vi, poppy, jewel) = (my_base[0], my_base[1], my_base[2]);
    let their_base = deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Jinx", 3, 1, "Calm", 3), unit("Ekko", 2, 1, "Calm", 2)],
        ZONE_BASE,
    );
    let (jinx, ekko) = (their_base[0], their_base[1]);
    let units = [vi, poppy, jinx, ekko].map(card_option);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    pick(&mut host, &mut client, 0, 4);
    let started = pick(&mut host, &mut client, ada, 4);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action), "{started:?}");
    assert_eq!(
        started.seat(0).draws,
        1,
        "the beginning-phase draw is the first"
    );
    assert_eq!(ready_runes(&host, 0), 2);

    let stupefies: Vec<u32> = cards_in(&host, 0, ZONE_HAND)
        .into_iter()
        .filter(|card| face_name(&host, *card) == "Stupefy")
        .collect();
    assert_eq!(stupefies.len(), 2);
    let discipline = named(&host, 0, ZONE_HAND, "Discipline");
    let hand_size = cards_in(&host, 0, ZONE_HAND).len();
    assert_eq!(hand_size, 5);

    let asked = moved(&mut host, &mut client, 0, stupefies[0], ZONE_CHAIN, 0, TOP);
    let prompt = asked.prompt.clone().expect("the caster picks a unit");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (0, 1, 1, true)
    );
    assert_eq!(asked.queue.len(), 1);
    assert!(asked.chain.is_empty(), "targets come before the cost");
    assert_eq!(
        ready_runes(&host, 0),
        2,
        "nothing is paid before the targets"
    );
    let strip = host.plugin_view(0);
    let why = format!("{}: choose a unit (0 of 1)", card_option(stupefies[0]));
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(why.as_str())
    );
    let mut expected: Vec<String> = units.to_vec();
    expected.push("cancel".into());
    expected.push("free table".into());
    assert_eq!(labels(&strip), expected);
    assert_eq!(strip.affordances[2].card, Some(jinx));
    assert!(client
        .plugin_view(ada)
        .status
        .contains(&format!("waiting for {{seat 0}}: {why}")));
    refused(&mut host, 0, TurnEvent::Pass, Refusal::PromptOpen);
    refused(
        &mut host,
        ada,
        TurnEvent::Pick(Pick {
            prompt: prompt.id,
            option: 2,
        }),
        Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NotYourPrompt { seat: 0 }),
    );

    let stacked = pick(&mut host, &mut client, 0, 2);
    assert!(stacked.prompt.is_none() && stacked.queue.is_empty());
    assert_eq!(stacked.chain.len(), 1);
    assert_eq!(stacked.chain[0].targets, [TargetRef::Card(jinx)]);
    assert_eq!(
        stacked
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((0, 0))
    );
    assert_eq!(ready_runes(&host, 0), 1, "one energy paid");
    assert!(host
        .plugin_view(0)
        .status
        .contains(&format!("chain: {} (top)", card_option(stupefies[0]))));
    assert_eq!(labels(&host.plugin_view(0)), ["pass", "free table"]);
    let defy = named(&host, ada, ZONE_HAND, "Defy");
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(labels(&client.plugin_view(ada)), ["pass"]);
    let error = host
        .intent(
            ada,
            WireIntent::Move {
                card: defy,
                to: WireZone::Plugin(ZONE_CHAIN),
                seat: 0,
                index: TOP,
            },
        )
        .expect_err("a seat without runes cannot react");
    assert!(error.is_refusal(), "{error}");
    assert!(cards_in(&host, ada, ZONE_HAND).contains(&defy));

    let resolved = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(might(&host, jinx), -1, "Stupefy shrank Jinx");
    assert_eq!(cards_in(&host, 0, ZONE_TRASH), [stupefies[0]]);
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_size,
        "one played, one drawn"
    );
    assert_eq!(resolved.seat(0).draws, 2);
    assert!(resolved.chain.is_empty());
    assert_eq!(resolved.queue.len(), 1, "the jewel's trigger is pending");
    let prompt = resolved
        .prompt
        .clone()
        .expect("Frigid Jewel asks its controller for a friendly unit");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (0, 1, 1, false)
    );
    let strip = host.plugin_view(0);
    let why = format!("{}: choose a friendly unit (0 of 1)", card_option(jewel));
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(why.as_str())
    );
    assert_eq!(
        labels(&strip),
        [card_option(vi), card_option(poppy), "free table".into()],
        "only seat 0's units, no cancel on a trigger"
    );
    assert!(client
        .plugin_view(ada)
        .status
        .contains(&format!("waiting for {{seat 0}}: {why}")));
    assert_eq!(might(&host, vi), 0);

    let triggered = pick(&mut host, &mut client, 0, 0);
    assert!(triggered.prompt.is_none() && triggered.queue.is_empty());
    assert_eq!(triggered.chain.len(), 1);
    assert!(matches!(
        triggered.chain[0].kind,
        ItemKind::Trigger { source, index: 0 } if source == jewel
    ));
    assert_eq!(triggered.chain[0].targets, [TargetRef::Card(vi)]);
    assert_eq!(
        triggered
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((0, 0))
    );
    assert!(triggered
        .log
        .contains(&format!("{} triggers", card_option(jewel))));
    assert_eq!(
        might(&host, vi),
        0,
        "nothing happens until the trigger resolves"
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let fired = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(fired.chain.is_empty() && fired.priority.is_none());
    assert_eq!(might(&host, vi), 2, "Vi has +2 this turn");
    assert_eq!(
        fired.log.last().unwrap(),
        &format!("{} ability resolves", card_option(jewel))
    );

    moved(&mut host, &mut client, 0, stupefies[1], ZONE_CHAIN, 0, TOP);
    pick(&mut host, &mut client, 0, 3);
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let quiet = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(quiet.seat(0).draws, 3);
    assert!(
        quiet.prompt.is_none() && quiet.queue.is_empty() && quiet.chain.is_empty(),
        "the third draw of the turn wakes nothing"
    );
    assert_eq!(might(&host, ekko), -1);
    assert_eq!(might(&host, vi), 2, "the jewel fires once a turn");
    assert_eq!(ready_runes(&host, 0), 0);
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), hand_size);

    let second = turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    assert_eq!((second.turn(), second.turn_player()), (2, ada));
    assert_eq!(
        (might(&host, vi), might(&host, jinx), might(&host, ekko)),
        (0, 0, 0),
        "this-turn mods expire at the end of the turn"
    );
    assert_eq!(second.seat(0).draws, 0);
    assert_eq!(second.seat(ada).draws, 1);
    assert_eq!(ready_runes(&host, ada), 3);

    let their_stupefy = named(&host, ada, ZONE_HAND, "Stupefy");
    let their_hand = cards_in(&host, ada, ZONE_HAND).len();
    let asked = moved(
        &mut host,
        &mut client,
        ada,
        their_stupefy,
        ZONE_CHAIN,
        0,
        TOP,
    );
    assert_eq!(asked.prompt.as_ref().map(|prompt| prompt.seat), Some(ada));
    let mut expected: Vec<String> = units.to_vec();
    expected.push("cancel".into());
    expected.push("free table".into());
    assert_eq!(labels(&client.plugin_view(ada)), expected);
    assert!(host.plugin_view(0).status.iter().any(|line| {
        line == &format!(
            "waiting for {{seat 1}}: {}: choose a unit (0 of 1)",
            card_option(their_stupefy)
        )
    }));
    pick(&mut host, &mut client, ada, 0);
    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let theirs_resolved = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(might(&host, vi), -1);
    assert_eq!(theirs_resolved.seat(ada).draws, 2);
    assert!(
        theirs_resolved.prompt.is_none()
            && theirs_resolved.queue.is_empty()
            && theirs_resolved.chain.is_empty(),
        "the other seat's second draw does not wake seat 0's jewel"
    );
    assert_eq!(cards_in(&host, ada, ZONE_HAND).len(), their_hand);
    assert_eq!(ready_runes(&host, ada), 2);

    let third = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((third.turn(), third.turn_player()), (3, 0));
    assert_eq!(might(&host, vi), 0);
    assert_eq!(ready_runes(&host, 0), 4, "two awakened and two channeled");
    assert_eq!(ready_runes(&host, ada), 2);
    let hand_size = cards_in(&host, 0, ZONE_HAND).len();
    let their_hand = cards_in(&host, ada, ZONE_HAND).len();

    let asked = moved(&mut host, &mut client, 0, discipline, ZONE_CHAIN, 0, TOP);
    assert_eq!(asked.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
    assert_eq!(
        host.plugin_view(0)
            .prompt
            .as_ref()
            .map(|summary| summary.why.clone()),
        Some(format!(
            "{}: choose a unit (0 of 1)",
            card_option(discipline)
        ))
    );
    let stacked = pick(&mut host, &mut client, 0, 0);
    assert_eq!(stacked.chain.len(), 1);
    assert!(matches!(stacked.chain[0].kind, ItemKind::Spell { card } if card == discipline));
    assert_eq!(stacked.chain[0].targets, [TargetRef::Card(vi)]);
    assert_eq!(ready_runes(&host, 0), 2, "two energy paid");
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(
        passed
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((ada, 1))
    );
    assert_eq!(labels(&client.plugin_view(ada)), ["pass"]);
    let their_deck = cards_in(&host, ada, ZONE_RUNE_DECK).len();

    let asked = moved(&mut host, &mut client, ada, defy, ZONE_CHAIN, 0, TOP);
    let prompt = asked
        .prompt
        .clone()
        .expect("Defy asks for the spell to counter");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (ada, 1, 1, true)
    );
    assert_eq!(asked.chain.len(), 1, "Defy is still pending");
    assert_eq!(asked.queue.len(), 1);
    assert_eq!(ready_runes(&host, ada), 2, "nothing paid before the target");
    let strip = client.plugin_view(ada);
    let why = format!("{}: choose a spell to counter (0 of 1)", card_option(defy));
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(why.as_str())
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{} on the chain", card_option(discipline)),
            "cancel".into()
        ]
    );
    assert_eq!(strip.affordances[0].card, Some(discipline));
    assert_eq!(strip.affordances[1].hotkey.as_deref(), Some("x"));
    assert!(host
        .plugin_view(0)
        .status
        .contains(&format!("waiting for {{seat 1}}: {why}")));
    refused(&mut host, ada, TurnEvent::Pass, Refusal::PromptOpen);
    refused(
        &mut host,
        0,
        TurnEvent::Pick(Pick {
            prompt: prompt.id,
            option: 0,
        }),
        Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NotYourPrompt { seat: ada }),
    );

    let countering = pick(&mut host, &mut client, ada, 0);
    assert!(countering.prompt.is_none() && countering.queue.is_empty());
    assert_eq!(countering.chain.len(), 2);
    assert!(matches!(countering.chain[1].kind, ItemKind::Spell { card } if card == defy));
    assert_eq!(
        countering.chain[1].targets,
        [TargetRef::Item(countering.chain[0].id)]
    );
    assert_eq!(countering.chain[1].controller, ada);
    assert_eq!(
        countering
            .priority
            .map(|priority| (priority.active, priority.passes)),
        Some((ada, 0)),
        "the reacting seat may respond to its own reaction"
    );
    assert_eq!(
        cards_in(&host, ada, ZONE_RUNE_DECK).len(),
        their_deck + 1,
        "a Calm rune is recycled for the power"
    );
    assert_eq!(ready_runes(&host, ada), 1);
    assert_eq!(labels(&client.plugin_view(ada)), ["pass"]);
    assert!(client.plugin_view(ada).status.contains(&format!(
        "chain: {} → {} (top)",
        card_option(discipline),
        card_option(defy)
    )));
    assert!(host
        .plugin_view(0)
        .status
        .contains(&"waiting for {seat 1}: respond or pass".to_string()));

    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(labels(&host.plugin_view(0)), ["pass", "free table"]);
    let countered = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert!(countered.chain.is_empty() && countered.priority.is_none());
    assert!(countered.prompt.is_none() && countered.queue.is_empty());
    assert!(cards_in(&host, 0, ZONE_TRASH).contains(&discipline));
    assert!(cards_in(&host, ada, ZONE_TRASH).contains(&defy));
    assert!(cards_in(&host, 0, ZONE_CHAIN).is_empty());
    assert_eq!(might(&host, vi), 0, "the countered spell never resolves");
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_size - 1,
        "no draw either"
    );
    assert_eq!(cards_in(&host, ada, ZONE_HAND).len(), their_hand - 1);
    assert_eq!(countered.seat(0).draws, 1);
    assert_eq!(
        ready_runes(&host, 0),
        2,
        "Discipline's cost is not refunded"
    );
    assert!(countered
        .log
        .contains(&format!("{} is countered", card_option(discipline))));
    assert!(countered
        .log
        .contains(&format!("{} resolves", card_option(defy))));
    assert!(!countered
        .log
        .contains(&format!("{} resolves", card_option(discipline))));
    assert!(
        !countered.seat(0).played_main,
        "a countered spell does not count for Legion"
    );
    assert_eq!(labels(&host.plugin_view(0)), ["end turn", "free table"]);
    assert_eq!(labels(&client.plugin_view(ada)).len(), 0);

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn a_full_attack_assigns_damage_across_the_defenders_and_the_survivor_conquers() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let filler: Vec<CardFace> = [
        unit("Scout", 1, 0, "Fury", 1),
        unit("Warden", 1, 0, "Fury", 2),
        unit("Lookout", 1, 0, "Fury", 1),
        unit("Brawler", 2, 0, "Fury", 2),
        unit("Reserve", 3, 1, "Fury", 4),
    ]
    .into_iter()
    .rev()
    .collect();
    deal(&mut host, &mut client, 0, filler.clone(), ZONE_MAIN_DECK);
    deal(&mut host, &mut client, ada, filler, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Fury"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let vi = deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Vi", 3, 1, "Fury", 5)],
        ZONE_BASE,
    )[0];
    let garrison = deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Jinx", 3, 1, "Calm", 2), unit("Ekko", 2, 1, "Calm", 1)],
        bf1,
    );
    let (jinx, ekko) = (garrison[0], garrison[1]);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    pick(&mut host, &mut client, 0, 4);
    let started = pick(&mut host, &mut client, ada, 4);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(
        started.holder(bf1),
        Some(ada),
        "two units alone at a battlefield hold it"
    );

    let staged = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    assert!(staged.prompt.is_none(), "Vi marches alone");
    let showdown = staged.showdown.clone().expect("a combat opens at once");
    assert_eq!(
        (
            showdown.zone,
            showdown.attacker,
            showdown.defender,
            showdown.combat
        ),
        (bf1, 0, ada, true)
    );
    assert_eq!(showdown.focus(), 0);
    let strip = host.plugin_view(0);
    assert!(strip.status.iter().any(|line| line
        == &format!(
        "attackers {{card {vi}}} (5 might) · defenders {{card {jinx}}}, {{card {ekko}}} (3 might)"
    )));
    assert!(host
        .state()
        .annotation(vi, "attacker")
        .is_some_and(|value| value == [1]));
    assert!(host
        .state()
        .annotation(jinx, "defender")
        .is_some_and(|value| value == [1]));

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let assigning = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let prompt = assigning
        .prompt
        .clone()
        .expect("the attacking seat orders the damage");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("assign 5 damage: who takes lethal next?")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {jinx}}} (lethal 2)"),
            format!("{{card {ekko}}} (lethal 1)"),
            "free table".into()
        ]
    );
    assert_eq!(strip.affordances[1].card, Some(ekko));
    assert!(client
        .plugin_view(ada)
        .status
        .contains(&"waiting for {seat 0}: assign 5 damage: who takes lethal next?".to_string()));
    refused(&mut host, 0, TurnEvent::Pass, Refusal::PromptOpen);
    assert_eq!(
        host.view().plugin_state,
        client.view().plugin_state,
        "the damage step leaves both replicas on the same bytes"
    );

    let closed = pick(&mut host, &mut client, 0, 0);
    assert!(closed.showdown.is_none() && closed.prompt.is_none());
    assert_eq!(units_in(&host, 0, bf1), [vi]);
    assert_eq!(cards_in(&host, ada, ZONE_TRASH), [jinx, ekko]);
    assert_eq!(closed.holder(bf1), Some(0));
    assert_eq!(closed.contester(bf1), None);
    assert!(closed.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(
        host.state().annotation(vi, "attacker"),
        None,
        "2e clears the designations"
    );
    assert!(closed.log.iter().any(|line| line
        == &format!("{{card {jinx}}} takes 2 · {{card {ekko}}} takes 3 · {{card {vi}}} takes 3")));
    assert!(closed
        .log
        .iter()
        .any(|line| line == &format!("{{seat 0}} conquers {{zone {bf1}}}")));
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn a_stunned_defender_blunts_the_attack_and_the_survivors_send_the_attackers_home() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let mine: Vec<CardFace> = [
        spell("Back Off", 3, 0, "Calm"),
        unit("Scout", 1, 0, "Calm", 1),
        unit("Lookout", 1, 0, "Calm", 1),
        unit("Brawler", 2, 0, "Calm", 2),
        unit("Reserve", 3, 1, "Calm", 4),
        unit("Sentinel", 2, 1, "Calm", 3),
        unit("Runner", 1, 0, "Calm", 1),
    ]
    .into_iter()
    .rev()
    .collect();
    let theirs: Vec<CardFace> = [
        unit("Dreamer", 1, 0, "Calm", 1),
        unit("Wisp", 1, 0, "Calm", 1),
        unit("Seer", 2, 0, "Calm", 2),
        unit("Keeper", 2, 0, "Calm", 2),
        unit("Wanderer", 1, 0, "Calm", 1),
        unit("Sleeper", 3, 0, "Calm", 3),
        unit("Drifter", 1, 0, "Calm", 1),
    ]
    .into_iter()
    .rev()
    .collect();
    deal(&mut host, &mut client, 0, mine, ZONE_MAIN_DECK);
    deal(&mut host, &mut client, ada, theirs, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let raid = deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Vi", 3, 1, "Fury", 5), unit("Spear", 2, 0, "Fury", 2)],
        ZONE_BASE,
    );
    let (vi, spear) = (raid[0], raid[1]);
    let garrison = deal(
        &mut host,
        &mut client,
        ada,
        vec![
            unit("Warden", 3, 1, "Calm", 4),
            unit("Guard", 3, 1, "Calm", 4),
        ],
        bf1,
    );
    let (warden, guard) = (garrison[0], garrison[1]);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    pick(&mut host, &mut client, 0, 4);
    let started = pick(&mut host, &mut client, ada, 4);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.holder(bf1), Some(ada));

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((theirs.turn(), theirs.turn_player()), (3, 0));
    assert_eq!(
        points(&host, ada),
        1,
        "the garrison holds the battlefield through its own beginning phase"
    );
    assert_eq!(
        ready_runes(&host, 0),
        4,
        "two turns of channelling pay for Back Off"
    );

    let asked = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    assert!(asked.showdown.is_none(), "the group move is offered first");
    let combat = pick(&mut host, &mut client, 0, 0);
    assert_eq!(units_in(&host, 0, bf1), [warden, guard, vi, spear]);
    let showdown = combat.showdown.clone().expect("a combat opens at once");
    assert_eq!(
        (
            showdown.zone,
            showdown.attacker,
            showdown.defender,
            showdown.combat
        ),
        (bf1, 0, ada, true)
    );
    assert_eq!(showdown.focus(), 0);
    let strip = host.plugin_view(0);
    assert!(strip.status.iter().any(|line| line == &format!(
        "attackers {{card {vi}}}, {{card {spear}}} (7 might) · defenders {{card {warden}}}, {{card {guard}}} (8 might)"
    )));

    let back_off = named(&host, 0, ZONE_HAND, "Back Off");
    let aimed = moved(&mut host, &mut client, 0, back_off, ZONE_CHAIN, 0, TOP);
    let prompt = aimed.prompt.clone().expect("Back Off chooses a unit");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(&*format!(
            "{}: choose a unit (0 of 1)",
            card_option(back_off)
        ))
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {warden}}}"),
            format!("{{card {guard}}}"),
            format!("{{card {vi}}}"),
            format!("{{card {spear}}}"),
            "cancel".into(),
            "free table".into()
        ],
        "Back Off reads 'a unit', so friend and foe are both fair game"
    );
    let stunned = pick(&mut host, &mut client, 0, 1);
    assert_eq!(stunned.chain.len(), 1);
    assert_eq!(stunned.chain[0].targets, [TargetRef::Card(guard)]);
    assert_eq!(ready_runes(&host, 0), 1, "three energy paid");
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let resolved = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(resolved.chain.is_empty());
    assert!(resolved
        .log
        .iter()
        .any(|line| line == &format!("{{card {guard}}} is stunned")));
    assert_eq!(cards_in(&host, 0, ZONE_TRASH), [back_off]);
    assert!(host
        .state()
        .annotation(guard, "stunned")
        .is_some_and(|value| value == [1]));
    let strip = host.plugin_view(0);
    assert!(strip.status.iter().any(|line| line == &format!(
        "attackers {{card {vi}}}, {{card {spear}}} (7 might) · defenders {{card {warden}}}, {{card {guard}}} (4 might)"
    )));

    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let assigning = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let prompt = assigning
        .prompt
        .clone()
        .expect("the attacking seat orders its seven");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("assign 7 damage: who takes lethal next?")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {warden}}} (lethal 4)"),
            format!("{{card {guard}}} (lethal 4)"),
            "free table".into()
        ]
    );

    let theirs = pick(&mut host, &mut client, 0, 0);
    let prompt = theirs
        .prompt
        .clone()
        .expect("the defending seat orders its four");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (ada, 1, 1));
    let strip = client.plugin_view(ada);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("assign 4 damage: who takes lethal next?")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {vi}}} (lethal 5)"),
            format!("{{card {spear}}} (lethal 2)")
        ]
    );

    let closed = pick(&mut host, &mut client, ada, 0);
    assert!(closed.showdown.is_none() && closed.prompt.is_none());
    assert_eq!(cards_in(&host, ada, ZONE_TRASH), [warden]);
    assert_eq!(
        units_in(&host, 0, bf1),
        [guard],
        "the stunned defender soaks the overflow and lives"
    );
    assert_eq!(
        units_in(&host, 0, ZONE_BASE),
        [vi, spear],
        "both attackers are sent home"
    );
    assert!(closed
        .log
        .contains(&"attackers 7 might vs defenders 4 might".to_string()));
    assert!(closed.log.contains(&format!(
        "{{card {warden}}} takes 4 · {{card {guard}}} takes 3 · {{card {vi}}} takes 4"
    )));
    assert!(closed.log.contains(&format!(
        "{{card {warden}}} dies · {{card {vi}}} survives (4 damage healed) · \
         {{card {guard}}} survives (3 damage healed) · {{seat 0}} returns to base"
    )));
    assert!(closed
        .log
        .contains(&format!("{{seat {ada}}} keeps {{zone {bf1}}}")));
    assert_eq!(closed.holder(bf1), Some(ada));
    assert_eq!(closed.contester(bf1), None);
    assert!(
        !closed.scored(bf1, ada),
        "a kept battlefield scores nothing"
    );
    assert_eq!(points(&host, ada), 1);
    assert_eq!(points(&host, 0), 0);
    assert_eq!(
        host.state().annotation(vi, "attacker"),
        None,
        "2e clears the designations"
    );
    assert_eq!(
        host.state().annotation(guard, "stunned"),
        Some(&[1u8][..]),
        "the stun outlives the combat and clears at the ending step"
    );
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn powered_gear(name: &str, energy: u8, power: u8, domain: &str) -> CardFace {
    CardFace::named(name)
        .with_kind("Gear")
        .with_cost(Some(energy), Some(power))
        .with_domain(vec![domain.to_string()])
}

fn board_zones() -> [u16; 4] {
    [
        ZONE_BASE,
        ZONE_BATTLEFIELD_FIRST,
        ZONE_BATTLEFIELD_FIRST + 1,
        ZONE_TRASH,
    ]
}

fn faces_named(host: &HostSession, seat: u8, name: &str) -> Vec<u32> {
    board_zones()
        .into_iter()
        .flat_map(|zone| cards_in(host, seat, zone))
        .filter(|card| face_name(host, *card) == name)
        .collect()
}

fn zone_of(host: &HostSession, card: u32) -> Option<(u8, u16)> {
    for seat in [0u8, 1u8] {
        for zone in board_zones().into_iter().chain([ZONE_HAND]) {
            if cards_in(host, seat, zone).contains(&card) {
                return Some((seat, zone));
            }
        }
    }
    None
}

fn on_the_table(host: &HostSession, card: u32) -> bool {
    host.face_of(card).is_some() && zone_of(host, card).is_some()
}

fn client_owns(client: &ClientSession, seat: u8, zone: u16, card: u32) -> bool {
    client
        .state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(zone))
        .any(|held| held.id.0 == card)
}

fn drain_priority(host: &mut HostSession, client: &mut ClientSession) -> GameBlob {
    let mut blob = blob_of(host, client);
    for _ in 0..64 {
        if blob.prompt.is_some() {
            break;
        }
        let seat = match (blob.priority, blob.showdown.as_ref()) {
            (Some(priority), _) => priority.active,
            (None, Some(showdown)) => showdown.focus(),
            (None, None) => break,
        };
        blob = turn_event(host, client, seat, TurnEvent::Pass);
    }
    blob
}

fn open_m4_game(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    first_battlefield: CardFace,
    my_hand: Vec<CardFace>,
    my_base: Vec<CardFace>,
) -> (Vec<u32>, Vec<u32>) {
    open_m4_game_with(
        host,
        client,
        ada,
        winner,
        first_battlefield,
        Vec::new(),
        my_hand,
        my_base,
    )
}

#[allow(clippy::too_many_arguments)]
fn open_m4_game_with(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    first_battlefield: CardFace,
    my_legend: Vec<CardFace>,
    my_hand: Vec<CardFace>,
    my_base: Vec<CardFace>,
) -> (Vec<u32>, Vec<u32>) {
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(
        host,
        client,
        0,
        vec![unit("Scout", 1, 0, "Mind", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(
        host,
        client,
        ada,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(host, client, 0, vec![rune("Mind"); 8], ZONE_RUNE_DECK);
    deal(host, client, ada, vec![rune("Calm"); 8], ZONE_RUNE_DECK);
    deal(host, client, 0, vec![rune("Mind"); 2], ZONE_RUNE_POOL);
    deal(host, client, 0, vec![first_battlefield], bf1);
    deal(host, client, ada, vec![battlefield("Sanctum")], bf2);
    if !my_legend.is_empty() {
        deal(host, client, 0, my_legend, ZONE_LEGEND);
    }
    let hand = if my_hand.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_hand, ZONE_HAND)
    };
    let base = if my_base.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_base, ZONE_BASE)
    };
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        host,
        client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(host, client, TurnEvent::StartGame { first_player: 0 });
    let keep = cards_in(host, 0, ZONE_HAND).len() as u16;
    pick(host, client, 0, keep);
    let keep = cards_in(host, ada, ZONE_HAND).len() as u16;
    let started = pick(host, client, ada, keep);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
    (hand, base)
}

#[test]
fn a_sprite_conquers_and_dies_to_temporary_at_the_start_of_its_owners_next_beginning_phase() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (hand, _) = open_m4_game(
        &mut host,
        &mut client,
        ada,
        winner,
        battlefield("Crossroads"),
        vec![powered_gear("Sprite Fountain", 2, 1, "Mind")],
        Vec::new(),
    );
    let fountain = hand[0];

    let played = moved(&mut host, &mut client, 0, fountain, ZONE_BASE, 0, TOP);
    assert!(
        matches!(played.chain[0].kind, ItemKind::Trigger { source, index: 0 } if source == fountain),
        "the play trigger waits on the chain"
    );
    assert!(
        faces_named(&host, 0, "Sprite").is_empty(),
        "no token before the trigger resolves"
    );
    let spawned = drain_priority(&mut host, &mut client);
    assert!(spawned.chain.is_empty() && spawned.priority.is_none());
    let sprites = faces_named(&host, 0, "Sprite");
    assert_eq!(sprites.len(), 1, "the fountain plays one Sprite");
    let sprite = sprites[0];
    assert_eq!(
        zone_of(&host, sprite),
        Some((0, ZONE_BASE)),
        "the token is seat 0's, in seat 0's base"
    );
    assert!(
        client_owns(&client, 0, ZONE_BASE, sprite),
        "so says the joiner"
    );
    assert!(!exhausted(&host, sprite), "178.1: it arrives ready");
    assert!(spawned.log.contains(&"{seat 0} plays a Sprite".to_string()));

    let marched = moved(&mut host, &mut client, 0, sprite, bf1, 0, TOP);
    assert_eq!(marched.contester(bf1), Some(0));
    assert!(exhausted(&host, sprite), "marching exhausts it");
    let conquered = drain_priority(&mut host, &mut client);
    assert_eq!(conquered.showdown, None);
    assert_eq!(conquered.holder(bf1), Some(0));
    assert!(conquered.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(
        zone_of(&host, sprite),
        Some((0, bf1)),
        "a token holds a battlefield like any unit"
    );
    assert!(client_owns(&client, 0, bf1, sprite));

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((theirs.turn(), theirs.turn_player()), (3, 0));
    assert_eq!(theirs.phase(), Some(Phase::Beginning));
    let batch = theirs
        .prompt
        .clone()
        .expect("two Temporary permanents, so their controller orders them");
    assert_eq!((batch.seat, batch.min, batch.max), (0, 2, 2));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("order your triggers (last placed resolves first)")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{} is Temporary", card_option(fountain)),
            format!("{} is Temporary", card_option(sprite)),
            "free table".to_string()
        ]
    );
    assert!(
        on_the_table(&host, sprite),
        "nothing has died before the batch is ordered"
    );
    let ordered = pick(&mut host, &mut client, 0, 1);
    assert!(
        ordered.prompt.is_none(),
        "one pick settles a two-way order: the last option is forced"
    );
    assert_eq!(ordered.chain.len(), 2, "both kills wait on the chain");
    let cleared = drain_priority(&mut host, &mut client);

    assert!(
        !on_the_table(&host, sprite),
        "the Sprite is despawned, not trashed"
    );
    assert!(
        !client_owns(&client, 0, bf1, sprite) && !client_owns(&client, 0, ZONE_BASE, sprite),
        "and the joiner's replica agrees"
    );
    assert_eq!(
        zone_of(&host, fountain),
        Some((0, ZONE_TRASH)),
        "the gear is a real card, so it is trashed"
    );
    assert_eq!(
        cleared.holder(bf1),
        None,
        "the Temporary kill lands before scoring, so the hold goes with it"
    );
    assert!(!cleared.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1, "no second point was scored");
    assert!(cleared
        .log
        .contains(&format!("{} is Temporary and dies", card_option(sprite))));
    let survivors = faces_named(&host, 0, "Sprite");
    assert_eq!(
        survivors.len(),
        1,
        "the fountain's Deathknell plays one that survives this Beginning Phase"
    );
    assert_ne!(survivors[0], sprite);
    assert_eq!(zone_of(&host, survivors[0]), Some((0, ZONE_BASE)));
    assert_eq!(cleared.phase(), Some(Phase::Action));
    assert_eq!(cleared.turn_player(), 0);
    assert_eq!(
        cleared.log,
        [
            format!("{} is Temporary and dies", card_option(fountain)),
            format!("{} ability resolves", card_option(fountain)),
            format!("{} triggers", card_option(fountain)),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            "{seat 0} plays a Sprite".to_string(),
            format!("{} ability resolves", card_option(fountain)),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            format!("{} is Temporary and dies", card_option(sprite)),
            format!("{} ability resolves", card_option(sprite)),
            "turn 3 · {seat 0}".to_string()
        ],
        "every death of the entry is still in the window the player reads"
    );

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn a_lab_holding_hero(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    my_base: Vec<CardFace>,
) -> Vec<u32> {
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (_, base) = open_m4_game(
        host,
        client,
        ada,
        winner,
        battlefield("Dusk Rose Lab"),
        Vec::new(),
        my_base,
    );
    let hero = base[0];
    moved(host, client, 0, hero, bf1, 0, TOP);
    let conquered = drain_priority(host, client);
    assert_eq!(conquered.holder(bf1), Some(0), "the lab is seat 0's");
    turn_event(host, client, 0, TurnEvent::EndTurn);
    let theirs = turn_event(host, client, ada, TurnEvent::EndTurn);
    assert_eq!((theirs.turn(), theirs.turn_player()), (3, 0));
    assert_eq!(theirs.phase(), Some(Phase::Beginning));
    base
}

#[test]
fn a_lab_kill_fires_the_heros_deathknell_and_both_draws_land_before_the_hold_scores() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let base = a_lab_holding_hero(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![unit("Unsung Hero", 2, 0, "Order", 5)],
    );
    let hero = base[0];
    let lab = cards_in(&host, 0, bf1)
        .into_iter()
        .find(|card| face_name(&host, *card) == "Dusk Rose Lab")
        .expect("the lab sits at its battlefield");
    let hand = cards_in(&host, 0, ZONE_HAND).len();

    let staged = blob_of(&host, &client);
    assert!(
        staged.prompt.is_none() && staged.chain.len() == 1,
        "352.10.c.1 · the trigger goes on the chain choosing nothing"
    );
    assert!(staged.chain[0].targets.is_empty());
    let asked = drain_priority(&mut host, &mut client);
    assert!(asked.prompt.is_some(), "the lab asks as it resolves");
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(
            format!(
                "{}: choose a unit you control here to kill (0 of 1)",
                card_option(lab)
            )
            .as_str()
        )
    );
    assert_eq!(
        labels(&strip),
        [card_option(hero), "skip".into(), "free table".into()],
        "the lab may be declined"
    );
    assert!(
        on_the_table(&host, hero) && zone_of(&host, hero) == Some((0, bf1)),
        "nothing is killed before the answer"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand,
        "and so does every draw"
    );
    pick(&mut host, &mut client, 0, 0);

    let done = drain_priority(&mut host, &mut client);
    assert_eq!(
        zone_of(&host, hero),
        Some((0, ZONE_TRASH)),
        "the hero is a card, so it is trashed"
    );
    assert!(
        done.log.contains(&"{seat 0} draws 2".to_string()),
        "the Deathknell of a Mighty hero draws two"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand + 4,
        "1 for the lab, 2 for a Mighty Deathknell, 1 for the Beginning-phase draw"
    );
    assert_eq!(
        done.holder(bf1),
        None,
        "the lab killed its own holder before scoring"
    );
    assert!(!done.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1, "only the turn-1 conquer scored");
    assert_eq!(done.phase(), Some(Phase::Action));
    assert!(done.chain.is_empty() && done.queue.is_empty() && done.prompt.is_none());
    assert_eq!(
        done.log,
        [
            "turn 2 · {seat 1}".to_string(),
            format!("{} triggers", card_option(lab)),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            format!("{} is killed for a card", card_option(hero)),
            format!("{} ability resolves", card_option(lab)),
            format!("{} triggers", card_option(hero)),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            "{seat 0} draws 2".to_string(),
            format!("{} ability resolves", card_option(hero)),
            "turn 3 · {seat 0}".to_string()
        ],
        "the kill and the Deathknell both survive to the end of the entry"
    );

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn zhonyas_replaces_the_lab_kill_and_the_heros_deathknell_never_fires() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let base = a_lab_holding_hero(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![
            unit("Unsung Hero", 2, 0, "Order", 5),
            gear("Zhonya's Hourglass", 2, "Calm"),
        ],
    );
    let (hero, hourglass) = (base[0], base[1]);
    let hand = cards_in(&host, 0, ZONE_HAND).len();

    let asked = drain_priority(&mut host, &mut client);
    assert!(asked.prompt.is_some(), "the lab asks as it resolves");
    pick(&mut host, &mut client, 0, 0);
    let done = drain_priority(&mut host, &mut client);

    assert_eq!(
        zone_of(&host, hero),
        Some((0, ZONE_BASE)),
        "366.1: the hero is recalled instead of dying"
    );
    assert!(exhausted(&host, hero), "and recalled exhausted");
    assert_eq!(
        zone_of(&host, hourglass),
        Some((0, ZONE_TRASH)),
        "the gear dies in its place"
    );
    assert!(done.log.contains(&format!(
        "{} replaces the death of {}",
        card_option(hourglass),
        card_option(hero)
    )));
    assert!(done.log.contains(&format!(
        "{} is recalled exhausted instead of dying",
        card_option(hero)
    )));
    assert!(
        !done.log.contains(&"{seat 0} draws 2".to_string()),
        "nothing keys on a death that never happened, so no Deathknell draw"
    );
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand + 1,
        "no Deathknell draw and no lab draw: 'if you do' never happened"
    );
    assert_eq!(
        done.holder(bf1),
        None,
        "the recall still empties the battlefield, so the hold is lost"
    );
    assert!(!done.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(done.phase(), Some(Phase::Action));
    let lab = cards_in(&host, 0, bf1)
        .into_iter()
        .find(|card| face_name(&host, *card) == "Dusk Rose Lab")
        .expect("the lab sits at its battlefield");
    assert_eq!(
        done.log,
        [
            format!("showdown at {{zone {bf1}}} · {{seat 0}} against {{seat 1}}"),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            format!("{{seat 0}} conquers {{zone {bf1}}}"),
            "turn 2 · {seat 1}".to_string(),
            format!("{} triggers", card_option(lab)),
            "{seat 0} passes".to_string(),
            "{seat 1} passes".to_string(),
            format!(
                "{} replaces the death of {}",
                card_option(hourglass),
                card_option(hero)
            ),
            format!(
                "{} is recalled exhausted instead of dying",
                card_option(hero)
            ),
            format!("{} ability resolves", card_option(lab)),
            "turn 3 · {seat 0}".to_string()
        ],
        "the replacement and the recall both survive to the end of the entry"
    );

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn a_legend_activation_and_a_gold_payment_cross_the_hardened_plugin() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (hand, base) = open_m4_game_with(
        &mut host,
        &mut client,
        ada,
        winner,
        battlefield("Crossroads"),
        vec![CardFace::named("Lillia - Bashful Bloom").with_kind("Legend")],
        vec![spell("Spark", 0, 1, "Mind")],
        vec![unit("Treasure Hunter", 1, 0, "Mind", 2)],
    );
    let spark = hand[0];
    let hunter = base[0];
    let lillia = cards_in(&host, 0, ZONE_LEGEND)[0];

    let strip = host.plugin_view(0);
    let offer = strip
        .affordances
        .iter()
        .find(|affordance| affordance.card == Some(lillia))
        .expect("the legend's ability is a numbered action of its own");
    assert_eq!(
        offer.label,
        format!("{}: play a Sprite (4 energy, exhaust)", card_option(lillia))
    );
    assert!(offer.enabled, "four ready runes pay for it");

    moved(&mut host, &mut client, 0, hunter, bf1, 0, TOP);
    let conquered = drain_priority(&mut host, &mut client);
    assert_eq!(conquered.holder(bf1), Some(0));
    let gold = faces_named(&host, 0, "Gold");
    assert_eq!(gold.len(), 1, "the hunter's move mints a Gold");
    let gold = gold[0];
    assert!(exhausted(&host, gold), "and it arrives exhausted");
    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((theirs.turn(), theirs.turn_player()), (3, 0));
    assert_eq!(theirs.phase(), Some(Phase::Action));
    assert!(!exhausted(&host, gold), "the Awaken step readies it");

    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: lillia,
            ability: 0,
        },
    );
    let asked = blob_of(&host, &client);
    assert!(asked.prompt.is_some(), "it asks where the Sprite is played");
    let strip = host.plugin_view(0);
    assert_eq!(
        labels(&strip),
        [
            format!("{{zone {ZONE_BASE}}}"),
            format!("{{zone {bf1}}}"),
            "cancel".to_string(),
            "free table".to_string()
        ]
    );
    pick(&mut host, &mut client, 0, 0);
    let played = drain_priority(&mut host, &mut client);
    assert!(played.chain.is_empty() && played.prompt.is_none());
    assert!(exhausted(&host, lillia), "the activation exhausts her");
    let sprites = faces_named(&host, 0, "Sprite");
    assert_eq!(sprites.len(), 1);
    assert_eq!(zone_of(&host, sprites[0]), Some((0, ZONE_BASE)));
    assert!(
        client_owns(&client, 0, ZONE_BASE, sprites[0]),
        "the joiner's replica has the token too"
    );
    assert_eq!(
        ready_runes(&host, 0),
        2,
        "four of the six runes paid for it"
    );

    moved(&mut host, &mut client, 0, spark, ZONE_CHAIN, 0, TOP);
    let paying = blob_of(&host, &client);
    assert!(
        paying.prompt.is_some(),
        "a Gold or a rune can pay the power"
    );
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(format!("pay 1 power for {} with", card_option(spark)).as_str())
    );
    assert_eq!(
        labels(&strip),
        [
            format!("kill {}", card_option(gold)),
            "recycle a rune".to_string(),
            "cancel".to_string(),
            "free table".to_string()
        ],
        "the play can still be taken back unpaid"
    );
    pick(&mut host, &mut client, 0, 0);
    assert!(!on_the_table(&host, gold), "the Gold paid with its life");
    let resolved = drain_priority(&mut host, &mut client);
    assert!(resolved.chain.is_empty());
    assert_eq!(zone_of(&host, spark), Some((0, ZONE_TRASH)));
    assert_eq!(
        ready_runes(&host, 0),
        2,
        "no rune was recycled: the Gold covered the power"
    );

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn seen_by(host: &mut HostSession, client: &mut ClientSession, seat: u8) -> PluginView {
    let mine = host.plugin_view(seat);
    let theirs = client.plugin_view(seat);
    assert_eq!(
        mine.arrows, theirs.arrows,
        "both replicas compute seat {seat}'s arrows alike"
    );
    if client.seat() == seat {
        assert_eq!(
            mine.legal, theirs.legal,
            "the seat's own two replicas compute its legal list alike"
        );
    } else {
        assert!(
            theirs
                .legal
                .iter()
                .all(|row| mine.legal.iter().any(|held| held == row)),
            "a replica that does not hold seat {seat}'s faces sees a subset of its legal list, \
             got {:?} against {:?}",
            theirs.legal,
            mine.legal
        );
    }
    mine
}

fn owners(host: &HostSession, view: &PluginView) -> Vec<u8> {
    let mut out: Vec<u8> = view
        .legal
        .iter()
        .filter_map(|row| card_owner(host, row.card))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn card_owner(host: &HostSession, card: u32) -> Option<u8> {
    for seat in [0u8, 1u8] {
        for zone in
            board_zones()
                .into_iter()
                .chain([ZONE_HAND, ZONE_CHAIN, ZONE_LEGEND, ZONE_MAIN_DECK])
        {
            if cards_in(host, seat, zone).contains(&card) {
                return Some(seat);
            }
        }
    }
    None
}

fn legal_kinds(view: &PluginView, card: u32) -> Vec<LegalKind> {
    view.legal
        .iter()
        .find(|row| row.card == card)
        .map(|row| row.kinds.clone())
        .unwrap_or_default()
}

#[test]
fn both_seats_read_the_same_arrows_while_each_reads_only_its_own_legal_list() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let mine: Vec<CardFace> = [
        spell("Stupefy", 1, 0, "Mind"),
        spell("Stupefy", 1, 0, "Mind"),
        unit("Warden", 1, 0, "Calm", 2),
        unit("Scout", 1, 0, "Calm", 1),
        unit("Lookout", 1, 0, "Calm", 1),
        unit("Brawler", 2, 0, "Calm", 2),
        unit("Reserve", 3, 1, "Calm", 4),
    ]
    .into_iter()
    .rev()
    .collect();
    let theirs: Vec<CardFace> = [
        spell("Stupefy", 1, 0, "Mind"),
        unit("Dreamer", 1, 0, "Calm", 1),
        unit("Wisp", 1, 0, "Calm", 1),
        unit("Seer", 2, 0, "Calm", 2),
        unit("Keeper", 2, 0, "Calm", 2),
        unit("Wanderer", 1, 0, "Calm", 1),
    ]
    .into_iter()
    .rev()
    .collect();
    deal(&mut host, &mut client, 0, mine, ZONE_MAIN_DECK);
    deal(&mut host, &mut client, ada, theirs, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let vi = deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Vi", 3, 1, "Fury", 5)],
        ZONE_BASE,
    )[0];
    let garrison = deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Jinx", 3, 1, "Calm", 2), unit("Ekko", 2, 1, "Calm", 1)],
        bf1,
    );
    let (jinx, ekko) = (garrison[0], garrison[1]);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let mulligan = seen_by(&mut host, &mut client, 0);
    assert_eq!(
        owners(&host, &mulligan),
        [0],
        "the mulligan candidates are the asking seat's own hand"
    );
    assert!(
        !mulligan.legal.is_empty()
            && mulligan
                .legal
                .iter()
                .all(|row| row.kinds == [LegalKind::Answer]
                    && (row.zones.is_empty() || row.zones == [ZONE_MAIN_DECK])),
        "a prompt candidate and the mulligan gesture are both Answers: {:?}",
        mulligan.legal
    );
    assert!(
        seen_by(&mut host, &mut client, ada).legal.is_empty(),
        "the seat that is not being asked is offered nothing"
    );
    pick(&mut host, &mut client, 0, 4);
    let started = pick(&mut host, &mut client, ada, 4);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
    assert_eq!(
        started.holder(bf1),
        Some(ada),
        "two units alone at a battlefield hold it"
    );

    let stupefy = named(&host, 0, ZONE_HAND, "Stupefy");
    let quiet = seen_by(&mut host, &mut client, 0);
    assert!(
        quiet.arrows.is_empty(),
        "a board with nothing aimed draws nothing"
    );
    assert_eq!(
        owners(&host, &quiet),
        [0],
        "seat 0's legal list names only seat 0's cards"
    );
    let marching = quiet
        .legal
        .iter()
        .find(|row| row.card == vi)
        .expect("Vi may march");
    assert_eq!(marching.kinds, [LegalKind::March]);
    assert!(
        marching.zones.contains(&bf1) && marching.zones.contains(&bf2),
        "the destination list rides with the march"
    );
    assert_eq!(
        legal_kinds(&quiet, stupefy),
        [LegalKind::Play { accelerate: false }],
        "the viewer's own hand faces ride with the view request, so a hand card it can \
         afford is highlighted"
    );
    assert!(
        host.plugin_view(0)
            .legal
            .iter()
            .any(|row| row.card == stupefy),
        "the host holds seat 0's faces"
    );
    assert!(
        !client
            .plugin_view(0)
            .legal
            .iter()
            .any(|row| row.card == stupefy),
        "ada's replica does not hold seat 0's hand faces and never highlights them"
    );
    let idle = seen_by(&mut host, &mut client, ada);
    assert!(
        idle.legal.is_empty(),
        "the seat that is not the turn player is offered nothing"
    );
    assert_eq!(idle.arrows, quiet.arrows, "both seats see the same arrows");
    assert_ne!(
        quiet.legal, idle.legal,
        "the arrows are shared but the legal lists are not"
    );

    let asked = moved(&mut host, &mut client, 0, stupefy, ZONE_CHAIN, 0, TOP);
    assert!(asked.prompt.is_some(), "the caster picks a unit");
    let aimed = pick(&mut host, &mut client, 0, 1);
    assert_eq!(aimed.chain.len(), 1);
    assert_eq!(aimed.chain[0].targets, [TargetRef::Card(jinx)]);

    let caster = seen_by(&mut host, &mut client, 0);
    let target = seen_by(&mut host, &mut client, ada);
    let spell_arrow = Arrow {
        from: ArrowFrom::Card(stupefy),
        to: Aim::Card(jinx),
        kind: ArrowKind::Spell,
    };
    assert_eq!(caster.arrows, [spell_arrow]);
    assert_eq!(
        target.arrows, caster.arrows,
        "the defender sees the aim before priority passes"
    );
    assert!(
        target.legal.is_empty(),
        "the seat without priority is offered nothing"
    );
    let named = owners(&host, &caster);
    assert!(
        named.is_empty() || named == [0],
        "the caster is offered only its own cards, got {named:?}"
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let reacting = seen_by(&mut host, &mut client, ada);
    let waiting = seen_by(&mut host, &mut client, 0);
    assert_eq!(
        reacting.arrows,
        [spell_arrow],
        "passing priority changes no arrow"
    );
    assert_eq!(waiting.arrows, reacting.arrows);
    assert!(
        waiting.legal.is_empty(),
        "the seat that just passed is offered nothing"
    );
    if !reacting.legal.is_empty() {
        assert_eq!(
            owners(&host, &reacting),
            [ada],
            "the reacting seat's list names only its own cards"
        );
    }

    let resolved = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(resolved.chain.is_empty());
    assert_eq!(might(&host, jinx), -1, "Stupefy shrank Jinx");
    assert!(
        seen_by(&mut host, &mut client, 0).arrows.is_empty(),
        "a resolved spell leaves no arrow behind"
    );

    let combat = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    let showdown = combat.showdown.clone().expect("the march opens a combat");
    assert_eq!(
        (showdown.zone, showdown.attacker, showdown.defender),
        (bf1, 0, ada)
    );
    assert!(showdown.combat);

    let attacker = seen_by(&mut host, &mut client, 0);
    let defender = seen_by(&mut host, &mut client, ada);
    assert_eq!(
        attacker.arrows,
        [
            Arrow {
                from: ArrowFrom::Card(vi),
                to: Aim::Zone(bf1),
                kind: ArrowKind::Attack,
            },
            Arrow {
                from: ArrowFrom::Card(vi),
                to: Aim::Card(jinx),
                kind: ArrowKind::Combat,
            },
            Arrow {
                from: ArrowFrom::Card(vi),
                to: Aim::Card(ekko),
                kind: ArrowKind::Combat,
            },
        ],
        "one attack arrow at the battlefield and one designation per defender"
    );
    assert_eq!(
        defender.arrows, attacker.arrows,
        "both seats receive the same combat arrows"
    );
    assert!(
        attacker
            .legal
            .iter()
            .all(|row| row.kinds == [LegalKind::React] && row.zones == [ZONE_CHAIN]),
        "the focus holder's own Reaction cards are the only thing an open showdown offers, \
         got {:?}",
        attacker.legal
    );
    assert!(
        !attacker.legal.is_empty(),
        "seat 0 still holds a Stupefy and holds focus"
    );
    assert!(
        defender.legal.is_empty(),
        "the seat without focus is offered nothing"
    );
    for (seat, view) in [(0u8, &attacker), (ada, &defender)] {
        let named = owners(&host, view);
        assert!(
            named.is_empty() || named == [seat],
            "seat {seat} is offered only its own cards, got {named:?}"
        );
    }
}

#[test]
fn a_busy_board_still_answers_view_inside_the_plugin_gas_budget() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;

    let hand: Vec<CardFace> = (0..30)
        .map(|index| unit(&format!("Recruit {index}"), 1, 0, "Calm", 1))
        .rev()
        .collect();
    deal(&mut host, &mut client, 0, hand.clone(), ZONE_MAIN_DECK);
    deal(&mut host, &mut client, ada, hand, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Calm"); 12],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 12],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let mine: Vec<CardFace> = (0..40)
        .map(|index| unit(&format!("Guard {index}"), 1, 0, "Calm", 1))
        .collect();
    deal(&mut host, &mut client, 0, mine, ZONE_BASE);
    let theirs: Vec<CardFace> = (0..20)
        .map(|index| unit(&format!("Sentry {index}"), 1, 0, "Calm", 1))
        .collect();
    deal(&mut host, &mut client, ada, theirs, bf2);

    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    pick(&mut host, &mut client, 0, 4);
    pick(&mut host, &mut client, ada, 4);

    let busy = seen_by(&mut host, &mut client, 0);
    assert!(
        !busy.status.is_empty(),
        "a gas-exhausted view decodes as an empty PluginView: the board is too big for the \
         presenter"
    );
    assert!(
        busy.legal
            .iter()
            .filter(|row| row.kinds == [LegalKind::March])
            .count()
            >= 40,
        "every guard at the base may march, got {} rows",
        busy.legal.len()
    );
    assert!(
        !host.plugin_view(ada).status.is_empty(),
        "the other seat's view survives the same board"
    );
}

fn hidden_move(
    host: &mut HostSession,
    client: &mut ClientSession,
    seat: u8,
    card: u32,
    to: u16,
    to_seat: u8,
) -> Vec<LogEntry> {
    let entries = host
        .intent(
            seat,
            WireIntent::MoveHidden {
                card,
                to: WireZone::Plugin(to),
                seat: to_seat,
                index: TOP,
            },
        )
        .unwrap_or_else(|error| panic!("hiding {card} at zone {to} is legal: {error}"));
    relay(client, &entries);
    entries
}

fn revealed_in(entries: &[LogEntry], card: u32) -> bool {
    entries
        .iter()
        .any(|entry| matches!(entry.action, LogAction::Reveal { card: shown, .. } if shown == card))
}

fn replica_face_is_blank(state: &agni_sim::log::LogState, card: u32) -> bool {
    state
        .table
        .cards()
        .iter()
        .find(|held| held.id.0 == card)
        .is_some_and(|held| held.face.is_hidden())
}

fn joiner_knows_the_face(client: &ClientSession, card: u32) -> bool {
    client
        .table()
        .cards()
        .iter()
        .find(|held| held.id.0 == card)
        .is_some_and(|held| !held.face.is_hidden())
}

fn hidden_at(blob: &GameBlob, card: u32) -> Option<u16> {
    blob.card_state(card).and_then(|row| row.hidden_at)
}

fn open_m6_game(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    their_deck: Vec<CardFace>,
) -> (Vec<u32>, Vec<u32>) {
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(
        host,
        client,
        0,
        vec![unit("Scout", 1, 0, "Mind", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(host, client, ada, their_deck, ZONE_MAIN_DECK);
    deal(host, client, 0, vec![rune("Mind"); 8], ZONE_RUNE_DECK);
    deal(host, client, ada, vec![rune("Calm"); 8], ZONE_RUNE_DECK);
    deal(host, client, 0, vec![rune("Mind"); 2], ZONE_RUNE_POOL);
    deal(host, client, ada, vec![rune("Calm"); 2], ZONE_RUNE_POOL);
    deal(host, client, 0, vec![battlefield("Crossroads")], bf1);
    deal(host, client, ada, vec![battlefield("Sanctum")], bf2);
    let hand = deal(
        host,
        client,
        0,
        vec![spell("Consult the Past", 4, 0, "Mind")],
        ZONE_HAND,
    );
    let base = deal(
        host,
        client,
        0,
        vec![unit("Unsung Hero", 2, 0, "Order", 5)],
        ZONE_BASE,
    );
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        host,
        client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(host, client, TurnEvent::StartGame { first_player: 0 });
    let keep = cards_in(host, 0, ZONE_HAND).len() as u16;
    pick(host, client, 0, keep);
    let keep = cards_in(host, ada, ZONE_HAND).len() as u16;
    let started = pick(host, client, ada, keep);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
    (hand, base)
}

fn a_held_battlefield_with_a_card_hidden_at_it(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    their_deck: Vec<CardFace>,
) -> (u32, u32) {
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (hand, base) = open_m6_game(host, client, ada, winner, their_deck);
    let consult = hand[0];
    let hero = base[0];
    moved(host, client, 0, hero, bf1, 0, TOP);
    let conquered = drain_priority(host, client);
    assert_eq!(
        conquered.holder(bf1),
        Some(0),
        "seat 0 holds the battlefield"
    );

    let runes = ready_runes(host, 0);
    let entries = hidden_move(host, client, 0, consult, bf1, 0);
    assert!(
        !revealed_in(&entries, consult),
        "737.1.b · a hide is a MoveHidden the host never reveals"
    );
    assert_eq!(host.hidden_plays(), [consult]);
    assert!(
        replica_face_is_blank(host.state(), consult),
        "the hider's own replica lays it face down too"
    );
    assert!(
        replica_face_is_blank(client.state(), consult),
        "and so does the other seat's"
    );
    assert!(
        !joiner_knows_the_face(client, consult),
        "the joiner holds no private face for it either"
    );
    assert!(!host.state().revealed.contains(&consult));

    let hidden = blob_of(host, client);
    assert_eq!(hidden_at(&hidden, consult), Some(bf1));
    assert_eq!(
        hidden.card_state(consult).map(|row| row.hidden_since),
        Some(hidden.turn())
    );
    assert_eq!(
        ready_runes(host, 0),
        runes - 1,
        "[A] recycles exactly one ready rune"
    );
    assert!(hidden
        .log
        .contains(&format!("{{seat 0}} hides a card at {{zone {bf1}}}")));
    move_refused(
        host,
        0,
        consult,
        ZONE_CHAIN,
        0,
        Refusal::Illegal(Reason::HiddenThisTurn),
    );
    (consult, hero)
}

#[test]
fn a_hidden_card_stays_blind_to_both_seats_and_is_revealed_in_the_trash_when_the_hold_is_lost() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (consult, hero) = a_held_battlefield_with_a_card_hidden_at_it(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let mine = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((mine.turn(), mine.turn_player()), (3, 0));
    let awake = drain_priority(&mut host, &mut client);
    assert_eq!(awake.holder(bf1), Some(0), "the hold survives two turns");
    assert_eq!(hidden_at(&awake, consult), Some(bf1));
    assert!(
        replica_face_is_blank(client.state(), consult),
        "still blind to the other seat a turn later"
    );

    let walked = moved(&mut host, &mut client, 0, hero, ZONE_BASE, 0, TOP);
    assert_eq!(zone_of(&host, hero), Some((0, ZONE_BASE)));
    let lost = if walked.holder(bf1).is_some() {
        drain_priority(&mut host, &mut client)
    } else {
        walked
    };
    assert_eq!(
        lost.holder(bf1),
        None,
        "the last unit walked off, so the hold is gone"
    );
    assert_eq!(
        hidden_at(&lost, consult),
        None,
        "cleanup step 5 drops the facedown mark with the card"
    );
    assert_eq!(
        zone_of(&host, consult),
        Some((0, ZONE_TRASH)),
        "and the card it was guarding goes to the trash"
    );
    assert!(
        lost.log
            .iter()
            .any(|line| line.contains("is revealed in the trash")),
        "the log says why: {:?}",
        lost.log
    );
    assert!(
        host.state().revealed.contains(&consult),
        "the trash is public, so the face is public"
    );
    assert!(
        !replica_face_is_blank(client.state(), consult),
        "and the other seat finally reads it"
    );
    assert_eq!(
        client
            .state()
            .table
            .cards()
            .iter()
            .find(|held| held.id.0 == consult)
            .map(|held| held.face.name.clone()),
        Some("Consult the Past".to_string())
    );
    assert!(host.hidden_plays().is_empty());

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn a_hidden_reaction_is_played_from_facedown_for_zero_inside_the_other_seats_chain() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (consult, _) = a_held_battlefield_with_a_card_hidden_at_it(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![spell("Whisper", 1, 0, "Calm"); 10],
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = blob_of(&host, &client);
    assert_eq!((theirs.turn(), theirs.turn_player()), (2, 1));
    let theirs = drain_priority(&mut host, &mut client);
    assert_eq!(theirs.phase(), Some(Phase::Action));
    assert!(
        replica_face_is_blank(client.state(), consult),
        "their own turn opens and they still cannot read it"
    );

    let whisper = cards_in(&host, ada, ZONE_HAND)
        .into_iter()
        .find(|card| face_name(&host, *card) == "Whisper")
        .expect("the joiner drew a Whisper");
    let staged = moved(&mut host, &mut client, ada, whisper, ZONE_CHAIN, 0, TOP);
    assert!(
        !staged.chain.is_empty(),
        "their play waits on the chain: {staged:?}"
    );
    assert_eq!(
        staged.priority.map(|priority| priority.active),
        Some(ada),
        "the seat that played keeps focus first"
    );
    let open = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(
        open.priority.map(|priority| priority.active),
        Some(0),
        "343 · then priority reaches the other seat"
    );
    assert_eq!(open.chain.len(), 1, "their spell is still waiting");
    assert!(
        replica_face_is_blank(client.state(), consult),
        "their chain is open and the face is still theirs to guess"
    );

    let hand_before = cards_in(&host, 0, ZONE_HAND).len();
    let runes_before = ready_runes(&host, 0);
    let entries = host
        .intent(
            0,
            WireIntent::Move {
                card: consult,
                to: WireZone::Plugin(ZONE_CHAIN),
                seat: 0,
                index: TOP,
            },
        )
        .expect("737.6 · a facedown card older than this turn reacts");
    assert!(
        revealed_in(&entries, consult),
        "the chain is public, so the play is the reveal"
    );
    relay(&mut client, &entries);
    let played = blob_of(&host, &client);
    assert_eq!(
        ready_runes(&host, 0),
        runes_before,
        "737.1.b · played from hidden for zero, its printed 4 energy ignored"
    );
    assert_eq!(hidden_at(&played, consult), None);
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_before,
        "nothing is drawn before it resolves"
    );
    let mine = played
        .chain
        .iter()
        .find(|item| matches!(item.kind, ItemKind::Spell { card } if card == consult))
        .expect("the reaction sits above their play");
    assert_eq!(mine.controller, 0);
    assert!(
        format!("{:?}", mine.origin).contains("Facedown"),
        "Origin::Facedown is the provenance the scripts read: {:?}",
        mine.origin
    );
    assert_eq!(
        played.chain.last().map(|item| item.kind),
        Some(mine.kind),
        "343 · the reaction resolves before their play"
    );

    let resolved = drain_priority(&mut host, &mut client);
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_before + 2,
        "737.2 · the printed instruction is unchanged: draw 2"
    );
    assert_eq!(zone_of(&host, consult), Some((0, ZONE_TRASH)));
    let played_line = format!("{{seat 0}} plays {{card {consult}}} from hidden at {{zone {bf1}}}");
    let reveal_line = format!("{{seat 0}} reveals {{card {consult}}}");
    let at = resolved
        .log
        .iter()
        .position(|line| *line == played_line)
        .expect("the play from hidden is in the window");
    assert_eq!(
        resolved.log.get(at - 1),
        Some(&reveal_line),
        "the face becomes public in the same breath as the play, never before: {:?}",
        resolved.log
    );
    assert!(
        resolved.log[..at - 1]
            .iter()
            .all(|line| !line.contains(&format!("card {consult}"))),
        "nothing named it while it lay facedown: {:?}",
        resolved.log
    );
    assert!(host.hidden_plays().is_empty());

    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn rendered_name(table: &agni_core::Table, card: u32) -> String {
    table
        .cards()
        .iter()
        .find(|held| held.id.0 == card)
        .map(|held| held.face.name.clone())
        .unwrap_or_default()
}

#[test]
fn neither_seats_rendered_table_names_the_others_facedown_card() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (consult, _) = a_held_battlefield_with_a_card_hidden_at_it(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
    );
    assert_eq!(hidden_at(&blob_of(&host, &client), consult), Some(bf1));
    assert_eq!(
        rendered_name(&host.table(), consult),
        "Consult the Past",
        "the hider's own table still names the card it hid"
    );
    assert_eq!(
        rendered_name(&host.table_for(ada), consult),
        "",
        "408.3 · the table the other seat is rendered leaves it blank"
    );
    assert_eq!(
        rendered_name(&client.table(), consult),
        "",
        "and so does the joiner's own replica"
    );
    let mine = host.table();
    let theirs = host.table_for(ada);
    for seat in [0u8, ada] {
        for zone in [ZONE_HAND, ZONE_MAIN_DECK] {
            for card in cards_in(&host, seat, zone) {
                let seen = if seat == 0 { &mine } else { &theirs };
                let hidden = if seat == 0 { &theirs } else { &mine };
                assert!(
                    rendered_name(hidden, card).is_empty(),
                    "card {card} in seat {seat}'s zone {zone} leaks to the other seat"
                );
                let _ = seen;
            }
        }
    }
}

#[test]
fn the_other_seat_may_neither_reveal_nor_re_hide_a_facedown_card() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let (consult, _) = a_held_battlefield_with_a_card_hidden_at_it(
        &mut host,
        &mut client,
        ada,
        winner,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
    );

    let refused = host
        .intent(ada, WireIntent::Reveal { card: consult })
        .expect_err("408.3 · a facedown card is not theirs to turn over");
    assert!(refused.is_refusal(), "{refused}");
    assert_eq!(
        host.hidden_plays(),
        [consult],
        "the refusal leaves the host's face suppression in place"
    );
    assert!(replica_face_is_blank(host.state(), consult));
    assert!(replica_face_is_blank(client.state(), consult));
    assert!(!joiner_knows_the_face(&client, consult));

    let stolen = host
        .intent(
            ada,
            WireIntent::MoveHidden {
                card: consult,
                to: WireZone::Plugin(bf2),
                seat: 0,
                index: TOP,
            },
        )
        .expect_err("nor to move where it lies");
    assert!(
        matches!(stolen, SessionError::Refused(FoldError::ForeignHand)),
        "the fold itself refuses a hide of another player's card: {stolen}"
    );
    assert_eq!(
        host.hidden_plays(),
        [consult],
        "and a refused hide never clears the entry a real hide put there"
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = drain_priority(&mut host, &mut client);
    assert_eq!(theirs.turn_player(), ada);
    assert_eq!(
        hidden_at(&theirs, consult),
        Some(bf1),
        "the card is still face down where it was hidden"
    );
    assert!(
        replica_face_is_blank(host.state(), consult)
            && replica_face_is_blank(client.state(), consult),
        "and no later entry published the face behind the refusal's back"
    );
    assert!(!joiner_knows_the_face(&client, consult));
    assert_eq!(rendered_name(&host.table_for(ada), consult), "");

    let shown = host
        .intent(0, WireIntent::Reveal { card: consult })
        .expect("411.2 · its own seat may still show it");
    assert!(revealed_in(&shown, consult));
    relay(&mut client, &shown);
    assert!(joiner_knows_the_face(&client, consult));
    let after = blob_of(&host, &client);
    assert_eq!(
        hidden_at(&after, consult),
        Some(bf1),
        "411.2 · a voluntary show leaves the card in its facedown zone"
    );
    assert_eq!(host.log(), client.log());
}

fn roll_labels(view: &PluginView) -> Vec<String> {
    labels(view)
        .into_iter()
        .filter(|label| label != "free table")
        .collect()
}

fn client_cards_in(client: &ClientSession, seat: u8, zone: u16) -> Vec<u32> {
    client
        .state()
        .table
        .in_area(PlayerId(seat), Zone::Plugin(zone))
        .map(|card| card.id.0)
        .collect()
}

#[test]
fn a_two_card_mulligan_recycles_through_a_roll_and_both_replicas_order_the_deck_alike() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let names = [
        "Herald", "Vanguard", "Scout", "Sentinel", "Warden", "Brawler", "Lookout", "Reserve",
    ];
    let mine: Vec<CardFace> = names
        .iter()
        .map(|name| unit(name, 1, 0, "Fury", 1))
        .collect();
    let my_deck = deal(&mut host, &mut client, 0, mine, ZONE_MAIN_DECK);
    deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Wisp", 1, 0, "Calm", 1); 8],
        ZONE_MAIN_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Fury"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let hand = cards_in(&host, 0, ZONE_HAND);
    assert_eq!(hand.len(), 4);
    assert_eq!(
        cards_in(&host, 0, ZONE_MAIN_DECK),
        my_deck[..4],
        "four cards were dealt from the top"
    );
    let set_aside = [hand[0], hand[1]];
    pick(&mut host, &mut client, 0, 0);
    let rolling = pick(&mut host, &mut client, 0, 0);
    let open = rolling
        .roll
        .clone()
        .expect("two recycled cards are ordered by a roll");
    assert_eq!(open.why, agni_riftbound_turns::engine::roll::WHY_MULLIGAN);
    assert!(open.id >= agni_riftbound_turns::engine::roll::ROLL_ID_BASE);
    let prompt = rolling.prompt.clone().expect("the shuffle prompt");
    assert_eq!(prompt.seat, 0);
    assert_eq!(prompt.picked, set_aside);
    assert!(rolling
        .log
        .contains(&"{seat 0} sets aside 2 and redraws".to_string()));
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), 4);
    let before_roll = cards_in(&host, 0, ZONE_MAIN_DECK);
    assert_eq!(before_roll.len(), 4);
    assert_eq!(&before_roll[2..], &my_deck[..2]);
    assert!(before_roll[..2].iter().all(|card| set_aside.contains(card)));
    for seat in [0, ada] {
        let strip = seen_by(&mut host, &mut client, seat);
        assert_eq!(roll_labels(&strip), ["roll"]);
        assert_eq!(
            strip.affordances[0].kind,
            AffordanceKind::Commit { roll: open.id }
        );
        assert_eq!(
            strip.prompt.as_ref().map(|summary| summary.why.as_str()),
            Some("roll to shuffle 2 recycled cards")
        );
    }
    refused(&mut host, 0, TurnEvent::EndTurn, Refusal::PromptOpen);
    refused(
        &mut host,
        0,
        TurnEvent::Pick(Pick {
            prompt: prompt.id,
            option: 0,
        }),
        Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NoSuchOption {
            option: 0,
            count: 0,
        }),
    );
    move_refused(&mut host, 0, hand[2], ZONE_BASE, 0, Refusal::PromptOpen);
    let secrets = [[5u8; 8], [6u8; 8]];
    refused(
        &mut host,
        ada,
        TurnEvent::RevealRoll { secret: secrets[1] },
        Refusal::Dice(agni_plugin_sdk::dice::DiceRefusal::NotEveryoneCommitted),
    );
    from_host(
        &mut host,
        &mut client,
        TurnEvent::CommitRoll {
            commit: commitment(&secrets[0]),
        },
    );
    assert!(host
        .plugin_view(0)
        .status
        .contains(&"waiting for every seat to roll".to_string()));
    assert_eq!(roll_labels(&client.plugin_view(ada)), ["roll"]);
    from_client(
        &mut host,
        &mut client,
        ada,
        TurnEvent::CommitRoll {
            commit: commitment(&secrets[1]),
        },
    );
    for seat in [0, ada] {
        let strip = seen_by(&mut host, &mut client, seat);
        assert_eq!(roll_labels(&strip), ["reveal"]);
        assert_eq!(
            strip.affordances[0].kind,
            AffordanceKind::Reveal { roll: open.id }
        );
    }
    from_client(
        &mut host,
        &mut client,
        ada,
        TurnEvent::RevealRoll { secret: secrets[1] },
    );
    assert!(blob_of(&host, &client).roll.is_some());
    assert!(host
        .plugin_view(ada)
        .status
        .contains(&"waiting for every seat to reveal".to_string()));
    from_host(
        &mut host,
        &mut client,
        TurnEvent::RevealRoll { secret: secrets[0] },
    );
    let ordered = blob_of(&host, &client);
    assert!(ordered.roll.is_none());
    assert_eq!(
        ordered.prompt.as_ref().map(|prompt| prompt.seat),
        Some(ada),
        "the next seat mulligans once the roll has ordered the recycle"
    );
    assert!(ordered
        .log
        .iter()
        .any(|line| line.starts_with("the roll orders the deck bottom")));
    let after_roll = cards_in(&host, 0, ZONE_MAIN_DECK);
    assert_eq!(
        after_roll,
        client_cards_in(&client, 0, ZONE_MAIN_DECK),
        "both replicas hold the deck in the same order"
    );
    assert_eq!(&after_roll[2..], &my_deck[..2]);
    assert!(after_roll[..2].iter().all(|card| set_aside.contains(card)));
    assert_eq!(
        host.state().table.cards().len(),
        client.state().table.cards().len()
    );
    refused(
        &mut host,
        0,
        TurnEvent::CommitRoll {
            commit: commitment(&secrets[0]),
        },
        Refusal::AlreadyStarted,
    );
    let keep = cards_in(&host, ada, ZONE_HAND).len() as u16;
    let started = pick(&mut host, &mut client, ada, keep);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
}

fn face_for(host: &HostSession, seat: u8, card: u32) -> Option<String> {
    let table = host.table_for(seat);
    let held = table.get(agni_core::CardId(card))?;
    (!held.face.is_hidden()).then(|| held.face.name.clone())
}

fn joiner_face(client: &ClientSession, card: u32) -> Option<String> {
    let table = client.table();
    let held = table.get(agni_core::CardId(card))?;
    (!held.face.is_hidden()).then(|| held.face.name.clone())
}

fn face_visible_in(view: &agni_sim::view::TableView, card: u32) -> bool {
    view.card(card).is_some_and(|held| held.face_visible)
}

fn deliver_private_faces(host: &mut HostSession, client: &mut ClientSession, seat: u8) {
    let faces = host
        .owed_faces()
        .into_iter()
        .filter(|(to, _)| *to == seat)
        .map(|(_, face)| face)
        .collect();
    client.add_faces(faces);
}

fn option_index(view: &PluginView, label: &str) -> u16 {
    view.affordances
        .iter()
        .position(|affordance| affordance.label == label)
        .unwrap_or_else(|| panic!("{label} is offered among {:?}", labels(view))) as u16
}

#[derive(Default)]
struct M7Deal {
    their_deck: Vec<CardFace>,
    their_pool: Vec<CardFace>,
    their_hand: Vec<CardFace>,
    my_hand: Vec<CardFace>,
    my_base: Vec<CardFace>,
    their_garrison: Vec<CardFace>,
}

fn open_m7_game(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    deals: M7Deal,
) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    open_m7_game_with_pick(host, client, ada, winner, deals, None)
}

fn open_m7_game_with_pick(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    deals: M7Deal,
    first_pick: Option<u16>,
) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let M7Deal {
        their_deck,
        their_pool,
        their_hand,
        my_hand,
        my_base,
        their_garrison,
    } = deals;
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    deal(
        host,
        client,
        0,
        vec![unit("Scout", 1, 0, "Mind", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(host, client, ada, their_deck, ZONE_MAIN_DECK);
    deal(host, client, 0, vec![rune("Mind"); 8], ZONE_RUNE_DECK);
    deal(host, client, ada, vec![rune("Calm"); 8], ZONE_RUNE_DECK);
    deal(host, client, 0, vec![rune("Mind"); 2], ZONE_RUNE_POOL);
    if !their_pool.is_empty() {
        deal(host, client, ada, their_pool, ZONE_RUNE_POOL);
    }
    deal(host, client, 0, vec![battlefield("Crossroads")], bf1);
    deal(host, client, ada, vec![battlefield("Sanctum")], bf2);
    let theirs = if their_hand.is_empty() {
        Vec::new()
    } else {
        deal(host, client, ada, their_hand, ZONE_HAND)
    };
    let hand = if my_hand.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_hand, ZONE_HAND)
    };
    let base = if my_base.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_base, ZONE_BASE)
    };
    let garrison = if their_garrison.is_empty() {
        Vec::new()
    } else {
        deal(host, client, ada, their_garrison, bf1)
    };
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        host,
        client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(host, client, TurnEvent::StartGame { first_player: 0 });
    let keep = first_pick.unwrap_or_else(|| cards_in(host, 0, ZONE_HAND).len() as u16);
    pick(host, client, 0, keep);
    if first_pick.is_some() {
        let keep = cards_in(host, 0, ZONE_HAND).len() as u16 - 1;
        pick(host, client, 0, keep);
    }
    let keep = cards_in(host, ada, ZONE_HAND).len() as u16;
    let started = pick(host, client, ada, keep);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
    (hand, base, theirs, garrison)
}

#[test]
fn scuttle_crabs_deathknell_reveals_the_opponents_hand_to_both_seats() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let their_deck: Vec<CardFace> = (0..10)
        .map(|index| unit(&format!("Wisp {index}"), 1, 0, "Calm", 1))
        .collect();
    let (_, base, _, garrison) = open_m7_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck,
            my_base: vec![unit("Scuttle Crab", 2, 0, "Calm", 0)],
            their_garrison: vec![unit("Sentry", 1, 0, "Calm", 1)],
            ..M7Deal::default()
        },
    );
    let crab = base[0];
    let sentry = garrison[0];
    deliver_private_faces(&mut host, &mut client, ada);
    let started = blob_of(&host, &client);
    assert_eq!(started.holder(bf1), Some(ada), "the sentry holds it alone");

    let their_hand = cards_in(&host, ada, ZONE_HAND);
    assert_eq!(their_hand.len(), 4);
    for card in &their_hand {
        assert!(
            face_for(&host, 0, *card).is_none(),
            "before the reveal seat 0 holds no face for the opponent's hand"
        );
        assert!(!face_visible_in(host.view(), *card));
        assert!(joiner_face(&client, *card).is_some_and(|name| name.starts_with("Wisp")));
        assert!(!host.state().revealed.contains(card));
    }

    let staged = moved(&mut host, &mut client, 0, crab, bf1, 0, TOP);
    let showdown = staged.showdown.clone().expect("a combat opens");
    assert_eq!((showdown.zone, showdown.combat), (bf1, true));
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let fought = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(
        fought.prompt.is_none(),
        "one attacker and one defender assign nothing by hand"
    );
    assert_eq!(
        zone_of(&host, crab),
        Some((0, ZONE_TRASH)),
        "a 0-might crab dies to the sentry's one damage"
    );
    assert_eq!(
        units_in(&host, 0, bf1),
        [sentry],
        "the sentry stands alone at the shared battlefield"
    );
    assert_eq!(fought.chain.len(), 1, "the Deathknell waits on the chain");
    assert!(
        matches!(fought.chain[0].kind, ItemKind::Trigger { source, index: 1 } if source == crab)
    );
    assert_eq!(
        fought.chain[0].targets,
        [TargetRef::Seat(ada)],
        "the only opponent answers its own prompt"
    );
    for card in &their_hand {
        assert!(
            face_for(&host, 0, *card).is_none(),
            "nothing is revealed before the Deathknell resolves"
        );
    }

    let before = host.log().len();
    let resolved = drain_priority(&mut host, &mut client);
    assert!(resolved.chain.is_empty() && resolved.prompt.is_none());
    let paid: Vec<(u8, u32)> = host.log()[before..]
        .iter()
        .filter_map(|entry| match &entry.action {
            LogAction::Reveal { card, face } => {
                assert!(!face.is_hidden(), "the host pays the debt with the face");
                Some((entry.seat, *card))
            }
            _ => None,
        })
        .collect();
    let mut expected: Vec<(u8, u32)> = their_hand.iter().map(|card| (ada, *card)).collect();
    expected.sort_unstable();
    let mut paid_sorted = paid.clone();
    paid_sorted.sort_unstable();
    assert_eq!(
        paid_sorted, expected,
        "every hand card is revealed once, by its owner"
    );
    for card in &their_hand {
        assert!(host.state().revealed.contains(card));
        assert!(client.state().revealed.contains(card));
        let name = face_for(&host, 0, *card).expect("seat 0 now reads the face");
        assert!(name.starts_with("Wisp"));
        assert_eq!(joiner_face(&client, *card).as_deref(), Some(name.as_str()));
        assert!(
            face_visible_in(host.view(), *card),
            "the reveal is shown in place for seat 0's table view"
        );
        assert!(face_visible_in(client.view(), *card));
        assert_eq!(
            zone_of(&host, *card),
            Some((ada, ZONE_HAND)),
            "the revealed cards stay in the hand"
        );
    }
    assert!(host.state().owed_reveals.is_empty());
    assert!(client.state().owed_reveals.is_empty());
    assert_eq!(
        resolved.seat(0).looks_facedown_of,
        1 << ada,
        "the look bit is set for the rest of the turn"
    );
    assert!(resolved
        .log
        .contains(&format!("{{seat {ada}}} reveals their hand")));
    assert!(resolved.log.contains(&format!(
        "{{seat 0}} may look at {{seat {ada}}}'s facedown cards this turn"
    )));
    assert!(resolved.log.contains(&"{seat 0} gains 1 XP".to_string()));
    assert_eq!(
        host.state()
            .counter(CounterTarget::Seat(0), agni_riftbound::COUNTER_XP),
        Some(1)
    );
    assert_eq!(resolved.holder(bf1), Some(ada), "the sentry keeps the hold");
    assert!(
        host.owed_faces().is_empty(),
        "a public face is owed to nobody in private"
    );
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = drain_priority(&mut host, &mut client);
    assert_eq!(theirs.turn_player(), ada);
    let wisp = their_hand[0];
    let entries = hidden_move(&mut host, &mut client, ada, wisp, bf1, 0);
    assert!(
        entries
            .iter()
            .any(|entry| matches!(entry.action, LogAction::Move { card, hidden: true, .. } if card == wisp)),
        "the hide travels as a hidden move"
    );
    assert!(!revealed_in(&entries, wisp));
    assert_eq!(
        face_for(&host, 0, wisp),
        None,
        "a card seat 0 was shown in the opponent's hand is secret again once it is hidden"
    );
    assert!(replica_face_is_blank(host.state(), wisp));
    assert!(replica_face_is_blank(client.state(), wisp));
    assert!(!host.state().revealed.contains(&wisp));
    assert!(!client.state().revealed.contains(&wisp));
    assert!(
        joiner_face(&client, wisp).is_some_and(|name| name.starts_with("Wisp")),
        "the hider keeps their own face"
    );
    assert_eq!(host.hidden_plays(), [wisp]);
    assert_eq!(hidden_at(&blob_of(&host, &client), wisp), Some(bf1));
    assert_eq!(host.log(), client.log());
}

#[test]
fn abandons_predict_peeks_the_top_card_for_its_own_seat_and_the_other_replica_never_holds_the_face()
{
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let their_deck: Vec<CardFace> = (0..10)
        .map(|index| unit(&format!("Wisp {index}"), 1, 0, "Calm", 1))
        .collect();
    let (hand, _, theirs, _) = open_m7_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck,
            their_pool: vec![rune("Chaos"); 2],
            their_hand: vec![spell("Abandon", 2, 0, "Chaos")],
            my_hand: vec![spell("Consult the Past", 4, 0, "Mind")],
            ..M7Deal::default()
        },
    );
    let consult = hand[0];
    let abandon = theirs[0];
    deliver_private_faces(&mut host, &mut client, ada);
    assert_eq!(ready_runes(&host, 0), 4);
    assert_eq!(ready_runes(&host, ada), 2);
    let my_hand = cards_in(&host, 0, ZONE_HAND).len();

    let cast = moved(&mut host, &mut client, 0, consult, ZONE_CHAIN, 0, TOP);
    assert!(cast.prompt.is_none(), "Draw 2 chooses nothing");
    assert_eq!(cast.chain.len(), 1);
    assert_eq!(ready_runes(&host, 0), 0, "four energy paid");
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(passed.priority.map(|priority| priority.active), Some(ada));

    let asked = moved(&mut host, &mut client, ada, abandon, ZONE_CHAIN, 0, TOP);
    let prompt = asked.prompt.clone().expect("Abandon asks for the spell");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (ada, 1, 1));
    let strip = client.plugin_view(ada);
    assert_eq!(
        labels(&strip),
        [
            format!("{} on the chain", card_option(consult)),
            "cancel".to_string()
        ]
    );
    let stacked = pick(&mut host, &mut client, ada, 0);
    assert_eq!(stacked.chain.len(), 2);
    assert_eq!(
        stacked.chain[1].targets,
        [TargetRef::Item(stacked.chain[0].id)]
    );
    assert_eq!(ready_runes(&host, ada), 0, "two energy paid");

    let deck_before = cards_in(&host, ada, ZONE_MAIN_DECK);
    let top = *deck_before.last().expect("the deck is not empty");
    let top_name = face_name(&host, top);
    assert!(
        top_name.starts_with("Wisp"),
        "the host dealt it, so it knows"
    );
    assert!(face_for(&host, ada, top).is_none());
    assert!(joiner_face(&client, top).is_none());
    for seat in [0, ada] {
        assert!(
            !host
                .faces_owed_to(seat)
                .iter()
                .any(|(card, _)| *card == top),
            "a deck card is owed to nobody before the peek"
        );
    }
    assert!(host.state().peeks.is_empty());

    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let before = host.log().len();
    let predicting = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(
        zone_of(&host, consult),
        Some((0, ZONE_HAND)),
        "the countered spell goes back to its owner's hand"
    );
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), my_hand);
    assert!(client_owns(&client, 0, ZONE_HAND, consult));
    let prompt = predicting
        .prompt
        .clone()
        .expect("Predict asks its controller about the top card");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (ada, 0, 1, false)
    );
    assert_eq!(predicting.chain.len(), 1, "Abandon waits on the chain");
    assert_eq!(predicting.chain[0].kind.source(), abandon);
    assert!(
        !host.log()[before..]
            .iter()
            .any(|entry| matches!(entry.action, LogAction::Reveal { .. })),
        "a look is not a reveal: the public log carries no face"
    );
    assert_eq!(
        host.state().peeks,
        std::collections::BTreeSet::from([(top, ada)])
    );
    assert_eq!(client.state().peeks, host.state().peeks);
    assert!(!host.state().revealed.contains(&top));
    assert!(
        face_for(&host, 0, top).is_none(),
        "the host's own table (seat 0) never renders the face"
    );
    assert!(host.view().peeked.is_empty(), "seat 0's view marks no peek");
    assert_eq!(client.view().peeked, vec![top]);
    assert!(
        !face_visible_in(host.view(), top),
        "seat 0's view keeps the deck card face down"
    );
    assert!(
        face_visible_in(client.view(), top),
        "the peeking seat's view lays it face up for the looker"
    );
    let owed = host.owed_faces();
    assert_eq!(owed.len(), 1, "one private face is owed");
    assert_eq!((owed[0].0, owed[0].1 .0), (ada, top));
    assert_eq!(owed[0].1 .1.name, top_name);
    assert!(host.owed_faces().is_empty(), "sent once");
    assert!(
        !host.faces_owed_to(0).iter().any(|(card, _)| *card == top),
        "a reconnecting seat 0 is owed nothing for it"
    );
    assert!(
        host.faces_owed_to(ada).contains(&owed[0].1),
        "a reconnecting seat {ada} gets the peeked face again"
    );
    assert!(
        joiner_face(&client, top).is_none(),
        "until the private face frame lands the joiner's replica is blank too"
    );
    client.add_faces(vec![owed[0].1.clone()]);
    assert_eq!(
        joiner_face(&client, top).as_deref(),
        Some(top_name.as_str())
    );
    let strip = client.plugin_view(ada);
    assert_eq!(labels(&strip), [card_option(top), "skip".to_string()]);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(
            format!(
                "{}: choose the top card of your deck to recycle (0 of 1)",
                card_option(abandon)
            )
            .as_str()
        )
    );
    assert!(host.plugin_view(0).status.iter().any(|line| line
        == &format!(
            "waiting for {{seat {ada}}}: {}: choose the top card of your deck to recycle (0 of 1)",
            card_option(abandon)
        )));
    assert!(predicting.log.contains(&format!(
        "{{seat {ada}}} looks at the top card of their deck"
    )));
    refused(&mut host, 0, TurnEvent::Pass, Refusal::PromptOpen);
    refused(
        &mut host,
        0,
        TurnEvent::Pick(Pick {
            prompt: prompt.id,
            option: 0,
        }),
        Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NotYourPrompt { seat: ada }),
    );

    let recycled = pick(&mut host, &mut client, ada, 0);
    assert!(recycled.prompt.is_none() && recycled.chain.is_empty());
    let deck_after = cards_in(&host, ada, ZONE_MAIN_DECK);
    assert_eq!(deck_after.first(), Some(&top), "recycled to the bottom");
    assert_eq!(deck_after.len(), deck_before.len());
    assert_eq!(&deck_after[1..], &deck_before[..deck_before.len() - 1]);
    assert_eq!(
        client_cards_in(&client, ada, ZONE_MAIN_DECK),
        deck_after,
        "both replicas hold the deck in the same order"
    );
    assert!(cards_in(&host, ada, ZONE_TRASH).contains(&abandon));
    assert!(recycled.log.contains(&format!(
        "{{seat {ada}}} recycles the top card of their deck"
    )));
    assert!(recycled.log.contains(&format!(
        "{} is countered · back to hand",
        card_option(consult)
    )));
    assert!(
        host.state().peeks.contains(&(top, ada)),
        "a face once seen stays known while the card is face down"
    );
    assert!(face_for(&host, 0, top).is_none());
    assert!(
        !host
            .log()
            .iter()
            .any(|entry| matches!(entry.action, LogAction::Reveal { card, .. } if card == top)),
        "the other replica never received the face"
    );
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn smoke_and_mirrors_swaps_two_units_inside_a_showdown_and_both_replicas_fold_the_same_blob() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (hand, base, _, _) = open_m7_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck: vec![unit("Wisp", 1, 0, "Calm", 1); 10],
            my_hand: vec![
                powered_gear("Sprite Fountain", 2, 1, "Mind"),
                spell("Smoke and Mirrors", 2, 0, "Mind"),
            ],
            my_base: vec![unit("Vi", 3, 1, "Fury", 5)],
            ..M7Deal::default()
        },
    );
    let (fountain, smoke) = (hand[0], hand[1]);
    let vi = base[0];
    assert_eq!(ready_runes(&host, 0), 4);

    moved(&mut host, &mut client, 0, fountain, ZONE_BASE, 0, TOP);
    let spawned = drain_priority(&mut host, &mut client);
    assert!(spawned.chain.is_empty() && spawned.prompt.is_none());
    let sprite = faces_named(&host, 0, "Sprite")[0];
    assert_eq!(zone_of(&host, sprite), Some((0, ZONE_BASE)));
    assert!(!exhausted(&host, sprite));
    assert_eq!(ready_runes(&host, 0), 2);

    let marching = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    let opened = if marching.prompt.is_some() {
        let strip = host.plugin_view(0);
        assert!(
            strip
                .prompt
                .as_ref()
                .is_some_and(|summary| summary.why.starts_with("move others to")),
            "the ready Sprite could come along"
        );
        let done = option_index(&strip, "done");
        pick(&mut host, &mut client, 0, done)
    } else {
        marching
    };
    let showdown = opened.showdown.clone().expect("a showdown opens");
    assert_eq!(
        (showdown.zone, showdown.combat, showdown.focus()),
        (bf1, false, 0)
    );
    assert_eq!(opened.contester(bf1), Some(0));
    assert_eq!(
        opened.holder(bf1),
        None,
        "nobody held the empty battlefield"
    );
    assert_eq!(zone_of(&host, vi), Some((0, bf1)));
    assert_eq!(zone_of(&host, sprite), Some((0, ZONE_BASE)));
    let hand_size = cards_in(&host, 0, ZONE_HAND).len();

    let asked = moved(&mut host, &mut client, 0, smoke, ZONE_CHAIN, 0, TOP);
    let prompt = asked.prompt.clone().expect("the first unit is chosen");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (0, 1, 1, true)
    );
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(format!("{}: choose a unit you control (0 of 1)", card_option(smoke)).as_str())
    );
    let mut offered = labels(&strip);
    offered.sort_unstable();
    let mut expected = [
        card_option(vi),
        card_option(sprite),
        "cancel".to_string(),
        "free table".to_string(),
    ];
    expected.sort_unstable();
    assert_eq!(
        offered, expected,
        "both units are offered, the Sprite included"
    );
    let second = pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(sprite)),
    );
    let prompt = second.prompt.clone().expect("the other unit is chosen");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(
            format!(
                "{}: choose another unit you control at a different location (0 of 1)",
                card_option(smoke)
            )
            .as_str()
        )
    );
    assert_eq!(
        labels(&strip),
        [
            card_option(vi),
            "cancel".to_string(),
            "free table".to_string()
        ],
        "the Sprite itself is at the same location as itself"
    );
    let stacked = pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(vi)),
    );
    assert!(stacked.prompt.is_none());
    assert_eq!(stacked.chain.len(), 1);
    assert_eq!(
        stacked.chain[0].targets,
        [TargetRef::Card(sprite), TargetRef::Card(vi)]
    );
    assert_eq!(
        ready_runes(&host, 0),
        0,
        "two energy paid inside the showdown"
    );
    assert!(
        stacked.showdown.is_some(),
        "an Action inside a showdown does not close it"
    );
    let arrows = seen_by(&mut host, &mut client, ada).arrows;
    assert!(
        arrows
            .iter()
            .filter(|arrow| arrow.kind == ArrowKind::Spell)
            .count()
            >= 2,
        "both seats see the spell aimed at both units: {arrows:?}"
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let before = host.log().len();
    let swapped = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(swapped.chain.is_empty() && swapped.prompt.is_none());
    let burst = &host.log()[before..];
    assert_eq!(
        burst.len(),
        1,
        "the pass that resolves the spell is one entry: both moves ride its effects as one batch"
    );
    assert!(matches!(burst[0].action, LogAction::Game { .. }));
    assert_eq!(zone_of(&host, sprite), Some((0, bf1)));
    assert_eq!(zone_of(&host, vi), Some((0, ZONE_BASE)));
    assert!(client_owns(&client, 0, bf1, sprite));
    assert!(client_owns(&client, 0, ZONE_BASE, vi));
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_size - 1 + 1,
        "Smoke and Mirrors leaves the hand and one card is drawn"
    );
    assert!(swapped.log.iter().any(|line| line.starts_with(&format!(
        "{} and {} swap places",
        card_option(sprite),
        card_option(vi)
    ))));
    let still = swapped
        .showdown
        .clone()
        .expect("the showdown at the battlefield stays open");
    assert_eq!((still.zone, still.combat), (bf1, false));
    assert_eq!(swapped.contester(bf1), Some(0));
    assert!(swapped.staged.is_empty(), "no second contest was staged");
    assert_eq!(cards_in(&host, 0, ZONE_TRASH), [smoke]);

    let settled = drain_priority(&mut host, &mut client);
    assert!(settled.showdown.is_none() && settled.prompt.is_none());
    assert_eq!(
        settled.holder(bf1),
        Some(0),
        "the Sprite that arrived by the swap conquers"
    );
    assert!(settled.scored(bf1, 0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(units_in(&host, 0, bf1), [sprite]);
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

#[test]
fn hweis_discard_by_numbered_pick_parks_until_the_host_reveals_and_the_spell_branch_draws_on_both_replicas(
) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let (hand, base, _, _) = open_m7_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M7Deal {
            their_deck: vec![unit("Wisp", 1, 0, "Calm", 1); 10],
            my_hand: vec![spell("Zap", 1, 0, "Mind")],
            my_base: vec![unit("Hwei - Brooding Painter", 5, 1, "Mind", 5)],
            ..M7Deal::default()
        },
    );
    let zap = hand[0];
    let hwei = base[0];
    let hand_before = cards_in(&host, 0, ZONE_HAND).len();
    let deck_before = cards_in(&host, 0, ZONE_MAIN_DECK).len();

    let marched = moved(&mut host, &mut client, 0, hwei, bf1, 0, TOP);
    assert!(
        marched.prompt.is_none(),
        "nothing else is ready to come along"
    );
    assert_eq!(
        marched.chain.len(),
        1,
        "322.12 · her Move trigger is finalized"
    );
    assert!(matches!(marched.chain[0].kind, ItemKind::Trigger { source, .. } if source == hwei));
    assert!(
        marched.showdown.is_none() && marched.staged.len() == 1,
        "the showdown waits for the chain"
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let asked = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let prompt = asked.prompt.clone().expect("draw 1, then discard 1 asks");
    assert_eq!(prompt.seat, 0);
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_before + 1,
        "the draw lands before the discard is asked"
    );
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("discard a card")
    );
    let offered = labels(&strip);
    assert!(
        offered.contains(&card_option(zap)),
        "the hand is offered by number: {offered:?}"
    );
    let theirs = host.plugin_view(ada);
    assert!(
        theirs
            .status
            .contains(&"waiting for {seat 0}: discard a card".to_string()),
        "{:?}",
        theirs.status
    );
    assert!(
        !labels(&theirs).contains(&card_option(zap)),
        "the other seat is offered nothing from that hand"
    );
    assert!(
        !host.state().revealed.contains(&zap),
        "the pick names a card whose face the plugin has not seen"
    );

    let before = host.log().len();
    let picked = pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(zap)),
    );
    let appended: Vec<&LogAction> = host.log()[before..]
        .iter()
        .map(|entry| &entry.action)
        .collect();
    assert_eq!(appended.len(), 2, "{appended:?}");
    assert!(matches!(appended[0], LogAction::Game { .. }));
    assert!(
        matches!(appended[1], LogAction::Reveal { card, .. } if *card == zap),
        "the host reveals the parked pick from the discarder's seat"
    );
    assert_eq!(host.log()[before + 1].seat, 0);
    assert!(picked.prompt.is_none());
    assert!(picked.chain.is_empty());
    assert_eq!(zone_of(&host, zap), Some((0, ZONE_TRASH)));
    assert!(picked
        .log
        .contains(&format!("{{seat 0}} discards {}", card_option(zap))));
    assert!(picked
        .log
        .contains(&format!("{} · a Spell · draw 1", card_option(hwei))));
    assert_eq!(
        cards_in(&host, 0, ZONE_HAND).len(),
        hand_before + 1,
        "draw, discard, draw again"
    );
    assert_eq!(cards_in(&host, 0, ZONE_MAIN_DECK).len(), deck_before - 2);
    assert_eq!(
        client_cards_in(&client, 0, ZONE_HAND).len(),
        hand_before + 1
    );
    assert!(client.state().revealed.contains(&zap));
    let showdown = picked.showdown.clone().expect("then the showdown opens");
    assert_eq!((showdown.zone, showdown.combat), (bf1, false));
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
    assert_eq!(
        host.state().plugin_state,
        client.state().plugin_state,
        "byte-identical blobs on both replicas"
    );
}

#[test]
fn a_two_point_table_with_three_battlefields_stages_three_and_ends_at_two() {
    let options = TableOptions {
        victory_score: 2,
        battlefields: 3,
    };
    let (mut host, mut client, ada) = open_table_with(Some(options));
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let bf3 = ZONE_BATTLEFIELD_FIRST + 2;
    assert_eq!(
        TableOptions::of_config(host.state().options.as_ref()),
        options,
        "the genesis carries the options"
    );
    assert_eq!(
        TableOptions::of_config(client.state().options.as_ref()),
        options,
        "and the replica folds them"
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![unit("Scout", 1, 0, "Mind", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![unit("Wisp", 1, 0, "Calm", 1); 10],
        ZONE_MAIN_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![rune("Mind"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![rune("Calm"); 8],
        ZONE_RUNE_DECK,
    );
    deal(
        &mut host,
        &mut client,
        0,
        vec![battlefield("Crossroads")],
        bf1,
    );
    deal(
        &mut host,
        &mut client,
        ada,
        vec![battlefield("Sanctum")],
        bf2,
    );
    deal(&mut host, &mut client, 0, vec![battlefield("Quarry")], bf3);
    let base = deal(
        &mut host,
        &mut client,
        0,
        vec![
            unit("Vi", 3, 1, "Mind", 6),
            unit("Caitlyn", 3, 1, "Mind", 4),
            unit("Jayce", 3, 1, "Mind", 3),
        ],
        ZONE_BASE,
    );
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        &mut host,
        &mut client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(
        &mut host,
        &mut client,
        TurnEvent::StartGame { first_player: 0 },
    );
    let keep = cards_in(&host, 0, ZONE_HAND).len() as u16;
    pick(&mut host, &mut client, 0, keep);
    let keep = cards_in(&host, ada, ZONE_HAND).len() as u16;
    let started = pick(&mut host, &mut client, ada, keep);
    assert_eq!(started.phase(), Some(Phase::Action));

    let conquer = |host: &mut HostSession, client: &mut ClientSession, card: u32, zone: u16| {
        let mut marched = moved(host, client, 0, card, zone, 0, TOP);
        if marched.prompt.is_some() {
            let strip = host.plugin_view(0);
            let alone = labels(&strip)
                .iter()
                .position(|label| label == "done")
                .expect("the companion prompt closes with done");
            marched = pick(host, client, 0, alone as u16);
        }
        assert_eq!(
            marched.showdown.as_ref().map(|showdown| showdown.zone),
            Some(zone),
            "a move to zone {zone} opens a showdown there"
        );
        turn_event(host, client, 0, TurnEvent::Pass);
        turn_event(host, client, ada, TurnEvent::Pass)
    };
    let first = conquer(&mut host, &mut client, base[0], bf1);
    assert_eq!(points(&host, 0), 1);
    assert!(first.scored(bf1, 0));
    assert_eq!(host.plugin_view(0).winner, None);

    let hand = cards_in(&host, 0, ZONE_HAND).len();
    let second = conquer(&mut host, &mut client, base[1], bf2);
    assert_eq!(
        points(&host, 0),
        1,
        "448.1.b at one of two: the third battlefield is unscored, so the conquer draws"
    );
    assert!(second.scored(bf2, 0));
    assert_eq!(cards_in(&host, 0, ZONE_HAND).len(), hand + 1);
    assert!(second.log.contains(&format!(
        "{{seat 0}} conquers {{zone {bf2}}} · draws instead of the final point"
    )));
    assert_eq!(host.plugin_view(0).winner, None);

    let third = conquer(&mut host, &mut client, base[2], bf3);
    assert_eq!(
        points(&host, 0),
        2,
        "every battlefield of the three scored this turn, so the final point lands"
    );
    assert!(third.scored(bf3, 0));
    assert_eq!(third.log.last().unwrap(), "{seat 0} wins with 2 points");
    let strip = host.plugin_view(0);
    assert_eq!(strip.winner, Some(0));
    assert!(strip
        .status
        .contains(&"{seat 0} wins with 2 points".to_string()));
    assert_eq!(client.plugin_view(ada).winner, Some(0));
    assert_eq!(host.log(), client.log());
    assert_eq!(host.view().plugin_state, client.view().plugin_state);
}

fn xp(host: &HostSession, seat: u8) -> i32 {
    host.state()
        .counter(CounterTarget::Seat(seat), agni_riftbound::COUNTER_XP)
        .unwrap_or(0)
}

fn client_xp(client: &ClientSession, seat: u8) -> i32 {
    client
        .state()
        .counter(CounterTarget::Seat(seat), agni_riftbound::COUNTER_XP)
        .unwrap_or(0)
}

fn same_bytes(host: &HostSession, client: &ClientSession, when: &str) {
    assert_eq!(host.log(), client.log(), "{when}: the logs agree");
    assert_eq!(
        host.state().plugin_state,
        client.state().plugin_state,
        "{when}: byte-identical blobs on both replicas"
    );
    assert_eq!(
        host.view().plugin_state,
        client.view().plugin_state,
        "{when}: the views carry the same bytes"
    );
}

#[derive(Default)]
struct M9Deal {
    my_deck: Vec<CardFace>,
    my_pool: Vec<CardFace>,
    my_hand: Vec<CardFace>,
    my_base: Vec<CardFace>,
    their_deck: Vec<CardFace>,
    their_pool: Vec<CardFace>,
    their_hand: Vec<CardFace>,
    their_garrison: Vec<CardFace>,
    my_battlefield: Option<String>,
}

struct M9Table {
    hand: Vec<u32>,
    base: Vec<u32>,
    theirs: Vec<u32>,
    garrison: Vec<u32>,
}

fn open_m9_game(
    host: &mut HostSession,
    client: &mut ClientSession,
    ada: u8,
    winner: u8,
    deals: M9Deal,
) -> M9Table {
    let M9Deal {
        my_deck,
        my_pool,
        my_hand,
        my_base,
        their_deck,
        their_pool,
        their_hand,
        their_garrison,
        my_battlefield,
    } = deals;
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let my_deck = if my_deck.is_empty() {
        vec![unit("Scout", 1, 0, "Mind", 1); 14]
    } else {
        my_deck
    };
    let their_deck = if their_deck.is_empty() {
        vec![unit("Wisp", 1, 0, "Calm", 1); 14]
    } else {
        their_deck
    };
    deal(host, client, 0, my_deck, ZONE_MAIN_DECK);
    deal(host, client, ada, their_deck, ZONE_MAIN_DECK);
    deal(host, client, 0, vec![rune("Mind"); 8], ZONE_RUNE_DECK);
    deal(host, client, ada, vec![rune("Calm"); 8], ZONE_RUNE_DECK);
    let my_pool = if my_pool.is_empty() {
        vec![rune("Mind"); 2]
    } else {
        my_pool
    };
    deal(host, client, 0, my_pool, ZONE_RUNE_POOL);
    if !their_pool.is_empty() {
        deal(host, client, ada, their_pool, ZONE_RUNE_POOL);
    }
    deal(
        host,
        client,
        0,
        vec![battlefield(
            my_battlefield.as_deref().unwrap_or("Crossroads"),
        )],
        bf1,
    );
    deal(host, client, ada, vec![battlefield("Sanctum")], bf2);
    let theirs = if their_hand.is_empty() {
        Vec::new()
    } else {
        deal(host, client, ada, their_hand, ZONE_HAND)
    };
    let hand = if my_hand.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_hand, ZONE_HAND)
    };
    let base = if my_base.is_empty() {
        Vec::new()
    } else {
        deal(host, client, 0, my_base, ZONE_BASE)
    };
    let garrison = if their_garrison.is_empty() {
        Vec::new()
    } else {
        deal(host, client, ada, their_garrison, bf1)
    };
    let act = |host: &mut HostSession, client: &mut ClientSession, event: TurnEvent| {
        if winner == 0 {
            from_host(host, client, event);
        } else {
            from_client(host, client, ada, event);
        }
    };
    act(
        host,
        client,
        TurnEvent::SetMode {
            mode: Mode::Enforced,
        },
    );
    act(host, client, TurnEvent::StartGame { first_player: 0 });
    let keep = cards_in(host, 0, ZONE_HAND).len() as u16;
    pick(host, client, 0, keep);
    let keep = cards_in(host, ada, ZONE_HAND).len() as u16;
    let started = pick(host, client, ada, keep);
    assert_eq!((started.turn(), started.turn_player()), (1, 0));
    assert_eq!(started.phase(), Some(Phase::Action));
    M9Table {
        hand,
        base,
        theirs,
        garrison,
    }
}

#[test]
fn native_registered_warmog_and_gardens_survive_saved_lent_continuations() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_pool: vec![rune("Body"); 2],
            my_hand: vec![spell("Spark", 1, 0, "Mind")],
            my_base: vec![
                unit("Tail-Cloaked Matriarch", 2, 0, "Chaos", 4),
                gear("Warmog's Armor", 0, "Body"),
            ],
            my_battlefield: Some("Gardens of Becoming".into()),
            ..M9Deal::default()
        },
    );
    let matriarch = named(&host, 0, ZONE_BASE, "Tail-Cloaked Matriarch");
    let warmog = named(&host, 0, ZONE_BASE, "Warmog's Armor");
    assert_eq!(ready_runes(&host, 0), 4, "the opening channel is included");
    let might_before_equip = might(&host, matriarch);
    assert_eq!(
        host.state()
            .counter(CounterTarget::Card(matriarch), COUNTER_BUFFED),
        Some(0)
    );

    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: warmog,
            ability: 0,
        },
    );
    let strip = host.plugin_view(0);
    pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(matriarch)),
    );
    let equipped = drain_priority(&mut host, &mut client);
    let equipped_state = equipped.card_state(warmog).expect("Warmog state");
    assert_eq!(equipped_state.attached_to, Some(matriarch));
    assert_eq!(might(&host, matriarch), might_before_equip + 1);
    assert_eq!(ready_runes(&host, 0), 3, "Equip recycles one Body rune");
    let might_before_conquer = might(&host, matriarch);

    moved(
        &mut host,
        &mut client,
        0,
        matriarch,
        ZONE_BATTLEFIELD_FIRST,
        0,
        TOP,
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let conquered = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(
        conquered.chain.iter().any(|item| matches!(
            item.kind,
            ItemKind::Granted { holder, lender, .. } if holder == matriarch && lender == warmog
        )),
        "the registered Warmog text is a Granted item before priority drains"
    );
    let granted_at = host.log().len();
    let _ = drain_priority(&mut host, &mut client);
    assert_eq!(might(&host, matriarch), might_before_conquer);
    assert_eq!(
        host.state()
            .counter(CounterTarget::Card(matriarch), COUNTER_BUFFED),
        Some(1)
    );
    let gardens = named(&host, 0, ZONE_BATTLEFIELD_FIRST, "Gardens of Becoming");

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let next = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((next.turn(), next.turn_player()), (3, 0));
    let xp_before = xp(&host, 0);
    let post_turn_view = host.plugin_view(0);
    let gain_xp = post_turn_view
        .affordances
        .iter()
        .find(|affordance| affordance.label.contains("gain 1 XP"))
        .expect("Gardens grants its XP ability");
    let gain_xp_ability = gain_xp.data[5];
    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: matriarch,
            ability: gain_xp_ability,
        },
    );
    let saved_at = host.log().len();
    let lent = blob_of(&host, &client);
    assert!(lent.chain.iter().any(|item| matches!(
        item.kind,
        ItemKind::Lent { holder, lender, .. } if holder == matriarch && lender == gardens
    )));
    let saved = lent.encode();
    let saved_blob = GameBlob::decode(&saved).expect("saved Lent blob");
    assert_eq!(saved_blob.chain, lent.chain);
    let resolved = drain_priority(&mut host, &mut client);
    assert_eq!(xp(&host, 0), xp_before + 1);
    assert!(exhausted(&host, matriarch));
    assert!(resolved.chain.is_empty());
    assert_eq!(host.log(), client.log());
    let entries = host.log().to_vec();
    assert_plugin_replay_parity(
        &entries,
        &encode_state(host.state()),
        "registered Gardens and Warmog",
    );
    let expected = encode_state(host.state());
    assert_registered_cold_suffix(&entries, granted_at, &expected, false);
    assert_registered_cold_suffix(&entries, saved_at, &expected, true);
}

#[test]
fn a_hunt_conquer_and_two_holds_reach_six_xp_and_level_six_switches_ganking_on_for_both_replicas() {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let bf2 = ZONE_BATTLEFIELD_FIRST + 1;
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_base: vec![unit("Master Yi - Tempered", 4, 0, "Body", 4)],
            ..M9Deal::default()
        },
    );
    let yi = table.base[0];
    assert_eq!(xp(&host, 0), 0);

    let marched = moved(&mut host, &mut client, 0, yi, bf1, 0, TOP);
    assert!(marched.prompt.is_none(), "Yi marches alone");
    assert_eq!(
        marched.showdown.as_ref().map(|showdown| showdown.zone),
        Some(bf1)
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let conquered = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert_eq!(conquered.holder(bf1), Some(0));
    assert_eq!(points(&host, 0), 1);
    assert_eq!(
        conquered.chain.len(),
        1,
        "823.1.b · the conquer puts the Hunt trigger on the chain"
    );
    assert!(matches!(
        conquered.chain[0].kind,
        ItemKind::Trigger { source, index } if source == yi && index == agni_riftbound_turns::cards::IMPLICIT_HUNT
    ));
    assert_eq!(xp(&host, 0), 0, "nothing until the trigger resolves");
    let hunted = drain_priority(&mut host, &mut client);
    assert!(hunted.chain.is_empty() && hunted.prompt.is_none());
    assert_eq!(xp(&host, 0), 2, "Hunt 2 on the conquer");
    assert_eq!(client_xp(&client, 0), 2, "the replica folds the XP counter");
    assert!(hunted
        .log
        .contains(&format!("{{card {yi}}} hunts · {{seat 0}} gains 2 XP")));
    assert!(
        host.plugin_view(0)
            .status
            .iter()
            .any(|line| line.contains("2 XP")),
        "{:?}",
        host.plugin_view(0).status
    );
    same_bytes(&host, &client, "after the conquer");

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((theirs.turn(), theirs.turn_player()), (3, 0));
    let held = drain_priority(&mut host, &mut client);
    assert!(held.chain.is_empty());
    assert_eq!(points(&host, 0), 2, "the hold scores");
    assert_eq!(xp(&host, 0), 4, "823.1.b · and hunts too");
    assert!(held.phase() == Some(Phase::Action));
    assert!(!exhausted(&host, yi), "the Awaken step readied him");
    move_refused(
        &mut host,
        0,
        yi,
        bf2,
        0,
        Refusal::Illegal(Reason::NeedsGanking),
    );
    assert!(
        !host
            .plugin_view(0)
            .legal
            .iter()
            .any(|row| row.card == yi && row.zones.contains(&bf2)),
        "at 4 XP the highlight rows do not offer the other battlefield"
    );
    same_bytes(&host, &client, "at four XP");

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let again = turn_event(&mut host, &mut client, ada, TurnEvent::EndTurn);
    assert_eq!((again.turn(), again.turn_player()), (5, 0));
    let leveled = drain_priority(&mut host, &mut client);
    assert!(leveled.chain.is_empty());
    assert_eq!(xp(&host, 0), 6, "the second hold reaches Level 6");
    assert_eq!(client_xp(&client, 0), 6);
    assert!(
        host.plugin_view(0)
            .legal
            .iter()
            .any(|row| row.card == yi && row.zones.contains(&bf2)),
        "Level 6 grants Ganking: the other battlefield is a legal march"
    );
    let ganked = moved(&mut host, &mut client, 0, yi, bf2, 0, TOP);
    assert_eq!(
        ganked.showdown.as_ref().map(|showdown| showdown.zone),
        Some(bf2),
        "a battlefield-to-battlefield march opens the showdown there"
    );
    assert_eq!(zone_of(&host, yi), Some((0, bf2)));
    assert!(client_owns(&client, 0, bf2, yi));
    let scored = drain_priority(&mut host, &mut client);
    assert_eq!(scored.holder(bf2), Some(0));
    assert_eq!(points(&host, 0), 4, "two holds and two conquers");
    assert_eq!(xp(&host, 0), 8, "and the third conquer hunts again");
    assert_eq!(client_xp(&client, 0), 8);
    assert_eq!(xp(&host, ada), 0);
    same_bytes(&host, &client, "at the end");
}

#[test]
fn an_ambush_into_an_open_combat_joins_the_defenders_and_its_defend_trigger_fires_on_both_replicas()
{
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_base: vec![unit("Vi", 3, 1, "Fury", 5)],
            their_pool: vec![rune("Chaos"); 5],
            their_hand: vec![unit("Kha'Zix - Mutating Horror", 4, 1, "Chaos", 4)],
            their_garrison: vec![unit("Wisp", 1, 0, "Calm", 1)],
            ..M9Deal::default()
        },
    );
    let vi = table.base[0];
    let horror = table.theirs[0];
    let wisp = table.garrison[0];
    let started = blob_of(&host, &client);
    assert_eq!(started.holder(bf1), Some(ada), "the garrison holds it");
    assert_eq!(ready_runes(&host, ada), 5);

    move_refused(&mut host, ada, horror, bf1, ada, Refusal::NotYourTurn);

    let staged = moved(&mut host, &mut client, 0, vi, bf1, 0, TOP);
    assert!(staged.prompt.is_none(), "Vi marches alone");
    let showdown = staged.showdown.clone().expect("a combat opens at once");
    assert_eq!(
        (
            showdown.zone,
            showdown.attacker,
            showdown.defender,
            showdown.combat
        ),
        (bf1, 0, ada, true)
    );
    assert_eq!(showdown.focus(), 0);
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(
        passed.showdown.as_ref().map(|showdown| showdown.focus()),
        Some(ada)
    );
    deliver_private_faces(&mut host, &mut client, ada);
    let strip = client.plugin_view(ada);
    let row = strip
        .legal
        .iter()
        .find(|row| row.card == horror)
        .unwrap_or_else(|| {
            panic!(
                "the Horror is a legal play for its owner inside the combat: {:?}",
                strip.legal
            )
        });
    assert!(
        row.kinds.contains(&LegalKind::React),
        "Ambush is a Reaction play: {:?}",
        row.kinds
    );
    assert!(
        row.zones.contains(&bf1) && !row.zones.contains(&ZONE_BASE),
        "only the battlefield with friendly units is open, never the base: {:?}",
        row.zones
    );

    let ambushed = moved(&mut host, &mut client, ada, horror, bf1, ada, TOP);
    assert!(
        ambushed.prompt.is_none(),
        "the location came with the drag and the runes pay"
    );
    assert_eq!(
        zone_of(&host, horror),
        Some((0, bf1)),
        "a shared zone normalizes to seat 0"
    );
    assert!(client_owns(&client, 0, bf1, horror));
    assert!(exhausted(&host, horror), "a played unit enters exhausted");
    assert_eq!(
        ready_runes(&host, ada),
        1,
        "four runes exhausted for the energy, one of them recycled for the Chaos power"
    );
    assert_eq!(cards_in(&host, ada, ZONE_RUNE_POOL).len(), 4);
    assert!(
        host.state()
            .annotation(horror, "defender")
            .is_some_and(|value| value == [1]),
        "the ambusher is designated a defender in the open combat"
    );
    assert!(
        ambushed
            .showdown
            .as_ref()
            .is_some_and(|showdown| showdown.zone == bf1 && showdown.combat),
        "the combat stays open"
    );
    assert_eq!(
        ambushed.chain.len(),
        1,
        "When I defend, if an enemy unit is alone here"
    );
    assert!(matches!(
        ambushed.chain[0].kind,
        ItemKind::Trigger { source, .. } if source == horror
    ));
    assert_eq!(xp(&host, ada), 0, "nothing until the trigger resolves");
    assert_eq!(might(&host, horror), 0, "no bonus yet");
    same_bytes(&host, &client, "with the trigger on the chain");

    let resolved = drain_priority(&mut host, &mut client);
    assert_eq!(
        might(&host, horror),
        2,
        "+2 this turn on top of the printed 4"
    );
    assert_eq!(xp(&host, ada), 2, "and 2 XP");
    assert_eq!(client_xp(&client, ada), 2);
    assert!(resolved
        .log
        .contains(&format!("{{card {horror}}} gets +2 this turn")));
    assert!(resolved.log.contains(&"{seat 1} gains 2 XP".to_string()));
    let prompt = resolved
        .prompt
        .clone()
        .expect("the attacking seat orders the damage");
    assert_eq!(prompt.seat, 0);
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some("assign 5 damage: who takes lethal next?")
    );
    assert_eq!(
        labels(&strip),
        [
            format!("{{card {wisp}}} (lethal 1)"),
            format!("{{card {horror}}} (lethal 6)"),
            "free table".into()
        ]
    );
    let closed = pick(&mut host, &mut client, 0, 0);
    assert!(closed.showdown.is_none() && closed.prompt.is_none());
    assert_eq!(zone_of(&host, wisp), Some((ada, ZONE_TRASH)));
    assert_eq!(
        zone_of(&host, vi),
        Some((0, ZONE_TRASH)),
        "one from the Wisp and six from the Horror"
    );
    assert_eq!(units_in(&host, 0, bf1), [horror]);
    assert!(client_owns(&client, 0, bf1, horror));
    assert_eq!(closed.holder(bf1), Some(ada), "the defenders keep it");
    assert_eq!(points(&host, 0), 0);
    assert!(closed.log.iter().any(|line| line
        == &format!(
            "{{card {wisp}}} takes 1 · {{card {horror}}} takes 4 · {{card {vi}}} takes 7"
        )));
    assert_eq!(
        host.state().annotation(horror, "defender"),
        None,
        "466.7.a clears the designations"
    );
    same_bytes(&host, &client, "after the combat");
}

#[test]
fn time_warp_takes_an_extra_turn_and_onslaught_flows_back_from_the_trash_into_banishment_on_both_replicas(
) {
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_pool: vec![rune("Mind"); 12],
            my_hand: vec![
                spell("Time Warp", 10, 4, "Mind"),
                spell("Onslaught", 4, 0, "Body"),
            ],
            my_base: vec![unit("Vi", 3, 1, "Fury", 5)],
            ..M9Deal::default()
        },
    );
    let (warp, onslaught) = (table.hand[0], table.hand[1]);
    let vi = table.base[0];
    assert_eq!(ready_runes(&host, 0), 14, "twelve dealt and two channelled");
    assert!(cards_in(&host, 0, ZONE_BANISHMENT).is_empty());

    let cast = moved(&mut host, &mut client, 0, warp, ZONE_CHAIN, 0, TOP);
    assert!(cast.prompt.is_none(), "nothing to choose");
    assert_eq!(cast.chain.len(), 1);
    assert_eq!(
        cards_in(&host, 0, ZONE_RUNE_POOL).len(),
        10,
        "four Mind runes recycled for the power"
    );
    assert_eq!(ready_runes(&host, 0), 4);
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let warped = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(warped.chain.is_empty());
    assert_eq!(
        cards_in(&host, 0, ZONE_BANISHMENT),
        [warp],
        "Time Warp banishes itself"
    );
    assert_eq!(client_cards_in(&client, 0, ZONE_BANISHMENT), [warp]);
    assert!(cards_in(&host, 0, ZONE_TRASH).is_empty());
    assert!(warped
        .log
        .contains(&"{seat 0} will take a turn after this one".to_string()));
    assert!(warped.log.contains(&format!("{{card {warp}}} is banished")));
    same_bytes(&host, &client, "with the extra turn queued");

    let extra = turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    assert_eq!(
        (extra.turn(), extra.turn_player()),
        (2, 0),
        "the extra turn is seat 0's, right after this one"
    );
    assert_eq!(extra.phase(), Some(Phase::Action));
    assert!(extra
        .log
        .contains(&"{seat 0} takes an extra turn".to_string()));
    assert_eq!(
        ready_runes(&host, 0),
        13,
        "readied, and turn two channels three whoever takes it"
    );
    refused(&mut host, ada, TurnEvent::EndTurn, Refusal::NotYourTurn);

    let asked = moved(&mut host, &mut client, 0, onslaught, ZONE_CHAIN, 0, TOP);
    let prompt = asked.prompt.clone().expect("a unit to give +6");
    assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(
            format!(
                "{}: choose a unit to give +6 (0 of 1)",
                card_option(onslaught)
            )
            .as_str()
        )
    );
    pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(vi)),
    );
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let charged = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(charged.chain.is_empty());
    assert_eq!(might(&host, vi), 6);
    assert_eq!(
        cards_in(&host, 0, ZONE_TRASH),
        [onslaught],
        "played from hand it lands in the trash"
    );
    assert_eq!(ready_runes(&host, 0), 9);

    let strip = host.plugin_view(0);
    let offer = strip
        .affordances
        .iter()
        .find(|affordance| affordance.card == Some(onslaught))
        .expect("829 · the Flow spell in the trash is offered as a play");
    assert_eq!(
        offer.label,
        format!(
            "{}: play from your trash (4 energy)",
            card_option(onslaught)
        )
    );
    assert!(offer.enabled);
    assert!(
        !client
            .plugin_view(ada)
            .affordances
            .iter()
            .any(|affordance| affordance.card == Some(onslaught)),
        "the other seat is offered nothing from that trash"
    );
    from_host(
        &mut host,
        &mut client,
        TurnEvent::Activate {
            source: onslaught,
            ability: agni_riftbound_turns::cards::IMPLICIT_FLOW,
        },
    );
    let flowing = blob_of(&host, &client);
    let prompt = flowing
        .prompt
        .clone()
        .expect("the Flow play chooses its unit too");
    assert_eq!(prompt.seat, 0);
    assert!(
        cards_in(&host, 0, ZONE_CHAIN).contains(&onslaught),
        "the card left the trash for the chain"
    );
    assert!(client_cards_in(&client, 0, ZONE_CHAIN).contains(&onslaught));
    let strip = host.plugin_view(0);
    let stacked = pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(vi)),
    );
    assert_eq!(stacked.chain.len(), 1);
    assert_eq!(stacked.chain[0].targets, [TargetRef::Card(vi)]);
    assert_eq!(ready_runes(&host, 0), 5, "the Flow cost is paid");
    same_bytes(&host, &client, "with the Flow play on the chain");
    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let flowed = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    assert!(flowed.chain.is_empty());
    assert_eq!(might(&host, vi), 12, "+6 twice");
    assert_eq!(
        cards_in(&host, 0, ZONE_BANISHMENT),
        [warp, onslaught],
        "829.1.b · a Flow play leaving the chain is banished"
    );
    assert_eq!(
        client_cards_in(&client, 0, ZONE_BANISHMENT),
        [warp, onslaught]
    );
    assert!(cards_in(&host, 0, ZONE_TRASH).is_empty());
    assert!(
        !host
            .plugin_view(0)
            .affordances
            .iter()
            .any(|affordance| affordance.card == Some(onslaught)),
        "banished, it is offered no more"
    );

    turn_event(&mut host, &mut client, 0, TurnEvent::EndTurn);
    let theirs = blob_of(&host, &client);
    assert_eq!(
        (theirs.turn(), theirs.turn_player()),
        (3, ada),
        "737 · the rotation resumes after the extra turn"
    );
    assert_eq!(might(&host, vi), 0, "this turn is over");
    same_bytes(&host, &client, "at the end");
}

fn damage_on(host: &HostSession, card: u32) -> i32 {
    host.state()
        .counter(CounterTarget::Card(card), agni_riftbound::COUNTER_DAMAGE)
        .unwrap_or(0)
}

#[test]
fn unyielding_spirit_prevents_every_point_of_an_alpha_strike_and_no_xp_is_gained_on_either_replica()
{
    let (mut host, mut client, ada) = open_table();
    let winner = roll_for_first(&mut host, &mut client, ada);
    let bf1 = ZONE_BATTLEFIELD_FIRST;
    let table = open_m9_game(
        &mut host,
        &mut client,
        ada,
        winner,
        M9Deal {
            my_pool: vec![rune("Body"); 2],
            my_hand: vec![spell("Alpha Strike", 3, 1, "Body")],
            my_base: vec![unit("Vi", 3, 1, "Fury", 5)],
            their_pool: vec![rune("Body"); 2],
            their_hand: vec![spell("Unyielding Spirit", 1, 1, "Body")],
            their_garrison: vec![unit("Wisp", 1, 0, "Calm", 1), unit("Jinx", 3, 1, "Calm", 2)],
            ..M9Deal::default()
        },
    );
    let strike = table.hand[0];
    let vi = table.base[0];
    let spirit = table.theirs[0];
    let (wisp, jinx) = (table.garrison[0], table.garrison[1]);
    assert_eq!(ready_runes(&host, 0), 4);
    assert_eq!(ready_runes(&host, ada), 2);

    let asked = moved(&mut host, &mut client, 0, strike, ZONE_CHAIN, 0, TOP);
    let prompt = asked.prompt.clone().expect("a friendly unit is chosen");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (0, 1, 1, true)
    );
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(format!("{}: choose a friendly unit (0 of 1)", card_option(strike)).as_str())
    );
    let stacked = pick(
        &mut host,
        &mut client,
        0,
        option_index(&strip, &card_option(vi)),
    );
    assert!(stacked.prompt.is_none());
    assert_eq!(stacked.chain.len(), 1);
    assert_eq!(stacked.chain[0].targets, [TargetRef::Card(vi)]);
    assert_eq!(ready_runes(&host, 0), 1, "three energy, one Body power");
    let passed = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(passed.priority.map(|priority| priority.active), Some(ada));

    let reacted = moved(&mut host, &mut client, ada, spirit, ZONE_CHAIN, 0, TOP);
    assert!(reacted.prompt.is_none(), "the Spirit chooses nothing");
    assert_eq!(reacted.chain.len(), 2);
    assert_eq!(ready_runes(&host, ada), 1);
    same_bytes(&host, &client, "with both spells on the chain");
    turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let shielded = turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    assert_eq!(shielded.chain.len(), 1, "the Spirit resolved first");
    assert_eq!(shielded.chain[0].kind.source(), strike);
    assert_eq!(zone_of(&host, spirit), Some((ada, ZONE_TRASH)));
    assert!(shielded.log.contains(&format!(
        "{{card {spirit}}}: all spell and ability damage is prevented this turn"
    )));
    assert_eq!(shielded.priority.map(|priority| priority.active), Some(0));

    turn_event(&mut host, &mut client, 0, TurnEvent::Pass);
    let splitting = turn_event(&mut host, &mut client, ada, TurnEvent::Pass);
    let prompt = splitting
        .prompt
        .clone()
        .expect("five points to place among the two enemies at the battlefield");
    assert_eq!(
        (prompt.seat, prompt.min, prompt.max, prompt.cancel),
        (0, 1, 1, false)
    );
    let strip = host.plugin_view(0);
    assert_eq!(
        strip.prompt.as_ref().map(|summary| summary.why.as_str()),
        Some(format!("{}: 5 of 5 to place", card_option(strike)).as_str())
    );
    assert_eq!(
        labels(&strip),
        [
            card_option(wisp),
            card_option(jinx),
            "free table".to_string()
        ],
        "enemy units at battlefields only"
    );
    assert!(client.plugin_view(ada).status.iter().any(|line| line
        == &format!(
            "waiting for {{seat 0}}: {}: 5 of 5 to place",
            card_option(strike)
        )));
    let mut blob = pick(&mut host, &mut client, 0, 0);
    assert_eq!(
        damage_on(&host, wisp),
        0,
        "the point is prevented as it lands"
    );
    assert!(blob
        .log
        .contains(&format!("{{card {wisp}}}: the damage is prevented")));
    for _ in 0..3 {
        assert!(blob.prompt.is_some(), "every point is still placed");
        blob = pick(&mut host, &mut client, 0, 1);
    }
    assert_eq!(
        host.plugin_view(0)
            .prompt
            .as_ref()
            .map(|summary| summary.why.as_str()),
        Some(format!("{}: 1 of 5 to place", card_option(strike)).as_str())
    );
    let done = pick(&mut host, &mut client, 0, 1);
    assert!(done.prompt.is_none() && done.chain.is_empty());
    assert_eq!(damage_on(&host, wisp), 0);
    assert_eq!(damage_on(&host, jinx), 0);
    assert_eq!(units_in(&host, 0, bf1), [wisp, jinx], "nobody died");
    assert!(client_owns(&client, 0, bf1, wisp) && client_owns(&client, 0, bf1, jinx));
    assert_eq!(zone_of(&host, strike), Some((0, ZONE_TRASH)));
    assert_eq!(xp(&host, 0), 0, "no kills, no XP trigger");
    assert_eq!(client_xp(&client, 0), 0);
    assert!(!done.log.iter().any(|line| line.contains("XP")));
    assert_eq!(done.holder(bf1), Some(ada));
    assert!(done.showdown.is_none());
    same_bytes(&host, &client, "after the prevented strike");
}

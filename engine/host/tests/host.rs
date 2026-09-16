use agni_engine_host::{load_engine, load_plugin, ModuleBytes};
use agni_harden::{harden, HardenConfig};
use agni_sim::engine::{fold_mediated, Engine, FoldMode, NativeEngine, PluginModule};
use agni_sim::log::{
    encode_state, fold_entry_with, AcceptAll, Decider, LogAction, LogEntry, LogState, TableConfig,
    Verdict,
};
use agni_sim::view::{apply_deltas, TableView};
use agni_sim::wire::{
    WireFace, WireZone, ZoneDecl, ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneVisibility,
};
use serde_bytes::ByteBuf;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn engine_module_path() -> PathBuf {
    if let Ok(path) = std::env::var("AGNI_ENGINE_WASM") {
        return PathBuf::from(path);
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            "agni-engine-wasm",
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
        "building agni-engine-wasm for wasm32 failed — set AGNI_ENGINE_WASM to a prebuilt module"
    );
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));
    target.join("wasm32-unknown-unknown/release/agni_engine_wasm.wasm")
}

fn raw_engine() -> &'static Vec<u8> {
    static RAW: OnceLock<Vec<u8>> = OnceLock::new();
    RAW.get_or_init(|| std::fs::read(engine_module_path()).expect("engine module reads"))
}

fn hardened_engine() -> &'static Vec<u8> {
    static HARDENED: OnceLock<Vec<u8>> = OnceLock::new();
    HARDENED.get_or_init(|| {
        harden(raw_engine(), &HardenConfig::engine())
            .expect("engine module survives the hardening pipeline")
            .bytes
    })
}

fn zone_decl() -> ZoneDecl {
    ZoneDecl {
        id: 0,
        name: "deck".into(),
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 1,
        label: "Deck".into(),
    }
}

fn face(name: &str) -> WireFace {
    WireFace {
        name: name.into(),
        tint: [128, 64, 192],
        foil: true,
        kind: None,
        energy: None,
        power: None,
        might: None,
        domain: Vec::new(),
    }
}

fn scripted_log() -> Vec<LogEntry> {
    vec![
        LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: Some("blob:test".into()),
                    plugin: None,
                    zones: vec![zone_decl()],
                    options: None,
                    counters: Vec::new(),
                    despawn_any: false,
                },
            },
        ),
        LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
        LogEntry::new(
            2,
            0,
            LogAction::Deal {
                cards: vec![0, 1, 2],
                to: WireZone::Hand,
            },
        ),
        LogEntry::new(
            3,
            1,
            LogAction::Deal {
                cards: vec![3],
                to: WireZone::Plugin(0),
            },
        ),
        LogEntry::new(
            4,
            0,
            LogAction::Reveal {
                card: 0,
                face: face("vanguard"),
            },
        ),
        LogEntry::new(
            5,
            0,
            LogAction::Move {
                card: 0,
                to: WireZone::Board,
                seat: 0,
                index: 0,
                hidden: false,
            },
        ),
        LogEntry::new(
            6,
            1,
            LogAction::Annotate {
                card: 0,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![0xf5])),
            },
        ),
        LogEntry::new(
            7,
            0,
            LogAction::Move {
                card: 9,
                to: WireZone::Board,
                seat: 0,
                index: 0,
                hidden: false,
            },
        ),
        LogEntry::new(
            8,
            1,
            LogAction::Game {
                data: ByteBuf::from(vec![1, 2, 3]),
            },
        ),
    ]
}

#[test]
fn the_hardened_engine_answers_the_hosts_abi_version() {
    let mut engine = load_engine(hardened_engine(), 1_000_000_000).unwrap();
    assert!(engine.engine_hash().is_some());
    let outcome = engine
        .fold_entry(&scripted_log()[0], None, FoldMode::Sequenced, 0)
        .unwrap();
    assert_eq!(outcome.result, Ok(()));
}

#[test]
fn native_and_wasm_folds_are_byte_identical() {
    for bytes in [raw_engine(), hardened_engine()] {
        let mut wasm = load_engine(bytes, 10_000_000_000).unwrap();
        let mut native = NativeEngine::new();
        let mut bare = LogState::new();
        for entry in &scripted_log() {
            let via_wasm = wasm
                .fold_entry(entry, None, FoldMode::Sequenced, 1)
                .unwrap();
            let via_native = native
                .fold_entry(entry, None, FoldMode::Sequenced, 1)
                .unwrap();
            assert_eq!(via_wasm, via_native);
            let _ = fold_entry_with(&mut bare, entry, &mut AcceptAll);
        }
        assert_eq!(wasm.snapshot().unwrap(), encode_state(&bare));
        assert_eq!(wasm.snapshot().unwrap(), native.snapshot().unwrap());
        assert_eq!(wasm.view(1).unwrap(), native.view(1).unwrap());
    }
}

#[test]
fn a_batched_fold_log_matches_entry_by_entry_folding() {
    let mut batched = load_engine(hardened_engine(), 10_000_000_000).unwrap();
    let mut stepped = load_engine(hardened_engine(), 10_000_000_000).unwrap();
    let log = scripted_log();
    let outcome = batched.fold_log(&log, 1).unwrap();
    let mut mirror = TableView::default();
    apply_deltas(&mut mirror, &outcome.deltas);
    for entry in &log {
        stepped
            .fold_entry(entry, None, FoldMode::Sequenced, 1)
            .unwrap();
    }
    assert_eq!(outcome.applied, 8);
    assert_eq!(batched.snapshot().unwrap(), stepped.snapshot().unwrap());
    assert_eq!(mirror, stepped.view(1).unwrap());
}

#[test]
fn a_snapshot_restores_into_a_fresh_instance_and_resumes_identically() {
    let log = scripted_log();
    let mut first = load_engine(hardened_engine(), 10_000_000_000).unwrap();
    for entry in &log[..4] {
        first
            .fold_entry(entry, None, FoldMode::Sequenced, 0)
            .unwrap();
    }
    let snapshot = first.snapshot().unwrap();
    let mut second = load_engine(hardened_engine(), 10_000_000_000).unwrap();
    second.restore(&snapshot).unwrap();
    for entry in &log[4..] {
        let a = first
            .fold_entry(entry, None, FoldMode::Sequenced, 0)
            .unwrap();
        let b = second
            .fold_entry(entry, None, FoldMode::Sequenced, 0)
            .unwrap();
        assert_eq!(a.result, b.result);
    }
    assert_eq!(first.snapshot().unwrap(), second.snapshot().unwrap());
}

#[test]
fn an_exhausted_gas_budget_is_an_engine_fault_not_a_verdict() {
    let mut starved = load_engine(hardened_engine(), 100).unwrap();
    let error = starved
        .fold_entry(&scripted_log()[0], None, FoldMode::Sequenced, 0)
        .unwrap_err();
    assert!(error.0.contains("gas"), "unexpected fault: {}", error.0);
}

fn plugin_wat(decide_body: &str, data: &str) -> Vec<u8> {
    let wat = format!(
        r#"(module
  (memory (export "memory") 1)
  (data (i32.const 4096) "{data}")
  (func (export "abi_version") (result i32) i32.const 0)
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "dealloc") (param i32 i32))
  (func (export "manifest") (result i64) i64.const 0)
  (func (export "decide") (param i32 i32) (result i64) {decide_body})
  (func (export "view") (param i32 i32) (result i64) i64.const 0))"#
    );
    wat::parse_str(wat).unwrap()
}

fn static_reply_plugin(verdict: &Verdict) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(verdict, &mut bytes).unwrap();
    let escaped: String = bytes.iter().map(|byte| format!("\\{byte:02x}")).collect();
    let body = format!("i64.const {}", (4096u64 << 32) | bytes.len() as u64);
    let module = plugin_wat(&body, &escaped);
    harden(&module, &HardenConfig::default()).unwrap().bytes
}

#[test]
fn host_mediated_plugin_verdicts_match_the_native_decider() {
    struct RejectAll;
    impl Decider for RejectAll {
        fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
            Verdict {
                accept: false,
                plugin_state: None,
                effects: Vec::new(),
                reason: None,
            }
        }
    }
    type NativeFold = Box<dyn Fn(&mut LogState, &LogEntry) -> Result<(), agni_sim::log::FoldError>>;
    let cases: Vec<(Verdict, NativeFold)> = vec![
        (
            Verdict::accept(),
            Box::new(|state, entry| fold_entry_with(state, entry, &mut AcceptAll)),
        ),
        (
            Verdict {
                accept: false,
                plugin_state: None,
                effects: Vec::new(),
                reason: None,
            },
            Box::new(|state, entry| fold_entry_with(state, entry, &mut RejectAll)),
        ),
    ];
    for (verdict, native_fold) in cases {
        let plugin_bytes = static_reply_plugin(&verdict);
        let mut plugin = load_plugin(&plugin_bytes, 1_000_000).unwrap();
        let mut engine = load_engine(hardened_engine(), 10_000_000_000).unwrap();
        let mut bare = LogState::new();
        for entry in &scripted_log() {
            let outcome = fold_mediated(
                &mut engine,
                Some(&mut plugin),
                entry,
                FoldMode::Sequenced,
                0,
            )
            .unwrap();
            assert_eq!(outcome.result, native_fold(&mut bare, entry));
        }
        assert_eq!(engine.snapshot().unwrap(), encode_state(&bare));
    }
}

#[test]
fn a_gas_trapped_plugin_decide_is_a_deterministic_rejection() {
    let module = plugin_wat("(loop (br 0)) i64.const 0", "");
    let hardened = harden(
        &module,
        &HardenConfig {
            gas_limit: 10_000,
            ..HardenConfig::default()
        },
    )
    .unwrap()
    .bytes;
    let mut observations = Vec::new();
    for _ in 0..2 {
        let mut plugin = load_plugin(&hardened, 10_000).unwrap();
        let mut engine = load_engine(hardened_engine(), 10_000_000_000).unwrap();
        let genesis = &scripted_log()[0];
        let outcome = fold_mediated(
            &mut engine,
            Some(&mut plugin),
            genesis,
            FoldMode::Sequenced,
            0,
        )
        .unwrap();
        assert_eq!(outcome.result, Err(agni_sim::log::FoldError::rejected()));
        observations.push(engine.snapshot().unwrap());
    }
    assert_eq!(observations[0], observations[1]);
}

#[test]
fn the_per_call_budget_written_by_the_host_overrides_the_baked_limit() {
    let plugin_bytes = static_reply_plugin(&Verdict::accept());
    let mut generous = load_plugin(&plugin_bytes, 1_000_000).unwrap();
    let mut starved = load_plugin(&plugin_bytes, 1).unwrap();
    let request = vec![0u8; 8];
    assert!(generous.decide(&request).unwrap().accept);
    assert!(!starved.decide(&request).unwrap().accept);
}

#[test]
fn module_bytes_pin_a_blob_ref() {
    let module = ModuleBytes::new(hardened_engine().clone());
    assert!(module.blob_ref().starts_with("blob:"));
    assert_eq!(module.blob_ref().len(), 5 + 64);
}

fn test_build_jobs() -> String {
    std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "4".into())
}

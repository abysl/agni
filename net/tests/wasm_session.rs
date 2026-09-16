use agni_core::CardFace;
use agni_engine_host::{load_engine, WasmEngine};
use agni_harden::{harden, HardenConfig};
use agni_net::session::{
    engine_blob_ref, genesis_engine_pin, verify_engine_pin, ClientSession, HostSession, WireIntent,
    WireZone,
};
use agni_sim::log::{encode_log, encode_state, TableConfig};
use serde_bytes::ByteBuf;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const BUDGET: u64 = 10_000_000_000;

fn engine_module_path() -> PathBuf {
    if let Ok(path) = std::env::var("AGNI_ENGINE_WASM") {
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

fn hardened_engine() -> &'static Vec<u8> {
    static HARDENED: OnceLock<Vec<u8>> = OnceLock::new();
    HARDENED.get_or_init(|| {
        let raw = std::fs::read(engine_module_path()).expect("engine module reads");
        harden(&raw, &HardenConfig::engine())
            .expect("engine module survives the hardening pipeline")
            .bytes
    })
}

fn wasm_engine() -> Box<WasmEngine> {
    Box::new(load_engine(hardened_engine(), BUDGET).unwrap())
}

fn faces(prefix: &str, n: usize) -> Vec<CardFace> {
    (0..n)
        .map(|i| CardFace::named(format!("{prefix} {i}")))
        .collect()
}

fn drive(host: &mut HostSession) {
    host.deal(0, faces("host", 3)).unwrap();
    let (ada, _) = host.join("ada").unwrap();
    host.deal(ada, faces("ada", 3)).unwrap();
    let mine: Vec<u32> = host
        .state()
        .table
        .cards()
        .iter()
        .filter(|card| card.seat.0 == 0)
        .map(|card| card.id.0)
        .collect();
    host.intent(
        0,
        WireIntent::Move {
            card: mine[0],
            to: WireZone::Board,
            seat: 0,
            index: 0,
        },
    )
    .unwrap();
    host.intent(
        ada,
        WireIntent::Annotate {
            card: mine[0],
            key: "exhausted".into(),
            value: Some(ByteBuf::from(vec![0xf5])),
        },
    )
    .unwrap();
    host.intent(
        ada,
        WireIntent::Game {
            data: ByteBuf::from(vec![9]),
        },
    )
    .unwrap();
}

#[test]
fn a_wasm_hosted_session_produces_the_same_log_and_state_as_the_native_one() {
    let mut native = HostSession::new("rae");
    let mut wasm =
        HostSession::with_engine("rae", TableConfig::default(), wasm_engine(), None).unwrap();
    drive(&mut native);
    drive(&mut wasm);
    let native_log: Vec<_> = native.log().iter().skip(1).cloned().collect();
    let wasm_log: Vec<_> = wasm.log().iter().skip(1).cloned().collect();
    assert_eq!(encode_log(&native_log), encode_log(&wasm_log));
    assert_eq!(encode_state(native.state()), encode_state(wasm.state()));
    assert_eq!(native.table().cards(), wasm.table().cards());
}

#[test]
fn a_wasm_replica_folds_a_native_hosts_entries_identically() {
    let mut host = HostSession::new("rae");
    host.deal(0, faces("host", 3)).unwrap();
    let (seat, _) = host.join("ada").unwrap();
    let mut replica = ClientSession::from_welcome_with(
        seat,
        host.roster(),
        host.log().to_vec(),
        wasm_engine(),
        None,
    )
    .unwrap();
    let (entry, wire) = host.deal(seat, faces("ada", 2)).unwrap();
    assert!(replica.apply(entry).unwrap());
    replica.add_faces(wire);
    let card = replica
        .table()
        .cards()
        .iter()
        .find(|card| card.seat.0 == seat)
        .unwrap()
        .id
        .0;
    let entries = host
        .intent(
            seat,
            WireIntent::Move {
                card,
                to: WireZone::Board,
                seat,
                index: 0,
            },
        )
        .unwrap();
    for entry in &entries {
        assert!(replica.apply(entry.clone()).unwrap());
        assert!(!replica.apply(entry.clone()).unwrap());
    }
    assert_eq!(replica.state().table, host.state().table);
    assert_eq!(encode_log(host.log()), encode_log(replica.log()));
}

#[test]
fn a_wasm_host_pins_the_hardened_engine_hash_into_genesis() {
    let engine = wasm_engine();
    let hash = agni_sim::engine::Engine::engine_hash(engine.as_ref()).unwrap();
    let host = HostSession::with_engine("rae", TableConfig::default(), engine, None).unwrap();
    let pinned = genesis_engine_pin(host.log()).unwrap();
    assert_eq!(pinned, engine_blob_ref(hash));
    assert!(verify_engine_pin(host.log(), Some(&pinned)).is_ok());
}

#[test]
fn a_joiner_with_the_wrong_or_missing_engine_is_refused_honestly() {
    let host =
        HostSession::with_engine("rae", TableConfig::default(), wasm_engine(), None).unwrap();
    let pinned = genesis_engine_pin(host.log()).unwrap();
    let wrong = engine_blob_ref([0x11; 32]);
    let mismatch = verify_engine_pin(host.log(), Some(&wrong)).unwrap_err();
    assert!(mismatch.contains(&pinned));
    assert!(mismatch.contains(&wrong));
    let missing = verify_engine_pin(host.log(), None).unwrap_err();
    assert!(missing.contains(&pinned));
    let native = HostSession::new("rae");
    assert!(verify_engine_pin(native.log(), None).is_ok());
    assert!(verify_engine_pin(native.log(), Some(&wrong)).is_ok());
}

fn test_build_jobs() -> String {
    std::env::var("CARGO_BUILD_JOBS").unwrap_or_else(|_| "4".into())
}

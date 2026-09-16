use agni_harden::{harden, HardenConfig, HardenError, DEFAULT_MEMORY_MAX_PAGES};
use wasmi::{Config, Engine, Instance, Linker, Module, Store, TrapCode, Val};

const HELLO_DECIDER: &[u8] = include_bytes!("fixtures/hello_decider_guest.wasm");
const ENTRY: &[u8] = b"\xa2\x64seat\x02\x66action\x64move";

fn guest_wat(decide_body: &str) -> String {
    format!(
        r#"(module
  (memory (export "memory") 1)
  (func (export "abi_version") (result i32) i32.const 0)
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "dealloc") (param i32 i32))
  (func (export "manifest") (result i64) i64.const 0)
  (func $decide (export "decide") (param i32 i32) (result i64) {decide_body})
  (func (export "view") (param i32 i32) (result i64) i64.const 0))"#
    )
}

fn guest(decide_body: &str) -> Vec<u8> {
    wat::parse_str(guest_wat(decide_body)).unwrap()
}

fn instantiate(bytes: &[u8]) -> (Store<()>, Instance) {
    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, bytes).unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(u64::MAX).unwrap();
    let linker: Linker<()> = Linker::new(&engine);
    let instance = linker.instantiate_and_start(&mut store, &module).unwrap();
    (store, instance)
}

fn gas_left(store: &Store<()>, instance: &Instance) -> i64 {
    match try_gas_left(store, instance) {
        Some(value) => value,
        None => panic!("gas_left global missing"),
    }
}

fn try_gas_left(store: &Store<()>, instance: &Instance) -> Option<i64> {
    match instance.get_global(store, "gas_left")?.get(store) {
        Val::I64(value) => Some(value),
        other => panic!("gas_left has unexpected type: {other:?}"),
    }
}

fn fuel_used(store: &Store<()>) -> u64 {
    u64::MAX - store.get_fuel().unwrap()
}

fn run_decide(bytes: &[u8], input: &[u8]) -> (Vec<u8>, Option<i64>, u64) {
    let (mut store, instance) = instantiate(bytes);
    let alloc = instance
        .get_typed_func::<u32, u32>(&store, "alloc")
        .unwrap();
    let decide = instance
        .get_typed_func::<(u32, u32), u64>(&store, "decide")
        .unwrap();
    let memory = instance.get_memory(&store, "memory").unwrap();
    let ptr = alloc.call(&mut store, input.len() as u32).unwrap();
    memory.write(&mut store, ptr as usize, input).unwrap();
    let packed = decide.call(&mut store, (ptr, input.len() as u32)).unwrap();
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    let mut out = vec![0u8; out_len];
    memory.read(&store, out_ptr, &mut out).unwrap();
    (out, try_gas_left(&store, &instance), fuel_used(&store))
}

#[test]
fn hardening_is_byte_deterministic() {
    let config = HardenConfig::default();
    let first = harden(HELLO_DECIDER, &config).unwrap();
    let second = harden(HELLO_DECIDER, &config).unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.hash, second.hash);
    assert_eq!(first.hash_hex(), second.hash_hex());
    assert_eq!(
        first.hash_hex(),
        "f30864793901bd6194911ef54ec0260e9b0d6bdc21f74d5fa125118bd3096c9d"
    );

    let looping = guest("(loop (br 0)) i64.const 0");
    let first = harden(&looping, &config).unwrap();
    let second = harden(&looping, &config).unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.hash, second.hash);
}

#[test]
fn hardened_hello_decider_preserves_semantics() {
    let hardened = harden(HELLO_DECIDER, &HardenConfig::default()).unwrap();

    let (raw_out, _, _) = run_decide(HELLO_DECIDER, ENTRY);
    let (out1, left1, fuel1) = run_decide(&hardened.bytes, ENTRY);
    let (out2, left2, fuel2) = run_decide(&hardened.bytes, ENTRY);

    assert_eq!(out1, raw_out);
    assert_eq!(out1, out2);
    assert_eq!(left1, left2);
    assert_eq!(fuel1, fuel2);
    assert!((left1.unwrap() as u64) < HardenConfig::default().gas_limit);

    let (mut store, instance) = instantiate(&hardened.bytes);
    let abi_version = instance
        .get_typed_func::<(), u32>(&store, "abi_version")
        .unwrap();
    assert_eq!(abi_version.call(&mut store, ()).unwrap(), 0);
}

#[test]
fn gas_limit_is_baked_into_the_module() {
    let config = HardenConfig {
        gas_limit: 123_456,
        ..HardenConfig::default()
    };
    let hardened = harden(HELLO_DECIDER, &config).unwrap();
    let (store, instance) = instantiate(&hardened.bytes);
    assert_eq!(gas_left(&store, &instance), 123_456);
}

#[test]
fn infinite_loop_traps_at_the_same_gas_count() {
    let config = HardenConfig {
        gas_limit: 10_000,
        ..HardenConfig::default()
    };
    let hardened = harden(HELLO_DECIDER, &config).unwrap();

    let mut observations = Vec::new();
    for _ in 0..2 {
        let (mut store, instance) = instantiate(&hardened.bytes);
        let spin = instance.get_typed_func::<(), u32>(&store, "spin").unwrap();
        let error = spin.call(&mut store, ()).unwrap_err();
        assert_eq!(error.as_trap_code(), Some(TrapCode::UnreachableCodeReached));
        observations.push((gas_left(&store, &instance), fuel_used(&store)));
    }
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[0].0, -1);
}

#[test]
fn looping_wat_guest_traps_at_the_same_gas_count() {
    let config = HardenConfig {
        gas_limit: 10_000,
        ..HardenConfig::default()
    };
    let hardened = harden(&guest("(loop (br 0)) i64.const 0"), &config).unwrap();

    let mut observations = Vec::new();
    for _ in 0..2 {
        let (mut store, instance) = instantiate(&hardened.bytes);
        let decide = instance
            .get_typed_func::<(u32, u32), u64>(&store, "decide")
            .unwrap();
        let error = decide.call(&mut store, (0, 0)).unwrap_err();
        assert_eq!(error.as_trap_code(), Some(TrapCode::UnreachableCodeReached));
        observations.push((gas_left(&store, &instance), fuel_used(&store)));
    }
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[0].0, -1);
}

#[test]
fn stack_bomb_traps_deterministically_before_gas_runs_out() {
    let hardened = harden(
        &guest("local.get 0 local.get 1 call $decide"),
        &HardenConfig::default(),
    )
    .unwrap();

    let mut observations = Vec::new();
    for _ in 0..2 {
        let (mut store, instance) = instantiate(&hardened.bytes);
        let decide = instance
            .get_typed_func::<(u32, u32), u64>(&store, "decide")
            .unwrap();
        let error = decide.call(&mut store, (0, 0)).unwrap_err();
        assert_eq!(error.as_trap_code(), Some(TrapCode::UnreachableCodeReached));
        observations.push((gas_left(&store, &instance), fuel_used(&store)));
    }
    assert_eq!(observations[0], observations[1]);
    assert!(observations[0].0 > 0);
}

#[test]
fn float_opcodes_are_rejected_with_a_named_opcode() {
    let error = harden(
        &guest("f64.const 1 f64.const 2 f64.add drop i64.const 0"),
        &HardenConfig::default(),
    )
    .unwrap_err();
    match error {
        HardenError::ForbiddenOpcode { opcode, .. } => assert!(opcode.contains("F64")),
        other => panic!("expected ForbiddenOpcode, got {other:?}"),
    }
}

#[test]
fn imports_are_rejected() {
    let wat =
        guest_wat("i64.const 0").replacen("(module", "(module (import \"env\" \"leak\" (func))", 1);
    let error = harden(&wat::parse_str(wat).unwrap(), &HardenConfig::default()).unwrap_err();
    match error {
        HardenError::ImportForbidden { module, name } => {
            assert_eq!(module, "env");
            assert_eq!(name, "leak");
        }
        other => panic!("expected ImportForbidden, got {other:?}"),
    }
}

#[test]
fn missing_required_exports_are_rejected() {
    let wat = guest_wat("i64.const 0").replacen("(export \"view\")", "", 1);
    let error = harden(&wat::parse_str(wat).unwrap(), &HardenConfig::default()).unwrap_err();
    assert_eq!(
        error,
        HardenError::MissingExport {
            name: "view".to_string()
        }
    );
}

#[test]
fn reserved_gas_export_is_rejected() {
    let wat = guest_wat("i64.const 0").replacen(
        "(module",
        "(module (global (export \"gas_left\") i64 (i64.const 0))",
        1,
    );
    let error = harden(&wat::parse_str(wat).unwrap(), &HardenConfig::default()).unwrap_err();
    assert_eq!(
        error,
        HardenError::ReservedExport {
            name: "gas_left".to_string()
        }
    );
}

#[test]
fn declared_memory_maximum_is_clamped_to_the_cap() {
    let wat = guest_wat("i64.const 0").replacen(
        "(memory (export \"memory\") 1)",
        "(memory (export \"memory\") 1 20000)",
        1,
    );
    let hardened = harden(&wat::parse_str(wat).unwrap(), &HardenConfig::default()).unwrap();
    assert_eq!(
        memory_limits(&hardened.bytes),
        (1, Some(DEFAULT_MEMORY_MAX_PAGES))
    );

    let unbounded = harden(&guest("i64.const 0"), &HardenConfig::default()).unwrap();
    assert_eq!(
        memory_limits(&unbounded.bytes),
        (1, Some(DEFAULT_MEMORY_MAX_PAGES))
    );
}

#[test]
fn memory_minimum_above_the_cap_is_rejected() {
    let wat = guest_wat("i64.const 0").replacen(
        "(memory (export \"memory\") 1)",
        "(memory (export \"memory\") 300)",
        1,
    );
    let error = harden(&wat::parse_str(wat).unwrap(), &HardenConfig::default()).unwrap_err();
    assert_eq!(
        error,
        HardenError::MemoryMinAboveCap {
            min_pages: 300,
            cap_pages: DEFAULT_MEMORY_MAX_PAGES
        }
    );
}

#[test]
fn hardened_output_still_has_zero_imports() {
    let hardened = harden(HELLO_DECIDER, &HardenConfig::default()).unwrap();
    for payload in wasmparser::Parser::new(0).parse_all(&hardened.bytes) {
        if let wasmparser::Payload::ImportSection(reader) = payload.unwrap() {
            assert_eq!(reader.into_iter().count(), 0);
        }
    }
}

fn memory_limits(bytes: &[u8]) -> (u64, Option<u64>) {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::MemorySection(reader) = payload.unwrap() {
            let memory = reader.into_iter().next().unwrap().unwrap();
            return (memory.initial, memory.maximum);
        }
    }
    panic!("no memory section");
}

#[test]
fn the_cli_publishes_a_hardened_module_into_a_spirit_store() {
    let scratch = std::env::temp_dir().join(format!("agni-harden-publish-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let input = scratch.join("input.wasm");
    let output = scratch.join("output.wasm");
    let store_dir = scratch.join("store");
    std::fs::write(&input, HELLO_DECIDER).unwrap();
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_agni-harden"))
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--store",
            store_dir.to_str().unwrap(),
            "--name",
            "hello",
            "--module-version",
            "0.1.0",
        ])
        .output()
        .unwrap();
    assert!(ran.status.success());
    let printed = String::from_utf8(ran.stdout).unwrap().trim().to_string();
    let hardened = std::fs::read(&output).unwrap();
    assert_eq!(printed, blake3::hash(&hardened).to_hex().to_string());
    let store = spirit_sdk::BlobStore::open(&store_dir).unwrap();
    let identity = spirit_sdk::identity::load(&store_dir).unwrap();
    let trust = spirit_sdk::Trust::new().with_own(identity.dgid());
    let version = spirit_sdk::modules::resolve(&store, &trust, "hello", None).unwrap();
    assert_eq!(
        spirit_sdk::modules::bytes(&store, &version).unwrap(),
        hardened
    );
    assert_eq!(version.module.role, spirit_sdk::modules::Role::Plugin);
    assert_eq!(version.module.version, "0.1.0");
    assert_eq!(version.blob.unwrap().to_string(), printed);
    assert_eq!(version.signer, Some(identity.dgid()));
    let td = version.td.expect("the release names its transform");
    let recipe = spirit_sdk::record::Tdr::decode(&store.get(td.hash()).unwrap()).unwrap();
    assert_eq!(recipe.kind, "wasm-harden");
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn the_cli_refuses_a_store_without_a_name() {
    let scratch = std::env::temp_dir().join(format!("agni-harden-noname-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let input = scratch.join("input.wasm");
    std::fs::write(&input, HELLO_DECIDER).unwrap();
    let ran = std::process::Command::new(env!("CARGO_BIN_EXE_agni-harden"))
        .args([
            input.to_str().unwrap(),
            scratch.join("output.wasm").to_str().unwrap(),
            "--store",
            scratch.join("store").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!ran.status.success());
    let _ = std::fs::remove_dir_all(&scratch);
}

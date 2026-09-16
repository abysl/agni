use wasmtime::{Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder};

const FUEL: u64 = 5_000_000;
const MEMORY_CAP: usize = 16 * 1024 * 1024;

struct Ctx {
    limits: StoreLimits,
}

fn engine() -> Engine {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.cranelift_nan_canonicalization(true);
    config.relaxed_simd_deterministic(true);
    Engine::new(&config).unwrap()
}

fn run_decide(engine: &Engine, module: &Module, input: &[u8]) -> (Vec<u8>, u64) {
    let limits = StoreLimitsBuilder::new()
        .memory_size(MEMORY_CAP)
        .instances(1)
        .build();
    let mut store = Store::new(engine, Ctx { limits });
    store.limiter(|ctx| &mut ctx.limits);
    store.set_fuel(FUEL).unwrap();
    let instance = Instance::new(&mut store, module, &[]).unwrap();
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    let alloc = instance
        .get_typed_func::<u32, u32>(&mut store, "alloc")
        .unwrap();
    let decide = instance
        .get_typed_func::<(u32, u32), u64>(&mut store, "decide")
        .unwrap();
    let ptr = alloc.call(&mut store, input.len() as u32).unwrap();
    memory.write(&mut store, ptr as usize, input).unwrap();
    let packed = decide.call(&mut store, (ptr, input.len() as u32)).unwrap();
    let (out_ptr, out_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    let mut out = vec![0u8; out_len];
    memory.read(&store, out_ptr, &mut out).unwrap();
    (out, FUEL - store.get_fuel().unwrap())
}

fn run_spin(engine: &Engine, module: &Module) -> (String, u64) {
    let limits = StoreLimitsBuilder::new().memory_size(MEMORY_CAP).build();
    let mut store = Store::new(engine, Ctx { limits });
    store.limiter(|ctx| &mut ctx.limits);
    store.set_fuel(FUEL).unwrap();
    let instance = Instance::new(&mut store, module, &[]).unwrap();
    let spin = instance
        .get_typed_func::<(), u32>(&mut store, "spin")
        .unwrap();
    let err = spin.call(&mut store, ()).unwrap_err();
    (format!("{}", err.root_cause()), store.get_fuel().unwrap())
}

fn main() {
    let wasm = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let engine = engine();
    let module = Module::new(&engine, &wasm).unwrap();

    assert!(module.imports().len() == 0);
    println!("imports: none (no WASI, no host functions)");

    let entry = b"\xa2\x64seat\x02\x66action\x64move";
    let (out1, fuel1) = run_decide(&engine, &module, entry);
    let (out2, fuel2) = run_decide(&engine, &module, entry);
    assert_eq!(out1, out2);
    assert_eq!(fuel1, fuel2);
    println!(
        "decide: verdict={} digest={:02x?} fuel={}",
        out1[0],
        &out1[1..],
        fuel1
    );
    println!("decide again: identical output, identical fuel ({fuel2})");

    let (trap1, left1) = run_spin(&engine, &module);
    let (trap2, left2) = run_spin(&engine, &module);
    assert_eq!(left1, left2);
    println!("spin: trapped with '{trap1}', fuel left {left1}");
    println!("spin again: '{trap2}', fuel left {left2} (deterministic exhaustion)");
}

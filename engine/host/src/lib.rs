use agni_sim::engine::EngineFault;
pub use agni_sim::engine::{AbiEngine, AbiPlugin, CallFault, ModuleCall};
pub use agni_sim::pins::{blob_ref, hash_hex, pin_hash};
use spirit_sdk::modules::{self, Role, Version};
use spirit_sdk::{identity, BlobStore, Trust};
use std::path::PathBuf;
use wasmi::{Config, Global, Instance, Linker, Memory, Module, Store, TrapCode, Val};

pub use agni_sim::engine::{ENGINE_GAS_BUDGET, PLUGIN_GAS_BUDGET};

pub const GAS_SENTINEL: i64 = -1;
pub const FUEL_BACKSTOP: u64 = 1 << 44;

pub type WasmEngine = AbiEngine<WasmModule>;
pub type WasmPlugin = AbiPlugin<WasmModule>;

pub struct WasmModule {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    gas: Option<Global>,
    budget: u64,
}

impl WasmModule {
    pub fn instantiate(bytes: &[u8], budget: u64) -> Result<Self, String> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = wasmi::Engine::new(&config);
        let module =
            Module::new(&engine, bytes).map_err(|e| format!("module does not compile: {e}"))?;
        if module.imports().len() > 0 {
            return Err("module declares imports".into());
        }
        let mut store = Store::new(&engine, ());
        store
            .set_fuel(FUEL_BACKSTOP)
            .map_err(|e| format!("fuel: {e}"))?;
        let linker: Linker<()> = Linker::new(&engine);
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|e| format!("module does not instantiate: {e}"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("module exports no memory")?;
        let gas = instance.get_global(&store, "gas_left");
        Ok(Self {
            store,
            instance,
            memory,
            gas,
            budget,
        })
    }

    pub fn gas_left(&self) -> Option<i64> {
        match self.gas?.get(&self.store) {
            Val::I64(value) => Some(value),
            _ => None,
        }
    }

    fn fault_from(&self, error: wasmi::Error) -> CallFault {
        match error.as_trap_code() {
            Some(TrapCode::OutOfFuel) => CallFault::Broken("host fuel backstop tripped".into()),
            Some(code) => {
                if code == TrapCode::UnreachableCodeReached && self.gas_left() == Some(GAS_SENTINEL)
                {
                    CallFault::GasExhausted
                } else {
                    CallFault::Trapped(format!("{code:?}"))
                }
            }
            None => CallFault::broken("call failed", error),
        }
    }
}

impl ModuleCall for WasmModule {
    fn abi_version(&mut self) -> Result<u32, CallFault> {
        let func = self
            .instance
            .get_typed_func::<(), u32>(&self.store, "abi_version")
            .map_err(|e| CallFault::broken("abi_version missing", e))?;
        func.call(&mut self.store, ())
            .map_err(|e| CallFault::broken("abi_version call", e))
    }

    fn call(&mut self, name: &str, request: &[u8]) -> Result<Vec<u8>, CallFault> {
        if let Some(gas) = self.gas {
            gas.set(&mut self.store, Val::I64(self.budget as i64))
                .map_err(|e| CallFault::broken("gas budget write", e))?;
        }
        self.store
            .set_fuel(FUEL_BACKSTOP)
            .map_err(|e| CallFault::broken("fuel backstop", e))?;
        let len = u32::try_from(request.len())
            .map_err(|_| CallFault::Broken("request exceeds the guest address space".into()))?;
        let ptr = if request.is_empty() {
            0
        } else {
            let alloc = self
                .instance
                .get_typed_func::<u32, u32>(&self.store, "alloc")
                .map_err(|e| CallFault::broken("alloc missing", e))?;
            let ptr = alloc
                .call(&mut self.store, len)
                .map_err(|e| self.fault_from(e))?;
            self.memory
                .write(&mut self.store, ptr as usize, request)
                .map_err(|e| CallFault::broken("request write", e))?;
            ptr
        };
        let func = self
            .instance
            .get_typed_func::<(u32, u32), u64>(&self.store, name)
            .map_err(|e| CallFault::broken(name, e))?;
        let packed = func
            .call(&mut self.store, (ptr, len))
            .map_err(|e| self.fault_from(e))?;
        let (reply_ptr, reply_len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
        let mut reply = vec![0u8; reply_len];
        self.memory
            .read(&self.store, reply_ptr, &mut reply)
            .map_err(|e| CallFault::broken("reply read", e))?;
        Ok(reply)
    }
}

pub fn load_engine(bytes: &[u8], budget: u64) -> Result<WasmEngine, EngineFault> {
    let module = WasmModule::instantiate(bytes, budget).map_err(EngineFault)?;
    AbiEngine::load(module, *blake3::hash(bytes).as_bytes())
}

pub fn load_plugin(bytes: &[u8], budget: u64) -> Result<WasmPlugin, EngineFault> {
    let module = WasmModule::instantiate(bytes, budget).map_err(EngineFault)?;
    AbiPlugin::load(module, *blake3::hash(bytes).as_bytes())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleBytes {
    pub bytes: Vec<u8>,
    pub hash: [u8; 32],
}

impl ModuleBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        let hash = *blake3::hash(&bytes).as_bytes();
        Self { bytes, hash }
    }

    pub fn blob_ref(&self) -> String {
        blob_ref(self.hash)
    }
}

pub fn module_matches_pin(pin: &str, bytes: &[u8]) -> bool {
    pin_hash(pin).is_some_and(|expected| *blake3::hash(bytes).as_bytes() == expected)
}

pub trait ModuleSource {
    fn engine_module(&self) -> Result<ModuleBytes, String>;
}

pub struct FileSource(pub PathBuf);

impl ModuleSource for FileSource {
    fn engine_module(&self) -> Result<ModuleBytes, String> {
        let bytes = std::fs::read(&self.0)
            .map_err(|e| format!("engine module at {}: {e}", self.0.display()))?;
        Ok(ModuleBytes::new(bytes))
    }
}

pub struct StoreSource {
    pub dir: PathBuf,
    pub name: String,
    pub abi_version: Option<u32>,
}

impl StoreSource {
    pub fn new(dir: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            dir: dir.into(),
            name: name.into(),
            abi_version: None,
        }
    }

    pub fn at_abi(mut self, abi_version: u32) -> Self {
        self.abi_version = Some(abi_version);
        self
    }

    pub fn trust(dir: &std::path::Path) -> Trust {
        let trust = Trust::load(dir);
        match identity::load(dir) {
            Some(identity) => trust.with_own(identity.dgid()),
            None => trust,
        }
    }

    pub fn versions(&self) -> Result<Vec<Version>, String> {
        let store = self.store()?;
        Ok(modules::versions(&store, &self.name))
    }

    fn store(&self) -> Result<BlobStore, String> {
        BlobStore::open(&self.dir)
            .map_err(|e| format!("spirit store at {}: {e}", self.dir.display()))
    }

    pub fn load(&self) -> Result<(Version, ModuleBytes), String> {
        let store = self.store()?;
        let trust = Self::trust(&self.dir);
        let version = modules::resolve(&store, &trust, &self.name, self.abi_version)
            .ok_or_else(|| format!("no trusted modules/{} in the store", self.name))?;
        let bytes = modules::bytes(&store, &version)?;
        Ok((version, ModuleBytes::new(bytes)))
    }
}

impl ModuleSource for StoreSource {
    fn engine_module(&self) -> Result<ModuleBytes, String> {
        let (version, bytes) = self.load()?;
        if version.module.role != Role::Engine {
            return Err(format!(
                "module ref {} is a {} module, not an engine",
                self.name,
                version.module.role.label()
            ));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    #[test]
    fn a_pin_round_trips_through_its_hex() {
        let hash = *blake3::hash(b"the real module").as_bytes();
        let pin = blob_ref(hash);
        assert_eq!(pin.len(), 5 + 64);
        assert_eq!(pin_hash(&pin), Some(hash));
        assert!(module_matches_pin(&pin, b"the real module"));
        assert!(!module_matches_pin(&pin, b"a tampered module"));
        assert!(!module_matches_pin("blob:zz", b"the real module"));
        assert!(!module_matches_pin("nonsense", b"the real module"));
        assert_eq!(
            hash_hex(&hash),
            blake3::hash(b"the real module").to_hex().to_string()
        );
    }
}

#[cfg(test)]
mod store_source_tests {
    use super::*;
    use spirit_sdk::modules::Module;
    use spirit_sdk::record::Tdr;
    use spirit_sdk::Identity;

    fn scratch_store(tag: &str) -> (PathBuf, BlobStore, Identity) {
        let dir =
            std::env::temp_dir().join(format!("agni-store-source-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = BlobStore::open(&dir).unwrap();
        let identity = identity::load_or_create(&dir).unwrap();
        (dir, store, identity)
    }

    fn publish(store: &BlobStore, identity: &Identity, name: &str, role: Role, bytes: &[u8]) {
        modules::publish(
            store,
            identity,
            &Module::new(name, role, "0.1.0", 0),
            &Tdr::new("wasm-harden", &("test",)).unwrap(),
            bytes,
        )
        .unwrap();
    }

    #[test]
    fn a_published_engine_resolves_through_the_store_source() {
        let (dir, store, identity) = scratch_store("engine");
        publish(&store, &identity, "engine", Role::Engine, b"engine bytes");
        let source = StoreSource::new(&dir, "engine");
        let module = source.engine_module().unwrap();
        assert_eq!(module.bytes, b"engine bytes");
        assert_eq!(module.hash, *blake3::hash(b"engine bytes").as_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plugin_ref_does_not_pass_as_an_engine() {
        let (dir, store, identity) = scratch_store("kind");
        publish(
            &store,
            &identity,
            "riftbound",
            Role::Plugin,
            b"plugin bytes",
        );
        let source = StoreSource::new(&dir, "riftbound");
        let refused = source.engine_module().unwrap_err();
        assert!(refused.contains("not an engine"));
        let (version, module) = source.load().unwrap();
        assert_eq!(version.module.role, Role::Plugin);
        assert_eq!(module.bytes, b"plugin bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_ref_names_itself_in_the_error() {
        let (dir, _store, _identity) = scratch_store("missing");
        let refused = StoreSource::new(&dir, "absent")
            .engine_module()
            .unwrap_err();
        assert!(refused.contains("absent"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_newest_published_version_is_the_one_that_loads() {
        let (dir, store, identity) = scratch_store("versions");
        modules::publish(
            &store,
            &identity,
            &Module::new("riftbound", Role::Plugin, "0.3.0", 0),
            &Tdr::new("wasm-harden", &("test",)).unwrap(),
            b"old plugin",
        )
        .unwrap();
        modules::publish(
            &store,
            &identity,
            &Module::new("riftbound", Role::Plugin, "0.4.0", 0),
            &Tdr::new("wasm-harden", &("test",)).unwrap(),
            b"new plugin",
        )
        .unwrap();
        let source = StoreSource::new(&dir, "riftbound");
        assert_eq!(source.versions().unwrap().len(), 2);
        assert_eq!(source.load().unwrap().1.bytes, b"new plugin");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

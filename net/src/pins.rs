pub use agni_sim::pins::{
    blob_ref as engine_blob_ref, genesis_engine_pin, genesis_plugin_pin, hash_hex, pin_hash,
    verify_engine_pin, verify_plugin_pin, PinError,
};

pub fn module_hash(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

pub fn module_matches_pin(pin: &str, bytes: &[u8]) -> bool {
    pin_hash(pin).is_some_and(|expected| module_hash(bytes) == expected)
}

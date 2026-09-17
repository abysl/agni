pub const PLUGIN_ABI_VERSION: u32 = agni_plugin_sdk::PLUGIN_ABI_VERSION;

pub const MANIFEST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/manifest.cbor"));
pub const ACCEPT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/accept.cbor"));
pub const VIEW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/view.cbor"));

agni_plugin_sdk::export_plugin!(
    manifest: crate::MANIFEST,
    decide: agni_plugin_sdk::decide::accept_all,
    view: agni_plugin_sdk::view::nothing,
);

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::log::Verdict;
    use agni_sim::wire::decode_plugin_manifest;

    #[test]
    fn the_manifest_declares_the_mtg_zone_table() {
        let manifest = decode_plugin_manifest(MANIFEST).unwrap();
        assert_eq!(manifest.name, "mtg");
        assert_eq!(manifest.display, "MTG");
        assert_eq!(manifest.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.zones, agni_mtg::zone_table());
        let keys: Vec<&str> = manifest
            .hotkeys
            .iter()
            .map(|hotkey| hotkey.key.as_str())
            .collect();
        assert_eq!(keys, ["e", "d", "t"]);
    }

    #[test]
    fn the_decider_answers_accept_all() {
        let verdict: Verdict = agni_sim::abi::decode(ACCEPT).unwrap();
        assert!(verdict.accept);
        assert!(verdict.plugin_state.is_none());
    }

    #[test]
    fn the_view_stub_is_cbor_null() {
        assert_eq!(VIEW, [0xf6]);
        assert_eq!(PLUGIN_ABI_VERSION, agni_sim::abi::PLUGIN_ABI_VERSION);
    }
}

use agni_sim::abi::encode;
use agni_sim::log::Verdict;
use agni_sim::wire::{encode_plugin_manifest, HotkeyDecl, PluginManifest};
use std::path::PathBuf;

fn hotkey(key: &str, action: &str) -> HotkeyDecl {
    HotkeyDecl {
        key: key.into(),
        action: action.into(),
    }
}

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));
    let manifest = PluginManifest {
        name: "riftbound".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        display: "Riftbound".into(),
        zones: agni_riftbound::zone_table(),
        counters: agni_riftbound::counter_table(),
        tokens: agni_riftbound::token_table(),
        hotkeys: vec![
            hotkey("e", "toggle-exhaust"),
            hotkey("d", "draw"),
            hotkey("t", "to-trash"),
            hotkey("space", "advance-turn"),
            hotkey("w", "pass"),
            hotkey("x", "cancel"),
            hotkey("h", "play-hidden"),
            hotkey("r", "reveal"),
            hotkey("k", "spawn-token"),
        ],
        despawn_any: false,
    };
    std::fs::write(out.join("manifest.cbor"), encode_plugin_manifest(&manifest))
        .expect("manifest.cbor writes");
    std::fs::write(out.join("accept.cbor"), encode(&Verdict::accept()))
        .expect("accept.cbor writes");
    std::fs::write(out.join("view.cbor"), [0xf6]).expect("view.cbor writes");
    println!("cargo:rerun-if-changed=build.rs");
}

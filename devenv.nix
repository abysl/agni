{...}: {
  cachix.enable = false;

  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer"];
    targets = ["wasm32-unknown-unknown"];
  };

  enterShell = ''
    echo "agni dev env — rust $(rustc --version)"
  '';

  env.AGNI_IMPORTER_FEATURES = "agni-importers/scryfall,agni-importers/riftbound-gateway,agni-importers/mtg-native";

  scripts.build.exec = "cargo build --workspace --all-targets --features $AGNI_IMPORTER_FEATURES";
  scripts."unit-test".exec = "cargo test --workspace --features $AGNI_IMPORTER_FEATURES";
  scripts."engine-build".exec = ''
    set -e
    cargo build -p agni-engine-wasm --target wasm32-unknown-unknown --release
    cargo run --release -p agni-harden -- \
      target/wasm32-unknown-unknown/release/agni_engine_wasm.wasm \
      target/engine.wasm --engine
  '';
  scripts."plugin-build".exec = ''
    set -e
    cargo build -p agni-riftbound-plugin -p agni-mtg-plugin --target wasm32-unknown-unknown --release
    cargo run --release -p agni-harden -- \
      target/wasm32-unknown-unknown/release/riftbound_plugin.wasm \
      target/riftbound.wasm
    cargo run --release -p agni-harden -- \
      target/wasm32-unknown-unknown/release/mtg_plugin.wasm \
      target/mtg.wasm
  '';
  scripts.clippy.exec = "cargo clippy --workspace --all-targets --features $AGNI_IMPORTER_FEATURES -- -D warnings";
  scripts.fmt.exec = "treefmt";
  scripts."fmt-check".exec = "treefmt --fail-on-change";
}

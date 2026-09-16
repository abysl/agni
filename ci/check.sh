set -euo pipefail

cargo test --locked -p agni-core -p agni-sim -p agni-plugin-sdk -p agni-deck --lib
cargo test --locked -p agni-net --lib --test undo --test session --test goldens
cargo check --locked --workspace --all-targets

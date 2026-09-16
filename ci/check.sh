set -euo pipefail

cargo test --locked -p agni-core -p agni-sim -p agni-plugin-sdk -p agni-deck --lib
cargo check --locked --workspace --all-targets

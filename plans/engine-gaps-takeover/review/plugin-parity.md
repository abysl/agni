# Riftbound plugin parity

Added a test-only native Riftbound `PluginModule` adapter in
`net/tests/riftbound_turns.rs`. It decodes the native
`agni_riftbound_turns::decide_bytes` and `present_bytes` ABI replies, then
replays host logs through that adapter and the hardened Wasm plugin beside
matching native and Wasm engines.

The parity test reuses the net fixture helpers and checks every entry in three
small corpora: an enforced target prompt through target selection and priority
passes, a one-card mulligan, and a hidden card's later public reveal. At every
entry it compares the ABI request, verdict and effects, admission result,
serialized plugin blob and engine state, engine views for both seats, and plugin
views for both seats. Both engines restore their matched snapshot before the
next entry, and each corpus's final snapshot is compared with the originating
host state.

Validation:

- `AGNI_ENGINE_WASM=<raw engine> AGNI_RIFTBOUND_WASM=<raw plugin> CARGO_TARGET_DIR=<parity target> CARGO_BUILD_JOBS=12 cargo test -p agni-net --test riftbound_turns native_and_hardened_riftbound_replays_match_at_every_entry -- --nocapture`: passed
- `cargo fmt -p agni-net --check`: passed
- `CARGO_TARGET_DIR=<parity target> CARGO_BUILD_JOBS=12 cargo clippy -p agni-net --test riftbound_turns --all-targets -- -D warnings`: passed

The harness covers the selected host-session corpus rather than every card or
every view permutation. Plugin view requests use the replayed public state
without the host's private face map, so this proves native/Wasm parity for that
same request shape. Snapshot restoration uses the same engine instances between
entries; fresh-instance construction at prompt boundaries is outside this
bounded corpus.

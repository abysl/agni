# Rules lane reconciliation

The completed `origin/gaps/rules` lane was merged into the takeover tip. This checkpoint intentionally stops before the statics lane; the parent coordinator will review this rules recovery before requesting that follow-up merge.

The merge retained the prompt-era `NameKind`, named-card CBOR helpers, `chosen_champion`, `won`, and the version-7 blob shape. It added the rules lane's `PlayLock` mask and `Brush`/`Baron Pit` token faces. `SeatState` writes the lock as the existing fourth array slot and accepts both the old eight-element row and the new nine-element row; a legacy boolean `true` decodes as `PlayLock::SPELLS`, preserving old version-7 blobs without a version bump. The optional chosen-champion value remains the ninth row element. `GameBlob`'s existing `wn` map key is unchanged. The public `spells_locked` helper now reports both the kind-specific spell lock and Fallen Feline's named-spell veto, while `unlocked` remains the generic kind mask reader.

Validation from `orgs/andrea/projects/agni/agni`:

- `CARGO_TARGET_DIR=<assigned external cache> CARGO_BUILD_JOBS=12 cargo test -p agni-riftbound-turns --no-fail-fast -q`: 4320 passed, 0 failed, 338 ignored.
- `CARGO_TARGET_DIR=<assigned external cache> CARGO_BUILD_JOBS=12 cargo clippy -p agni-riftbound-turns --all-targets -- -D warnings`: passed.
- `CARGO_TARGET_DIR=<assigned external cache> CARGO_BUILD_JOBS=12 cargo fmt -p agni-riftbound-turns --check`: passed.
- `CARGO_TARGET_DIR=<assigned external cache> CARGO_BUILD_JOBS=12 cargo check -p agni-riftbound-plugin --target wasm32-unknown-unknown`: passed.

The remaining ignored count is 338 in the full turn-engine suite. The rules merge does not claim any of those engine-gap tests; statics and the other unimplemented clusters remain for later checkpoints. The new writer encodes the lock as a numeric value, while the decoder accepts the legacy boolean; this is backward-read compatible for existing version-7 blobs, but older readers may reject newly written rows. Because blobs are module-pinned, the final schema decision must evaluate whether this compatibility boundary needs a version bump. A later statics merge must preserve both the nine-element seat row and its legacy eight-element decoder when adding its own granted-keyword fields.

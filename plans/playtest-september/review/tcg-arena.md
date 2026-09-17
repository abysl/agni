# TCG Arena importer review

## Changed paths

- `importers/src/riftbound/tcg_arena.rs`
- `importers/src/riftbound/mod.rs`
- `importers/src/riftbound/link.rs`
- `importers/src/riftbound/query.rs`
- `importers/src/riftbound/gateway.rs`
- `wiki/design/tcg-arena-import.md`

## Root cause

TCG Arena uses its own Riftbound deck formats rather than MTG Arena formats. Its standard export is a counted text list with a `Sideboard:` trailer, its starter-deck export is category-based JSON, and its public deck import link embeds the text list as `encodeURIComponent(btoa(decklist))` at `/import`. The existing importer did not recognize that URL or JSON shape, and generic text parsing had no TCG Arena-specific entry point.

## Implementation

The importer parses synthetic TCG Arena text and starter-deck JSON, preserving legend, chosen champion, runes, battlefields, main deck, sideboard, counts, IDs, and optional title. Single pasted `/import` links are decoded locally. The gateway recognizes `tcg-arena.fr` but performs no request for an embedded deck URL; other site fetches retain the existing bounded transport policy and exact-host allowlist. `/load/...` is rejected because TCG Arena uses it for game-file sharing.

## Tests

- `cargo test --locked -p agni-importers --features riftbound --lib tcg_arena`
- `cargo test --locked -p agni-importers --features riftbound --lib`
- `cargo check --locked -p agni-importers --features riftbound-gateway`
- `git diff --check`

The updated Riftbound importer library suite passed with 111 tests. The native-feature TCG Arena suite passed with 12 tests, including end-to-end JSON and URL resolution with title preservation and zero fetches. The gateway feature check passed after the final parser addition.

## Deployment and limitations

Deploy the updated native gateway executable with the private infrastructure pin after this importer lands. No Kai changes are included; Kai owns the UI recognition hint for `tcg-arena.fr/import`. Browser clients can parse pasted text, JSON, and embedded import URLs locally. TCG Arena game-file `/load` links are intentionally not deck imports. No card database, card art, network fixture, or copyrighted source data was committed. No rules or versions were changed.

## Final public-source verification

Inspected the publicly served
[`index-VzYe3nvQ.js`](https://tcg-arena.fr/assets/index-VzYe3nvQ.js)
on 2026-09-16. `Y$e` contains standard-text and starter-JSON export;
`F0e` generates import URLs and `L0e` decodes them. The design document now
records these exact source locators and their conclusions.

The starter exporter assigns the input deck's game without checking presence;
serialization omits undefined values. The parser now accepts an absent JSON
game within the Riftbound context while rejecting explicit wrong/malformed
values. The public Riftbound starter asset currently includes the game field.
The URL generator applies two encoding layers; the importer now decodes both,
including form-encoded title spaces. URLs still require the game parameter.
Synthetic entry-point/query tests cover these corrections and preserve titles.

Upstream limitations: standard text omits chosen-champion section information,
and starter JSON omits sideboard unless it is among the exported categories.
Agni cannot restore information absent from the payload. No public source
bundle, card database, or art was committed.

Final verification: all 14 tests selected by `tcg_arena` passed with
`--offline --locked -p agni-importers --features riftbound-native --lib`, using
`CARGO_TARGET_DIR=/tmp/kai-dusk-rose.adTLQw/agni/target`, three build jobs,
debug info disabled and incremental compilation disabled. Changed Rust files
were formatted; `git diff --check` passed. Broader checks were not repeated
for this bounded verification. This follow-up changes only
`importers/src/riftbound/{tcg_arena,query}.rs`, the importer design document,
and this review summary.

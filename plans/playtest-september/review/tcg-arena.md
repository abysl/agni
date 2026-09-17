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

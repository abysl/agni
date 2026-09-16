# Shared deck types

Audience: Rust developers integrating a game-specific deck model with Agni.

This crate contains the deck pieces several games share: named entries with
counts, expansion/flattening helpers, and saved snapshots. It does not decide
a particular game's deck-construction rules.

A snapshot records cards grouped into zones. Its identity represents the
card list rather than presentation details such as a display name or artwork.
That allows clients to rename a saved deck without treating it as new content.

Use a game's resolved-card type with the shared entry helpers instead of
implementing another count/expansion loop. Keep game-specific legality in that
game's crate.

Run `cargo test --locked -p agni-deck` from the repository root.
When adding snapshot fields, test old data without the new field and verify
whether the field should affect identity.

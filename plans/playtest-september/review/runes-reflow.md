# Rune recycling and Kennen/Reflow review

## Root cause

Power payment selected runes inside `runes_plan` by deterministic table order. That made replay stable but gave the player no choice between rune identities, domains, or ready states. Kennen's Reflow problem was separate: the engine already publishes legal Flow plays from public trash, but Kai had no public-trash browser.

## Changes

- `games/riftbound-turns/src/engine/pay.rs`
  - Marks selected rune identities through the existing serialized payment pin.
  - Offers every legal rune identity instead of collapsing equivalent visible attributes.
  - Uses deterministic backtracking across pinned and unpinned runes so overlapping hybrid needs retain a globally valid assignment.
  - Tests identity choices, ready-state/domain choices, reload, invalid matching, cancellation cleanup, and hybrid matching.
  - Clears rune payment pins before recycled cards move so later channeling and payment cannot inherit a stale selection.
- `games/riftbound-turns/src/engine/play.rs`
  - Pauses pending standard plays, activations, and accepted trigger costs at `STAGE_PAY` when more than one rune can legally recycle.
  - Validates each selected rune before pinning it and leaves all payment effects unapplied until the final plan succeeds.
- `games/riftbound-turns/src/engine/prompts.rs`
  - Reuses `PromptWhy::PayWith` and `Answer::Card` to publish `recycle {card N}` choices without a state or protocol format change.
- `games/riftbound-turns/src/present.rs`
  - Proves a granted Flow spell in public trash is published as a card-bound `play from your trash` affordance carrying `TurnEvent::Activate`.
  - Gives yes/no choices hotkeys `1` and `2`; true cancel remains `X`.
- `games/riftbound-turns/src/engine/fixtures.rs`
  - Exposes a seat-checked helper that tests call explicitly to answer rune-payment prompts.
- `games/riftbound-turns/src/cards/*.rs` and owned engine tests
  - Updates payment-bearing play, activation, and accepted-trigger flows to explicitly answer the new rune prompt while retaining their production assertions.

## Kennen/Reflow status

No Kennen rule patch was needed. `Kennen, Storm of Shuriken` already limits its conquer target to a friendly Spell in the player's public Trash, grants Flow at printed cost through end of turn, publishes the legal Flow activation, and banishes the spell after the Flow play. The user-visible root cause was trash access/discoverability, owned by the Kai UI work.

## Compatibility

No `state.rs`, blob version, ABI, wire format, Cargo manifest, or `Cargo.lock` change was made. Rune selections use the existing serialized `FLAG_PAYING` card flag and existing prompt answer encoding.

## Direct-payment audit

Pending items routed through `play::advance` now ask for rune identity on standard plays, activated abilities, and accepted trigger costs. Direct `pay::pay` callers remain outside this prompt flow, including hide, pay-or-let handling in `engine/prompts.rs`, replacement/prelude helpers, and card-specific immediate payments. Giving those paths interactive rune choice requires a resumable pending-item design and remains a limitation.

## Verification

Passed:

- `cargo test --locked -p agni-riftbound-turns --lib` — 4,558 passed, 0 failed, 176 ignored
- The full run covers standard plays, activated abilities, and accepted trigger costs through explicit rune-prompt answers.
- `cargo fmt --all -- --check`
- `git diff --check`

## Rules basis

The official Rules Hub dated July 16, 2026 remains the current primary source. Checked-in Core Rules 403.6 states that the instructed player chooses cards recycled from the relevant zone. The March 2026 tournament clarification requires a recycled rune to move immediately to the bottom of its Rune Deck. The implementation preserves both choice and immediate movement once payment finalizes.

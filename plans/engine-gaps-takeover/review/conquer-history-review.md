# Harrowing / Perched Grimwyrm: bounded failure disposition

Read-only inspection of `/tmp/agni-play-combined-root.log` and Play WIP based on e9235605. No builds or production edits. The combined run reports 4451 passed, 236 ignored, one failure: `cards::the_harrowing::tests::a_perched_grimwyrm_is_offered_only_with_a_conquest_to_land_on_and_never_goes_to_the_base`.

## Immediate recovery decision

Repair the fixture; retain current Limited prohibition rechecks. In `cards/the_harrowing.rs:358`, the successful half of the fixture seeds `set_holder(BF1, Some(0))` plus `mark_scored(BF1, 0)` without a friendly unit there or an actual Conquer. Its initial offer therefore satisfies the current Perched approximation. After the parent resolves, cleanup removes unsupported control from the empty battlefield. The child's refreshed restriction returns no destination and the play is taken back; the failure log shows precisely that sequence.

Source precision: `cleanup::settle_control`/`establish` can clear the holder; `state::set_holder(None)` does **not** clear `Control.scored`. The failing conjunction loses its holder term, not both terms. This is not evidence that the new Limited recheck should preserve obsolete control or bypass `OnlyPlayLocations`.

Minimal robust fixture: place a friendly unit on BF1, establish an actual Conquer through `cleanup::establish` (assert `Established::Conquered(0)`), and settle before playing Harrowing. Leave that unit present through the parent and child. Preserve the negative half, the two recruit candidates, no-base assertion, actual power payment and final Grimwyrm destination. If convenient, reconstruct table and blob before choosing the recruit so the test also demonstrates persistent current control. Do not merely re-seed holder/scored after cleanup, change the expected result to cancellation, or weaken restriction checks.

This fixes the fixture's claimed current-control scenario without pretending to solve historical Conquer semantics. It is sufficient for the bounded Play recovery failure.

## Existing behavior debt, separately tracked

`cards/perched_grimwyrm.rs:10` implements `conquered_this_turn` as current `holder == seat && scored(zone, seat)`. The supplied card text restricts play to a battlefield the player **conquered this turn**. Current ownership and the generic once-per-turn scoring mask are not a Conquer history:

- `cleanup::hold` and `cleanup::conquer` both call `mark_scored`; a Beginning-phase Hold can therefore falsely satisfy the restriction. This is already explicitly recorded in the ignored Perched test `a_battlefield_held_since_the_beginning_phase_was_not_conquered_this_turn` and `wiki/design/rules-engine.md:4097`'s per-turn-counters row.
- Losing control does not erase a past Conquer. A historical query must retain the occurrence; current destination permissions and restrictions must be checked separately. The existing Perched test that expects `conquered_this_turn` to become empty after changing holder encodes the approximation. Normal hand-play permission may still exclude the lost battlefield; that is not proof the history vanished. A separately granted Limited destination is the decisive case for separating the two predicates.
- Do not derive history from points gained. Core 469.1 defines Conquer; 469.2 defines Hold; 470 shares the once-per-battlefield limit. Core 471.1.b.1 substitutes a draw for a prohibited final point, and 383.4.c.2.c preserves Conquer triggers when gaining the point is negated/replaced. **The present engine already marks scored and raises `Event::Conquered` on its `FinalPointDrawn` path** (`cleanup::conquer:262–274`). Thus final-point denial is an essential acceptance case, not a reproduced missing-event bug in this path. The mask remains insufficient because it cannot distinguish why scoring occurred.

## Bounded follow-up assignment

E1 owns a deterministic persisted per-turn Conquer record keyed by actual seat and battlefield, written at the authoritative Conquer boundary independently of point gain, and reset at the established turn-expiration boundary. A3 supplies/validates capture of the Conquer occurrence and its participants before later instructions mutate the Board. Per-unit consumers such as Blighted Battleaxe need the corresponding captured participant identity, with A1 semantics when that foundation lands. Do not rebuild historical records by scanning only the current request's events or interpreting a current `Control.scored` bit.

The existing plan's E1 “close Kayn debt” wording should explicitly retain this known Conquer-history debt; A3 alone does not provide cross-request per-turn persistence. This is preexisting behavior debt exposed by stricter recovery checks, not a new history regression introduced by Play.

Acceptance for that follow-up: Hold does not qualify; a real Conquer qualifies across save/reload and later loss of control; a separately legal Limited destination can use that history without bypassing mutable prohibitions; final-point replacement/draw still records Conquer; the next turn resets it; history is seat-specific. Retain scoring-once behavior and Conquer/Hold trigger distinctions. No need to expand this recovery failure into a scoring-engine rewrite.

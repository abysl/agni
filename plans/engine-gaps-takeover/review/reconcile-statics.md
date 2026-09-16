# Statics reconciliation checkpoint

This checkpoint merges `origin/gaps/statics` into the reviewed rules and UI
integration on `takeover/reconcile`. The merge had 27 files changed on both
parents and 134 statics-touched files overall. All statics changes are included;
economy is intentionally outside this checkpoint.

## Wire schema

The merged writer emits `BLOB_VERSION` 10. It preserves the rules `wn` winner
field and adds statics `dt` death records on `GameBlob`. Seat rows emit eleven
fields: the legacy eight values, `units_enter_ready_this_turn`,
`next_unit_enters_ready`, and `chosen_champion`. Card rows emit thirteen fields:
ordinary grants, costed grants, attachment/hidden/entered/control state, and
`named`.

The decoder accepts version 7 rules rows and version 9 statics rows through
explicit version dispatch, plus version 10. Version 7 eight/nine element seat
rows and version 9 ten element pre-lock rows are handled deliberately; the
version 9 boolean spell lock maps to `PlayLock::SPELLS`. Version 7 and version
9 length-12 card rows have different layouts and are never guessed by length.
Only version 10 is emitted. Unknown versions remain a fresh-lobby refusal in
line with module pin compatibility.

## Semantic reconciliation

The merged engine retains rules locks, named spell restrictions, scoring,
control, token and deflect behavior alongside statics readiness/suppression,
NoMoveByEnemy/NoUnitsMoveToBase, costed Flow/Repeat/Accelerate grants, implicit
Vision/Weaponmaster, `Event::Entered`, and cross-request death records. The
source-bearing movement API is used by all migrated card and test callers.
Death records are cleared at Expiration and a banish replacement does not
produce a death record or `Died` event.

Entry processing raises `Played` and `Entered` before applying the entry's
exhaustion. A3 must capture entry triggers after the inciting action's entry
state, including exhaustion, is complete. Keeping capture at the current
raise sites would freeze a premature ready snapshot; the refactor must also
avoid emitting the entry twice.

Inventory exceptions remain explicit: Towering Pairofant has both same-request
and earlier-request readiness tests. Fallen Feline retains its active corrected
fixture. Rift Herald and Herald of Scales still await their play/economy lane
replacements and must not be claimed complete at this statics checkpoint.

## Verification

- `cargo test -p agni-riftbound-turns --no-fail-fast -q`: **4366 passed, 0 failed, 294 ignored**.
- `cargo clippy -p agni-riftbound-turns --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo check -p agni-riftbound-plugin --target wasm32-unknown-unknown`: passed.

Remaining follow-up is the pre-existing `Power::intern` process-global leaked
slice cache used by costed keyword decoding. It preserves value equality but
should be replaced with an owned or deterministic cost representation before
metered replay work; changing it is outside this merge checkpoint. That repair
must also test complete legacy v7/v9/v10 blobs, including the two different
length-12 card layouts, before claiming the compatibility dispatch verified.

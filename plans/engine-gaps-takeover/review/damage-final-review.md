# Damage candidate review

Candidate `06546f4b` in `/tmp/agni-takeover-damage`, recovery base `f1df9f10`; `e1aa4506` carries the final Costs correction. Read-only source/test review of the five saved Damage clusters and v13 migration. No builds or edits. Worker final gates were pending at review time.

**Disposition: changes required. Three bounded ordinary-behavior issues must be fixed, with the omitted regression coverage restored.** They are separate from the explicitly deferred A11 replacement-order selection and D1 continuations.

## Must fix

1. **P1 — Real enemy spell resolution bypasses Esteemed Hierophant immunity.** `cards/esteemed_hierophant.rs::from_an_enemy_spell_or_ability` still resolves Cause::Item through `ctx.chain_item`. `chain::run` removes the item and exposes its current execution only through the new `ctx.resolving` / `live_item` seam. Thus an actual resolving enemy spell fails the enemy-controller test. The existing enabled test leaves a synthetic item in blob.chain and cannot detect this. Use the live-item lookup and test a real enemy spell through resolution, preferably after table/blob reconstruction before passes; seven runes must prevent its damage, while own spell and combat damage still land. This was also a weakness in the saved helper, but the recovered advertised immunity is ineffective in its normal execution path until corrected.

2. **P1 — Empty-chain lethal cleanup from the saved lane was omitted.** Saved `ba225839` has an empty-chain branch in `chain::proceed` which runs `cleanup::run(ctx,None)` when `cleanup::dying` is nonempty, before `open_state`. Candidate lacks it. The Lotus Trap test now explicitly calls cleanup after settle, masking the lost normal progress behavior. Port the bounded branch while retaining Play's current Resolving-before-child ordering. Restore the saved outside-resolution lethal regression and assert the normal chain/settle entry point performs cleanup; the test should not invoke the missing cleanup manually.

3. **P1 — Finite prevention wrongly exempts a unit from combat assignment.** Saved `ba225839` uses `ctx.no_damage || prevent::unbounded` in `combat::exempt`, and its lethal early return uses `exempt`. Candidate retains the old `prevent::amount(needed)==0` exemption, while importing the new unbounded helper unused there. A finite Ki Barrier at least as large as the unit's Might is therefore treated as exempt rather than requiring assignment through its finite shield. Restore the saved distinction between N shields and Next/All, with finite-shield assignment versus unbounded exemption tests. This is a concrete lost saved behavior, not the deferred choice between prevention/multiplier orders.

All three issues were sent directly to `luna_ui_resume` and root as found.

## Missing acceptance and test limits

The saved inventory's new `combat`, `ctx`, and `cleanup` regressions were not recovered: assignment to a doubled unit; lethal assigner allocating one per unit; lethal marked damage versus current Might; marker attribution/healing; multiplier stacking/expiry; and lethal cleanup outside resolution. Current combat/cleanup diffs contain production changes without their saved new tests. Recover those focused cases, adapting them to the current APIs and saved table/blob boundaries. Enabled card tests and the per-unit prevention test are useful but do not establish these pipeline/provenance contracts.

In particular, `ravenborn_tome::the_next_spell_this_turn_deals_one_more_and_the_one_after_does_not` currently plays only one spell; extend it to actually play the second spell and assert ordinary damage. `lotus_trap::every_damage...until_the_turn_ends` kills the trapped unit and later damages a different untrapped unit, so it does not prove expiry on a surviving trapped unit. The saved stacking/expiry regression should fill that gap. Add one reconstruction across a real pending damage item so `resolving` is reconstructed by chain execution and attribution does not depend on retaining a Ctx across requests.

## Schema review

The three preliminary decoder source findings are closed: v11 and v12 SeatState require 14 fields; damage marks decode only in strictly increasing seat order; Amount::Next's boolean is v13-only. v13 preserves the 14-field seat prefix and appends two bonus fields; preserves the 13-field card prefix and appends multiplier/marks; ChainItem remains 14 fields with unchanged Limited, execution, awaiting and mode/optional slots. Old prevention rows remain three fields with unit=None; v13 rows are four fields. A9 owned costed grants, named/control fields, Economy promises/pool and Play continuation fields are retained.

The three card-test v11 helpers now lower seats, cards and chain/pending rows, and explicitly reject unexpected nonempty `pv`, so they do not merely relabel v13 bytes. `tests/blob_compatibility.rs` preserves independent old input writers and separately updates canonical expected fields/defaults to v13. Its helper name `canonical_v12` is stale naming only. The new independent v12 case includes nonzero seat promise/pool/champion and an old prevention; nonzero named/control/owned-grant preservation is covered by the older independent input cases.

One narrow test correction remains advisable before handback: `malformed_old_seat_and_unsorted_damage_marks_are_rejected` writes an array header with no body for the bad v11 row, so it would fail even with the previously missing length guard. Supply a complete valid-looking 14-value body under the wrong declared length to distinguish the guard. Its marks case exercises duplicates, not descending keys; add descending keys and legacy Next-bool rejection. These strengthen the now-correct source checks rather than identify another production bug.

## Preserved and deferred behavior

The recovered normal pipeline is NoDamage → saturating bonus → matching per-unit/global prevention spend → saturating multiplier → damage counter and ordered marker attribution. Play finalization binds next-spell bonus after the item receives its ordinal, and chain callbacks expose their execution view without replacing the current deferred-child runner. Costs' strict Legend-zone and Disempower corrections and captured Empowered/Banished actors are retained. No Economy slot reassignment, early additional payment stage, leaked cost interner or replacement of owned grants was found.

Controller-selected replacement order, assignment-time choice persistence and exactly-once replacement application remain A11. Kill/death callback suspension and transient death-queue rollback remain D1/A5. Cause::Ability current-source attribution and object references still have the separately planned provenance/incarnation limits; this review does not close A1/A2/A4a. Kayn's move-count ignore remains E1. The finite-shield correction above should preserve the saved ordinary behavior without claiming those foundations implemented.

Re-review the small corrective delta and its tests, then run the planned fresh merged artifact/parity/Kai gates. No gate results were independently reproduced here, and no complete Damage acceptance is issued for `06546f4b`.

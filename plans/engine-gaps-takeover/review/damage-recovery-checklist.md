# Damage recovery checklist

This is a read-only comparison of integration `ba3a7b6d` with all five saved
Damage commits: `6618bb67`, `3c6275de`, `eab5ca39`, `a1537612`, and
`ba225839`. They are separate behavior/schema steps, not one cherry-pick.
Port them against the Economy, Play, and Costs recovery plans and retain the
old readers and original test dispositions.

## Saved commit clusters

- `6618bb67`: `Static::BonusDamage`, cause-keyed `Ctx::damage`, resolving-item
  lookup, and the next-spell bonus. It changes `SeatState` from an 8-field to
  a 10-field row and `BLOB_VERSION` 6 to 7.
- `3c6275de`: `Static::NoDamage` is consulted before bonus damage and numeric
  prevention; combat assignment also exempts a unit whose static applies.
- `eab5ca39`: `Prevention` gains `unit: Option<u32>` and `Amount::Next`, making
  the row four fields and `BLOB_VERSION` 8; reduction must filter by unit and
  expire through the existing `until` rule.
- `a1537612`: `CardState.damage_multiplier_this_turn` is a twelfth card-row
  field and `BLOB_VERSION` 9; the saved path multiplies after prevention and
  uses the resulting amount for marks and `DamageDealt`.
- `ba225839`: ordered per-seat `damage_marks`, `damage_by`, source markers,
  `Static::LethalDamage`, and cleanup/combat lethal checks add a thirteenth
  card-row field and `BLOB_VERSION` 10.

## Concrete reconciliation hazards

1. **Blob row unions.** Integration `state.rs` already writes a 13-field
   `CardState` containing owned `granted_costed` and `named` (A9/statics), and
   `SeatState` has the 11-field statics row. The saved Damage rows use the same
   historical arities for different fields. Append multiplier and mark data
   after the integrated cost/named fields, extend the final Economy/Costs row
   explicitly, advance the integrated schema version when needed, and preserve
   all integrated legacy readers (including Economy/Costs versions). Check exact array
   lengths, field order, `Amount::Next`'s CBOR boolean, and malformed-row
   rejection; a compiling decoder or inferred row length is insufficient.

2. **Damage precedence.** The saved implementation order in
   `engine/ctx.rs` is: `NoDamage` can end the damage action without spending numeric
   prevention or raising `DamageDealt`; `BonusDamage` is included before
   prevention; the saved multiplier is applied after prevention; only the
   amount that lands receives a mark and event. This fixed replacement order
   is preexisting A11 debt: Core 372/437.7/465.2.c.5 lets the affected controller
   order prevention and doubling. Preserve the distinction between recovery
   and correctness; D2 must add that choice and assignment/deal bookkeeping.
   Bonus Damage remains included before replacement, per 715.4.a. Keep `DamageSource`/`Cause`
   coverage in `engine/prevent.rs` explicit. Do not generalize an immunity or
   prevention exclusion beyond the static/cause predicates in the saved code.

3. **Combat and lethal statics.** Reconcile `engine/combat.rs` and
   `engine/cleanup.rs` so immunity, `Amount::All`/`Next`/`N`, multiplier ceiling,
   and marked `Static::LethalDamage` agree in both assignment and cleanup.
   A numeric shield is not itself an immunity, and a lethal static applies
   only through its declared source/unit predicate. Test a prevented hit, a
   multiplied hit, a lethal mark, and open-state cleanup after damage between
   chain items; do not turn narration or a compile-only static into proof.

4. **A2 attribution.** Keep `Ctx::marker_of` and `damage_by` as separate
   seams. The saved source is a live item's controller, an ability source's
   controller, the opposing side for combat, and no marker for rule, cost, or
   replacement damage. `cards/prelude.rs` passes an item's controller even
   when the item has left the chain; `engine/combat.rs` passes the opposing
   seat. Challenge/Bonus Damage, prevention, marks, and Elder Dragon must use
   the same marker rather than the damaged card's owner.

5. **A1 identity and timestamps.** `Prevention.unit`, `damage_marks`, pending
   items, `TargetRef`, and `deaths_this_turn` currently carry physical ids or
   snapshots. When a card crosses a logical non-Board boundary, A1 must
   invalidate or version the protected unit and every captured source; a
   shield must not protect a fresh object reusing its id. `Ctx::live_item` and
   resolving-item attribution must remain bound to the original chain item.
   Preserve captured “moved from”/cause data rather than reading a later card
   incarnation, as required by `review/object-identity-design.md`.

6. **A5/A6 cleanup rollback.** Damage can enqueue deathknells and then run
   `cleanup::dying`; `engine/chain.rs::Checkpoint` currently does not restore
   transient `Ctx::deaths`. Extend the rollback contract before accepting
   damage-trigger continuations: an invalid effect after a marked lethal hit
   must restore marks, counters, events, cleanup work, and deathknells with no
   ghost trigger. Preserve `deaths_this_turn` serialization separately.

## Test and dependency disposition

Carry forward the Bonus Damage tests for Rabadon's Deathcrown, Ravenborn Tome,
and Void Gate; Ambessa/Esteemed immunity tests; Counter Strike/Ki Barrier
unit-shield tests; Lotus Trap multiplier tests; and Elder Dragon attribution
and lethal-cleanup tests. Keep Kayn - Unleashed's ignored test explicitly
dependent on the missing per-card move-counter row (E1); the NoDamage static
port does not cover it. Preserve every remaining ignore until its replacement
asserts bytes, expiry, attribution, prevention/multiplier order, cleanup, and
final state. Enabled source is not rule proof.

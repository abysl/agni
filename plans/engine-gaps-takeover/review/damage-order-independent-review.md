# A11: damage replacement order

Read-only review of saved damage tip `ba225839`, the immutable takeover inventory, and pinned `games/riftbound/rules/riftbound-core-rules-v2026-07-16.txt`. No production edits, builds, or tests. The earlier reconciliation worktree has been removed; all saved code was read with `git show` from the integration checkout.

## Confirmed rule gap

**P2, separate rules debt:** the affected unit's controller cannot choose the order of damage prevention and Lotus Trap's damage multiplier.

- Core 372 (local text line 3188) assigns replacement order to the controller of the affected object. Rule 437.7 (4733) expressly makes Prevent a delayed replacement effect.
- Core 465.2.c.5 (5186 onward) applies damage replacements during combat assignment. Its explicit example uses 3 incoming damage, a 2-Might unit, Prevent 2, and Lotus Trap: prevention first yields 2 damage dealt; doubling first yields 4. The affected unit's controller chooses, even though another player assigns the damage.
- The preceding example says a point doubled to 2 during assignment does not double again when dealt. Core 465.2.c.4.a also requires the minimum original allocation that becomes lethal after modification when further units remain.
- Bonus Damage is not an arbitrary reorderable stage: 715.4.a (5997) and 437.1.a.1 (4682) require reductions/replacements to include it in the incoming total. This finding does not establish that every step of the current pipeline should become a replacement option.

## Saved code evidence

All paths below are under `games/riftbound-turns/src/` at `ba225839`.

- `engine/ctx.rs:1663`, `damage_by`: early NoDamage return, then bonus, `prevent::spend`, then `damage_multiplier`; it emits counters and DamageDealt synchronously. No controller-order selection or persisted replacement continuation exists here. The exact rules example can only take the prevention-first branch.
- `engine/prevent.rs:18`, `reduce`: prevention rows are consumed in vector order. Their insertion order can affect which finite/Next shield remains for a later hit. This warrants inclusion in the same replacement-instance audit rather than treating prevention as one orderless scalar.
- `engine/combat.rs:117`, `lethal`: `ceil(needed / factor) + prevent::held(...)` hardcodes prevention before doubling. For a 2-Might unit with Prevent 2 and factor 2, it requires 3 budget points; doubling first instead reaches lethal with 2. This changes legal allocation to subsequent targets.
- `engine/combat.rs:279`, `assign`: persists only `(unit, raw_budget_amount)` while subtracting that raw amount from remaining Might. `deal` at line 319 passes those raw values to `damage_by`. There is no persisted affected-controller replacement choice or processed-assignment marker.
- This raw-budget representation currently applies the multiplier only at deal time. The review therefore does **not** establish an existing double-doubling bug. The required future assignment-aware change must avoid applying a replacement again at deal time and must preserve budget, modified assignment, prevented amount, and final dealt amount coherently.
- `ctx.rs::tests::a_damage_multiplier_stacks_applies_after_prevention_and_lapses_with_the_turn` and `combat.rs::tests::a_doubled_unit_is_assigned_the_least_that_doubles_to_lethal` assert the fixed order. Their explanatory strings say the controller chooses that order, but neither test supplies such a choice. The ordinary Lotus Trap card regression tests doubling without interacting prevention.

These behaviors already exist in saved commits, notably `a1537612` before `ba225839`. They are not introduced by the takeover merge. Recovering that code preserves the gap; merge review should keep this disposition separate from merge regressions.

## Inventory and bounded assignment

No existing inventory cluster covers controller ordering of damage replacements. `turn-scoped-damage-multiplier` explicitly requests multiplication after prevention and lists the isolated Lotus Trap test. `per-unit-prevention` tests shield scope and consumption. D1's kill-path and floating replacements concern replacing death, while D2's Might layers and stun/Might/bounce work concern other operations. None is an adequate disposition for this missing damage choice.

Add A11 as an explicit damage-replacement-ordering subtask under D2, or a separately named damage batch, after A1 object identity, A2 damage provenance, A4a stable replacement/ability identity, and A5 persisted suspension with atomic rollback. It is campaign followup debt, not a reason to relabel a faithful damage recovery as a merge failure.

Minimum regressions:

1. The exact 465.2.c.5 scenario: affected controller selects each order on separate runs; observe 2 versus 4 damage dealt from the same 3 incoming budget. Assert chooser identity, emitted damage amount/provenance, shield consumption, and reconstruction across the order prompt.
2. Two defending targets: order choice changes the minimum raw amount needed for lethal and leaves the correct remaining attack budget. Keep all combat damage simultaneous through any suspension.
3. The rule's 3-budget/two-2-Might-unit example: 2 raw points kill the ordinary unit and 1 raw point doubles to 2 for the trapped unit. Dealing must produce 2, not 4, on the trapped unit; persisted assignment must survive reload without applying or consuming replacements twice.
4. Independently tracked prevention instances, including Next plus finite prevention, retain the appropriate remaining shield according to the chosen order. Invalid choices/refusal/replay must not leak partially spent prevention or marked damage.

This recommendation does not prescribe a new architecture or broaden the active recovery worker's implementation scope.

# Damage recovery inventory

This is a read-only audit made 2026-09-14 after reading `wiki/design/architecture.md`.
The architecture requires deterministic, ordered state and replayable inputs; this
lane must therefore keep damage provenance and replacement state in the versioned
blob rather than in callback locals or process state.

## Saved order and scope

The saved branch is a linear five-commit cluster. Recover in this order, retaining
each commit's tests and readers while reconciling it with the newer baseline:

1. `6618bb67` — Bonus Damage, cause-aware `Ctx::damage`, resolving-item lookup,
   and the next-spell bonus. Files: `cards/mod.rs`,
   `cards/{rabadons_deathcrown,ravenborn_tome,void_gate}.rs`,
   `engine/{chain,ctx,play}.rs`, `state.rs`, and `wiki/design/rules-engine.md`.
   It changes the historical blob from v6 to v7 and adds `SeatState`'s
   `next_spell_bonus` and bound `spell_bonus`.
2. `3c6275de` — `Static::NoDamage`, cause filtering in `Ctx::damage`, and combat
   assignment exemption. Files: `cards/{ambessa_the_wolf,esteemed_hierophant,kayn_unleashed}.rs`,
   `cards/mod.rs`, `engine/{combat,ctx}.rs`, and the rules wiki.
3. `eab5ca39` — per-unit `Prevention` and `Amount::Next`/`Amount::N`, with
   unit-aware reduction and expiry. Files: `cards/{counter_strike,ki_barrier,unyielding_spirit}.rs`,
   `engine/{combat,ctx,prevent}.rs`, `state.rs`, and the rules wiki. Historical
   blob v8.
4. `a1537612` — turn-scoped per-card damage multiplier, applied after prevention,
   and Lotus Trap/combat assignment coverage. Files:
   `cards/lotus_trap.rs`, `engine/{chain,combat,ctx}.rs`, `state.rs`, and the
   rules wiki. Historical blob v9.
5. `ba225839` — ordered per-seat damage marks, source marker attribution,
   `Static::LethalDamage`, and lethal cleanup/combat assignment. Files:
   `cards/{elder_dragon,mod,prelude}.rs`, `engine/{cleanup,combat,ctx}.rs`,
   `state.rs`, and the rules wiki. Historical blob v10.

The tip is faithful damage behavior, not the later A11 replacement-order fix. It
always runs prevention before the multiplier and does not persist an affected
controller's choice. That is pre-existing A11 debt and must remain explicit.

## Test inventory

The enabled or newly enabled damage tests are:

- Bonus Damage: `rabadons_deathcrown::{the_script_is_a_rainbow_equipment_with_three_might_and_a_bonus_damage_static,your_spells_and_abilities_carry_three_bonus_damage_while_the_crown_is_attached,a_spell_of_the_wearers_controller_deals_three_more_through_the_engine}`;
  `ravenborn_tome::the_next_spell_this_turn_deals_one_more_and_the_one_after_does_not`;
  and `void_gate::{spells_and_abilities_carry_one_bonus_damage_against_units_here_only,combat_damage_and_rule_damage_carry_no_bonus,a_unit_arriving_here_picks_the_bonus_up_and_one_leaving_sheds_it,a_spell_dealing_one_to_a_unit_elsewhere_deals_exactly_one,a_spell_dealing_one_to_a_unit_here_deals_two}`.
- No Damage: `ambessa_the_wolf::while_empowered_she_is_dealt_no_damage_unless_she_is_in_combat`;
  `esteemed_hierophant::with_seven_runes_an_enemy_spell_deals_it_nothing_while_your_own_and_combat_still_land`;
  and `combat::a_unit_that_cannot_be_dealt_combat_damage_is_exempt_from_assignment_and_marks_nothing`.
  Kayn's `after_two_moves_in_a_turn_he_takes_no_damage` remains ignored because
  the per-card move counter is a separate E1 foundation.
- Per-unit prevention: `counter_strike::the_next_damage_to_the_unit_this_turn_is_prevented_once_and_other_units_still_take_theirs`;
  `ki_barrier::the_next_seven_damage_to_the_unit_this_turn_is_prevented_across_instances_and_others_take_theirs`;
  `prevent::a_shield_on_one_unit_spends_only_on_that_unit_and_lapses_at_expiration`;
  and the existing `prevent::a_kill_attributed_to_an_item_under_prevention_marks_nothing_and_fires_nothing_after`.
- Multiplier: `lotus_trap::every_damage_dealt_to_the_trapped_unit_this_turn_is_doubled_until_the_turn_ends`;
  `combat::a_doubled_unit_is_assigned_the_least_that_doubles_to_lethal`;
  `ctx::lethal_damage_marked_outside_a_resolution_is_cleaned_up_when_the_chain_proceeds`;
  and `ctx::a_damage_multiplier_stacks_applies_after_prevention_and_lapses_with_the_turn`.
- Damage marks and lethal static: `cleanup::dying_reads_a_lethal_damage_static_of_a_card_the_marker_controls_before_might`;
  `combat::an_assigner_whose_damage_is_lethal_assigns_one_per_unit_and_the_hit_units_die`;
  and `ctx::damage_is_marked_by_the_controller_of_its_cause_and_a_heal_clears_the_marks`.
- State coverage in the commits updates the quiet-blob version bytes and the busy
  blob round trip. When ported, those fixtures must cover the current v12 rows,
  damage multiplier/marks, all present promises/grants/named fields, and explicit
  old-reader behavior.

## Reconciliation with Play and Costs

Against Play tip `7b94eec0`, damage overlaps `cards/mod.rs`,
`cards/prelude.rs`, `engine/ctx.rs`, and `state.rs`. The direct Play behavior
that must survive is the resolving-item/child-deferral seam in `chain.rs` and
`ctx.rs`, the current `play.rs` finalization path, projected moves, captured
Limited permissions, and all existing source attribution. The damage branch's
`ctx.resolving` field and `live_item` lookup must be merged into that seam rather
than restoring the old chain runner.

Against the Costs baseline, the relevant commits are `2712d6e6` (legends in
their zone can be Empowered), `c92d252f` (Disempower activation cost), and
`4d661781` (captured actors on Empowered/Banished events). Their shared engine
surface is `cards/mod.rs`, `engine/ctx.rs`, and `state.rs` (plus the rules wiki).
Damage must preserve the current legend-zone checks, disempower payment, and
captured actor fields while reconciling its own damage state. Damage's
historical CardState rows and v7–v10 SeatState readers cannot be reused by
length guessing. The current writer is v12: append damage fields to the current
rows in an explicit new version, preserve `granted_costed`, `named`, promises,
pool, chosen champion, PlayLock and counters, and retain every accepted legacy
reader. The current economy representation must not be replaced by the old
`next_discount` pair.

`engine/{prevent,cleanup,combat}.rs` and the card files are otherwise ordinary
portable damage pieces once their APIs are reconciled. The static declarations,
unit-scoped prevention, multiplier state, marker accumulation, and immediate
damage tests can be ported independently of the economy pricing code. The rules
wiki must be manually reconciled with the current reviewed docs; do not merge its
historical rows mechanically.

## Continuation dependencies and disposition

The saved lane does not implement A11's controller-selected prevention/multiplier
order. `damage_by` is synchronous, `combat::assign` stores raw amounts only, and
`combat::deal` applies the replacement pipeline once at deal time. Full A11/D2
work needs the typed serialized operation continuation described in
`review/replacement-continuation-design.md`, including original amount,
provenance, replacement identities, assignment cursor, and exactly-once result
consumption. The 3-damage/Prevent-2/Lotus Trap case must yield the controller's
chosen 2 or 4, survive reload, and avoid double application.

D1 is the corresponding kill/death continuation foundation. Damage's lethal
cleanup calls `lethal_kills` and can enqueue deathknells; a suspended replacement
must keep the source Resolving, preserve simultaneous death membership and
transient death queues, and resume the suffix exactly once. Complete A1 object
identity, A2 provenance, A4a source identity, A5 rollback, and A6 bounds before
claiming damage replacement callers are complete. The existing A11 review also
calls out cleanup rollback of `Ctx::deaths`, which the current checkpoint does
not restore.

Therefore the ordinary static/damage recovery is portable as a bounded lane, but
its acceptance must retain the explicit A11/D1 debt and the remaining Kayn/E1
ignore. Required future commands after implementation are the full
`cargo test -p agni-riftbound-turns`, blob and match integration suites, native
and hardened replay parity with damage/cleanup continuations, warnings-denied
clippy, scoped fmt/diff checks, and the explicit wasm/Kai gates. No builds were
run for this inventory.

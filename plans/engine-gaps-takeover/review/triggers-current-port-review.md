# Trigger lane port review against the recovered baseline

Read-only check of integration `review/triggers-recovery-checklist.md`, saved tip `7dfaef5f`, reviewed Costs production and Damage v13 direction. No production edits/builds. This covers the five saved trigger commits only; it does not implement or approve the remaining twenty clusters.

## Recovery sequence and preserved evidence

Port these deltas serially, but accept the lane only at the corrected final representation:

1. `e980b53a`: UnitDies(Who), captured Noted.buffed, departed token faces and seven formerly ignored card regressions. Preserve `engine/triggers::a_unit_death_reaches_the_friendly_or_enemy_watchers_and_carries_its_snapshot`, source-self exclusion, and friendly gear/enemy negative cases.
2. `3fa5ba42`: fifteen attachment/wearer regressions and Ability/Copied/Mirror grants. Do not leave the intermediate gear-as-source item representation in the final port.
3. `57f1f8ac`: three aura/borrowed activation regressions (Forge, Gardens, Heimerdinger), including Legend scope and holder payment/exhaust. Do not leave dynamic holder-offset lookup as the final queued representation.
4. `786200d1`: six excess-dependent gameplay regressions AND the Granted/Lent holder/lender repair. These are independent behavioral responsibilities bundled in one commit; neither may disappear during conflict resolution. Preserve the additional detach/control-change/dead-lender/copy-self tests listed below.
5. `7dfaef5f`: Who::Any/Enemy expansion for Move/Attacks, parameterized Readied/ChosenFriendly, effect mover actor and six enabled watcher cases. Preserve the existing saved Mask/Lillia/Smoke and Mirrors continuation evidence and its 4+3=7 result without claiming it fixes A3 chronology.

Keep an immutable original-test→ported-test mapping. Capture both newly enabled tests and added regression tests from each commit, not just removed ignore attributes. `the_zero_drive` changes its ignore explanation in commit 4; its full banished-with/reclaim test remains ignored and must not be counted as recovered. Hextech Gauntlets' equipment-cost ignore belongs to the separate cost lane; retain any stronger already integrated cost behavior instead of reinstating that historical ignore.

## Schema checklist

- Baseline is Damage v13: SeatState 16 fields, CardState 15 fields, ChainItem 14 fields with Limited; Economy's promise/pool/gear counters and A9 owned grants remain authoritative. The saved lane's BLOB_VERSION=7 is a different history and must never replace these readers or be guessed by row length.
- Trigger schema needs the next unused plugin blob version after its actual integrated parent (v14 if Damage v13 is still the parent; advance if fourth Costs has already used it). Freeze this before writing fixtures. Neutral engine snapshot/ABI versions are independent.
- Noted appends buffed as its fifth field. All accepted older plugin versions retain their four-field Noted decoder and migrate buffed=false; new version requires five. Preserve all nested occurrences in chain and queued Pending items. ChainItem itself remains 14 fields; do not remove Limited or add a spurious fifteenth item field merely for nested Noted.
- Add Granted `{holder,lender,index}` and Lent `{holder,lender,index}` as four-element ItemKind arrays under tags 4/5, with the old three-element tags 0–3 unchanged. Reject new tags under unsupported legacy versions and reject incorrect arities/unknown variants. Do not auto-rebind ambiguous old dynamic granted offsets to a new lender during import.
- Add `xd` rows `[seat,zone,amount]`, preserving None versus Some(0), canonical order, replacement of the existing seat/zone record and Expiration clearing. Keep records through combat-end healing. Preserve every existing top-level key and v13 Damage field.
- Independent bytes must cover old v12/v13 chain AND pending items with nonzero Noted, Limited, mode/Repeat slots, awaited/remembered state, seat promises/pool and owned grants. Separately author new expected encoding and active Granted/Lent/excess roundtrips. Do not produce fake old fixtures by changing only the version byte; lower new nested rows or retain immutable historical input writers.

## Concrete current-baseline port hazards

- **Moved actor:** saved `Ctx::arrived` sets nonstandard `Event::Moved.by = Some(self.actor)`. That is the request/pass actor, not necessarily the captured controller of the resolving effect. Port the new field with an explicit item-controller path through effect movement helpers; preserve Standard=None. A source changing control or a different seat delivering the last pass must not attribute Blast Cone's event to that seat. Damage's current execution view can support the bounded synchronous callback path, but does not replace A2's later complete provenance model. Preserve current Empowered.by and Banished.{by,owner,token}; the saved trigger Event enum lacks those Costs additions.
- **Excess versus Damage attribution:** saved `record_excess` runs in `combat::deal` after `showdown.take()` and calls `lethal(ctx,unit)`. Current Damage lethal reads `assigner(ctx)` for Elder Dragon, which is None after the take. Compute lethal/excess with the explicit local assigner/side and showdown, rather than the now-absent ambient state. Retain corrected finite-vs-unbounded prevention, multiplier-aware required assignment and explicit combat damage marker. Add an Elder Dragon excess case, finite prevention/multiplier case, and both sides' separate records. This is a bounded integration correction; it does not implement A11 controller-selected order.
- **Whole item pricing/classification:** port Granted as a trigger and Lent as an activation through `play::path_of/finalize`, `activate::self_cost_of_item`, `cost::base_of_item/activation_item`, targeting, offers, additional quotes, prompts, chain leave and narration. Do not copy saved `discounts_of`'s old free-only ability branch over Economy's current complete-item rules. Holder pays, exhausts and supplies “me”; lender supplies text and “this.” Keep Disempower prevalidation and `banish_by(...,item.controller)`, which are missing in the saved activation payment body. Preserve gear activation counters according to the actual holder, not merely the lender's card kind.
- **Damage callbacks and chain runner:** Granted/Lent causes must resolve through the current live-item lookup and captured item controller. Retain Damage's resolving execution view, pending bonus binding, marks and expiry, and the corrected empty-chain lethal cleanup. Retain Play's Resolving-before-child handoff, per-execution targets/remembered suffix and deferred child declarations; do not replace chain.rs from the saved trigger branch.
- **Statics and departed faces:** retain current active-face, proper Legend zone, attachment and projected-grant rules when adding ability grants. Capture a token's public departed face before Despawn/shed, because looking it up afterward cannot distinguish a Recruit. Keep death controller/unit/buffed facts from before departure. Do not import old token vocabulary or old counter eligibility with the ctx merge.
- **Determinism boundary:** do not newly import `interchangeable`'s pointer-based prompt elision. Conservative ordering for a multi-item batch requires no stable-identity redesign; do not declare same callback address semantically interchangeable. Full stable ability/lender identity, independent once accounting and stale activation-index binding remain A4a. Any retained dynamic-index admission path must keep its existing explicit limitation, with checked finite bounds rather than new u8 aliasing; A6 covers the broader audit.

## Required saved regression inventory

Preserve these exact enabled tests (file stem followed by function):

- altar_of_memories: `a_friendly_unit_dying_asks_to_exhaust_the_altar_draws_one_and_places_a_card`
- shard_of_undoing: `the_first_friendly_death_in_your_beginning_phase_makes_each_opponent_kill_one_of_theirs`
- spectral_centaur: `every_other_friendly_death_gives_two_might_this_turn_and_an_enemy_death_gives_nothing`
- vanguard_helm: `a_buffed_friendly_unit_dying_triggers_the_helm_to_buff_another`
- vicious_snapjaws: `every_other_friendly_death_gains_one_xp_and_an_enemy_death_gains_nothing`
- viktor_leader: `a_friendly_units_death_queues_his_trigger_and_his_own_recruits_death_does_not`
- wraith_of_echoes: `the_first_friendly_death_each_turn_draws_one_and_the_second_draws_nothing`
- blighted_battleaxe: `at_the_end_of_your_turn_a_wearer_that_did_not_conquer_drops_the_axe_and_takes_four`
- boneshiver: `a_conquer_by_the_wearer_channels_through_the_engine`
- cull: `a_conquer_by_the_wearer_queues_the_gold_trigger`
- dorans_ring: `a_conquer_by_the_wearer_cycles_a_card_through_the_engine`
- eye_of_the_herald: `a_move_by_the_wearer_plays_a_recruit_through_the_engine`
- forgefire_cape: `an_attack_by_the_wearer_blazes_through_the_engine`
- last_rites: `while_attached_the_wearers_conquer_or_hold_may_play_a_unit_from_your_trash`
- pendulum_blade: `a_move_by_the_wearer_swings_through_the_engine`
- recurve_bow: `an_attack_by_the_wearer_shoots_through_the_engine`
- sacred_shears: `the_wearers_death_draws_one_through_the_engine`
- skyfall_of_areion: `a_hold_by_the_wearer_queues_both_of_its_triggers_through_the_engine`
- svellsongur: `while_attached_the_wearer_has_its_own_text_twice`
- trinity_force: `a_hold_by_the_wearer_scores_through_the_engine`
- warmogs_armor: `a_conquer_by_the_wearer_buffs_it_through_the_engine`
- world_atlas: `a_hold_by_the_wearer_plays_the_golds_through_the_engine`
- forge_of_the_fluft: `while_the_forge_is_held_the_holders_legend_lists_and_activates_the_lent_ability`
- gardens_of_becoming: `a_unit_here_lists_and_activates_the_lent_ability_and_loses_it_when_it_leaves`
- heimerdinger_inventor: `activating_a_borrowed_ability_exhausts_him_and_not_its_owner`
- hextech_gauntlets: `a_conquer_after_an_attack_with_three_excess_draws_one_through_the_engine`
- sivir_ambitious: `five_excess_damage_after_her_attack_may_be_dealt_to_an_enemy_unit`
- trapping_grounds: `three_excess_damage_after_an_attack_plays_a_bird_when_the_trigger_resolves`
- tryndamere_barbarian: `five_excess_damage_after_his_attack_scores_a_second_point_when_the_trigger_resolves`
- vi_piltover_enforcer: `three_excess_damage_after_an_attack_lets_her_exhaust_to_ready_a_unit`
- yeti_brawler: `four_excess_damage_after_his_attack_plays_two_golds_and_two_excess_does_not`
- ahri_nine_tailed_fox: `an_enemy_marching_into_her_held_battlefield_fires_the_trigger_through_the_engine`
- back_alley_bar: `an_enemy_unit_moved_from_here_by_an_effect_gets_the_bonus_too`
- blast_cone: `moving_an_enemy_unit_asks_to_exhaust_the_cone_and_yes_stuns_it`
- pirates_haven: `readying_a_friendly_unit_gives_it_one_might_this_turn`
- the_dreaming_tree: `the_opponent_choosing_their_own_unit_here_with_a_spell_draws_too`
- volibear_imposing: `an_opponent_moving_to_another_battlefield_draws_him_one_when_the_trigger_resolves`

Also mandatory from commit 4: Forge `a_pending_activation_resolves_after_the_forge_changed_hands`; Gardens `a_pending_activation_resolves_after_the_unit_left_the_gardens`; Heimerdinger `a_pending_borrowed_activation_resolves_after_its_lender_left_play` and `he_borrows_an_exhaust_ability_a_friendly_unit_itself_only_holds_by_grant`; Svellsongur `the_copied_text_resolves_as_the_wearer_so_blitzcranks_hold_returns_him_and_not_the_gear`; Warmog's `a_detach_before_the_trigger_resolves_still_buffs_the_wearer_and_a_dead_wearer_gets_nothing`. Add explicit table/blob reconstruction before these queued resolutions, not just live-Ctx mutations, and replay one Granted plus one Lent case under native/hardened artifacts.

## Gate and deferred scope

Port on the final green Damage parent, not the presently reviewed-but-correcting candidate. Freeze schema and original test mapping before coding; review the five-cluster production delta before full gates. Then run library, independent blob/MatchState, native/hardened pending-item parity, scoped fmt/clippy/wasm and Kai prompt/brain gates. Record substitutions; do not drop a newly failing test because a manual helper still passes.

A3 owns captured-event persistence, simultaneous trigger membership and legal ordering boundaries; adding Who does not fix those. A4a owns stable ability/lender/once/request identities; saved Granted/Lent physical addresses are a useful ordinary recovery but not incarnation-safe identities. D1/A5 own suspended kill/replacement prefixes and departed/death queue rollback. A11 owns affected-controller damage replacement ordering. These remain explicit later work; this checklist authorizes no expansion into the remaining clusters.

# Costs first three clusters: independent review

Candidate bfe8bbc8 in `/tmp/agni-takeover-costs`, base 432852f4. Read-only production/test review; no builds. Additional-stage d0639152 is absent and is not reviewed as complete. The reported 4473 library / 222 ignored, 10 compatibility, 10 MatchState and formatting/clippy/wasm results are worker reports, not independently reproduced here.

**Disposition: fix two bounded source gaps before accepting the three-cluster recovery.** No loss of the integrated Play/Economy/A9 pricing or serialization was found in this delta.

## Must fix: new legend counter eligibility admits off-zone Legends

`engine/ctx.rs::bump_counter` and `set_counter` now call new `Ctx::in_play`. That delegates to `face_in_play`, whose Legend arm is simply `held.is_kind(KIND_LEGEND)`, independent of zone. Consequently a Legend in Hand/Trash/another non-play zone can now receive/remove Empower counters; the old board-only counter gate rejected it. `empower_lands_on_a_legend_in_its_zone_and_not_on_a_card_in_hand` tests the positive Legend case and a normal hand unit, so it does not detect this regression.

Require the proper Legend zone for the Legend exception. Keep the legitimate board and attachment behavior; do not broadly rewrite A1's logical-zone foundation here. Add a Legend in its own zone positive test, off-zone Legend negatives, and a disempower attempt against a non-play source asserting no mutation. This was an existing weakness in the shared predicate, newly exposed to counter writes by this recovery.

## Must fix: actor propagation remains incomplete in item-owned self effects

The targeted empower/banish effects correctly pass `item.controller`, but several item callbacks still invoke convenience helpers which infer the actor from the **current object's controller**. `Ctx::controller` reads `CardState.controlled_by` or physical owner, not the resolving ChainItem's controller. If the source changes control after its ability is placed on the chain, the new Event.by is wrong and can trigger the wrong player's YouEmpower/YouBanish effects.

Concrete live paths to fix or explicitly prove equivalent:

- `cards/prelude.rs::empower_run` → `ctx.empower(item.kind.source())`, covering the shared Empower activation vocabulary;
- `cards/kayle_justified.rs::ascend` → `empower_once_more`, including both the first-counter `ctx.empower` path and its direct `Event::Empowered` constructor;
- `cards/ambessa_matriarch_of_war.rs::empower_me`, also reused by Mel Soul's Reflection and Zed Master of Shadows;
- `cards/mel_defiant_soul.rs::empower_by_discarding_a_spell`, `escaped_grayback.rs::break_free`, `punching_poro.rs::punch`;
- `cards/wild_claw.rs::empower_it` → `empower_the_played_card`, whose helper currently has no actor argument;
- `cards/time_warp.rs::take_a_turn_after_this_one` and `arcane_shift.rs::strike_and_vanish` self-banish using `banish` rather than the item's actor.

The same helper pattern exists in the Mournful Witness/Renekton staged seams; retain explicit actor arguments wherever a supplied Item owns the operation, even if those callers' broader trigger work remains deferred. Keep default `Ctx::empower/banish` for actual rule-driven operations and tests where current-controller attribution is intentional. Do not change the default helper to use the request actor: a priority-pass/face-arrival request actor is also not necessarily the ability's controller.

Required focused regression: activate a normal Empower ability for seat 0, change its source's control before resolution, reconstruct table+blob, resolve, and assert Event.by remains 0 while the physical source is controlled by 1. Assert the correct seat's YouEmpower match. Add a self-banish case where item.controller differs from the card's owner/current controller, and assert by/owner remain distinct. These use the existing captured item controller; no A4a identity redesign is required.

## Accepted portions and test limits

- `SelfCost::Disempower` participates in initial ready/empowered legality, offers, payment readiness, labels and prompt text. `pay_self` checks empowerment before exhaust and disempowers during activation finalization rather than in the eventual ability body. The saved card regressions cover pre-chain payment, no early draw/effect, unempowered refusal and exhausted refusal. The new Lantern test verifies the paid source cannot pay again.
- The tests demonstrate **validation before ordinary refusal**, not a general post-mutation rollback framework. No fresh native saved-target-prompt cancellation/refusal test was added in this delta. Add/identify one if the recovery summary claims saved continuation restoration; do not claim the pending A5 cost-transaction work exists. Also check the result of `disempower` or validate its full preconditions before exhaust: currently that boolean is ignored. The off-zone guard above must not allow “exhausted and reported paid, but not disempowered.” Application faults are rejected by the existing top-level finish boundary; broad cross-request compensation remains the separate fourth-cluster prerequisite.
- `Event::Banished` captures owner and token before despawn; actor is passed explicitly by activation BanishTarget and chain cleanup. `YouEmpower` excludes the source itself, matching the recovered legends' “something else” trigger. `Banished(Who::You)` requires actor + ownership + non-token, matching the recovered Zed condition. Tests distinguish opponent actor, opponent ownership, token and ordinary card; source-side Empowered matching is preserved.
- Reviewed target-effect merges retain the item actor in Profiteer, Sanction including its delayed empower, Tornado Warrior, Hextech Formula, Thrill of the Hunt, Reinforce, Wild Claw banish, Endless Riches' hand/trash banishes and the other migrated card banish callers. These changes preserve the existing Limited calls and declared optional-cost state. Kayle's capped counter implementation is preserved but needs the actor fix above.
- `banish_instead_of_trash` remains a rule/replacement convenience path which supplies the affected card's controller via `banish`; the source-aware ownership/order semantics of Endless Riches replacements were not established by this recovery. Do not cite this review as closing D1 replacement attribution/ordering. This is distinct from the proved item-controller omissions above.
- No blob/wire schema change, global cost interner, optional-slot reassignment or early additional-cost stage was introduced. The actor fields are transient Events, not a persisted occurrence ledger; A3 remains responsible for later captured-event persistence/matching boundaries.

Caller indexes used in this review are `/tmp/agni-costs-actor-callers-final.txt` and `/tmp/agni-costs-event-callers-final.txt` (lexical inventories include some test helpers). After the two source fixes and focused regressions, recheck the small delta and run the planned final merged-tree gates. Additional settlement remains explicitly incomplete.

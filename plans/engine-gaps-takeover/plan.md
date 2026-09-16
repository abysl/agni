# Engine gaps takeover

Paused by the user on 2026-09-14. The broad continuous-worker execution plan below is historical. Follow the [incremental roadmap](../riftbound-roadmap.md): choose one bounded milestone before restarting any implementation. Preserve all completed and WIP branches; do not resume the original Claude workflows.

## Objective and ownership

Finish the recovered Riftbound scripting primitives and three kai playtest fixes, preserving Claude's completed work. Astra plans and independently reviews; Luna implements in isolated worktrees. No code comments. Rules authority is the pinned 2026-07-16 Core Rules and card text in `games/riftbound/rules/pool/`.

## Preserved baseline

The takeover integration branch joins the handoff's `26972c9c` and local main's `3d7d5f33`, preserving both divergent histories. Original worktrees remain untouched. Baseline engine tests: 4297 passed, 358 ignored, zero failures. Kai baseline and fresh module checks remain to run.

`inventory.json` is the recovered immutable 100-cluster, 381-test inventory. `progress.md` records actual dispositions; commit counts are not completion counts.

| Lane | Implemented / total | Remaining | Preserved tip |
|---|---:|---:|---|
| triggers | 5/25 | 20 | 7dfaef5f |
| costs | 4/13 | 9 | d0639152 |
| statics | 7/11 | 4 | f8322afb |
| play | 5/10 | 5 | 1bad9b25 |
| economy | 6/15 | 9 | 1c790ef1 |
| damage | 5/9 | 4 | ba225839 |
| prompts | 8/8 | 0 | 02352795 |
| rules | 9/9 | 0 | fb4d7c73 |
| kai UI | 3/6 | 3 | 13be83bf |

49 implemented clusters cover 223 original tests; three retain ignores, leaving 220 apparently covered behaviors pending integration verification. 51 untouched clusters cover 158 tests.

Exceptions requiring explicit disposition: Herald of Scales opponent fixture and Fallen Feline untargetable Defy fixture have rules-question ignores and replacement tests. Kayn's damage test depends on card-per-turn counters. Towering Pairofant and Rift Herald have renamed/replaced tests whose equivalence needs review.

## Execution

1. Recover inventory and preserve branch tips. Review and integrate completed lanes sequentially: rules, statics, economy, play, costs, damage, triggers. Each is a separate bounded Luna task from the last reviewed tip. Review the merge against both parents and require a green checkpoint before the next lane. Preserve every test and both sides of shared APIs.
2. Audit serialization at every merge and again when combined: unique CBOR fields and variant tags, coherent blob version, round trips, reset/expiry behavior. Original lanes independently changed versions 6–10.
3. Implement batches below, normally 2–4 primitives per worker assignment with one logical commit each. Use at most two engine workers plus one UI worker. Integration and expensive kai gates are serialized. Start dependent work from the latest reviewed integration tip.
4. Perform final cross-project validation, independent Astra review, Luna repairs, and document every original test's disposition plus the additional audit regressions.

The independent Astra plan review requires a serial foundation stage after recovery: A1 object incarnations; A4a stable ability identities/accounting; A5/A6 atomic rollback and complete processing boundaries; A3 captured trigger occurrences; A4b simultaneous Nth-event selection. A3 captures but does not prematurely finalize/resolve abilities, and simultaneous movement/death remains atomic. Complete A2 provenance before killer-attribution work. Two engine workers may run concurrently only with explicit disjoint file ownership. A7's small real-plugin parity harness and A8's baseline regression start early and expand through the campaign.

A1 uses `review/a1-incarnation-design.md` and the bounded mutation manifest. The observer must account for logical Chain transitions even when a card is physically projected into a board destination; a raw Move hook alone is insufficient. Keep structural legacy decoding separate from safe resume admission, and preserve authorized linked results without refreshing arbitrary stale references.

A4a uses `review/a4a-ability-identity-design.md` and its caller index. Stable identity must enter activation requests before a dynamic index is interpreted. Separate performed-once usage from first/Nth occurrence accounting; a declined optional once-per-turn trigger does not spend its use. A4b still owns simultaneous-event choice.

Costs recovery uses `review/costs-settlement-port-design.md`: recover the three
independent legend/activation/actor clusters first, then complete the bounded
cost-owned replacement continuation and serialized rollback prerequisite before
integrating the fourth additional-cost cluster. This brings the required subset
of A5/D1 forward; the broader foundation/caller migration still follows recovery.
An optional cost's accepted choice remains rules-facing even if physical payment
is zero; settlement receipts are a separate completion condition.

The neutral implementation now follows the frozen interface and feasibility
addendum in `review/effect-group-implementation-plan.md`: engine ABI 4,
snapshot version 1, plugin ABI 1, exact mechanical restoration, and explicit
information obligations. The source audit additionally found that
`hide::reveal_before` only narrates and clears hidden flags; it emits no Reveal.
The complete cost cursor must therefore perform Reveal, persist, await the
logged face, then move/recycle/despawn where required. Engine guards alone do
not implement those legal continuations. Damage reserves plugin blob v13;
additional Costs uses the next version after its final integration baseline.

`review/cost-undo-seam-review.md` refines that prerequisite: existing effects
cannot restore a despawned original identity or all shed state across saved
payment. Use a neutral engine mechanical journal plus plugin rules undo,
with SDK/ABI/snapshot compatibility updates. Public decide state and private
view overlays must remain separate. Cover the entire payment plan, including
Gold's `Spend::Kill`. Cross-request compensation preserves accepted disclosure
history and allocation high-water marks; A1 therefore separates current
incarnation from future generation allocation where compensation requires it.

D1/A11 use the reviewed explicit-stage operation design in `review/replacement-continuation-design.md`. A synchronous Rust callback cannot transparently pause: it must return a typed operation and resume only its suffix from serialized locals/results. Preserve accepted prefix effects and private information boundaries. Regenerate and account for every direct/transitive caller in `review/replacement-direct-callers.txt`, including costs, cleanup, combat and replacement callbacks; no bool-returning wrapper may hide a pending choice.

Hard dependencies: E3/P1/S1 floating aura/D1/T3 require A1; T1/T2/T4 require A3 and A4a; T5 requires A4b; T1 killer attribution/D2 require A2; every new suspension requires A5 and cross-request tests. Rematch backend is a separate authoritative-state task before UI consumption.

| Batch | Scope | Dependencies |
|---|---|---|
| E1 | seat-per-turn-counters, card-per-turn-counters, seat-turn-number, alternative-cost; historical Conquer by seat/battlefield | Reconciled baseline; close Kayn and Perched Grimwyrm history debt |
| T1 | combat-ended-trigger, showdown-begins-trigger, attached-trigger; then stunned-event, died-killer, buffed-events | Reconciled baseline; actor/event snapshot audit |
| S1 | quick-draw, filter-equipped, no-combat-damage-from-self, floating-aura-for-the-turn | Reconciled statics |
| C1 | xp-and-buff-additional-costs, additional-cost-from-another-card | Existing additional cost machinery |
| C2 | activation-pay-stage-picks, trigger-finalization-picks; then empower-cost-variants, base-cost-discard | C1; atomic cost suspension |
| C3 | repeat-cost-variants, pay-stage-alternatives, target-dependent-costs; A10 multiplicity and A12 discount ordering | C2 and integrated modes/pricing |
| E2 | tags-on-the-face, token-faces | Real face ingress through SDK/ABI, not just fixtures |
| E3 | copies, token-play-replacement, move-surcharge | E2 and reference identity audit |
| P1 | replay-from-trash-mid-resolution, plays-from-trash, owner-zones | Limited plays; reference identity audit |
| P2 | hidden-rules, reveal-replacements | P1 and information-boundary tests |
| D1 | kill-path-prompt-and-legend-replacements, floating-kill-replacements | C2, complete caller continuation, rollback audit |
| D2 | might-layers, stun-might-bounce-replacement; A11 damage replacement ordering and combat assignment bookkeeping | A1, A2, A4a, A5; static foundation and D1 continuation machinery |
| T2 | became-mighty-event, delayed-when-arms, played-spell-payload, discarded-event | E1, C2, D2 |
| T3 | triggers-off-the-board, floating-turn-triggers, turn-scoped-granted-abilities | P1, D1, T2; trigger capture and ability identities |
| T4 | recycled-event, each-players-beginning-phase, scored-event, writer-events | All actual mutation paths; correct action boundaries |
| T5 | extra-trigger-queues, implicit-temporary-veto, banish-self-and-banished-with | T3/T4; per-ability occurrence accounting |

## Additional gaps from independent Astra audit

- A1: Non-board zone changes create new objects (rule124). Targets currently retain only physical card IDs; bounce/replay can revive stale targets. Add serialized incarnations or equivalent invalidation for targets, shields, delayed effects and source references, preserving explicit linked-instruction exceptions. Complete before P1/E3/floating effects.
- A2: Challenge damage belongs to each chosen unit, with that controller responsible (417.6.b.3–4), rather than the spell. Correct provenance through bonus damage, prevention and lethal attribution. Test Challenge with Deathcrown and Elder Dragon.
- A3: Capture trigger eligibility/conditions at each inciting action boundary (383.2.c), preserving simultaneous events. Current deferred matching can observe later board state or missing sources. Test sequential instructions changing the condition or removing the source.
- A4: Once limits must identify the ability instance/lender, not just its card. Simultaneous Nth occurrences require the controller's event choice (383.1.b). Test independent granted/copied once abilities and simultaneous first-death choices. A4a also removes saved `triggers::interchangeable` pointer equality from prompt decisions; any ordering elision needs explicit semantic equivalence including source/lender provenance, with native/Wasm prompt parity.
- A5: Fault rollback must restore every transient queue including `Ctx::deaths`. Test deathknell followed by an invalid effect leaves no ghost trigger after rollback.
- A6: Processing limits must never return successful unfinished mandatory work. Use deterministic failure or explicit continuation; test limit boundaries and full repeated cleanup (322).
- A7: Compare the real hardened Riftbound plugin against native execution with a replay corpus. Round-trip/reconstruct persisted state between actions and prompt/reveal suspensions, comparing emitted effects, bytes and views.
- A8: Add exact Mask of Foresight + defending Lillia + Smoke and Mirrors regression: existing Sprite gains Defender alone before Lillia creates the second Sprite; final4+3=7. Baseline reaches that total but incorrectly offers an ordering choice between Lillia's movement occurrence and Mask's later cleanup occurrence. A3 must remove that choice and retain the final total across persisted requests.
- A9: Recovered statics interns runtime power costs in a process-global leaked-slice cache. The persistent Wasm instance resets gas/fuel but retains that cache; both decide and view decode through it, making metered work depend on unlogged view/cache history. Replace it with owned serialized runtime costed-grant data before statics integration. Preserve complete runtime cost semantics (including alternative domain needs) instead of converting them lossily through static script Cost. Add warm/cold/view-traffic parity and explicit legacy v7/v9/v10 whole-blob decoding coverage.
- A10: Multiple Flow costs require the controller's choice (829.1.c.3), and each Repeat instance can be paid independently for another execution (820.1.c.2, 820.3). A9 preserves the prior first-cost selection; recovered economy also coalesces promised Repeat instances. C3 must cover arbitrary printed/granted/promised instances, including two Portal/Academy promises, after A4a identities and A5 suspension. Test choices and payments across saved-state reconstruction. Economy promise indices saturating at 255 must be addressed in A6 rather than aliasing distinct promises.
- A11: Saved damage fixes prevention before doubling and hardcodes that order into combat lethal assignment. Core 372/437.7/465.2.c.5 requires the affected controller's order, explicitly yielding 2 or 4 damage from a 3-damage event with Prevent 2 and Lotus Trap. D2 must support both choices, correct assignment budgeting, and replacement bookkeeping so dealing already-assigned damage does not apply a replacement again. Bonus Damage is included before replacements (715.4.a/437.1.a.1), not freely reordered. This is preexisting debt beyond the original inventory; see `review/damage-order-independent-review.md`.

- A12: Cost discounts with individual minima require ordered application (356.4.c/d.1/e). Eager Apprentice plus Sky Splitter at printed 8 Energy with a 7-Might unit can legally cost 0 or 1 depending on controller-selected order. Current cost aggregation sums discounts and Eager infers order from physical IDs; it cannot represent that choice. Add structured component/total discount operations and saved choice/payment tests in C3. Root source finding independently confirmed by Astra; no gameplay reproduction yet. See `review/discount-order-review.md`.

## Kai acceptance

Firefox text: reproduce in Firefox, distinguish loading CSS from egui/font atlas, fix demonstrated cause and retain reproduction/visual evidence. Do not label speculative CSS changes a verified fix.

Play-location picker: visible clickable/tappable options for every legal normal/Hidden play and hide destination, including battlefield2; test two legal destinations and an illegal one. Engine affordances remain authoritative, with keyboard parity.

Rematch: preserve match wins, previous result and used battlefield identities; reset per-game selection/placement; synchronize host, joiner and AI. Rules486.5–486.6 exclude used battlefields after won games and allow reuse after a draw. Tournament Rules407.4 (2026-07-16) gives the previous game's loser the first/last choice and preserves the prior starting play after a draw. Verified directly in the PDF linked by Riot's Rules Hub: [Tournament Rules](https://cmsassets.rgpub.io/sanity/files/dsfx7636/news_live/503da65669ced10598d62925a6f6bc15111af726.pdf). Match authority belongs in agni, UI in kai.

## Review and validation

The later Astra rematch design in `review/rematch-design.md` refines draw policy: Tournament Rules406.1.b requires the same battlefields after a draw, and403.10 forbids sideboarding. Existing code has no recorded-draw producer; unfinished enforced-Match resets must refuse rather than invent a draw. A negotiated-draw UI, simultaneous private battlefield presentation, and original deck/pool registration are separately identified gaps beyond the bounded rematch fix. Match mode must be pinned in genesis options, and StartGame must validate actual presented battlefield identities against the preserved ledger.

Each batch runs its regressions, complete agni-riftbound-turns suite, clippy with warnings denied, scoped formatting, and Riftbound plugin wasm check. State/prompt changes require round-trip, cross-request, refusal/cancel and replay tests. No weakened assertions or concealed ignores.

`review/play-ignored-optional-correction.md` supersedes the earlier Play recommendation
to force-decline optional costs for `Price::Ignored`. Core 355.1.a/356.4.f.1/356.5.a
preserves the choice while setting the total physical cost to zero. Accepted
Accelerate, Repeat and optional additional effects must remain available.

Integration gates include kai library tests (especially every_prompt_kind_the_engine_can_raise_is_answerable_from_a_tool), kai clippy, and rebuilt/hardened plugin assets where loaded. New prompt kinds get actual brain tool vocabulary.

Final gates: all381 original test entries accounted for with mappings; no unexplained engine-gap ignore; affected suites pass; entire agni workspace compiles/tests as appropriate; fresh wasm build/hardening and kai wasm check; real-plugin parity; concrete UI validation; deterministic schema and visibility review. Preserve original tips as ancestors. Version bumps follow blob/ABI/wire rules; kai patch bump once if shipping a build. No push is part of worker tasks.

Every worker commits on its branch and writes `plans/engine-gaps-takeover/review/<task>.md` within the project. The coordinator reviews the diff, creates a review branch and removes the clean worker worktree before integration. Original Claude worktrees are retained.

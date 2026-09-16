# Riftbound incremental roadmap

Status: the bounded recovery pass is complete. The reviewed baseline landed on local main at `915e8cb5`, followed by the five-cluster Triggers recovery at `345b1646`. No implementation milestone is active. Neutral effect groups, replacement Costs and the broader roadmap remain parked. See [baseline evidence](engine-gaps-takeover/review/baseline-main-landing.md), [Triggers evidence](engine-gaps-takeover/review/triggers-main-landing.md), and the superseding [scope checkpoint](engine-gaps-takeover/review/recovery-scope-checkpoint.md).

## How we work from here

- Select one player-visible outcome at a time, with named cards or a concrete game sequence.
- Define what is included, what is deferred, and the acceptance checks before implementation.
- Use Astra for a bounded plan/review and Luna for implementation when available. Parallel work is optional and must serve the selected milestone; it must not open unrelated lanes.
- Bring in only the prerequisites necessary for that outcome. Split a large prerequisite into its own reviewable milestone instead of silently expanding the task.
- Review and test the resulting change, including Kai when engine prompts or state change. Promote a verified milestone to main independently; do not accumulate another campaign-sized merge.
- Report completion and choose the next milestone. Do not automatically start the rest of this backlog. Remote publication is a separate action from local integration.

## Recommended priority

This order is a proposal based on the recent Empower questions, not authorization to restart implementation.

| Priority | Outcome | Bounded scope and exit condition |
|---|---|---|
| 0 | Make completed work available on main | Completed for the reviewed recovery baseline at `915e8cb5` and five Triggers clusters at `345b1646`, with engine/Kai/lint/Wasm replay gates. The unfinished neutral SDK/protocol stays parked. |
| 1 | Standard Empower is reliable in a playable build | Verify standard rune-cost Empower, legend Empower, Disempower costs, and the intended Kai affordances on the release baseline. Use Tail-Cloaked Matriarch, Akali/Jayce, and one Disempower legend as explicit cases. Treat Risen Altar and copied Empower discounts as a separately named substep if they require pending branch changes. End with real payment/resolution and saved-state checks, plus the Kai gate. |
| 2 | Empower with discard/kill costs resolves correctly | Handle Punching Poro and Escaped Grayback first; add Mel's restricted discard only as an explicit extension. Select/pay costs before the ability reaches the chain, preserve legal response timing, and prove save/resume and refusal behavior. This needs bounded payment continuation/rollback work; do not claim the neutral journal alone completes it. |
| 3 | Finish one game and start the next correctly | Complete the rematch ledger's plugin/session/UI integration: previous loser chooses first/last, used battlefields are excluded after a win, draw policy is preserved, and host/joiner/AI agree. The pure MatchState foundation is already available in integration. |
| 4 | Resolve the Mask/Lillia sequence with correct timing | Keep the established final Sprite total of 7 and remove the illegal ordering choice between the movement trigger and later cleanup trigger. Scope captured events and ability identity work to a reviewed dependency plan before coding. |
| 5 | Expand coverage around actual decks/playtests | Choose a small named card set or one observed game failure from the deferred backlog. Repeat the same bounded milestone process. |

If an actual playtest blocker is more important than Empower, change this order before starting priority 1. Priority 0 is release consolidation, not an invitation to finish every parked branch.

## Preserved state

The implementation baseline on main was `3d7d5f33` when the campaign was paused. Recovery implementation subsequently reached local main at `915e8cb5` and `345b1646`; this is not a remote deployment claim.

| Work | Checkpoint | Disposition |
|---|---|---|
| Accepted recovery integration | Main merge `915e8cb5`; candidate `4d22159c` | Rules, Statics recovery/A9, Economy, Play, three Costs clusters, Damage, and related Kai/test improvements are now on local main. Fresh combined gates are recorded in the landing evidence. |
| Triggers recovery | Main merge `345b1646`; candidate `1e2951df` | Five saved clusters landed after source review, corrected independent migration fixtures, registered Granted/Lent cold replay, 4,536 library tests and 602 Kai tests. Remaining trigger clusters stay on the roadmap. |
| Neutral effect groups | `4e10886c` / `codex/neutral-protocol-paused`; separate restore repair `3f9f9db8` / `codex/neutral-restore-wip` | Engine/SDK implementation and improved synthetic harness are preserved as WIP. Required boundary and hardened/session acceptance remains incomplete; neither ref is on main. |
| Interrupted SDK preflight patch | `8e4e9094` / `codex/effect-groups-wip` | Saved as an explicit WIP commit after stopping the worker. It contains unfinished bounds changes/tests; no passing or reviewed status is claimed. |
| Original handoff lanes | Original `gaps/*` branches and recovered inventory | Preserved history, not alternative branches to resume blindly. Recovery made compatibility and rules corrections that the original workflow does not contain. |

Refs are copied into the main repository so the work does not depend solely on temporary checkouts. Before resuming, verify the selected ref and working-tree state. Do not discard interrupted changes, restart the old Claude scripts, or merge unverified WIP as release work.

The detailed plan, immutable inventory, progress history and review notes remain under `plans/engine-gaps-takeover/` on `codex/engine-gaps-takeover`. For example, from the repository root:

```sh
git show codex/engine-gaps-takeover:orgs/andrea/projects/agni/agni/plans/engine-gaps-takeover/plan.md
git show codex/engine-gaps-takeover:orgs/andrea/projects/agni/agni/plans/engine-gaps-takeover/inventory.json
```

## Deferred engineering backlog

The recovered planning inventory contains 100 clusters and 381 original ignored-test entries; its earlier baseline identified 51 untouched clusters. This is an inventory reference, not a current completion percentage. Reconcile individual entries when selecting a milestone; an enabled test or high passing-test count is not proof of complete card behavior.

| Area | Remaining work | Existing plan references |
|---|---|---|
| Neutral rollback prerequisite | Review interrupted nested-cards/duplicate-or-missing-roster guards; independent current/capsule bounds tests; action-versus-effect disclosure semantics; direct Join/counter controls. Replace shallow fixture checks with full incremental/projected/authoritative equality, real mechanical workload, cold replay, roster operations, ABI refusal/migration tests, hardened modules and private-face session delivery. | `review/effect-group-integration-gate-plan.md`, `review/effect-sdk-final-review.md`; latest resumed Astra findings must accompany any restart |
| Payment and Empower variants | Fourth Costs recovery, serialized resource/additional payment cursor, legal Reveal/wait/resume, plugin rules undo, XP/Buff and external additional costs, activation/trigger payment choices. | Costs settlement design; C1/C2; bounded A5/D1 prerequisites |
| Cost choices | Multiple Flow/Repeat instances, optional/alternative/target-dependent payments and ordered discounts with individual minima. | C3, A10, A12 |
| Identity and event correctness | Object incarnations across zone changes, stable copied/granted ability identity, once versus first/Nth accounting, captured trigger eligibility, atomic simultaneous events, per-unit Challenge attribution. | A1, A2, A3, A4a/A4b |
| Completion and replacement safety | Restore transient queues on failure; never silently succeed with unfinished mandatory work; serialized kill/damage replacement continuations and complete caller coverage. | A5, A6, D1/D2 |
| Damage and statics | Controller-selected prevention/doubling order, correct assignment accounting, Might layers, stun/bounce replacements, Quick Draw and remaining equipment/aura rules. | A11, D2, S1 |
| Economy and play | Historical Conquer/movement counters, face tags/token ingress, copies, token-play replacement, move surcharges, trash/replay permissions, Hidden and reveal replacements. | E1–E3, P1/P2 |
| Trigger coverage | Combat/showdown/attachment events; Mighty/discard/recycle/score events; off-board, delayed and floating triggers; simultaneous Nth-event selection. Recover existing Triggers candidate separately from new clusters. | T1–T5, A3/A4 |
| Kai | Rematch outcome described above; Firefox text only if a concrete failing environment is reproduced. The local Firefox reproduction was readable, so no verified Firefox fix is claimed. | Rematch design/R0–R3 and Kai review notes |

## Acceptance for one milestone

1. A concrete before/after game sequence and explicitly named supported cases.
2. Meaningful regressions, including saved-state continuation where the sequence can pause. Preserve independent old-format fixtures when schemas change.
3. Relevant native suite and strict lint/format checks. State/ABI/visibility changes additionally require fresh native/hardened replay and actual export/session checks.
4. Kai prompt vocabulary, UI/AI and build checks when affected. Building or hardening a Wasm file is not evidence that a test actually loaded it.
5. Review, a recorded local-main disposition, and an explicit list of deferred cases. Then select the next milestone.

No date is promised for the full backlog. Estimate each selected milestone after its dependency review; the former multi-day campaign estimate is not a delivery commitment.

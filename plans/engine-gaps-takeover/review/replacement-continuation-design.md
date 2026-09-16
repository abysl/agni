# Replacement continuation: bounded D1/A11 design

Read-only architecture review against integration `bf48bd21` and the accepted A11 saved-damage audit. No implementation or builds. Complete A1, A4a, A5/A6 and the relevant A2/A3 foundations before enabling these suspensions.

## Decision

Use explicit staged callbacks, with a small typed engine-operation continuation between stages. Do not replay arbitrary Rust callbacks to find their suspension point. Extend the existing `Stage`/`Flow` model rather than introducing a general scripting interpreter.

`cards::Run` is `fn(&mut Ctx, &Item, Stage) -> Flow`. `chain::run` observes its result only after the function returns. `kill::park` currently pushes a resolving pseudo-trigger and returns `Killed::Replaced`; the caller continues, and `chain::finish` can dispose of the source before the choice. The test `a_spells_kill_with_two_replacements_finishes_the_spell_then_asks_and_if_you_do_is_false` explicitly preserves this premature completion. Neither `Flow::Ask` nor a global pending flag can suspend the Rust call stack.

## Smallest safe execution seam

Add one conceptual variant, `Flow::Perform { operation, next_stage, locals }`. Names are illustrative; exact types belong to Luna's foundation implementation.

- A card stage computes and stores the values it must retain, then **returns** the operation. It does not call a helper that opens a prompt while returning a pretend boolean/result.
- Initial operations are Kill/KillBatch for D1; add Damage/DamageBatch and combat assignment for A11. Helpers construct these requests and return `Flow`; raw mutation primitives become private to their operation runners.
- The runner either completes immediately with a typed result and invokes the next stage in the same request, or persists its frame and opens the required choice. Thus ordinary one-step cards need no visible prompt or extra log entry when no choice is necessary.
- The resumed stage consumes a typed completed result exactly once. It never reissues the operation. A Pending state is not `Killed::Replaced`, `false`, or success. Prefer requiring every affected call site to tail-return `Flow::Perform`, rather than returning an easily ignored `Result/Pending` value.
- A tail-only card resumes to a common finish stage. A conditional/multi-instruction card uses its own next stage. A loop either submits a genuine simultaneous batch or saves its target list, cursor and accumulators for sequential execution; do not infer simultaneity from Rust loop syntax.

For Blood Money, stage 0 saves the victim and pre-kill bounty, then returns Kill. Stage 1 reads the final kill result and spawns the saved bounty only on the applicable success outcome. It must not recompute bounty from the victim after a zone/control change. For Void Seeker, damage completion precedes the unconditional draw; zero/prevented damage still reaches the draw stage. Smite's post-damage watch follows the completed damage instruction without introducing an extra cleanup boundary. Teemo keeps its revealed-card list and computed damage across the replacement choice, then recycles exactly those cards afterward.

## Serialized continuation contract

Persist this in GameBlob/ChainItem through ordinary versioned CBOR, with an explicit blob-version change and compatibility tests. No function pointers, closures, Rust stack, process cache, pointer identity, or host memory may constitute the continuation.

1. Caller identity: stable source/ability identity from A4a, parent item ID, execution number for Repeat, next stage, and typed/local versioned payload. Card references include A1 incarnations. Preserve targets/modes and do not overload user `picks`, target arrays, or synthetic ability index 254 with internal control state.
2. Operation identity and input: unique operation ID, kind, subject(s), original damage/kill arguments, cause/controller/provenance from A2, and stable replacement-instance identities. Persist already-applied/declined choices and remaining work so a replacement is not offered or spent twice on one event.
3. Operation progress: pending chooser/prompt ID, operation substage, typed local data, batch membership/cursor where relevant, and completed result awaiting consumption. A resumed request must match the pending prompt and authorized chooser; duplicate, stale or malformed answers cannot advance it twice.
4. An engine-owned bounded frame stack handles child operations triggered while executing replacements. A frame's return destination is either a card stage or a specific engine phase such as cost finalization, cleanup, or combat; it is not a free-standing chain ability. Enforce depth/work bounds deterministically under A6, without saturating/aliasing IDs or returning unfinished success.
5. The source stays Resolving until all its operations and suffix instructions finish. No priority pass, Repeat restart, leave-to-trash, turn advancement, or unrelated trigger resolution occurs because a replacement needs input. Extend the runner to recognize all continuation kinds directly; current `chain::run` only writes `item.stage` for Resume/Discard/Name asks.

Prefix state is real: all completed instructions before the pending operation remain in the accepted request's effects and serialized state, with faces revealed only to the seats entitled to them. The chooser sees exactly the state/information reached at that point. Stage-local values that must survive are explicitly saved. Transient event/death queues that need to survive the request must be captured/serialized by A3/A5; preserving only the table and blob while losing those queues is insufficient. Capturing an occurrence must not prematurely resolve it while its source instruction is still suspended.

A5 checkpoints remain for faults in the current request segment and must restore every transient queue and projected state. They are not a suspension mechanism. An invalid answer rolls back/rejects that answer without undoing an earlier accepted prefix. Once a prior request exposed information, a later error cannot pretend that exposure never occurred. Define the existing engine-fault/fizzle behavior at this boundary during A5; do not silently add “restore before the original callback” or “accept partially completed suffix” behavior.

## Non-card caller boundary

D1 cannot be implemented only by changing card callbacks. Migrate the complete upward call path from:

- `engine/cleanup.rs::{lethal_kills, ...}` through `kill::batch` and the enclosing cleanup/chain/phase continuation. Preserve simultaneous death membership and captured deathknells; do not process an unrelated suffix or fresh cleanup while a batch is waiting.
- `engine/activate.rs` self-kill cost and `engine/pay.rs` Gold kills, plus card-specific pay helpers. Suspend the actual finalization/payment frame and retain prior cost choices; a replacement prompt must not report “paid” or put the item on the chain early. Distinguish completed replacement outcomes from the separate rules governing whether an attempted cost is satisfied.
- Replacement callbacks themselves (`Replacement.run` currently returns `()`): migrate those that can call a suspending helper or pay/choose through the same staged operation contract. Retain a synchronous internal primitive only where the rules prove that particular action cannot require a choice; a `Cause::Replacement` label alone is not that proof.
- A11 combat assignment/deal and ordinary damage callers. Persist choices during assignment and apply the resulting plan once, retaining simultaneous damage and avoiding a second prevention/multiplier pass during deal. This follows the separate A11 recommendation.

## Bounded migration inventory

A mechanical pre-test-section scan at the reviewed integration tip found 133 direct `kill/deal/damage/damage_by` matches across 116 card/helper files, saved as `/tmp/agni-replacement-direct-callers.txt`. This includes helper definitions and is a seed manifest, not a complete transitive call graph. Regenerate after the remaining recovery merges; new damage/economy callers must join it. Avoid a broad unrelated card rewrite.

D1's direct-kill files (excluding prelude) are:

`acceptable_losses`, `adaptatron`, `ambessa_respected_and_feared`, `atakhan`, `baited_hook`, `blade_of_the_ruined_king`, `blood_money`, `bottled_constellation`, `brittle_steel`, `bullet_time`, `commander_ledros`, `cruel_patron`, `death_from_below`, `deathgrip`, `decree_of_unity`, `detonate`, `disarming_rake`, `drag_under`, `dusk_rose_lab`, `fox_fire`, `generic`, `glowstone`, `guardian_angel`, `harnessed_dragon`, `hidden_blade`, `jayce_man_of_progress`, `lacerate`, `malzahar_fanatic`, `noxian_demolitionist`, `noxian_guillotine`, `pickpocket`, `public_execution`, `rocket_barrage`, `salvage`, `sandshifter`, `solari_chief`, `soul_harvest`, `stalking_wolf`, `tomb_raider_barbara`, `treasure_trove`, `vengeance`, `zaun_punk`, `zhonyas_hourglass`.

Audit each function's callers, not just its matching line: e.g. `bottled_constellation::pay_kills`, `commander_ledros` and card self-cost helpers return ordinary values to another callback. Classify each manifest entry as tail-only, conditional suffix, sequential loop, simultaneous batch, engine-cost caller, or proven non-suspending primitive. Compiler-enforced helper signature changes are useful coverage; an unused Pending result or retained bool-returning wrapper is not an acceptable migration escape hatch.

A11's damage half especially needs `void_seeker`, `smite`, `teemo_strategist`, `unchecked_power`, `volibear_furious`, `twisted_fate_gambler`, and the source-attributing Challenge path after A2. Damage helpers that call kill/cleanup internally must retain both continuations. D1 need not migrate ordinary damage callbacks until their helpers can actually suspend.

## Why generic replay is not the bounded solution

Rolling back the callback prefix before asking gives the chooser stale state and can hide already-revealed faces. Committing the prefix and rerunning the callback duplicates draws, spawns, payments and counters, and can take different branches because the table or newly received faces changed. Skipping already-emitted effects still reexecutes reads and control flow; it does not restore consumed local values or the correct loop position. A panic/unwind sentinel saves no portable Rust stack and conflicts with wasm trap behavior.

Generic replay could only be correct with a complete deterministic journal of every observable read, result, mutation and branch-relevant operation against a versioned execution snapshot, plus exact replay semantics for information arrival. The present public Ctx/table access is not mediated that way. That is a substantially different interpreter architecture, not a safe shortcut. A restricted replay optimization for a separately proven pure callback is unnecessary here and must not become the default for arbitrary cards.

## Five decisive tests

1. **Final result controls the suffix:** Blood Money or a small equivalent with optional kill replacement. While pending, source remains Resolving and no Gold/draw suffix occurs. Decline makes the completed kill outcome drive exactly one reward; accept replacement takes the appropriate other branch. Reconstruct before answering and replay the same log natively and in hardened wasm.
2. **Committed prefix and hidden information:** a staged reveal/draw or Teemo-like prefix followed by damage replacement and recycle/spawn suffix. The entitled chooser sees the prefix state/faces; others see no extra private data. Resume never draws/reveals/spawns twice, recomputes a saved quantity from changed state, or recycles before damage completes.
3. **Batch suspension:** simultaneous deaths with two replacement choices and a shared once-use source; suspend/reload between answers. Preserve original membership/occurrence snapshots, correct chooser order, single resource consumption, and no partial trigger/cleanup execution. Add a sequential-two-kill variant to prove the second instruction waits for the first's final result.
4. **A11 assignment then deal:** 3 damage plus Prevent 2 and Lotus Trap yields the controller-selected 2 or 4 dealt; a two-target case spends the correct original Might budget. Reconstruct after the order choice and before completion; replacement effects and shields apply once, and all damage is dealt at the required simultaneous boundary.
5. **Fault/refusal and nested frames:** wrong-seat/stale choice and injected failure after a resumed replacement cost restore the current segment's state/effects/events/deaths/spawn IDs. Earlier committed prefix stays intact. A nested replacement operation reaches its parent once; depth/work exhaustion is a deterministic failure, never successful unfinished work.

The implementation is a bounded operation runner plus explicit callback stages. Every affected caller must join the migration; Pending cannot be treated as a completed outcome.

# Saved cost payment undo: SDK/engine seam

Read-only refinement of the Costs port design against integration source. This is a prerequisite for the fourth Costs cluster, not a Play blocker. No production edits or builds.

## Decision

**Existing effects cannot express exact general compensation after a saved replacement wait.** Prefer a small game-neutral engine-owned mechanical undo journal, controlled through explicit begin/commit/rollback operations, plus a separate plugin-only undo body. Do not serialize a shared `Ctx::table` as the authoritative undo document, and do not add a plugin effect that installs arbitrary hidden faces. This needs a reviewed SDK/engine ABI and snapshot-schema extension; it is not implementable solely by adding `paid_with` to the Riftbound blob.

A hand-authored inverse list using today's effects is sufficient only for a proved restricted subset. It is not the completion boundary for the original fourth Costs cluster, because payment/replacement can destroy tokens, recycle runes, shed state or disclose information. Keep the previously recommended three-cluster green integration point while implementing this prerequisite.

## Source facts and why a Snapshot copy is insufficient

- `plugins/sdk/src/decide.rs::parse` obtains the public engine `state` and entry. It does not parse a private-face overlay. `view.rs::parse` additionally reads `faces`, then `table::unveil` fills hidden `CardInfo` face fields. Both paths create the same Rust `Snapshot` type; generic `Ctx` helpers cannot infer that their table was built for authoritative execution merely from its type.
- `Snapshot` includes cards/counters/next_id/revealed/tokens, but omits engine `shown`, `owed_reveals`, `peeks`, full face presentation fields and other LogState data. `CardInfo` is a projection, not a lossless `agni_core::Card` record. An overlay-filled card can still be absent from `Snapshot.revealed`.
- `sim/src/log.rs::apply_effect(Move)` clears `shown`, may blank the face/remove `revealed` on entering a no-visibility zone, and calls `shed` in state-shedding zones. `shed` deletes annotations/counters and can remove a token entirely. Move-back alone cannot recover these changes.
- `Despawn` removes a card, token flag, counters, annotations and disclosure bookkeeping. `Spawn` accepts a face and allocates a fresh ID; it cannot recreate the destroyed token with its previous ID. SDK Face also lacks the full core face's tint/foil representation.
- `Counter` saturates and clamps. Negating the requested delta is not necessarily an inverse; even a value-restoring delta may leave a formerly absent counter row present. `Annotate(None)` can restore absence only if the prior presence/value was recorded.
- `Reveal` requests a future public disclosure; it is neither a face setter nor an inverse. `Peek` authorizes one seat to learn a face; it cannot undo knowledge. Neither operation can safely reconstruct missing private data.

Thus recording `Ctx::table.clone()` in `GameBlob` risks both incomplete restoration and public persistence of view-only faces. A predicate such as `card.name != ""` is not a public-provenance check. Deriving new public facts from a view must remain impossible.

## Minimal public mechanical journal

The engine, at its existing authoritative `apply_effect` boundary, records before-images for successfully applied mutations in an active operation group. The plugin names a deterministic group ID; it never supplies replacement card faces or engine internals. The journal is engine state serialized with engine snapshots, separate from both the current plugin blob and private host/view caches.

For a first implementation, use typed before-images rather than whole `LogState` snapshots:

- touched card record (or explicit absence), exact physical ID, owner, zone/seat/order and full **engine-authoritative** face; token membership;
- touched annotations and counter rows, including absent versus present values;
- affected ordered zone contents or exact global card-order restoration data; preserving only a destination index is insufficient unless reverse-order replay is proved to restore all affected order;
- identities allocated by the group, so rollback removes group-created objects and restores removed preexisting objects with their original identities;
- mechanical disclosure-state changes caused by those moves/despawns, recorded distinctly from accepted disclosure inputs (next section).

Capture the real applied result, including implicit shedding and clamping. Reversing records is an internal restore operation, **not** an ordinary Move/Spawn that sheds state again, raises card events, or invokes replacement rules. A deterministic bounded before-image snapshot of just these mechanical fields is also acceptable initially if simpler; do not call `LogState::clone` including plugin state, logs or another undo journal.

Suggested versioned neutral protocol: `BeginEffectGroup { id }`, `CommitEffectGroup { id }`, `RollbackEffectGroup { id }`, with one active group per plugin in this bounded implementation. Begin precedes the first payment effect; completion or rollback closes exactly that group. Unknown IDs, double completion, nesting and unsupported interleaving fail before mutation. The engine does not know costs, runes, deathknells or Riftbound phases. Neutral effect groups belong to agni simulation/SDK, not spirit, core game data or a renderer.

One active group means nested replacement work joins its parent payment group; it does not start a recursive cost snapshot. If that work requires a distinct nested transaction, mark that as an explicit unsupported implementation boundary to resolve before claiming the affected legal cost recovered, not a silently flattened second payment.

The SDK projection must mirror the new operations. On a subsequent decide request the plugin needs an authoritative public rollback projection/journal sufficient to compute its restored table, or an explicit engine-mediated rollback response followed by deterministic continuation. **Choose the former for this bounded seam:** include the active group's public before-images in the decide request, parse them into a separate SDK journal type, and use the same pure restoration algorithm in SDK projection and engine fold. Do not expose private view overlays as journal input. Keep the journal out of ordinary player view payloads except whatever current public state those views already need.

## Disclosure and ID policy

Accepted public Reveal entries and seat-specific face delivery remain accepted history even if the play is later undone. Never rewind the log/`next_seq`, remove those entries, relabel a seat-only disclosure as public, or synthesize Reveal to restore a face. Engine-created journal records contain only faces already present in authoritative public engine state at capture; newly received private `faces` never enter them.

Keep a separate ordered record of accepted information changes while the group is open. Rollback restores mechanical state, then reconciles those information changes for surviving identities under their restored zone's visibility rules. Preserve completed legitimate disclosures and their actual audience; do not blindly copy old `revealed/peeks/shown/owed_reveals` sets over newer accepted information. Nor should those sets be unioned indiscriminately: current public visibility/peek entitlement and historical knowledge are different. In particular, returning a publicly revealed cost card to a hidden zone does not undo the fact that players saw it, and should not make an otherwise hidden zone globally face-up forever. Keep existing log/host knowledge and information-delivery obligations; test the exact reconstructed state and each seat's view. Removed transient tokens have no new face-delivery obligation after rollback; their already observed history still exists.

Within one rejected, uncommitted request, restore tentative allocation exactly. Across a **committed** saved prefix, retain the physical-ID allocation high-water mark and retire IDs of rolled-back spawns; do not reuse identities already exposed in accepted inputs or views. Restore a preexisting despawned token with its original ID via the engine journal. This corrects the earlier cost-plan shorthand “restore spawned IDs”: mechanical rollback is not byte-for-byte return to the old log sequence or allocator. A1 incarnation changes require the same explicit undo policy rather than treating compensation as an ordinary new zone transition.

## Plugin-only state and payment coverage

Persist `CostTransaction { id, owner_item, operation_cursor, waiting_continuation, frozen_quote, rules_before }` in the plugin blob. `rules_before` is a leaf `CostUndoBody` containing the mutable rules state that payment/replacements can change, **excluding the active transaction/journal fields themselves**. A typed body is preferable to embedding another `GameBlob::encode()` recursively. Restore selected cards' state plus affected seats' pools/promises/accounting, control/combat/delay/trigger state as required by the bounded operation set; if using a full rules-body copy initially, expressly omit transaction metadata and immutable configuration. All of it must originate from authoritative decide execution.

Resource pool consumption and promise removal are plugin-only mutations (`pay::pay`/`spend_pool`/`keep_promise`), whereas rune exhaustion/recycling, XP counters, token destruction and burn moves also change the engine table. Both halves must commit/rollback together. `pay::pay` also invokes `ctx.kill` for `Spend::Kill` resource sources (e.g. Gold) before later rune/XP/pool work: the cost-owned completion cursor must encompass the entire payment plan, not just new `AdditionalKind::Kill`.

Transient `ctx.events`, collection cursor, `deaths`, `picked`, `remembered`, awaiting faces, fault and allocation tracking cannot be recovered from a blob alone. Same-request failure uses the expanded in-memory checkpoint. At a saved wait, serialize captured pending occurrences/death work needed after completion into the cost-owned frame; do not execute their trigger effects while payment is unfinished and then try to delete those effects by clearing a queue. Rollback discards transaction-generated pending work and restores preexisting work. Valid answers preserve the committed payment prefix; they must never replay it.

While a payment group is active, allow only its replacement/continuation inputs and required logged information arrivals to mutate it. Other gameplay interleaving must wait; otherwise a broad rules-body restoration could erase another accepted action. This boundary is enforced by the plugin's authoritative scheduler, with engine group-owner validation. Invalid answers refuse that answer without rolling back earlier valid payment progress; rule-mandated failed final legality invokes explicit group rollback plus declaration cancellation.

## Required extension and decisive tests

Update SDK Effect encoding/projection, agni-sim effect decoding/folding, ABI negotiation/version, engine snapshot serialization/restoration and guest/host compatibility vectors together. The new journal must replay through native and hardened engines. No spirit change and no renderer-specific transaction logic is needed. Plugin blob versioning alone cannot communicate an engine rollback contract.

1. Pay with an exhausted/annotated rune plus pooled resources and a promise, wait on replacement, save/reload, then fail legality: restore exact rune order/annotations/counters and pool/promise state, with no duplicated resource or trigger event.
2. Destroy a preexisting Gold/token and spawn another during replacement, save/reload, then rollback: original physical ID and state return, new object disappears, already exposed IDs are not reused. Assert native/SDK/fold parity, not just equivalent card names.
3. Clamp a counter, shed state through a move, then rollback: exact before-values and row absence/order return; a negated requested delta or simple Move-back implementation must fail this test.
4. Private-view overlay can quote/grey a cost but cannot create or alter a persisted journal. Authoritative decide with the same public inputs produces identical blob/journal regardless of owner view history. Search the encoded undo document for a fixture's private-only face and assert absence.
5. A saved replacement legitimately reveals a previously hidden card to the proper audience, then the play fails: mechanical state rolls back, accepted disclosure/history is retained, unrelated seats gain no private face, and no fresh synthetic Reveal is needed. Check restored public state plus separate owner/opponent views and reload.

These are the minimal correctness gates for the seam. Until they pass, the fourth Costs cluster remains incomplete even if ordinary discard/kill fixtures pass.

A1 refinement: if cross-request compensation restores a pre-payment object, it restores that object's original **current ObjectRef**, not merely its physical card ID with a fresh generation. At the same time, no generation/physical ID exposed by the committed but compensated prefix may later be reallocated. This may require the A1 ledger to distinguish `current_incarnation` from `next_generation`/allocation high-water; a single counter that is both current identity and allocation cursor cannot be safely decremented. Retire intermediate references, restore the old live reference through the explicit compensation operation, and keep allocation monotonic. Add a bounce/spawn-during-payment → saved wait → rollback → later move/spawn test proving the restored old reference works while every intermediate retired reference stays stale. This refines A1's undo contract; it does not block the first three independent Costs clusters.

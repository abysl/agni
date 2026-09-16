# Phase-two harness 8946965d — bounded review

Read committed harness/fixture changes only, against the frozen integration gate. No builds or edits; moving SDK repair excluded. **Useful native scaffolding, not integration acceptance.** Worker reports five native host tests and one native net test; there is no wasm acceptance claim.

## What is now demonstrated

- Fixture manifest uses version text "1" and passes the actual decode_plugin_manifest type path, not just a self-authored CBOR expectation.
- Real native_decide_request bytes feed strict SDK parse/from_decide; the fixture's real SDK verdict runs through fold_mediated. One active mechanical projection is compared with a fresh authoritative post-state request.
- Native Begin→snapshot→fresh engine and fresh fixture→Commit matches the warm snapshot. The active group in that cold test has no mechanical or disclosure workload.
- HostSession now asserts its state actually contains group0, closing the phase-one Game-entry-only weakness. Sparse [0,7] and a complete256-seat setup reach Begin through SDK parsing.

## Concrete weak vectors

1. Opcode1's Move sends card1 from All Aux/index0 back to the same placement. It does not test relocation, global vector order, hidden-zone face clearing or state shedding. The original token is unannotated, and the transient Spawn is immediately Despawned before the checkpoint. No declared/changed counters exist; comparing two empty counter vectors proves no counter restoration.
2. `sdk_fixture_mechanical_workload_rolls_back_with_exact_suffix` rolls back warm and has no suffix. It compares card rows, allocator, counters and group retirement, but omits restored annotations, tokens/revealed/disclosures and information/capsule state. An annotation-restoration regression can survive this test.
3. The projection test hand-copies the fixture's effect list. `fixture::decide_bytes(...).len()>0` does not assert acceptance or effect equality. It checks table/disclosures/IDs only, so a wrong before capsule or information journal can pass. It never compares the incremental API with project_verdict.
4. The256-seat test gives viewer255 to Begin-only, which never uses that argument. It proves parsing/count-preserving Begin, not Counter/Peek/Card membership for255. Sparse viewer7 is likewise unused in the current Begin/cold vector. Neither test includes absent-seat controls or a meaningful group checkpoint at those rosters.
5. The native-only HostSession test has no clients, disclosure fulfillment/private delivery or cold artifact restore. The fixture's view text lists face names present in its input projection; this is a diagnostic of supplied information, not a substitute for engine/client audience assertions.

## Narrow next worker checkpoints

### 1. Reusable differential driver, then stop for review

Use the actual decoded fixture verdict as the effect source; remove copied effect literals and assert accepted/refused disposition explicitly. For each entry obtain authoritative DecideRequest, parse once, and run (a) begin_entry/apply_effect/finish_entry and (b) project_verdict from independent equal starting projections. Require equality. On an incremental failure compare the whole immediate successful prefix and retry a legal operation; project_verdict failure leaves its starting object unchanged.

After fold_mediated, obtain an unsubmitted no-op request at actual next_seq. Compare the entire EffectProjection, including active before capsule and information, after normalizing only per-entry flags on both clones with begin_entry(empty Game). Assert normalization does not mutate table/disclosures. Compare plugin state separately. Include one simple Annotate Begin/Continue/Rollback vector and one real RequestReveal/logged Reveal vector so full-journal comparison is exercised. Add independent semantic assertions; equal implementations alone do not establish correct rules. Do not introduce a production introspection getter when existing PartialEq suffices.

Acceptance: one reusable driver, passing exact full-projection/entry comparisons, plus deliberate negative tests proving it catches a changed capsule and an information mismatch. Native only is an explicit checkpoint, not final gate.

### 2. Actual mechanical corpus and cold suffix, then stop for review

Extend the fixture/setup with All/Owner/None Aux plus explicit Deck/Discard, three stable input IDs and declared counters. Opcode1 must move a normal card to a genuinely different zone/order, modify exhaustion/custom annotation state, destroy an original annotated public token, clamp an existing counter on stable card_c and create a formerly absent row, then Spawn a transient which remains live at the save. Use a nondefault face tint/foil in the authoritative engine baseline. Test despawn_any=false/true separately; false non-token Despawn must refuse.

Save the full active engine snapshot with real changes. Instantiate a new native engine AND fixture, restore, resume at exact suffix entry, Rollback then a post-terminal annotation, then Spawn again. Compare cold/warm snapshots and independent exact restored card order/faces, original token/annotations/counters and missing-row absence, retired accepted allocation IDs, group retirement/cursor and plugin state. The post-terminal annotation is the intended suffix and must be the only rollback-snapshot difference beyond documented cursors.

Add same-verdict Begin→Spawn→Rollback high-water control and a separately named adversarial verdict that Spawns then fails: rejected tentative allocation must not consume IDs. Do not force an invalid effect through the honest SDK fixture and mistake its refusal for an engine guard test.

### 3. Disclosure, roster and adversarial native boundaries

Run the frozen ordered-debt/Peek matrix using the reusable driver: actual owed state before/after logged Reveal; duplicate requests fulfilled once; unsolicited public reveal followed by fresh debt; same-Owner shown cleanup versus necessary other-viewer Peek after final pending cleanup; baseline None rollback refusal until fulfillment; transient fulfilled history and unfulfilled transient refusal; request→hidden/Despawn/token-shed guards. Both viewer TableViews and final disclosure sets must agree after cold reconstruction.

For [0,7], actually Counter seat7 and Peek(card,7); absent1 refuses. For256, actually Counter/Peek255 and verify Request.players/table count256, exact roster/order and saved current/capsule projection. Add PerSeat7/absent1 and Shared-supplied1→effective0 moves/Spawns and explicit/default owner controls. Assert authoritative ABI1 keys and stripped embedded group copy; include despawn_any immutable config and actual-roster capacity boundaries already covered independently by unit tests.

Use separate adversarial/native ABI0 adapters: no-group ordinary acceptance, forbidden Begin, active rejection before decide (call counter), and illegal first/post-terminal markers. Restore independent legacy/malformed snapshots through the engine interface and assert failed restore preserves prior state. Keep fine-grained malformed CBOR cases in SDK/sim unit tests rather than inflating the host matrix.

Acceptance: every vector has one exact expected result/cause and meaningful positive control. Wait for SDK source fixes; any differential disagreement returns to the owning production worker, not a relaxed assertion.

### 4. Refreshed wasm exports and real session delivery

Once native vectors and source reviews pass, build fresh engine and SDK fixture cdylib artifacts at the merged commit, harden through existing loaders/budgets, and run the same driver natively and under wasm. Compare request/verdict bytes or decoded canonical values, admission outcomes/deltas, exact engine snapshots and both views at every entry. Each cold checkpoint creates new engine AND plugin instances. Record raw/hardened hashes/config/source revision; preserve independent ABI/golden and legacy-restore assertions through actual exports.

Then complete the one planned hardened HostSession plus two separately instantiated ClientSessions case: normal Deal secrets, Begin+Peek(d,viewer)+Move(c,All)+RequestReveal; let HostSession automatically emit its owed Reveal; deliver owed_faces only to their seat. Verify private d bytes never enter authoritative state/capsule/DecideRequest after view calls. Rollback returns c to None; d's exact audience remains. Later grant c's Peek without new OwnerFaces to prove the client's previously public knowledge survives without global visibility. Compare client/host state under identical pinned artifacts.

Final gate still includes ABI1 consumers/game fixtures, scoped checks and one refreshed full-game compatibility replay. Synthetic phases do not complete Costs4 payment continuations, stable object/ability identities or universal allocator-gas equality.

## Engine record-location guard

Root independently found PublicReveal.zone_at_reveal validation allowed declared None zones while SDK required revealable All/Owner. Root is adding a failing regression and bounded engine guard. This phase-two review does not inspect uncommitted production; append its committed source disposition separately when supplied.

Committed guard76335cb7 reviewed: approved bounded correction. validate_state now requires a PublicReveal record's recorded placement to have All or Owner visibility. The regression creates a real admitted public Reveal, proves its complete active snapshot round-trips exactly, changes only the recorded zone to a declared None zone, and requires restore rejection. This discriminates the former declared-zone-only predicate without invalidating the control for another reason. Root reports the intended pre-fix failure and post-fix sim92+5golden/clippy/fmt gates; no builds were reproduced here. Refreshed engine artifacts and the cross-layer gate remain pending.

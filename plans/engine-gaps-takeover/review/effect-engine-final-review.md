# Final bounded engine disposition

Reviewed engine production 4d292547, root test-only 8ac7a468/af477d9d and phase-one fixture 0d113353 in /tmp/agni-effect-groups, against effect-engine-32ba-review.md and the frozen Peek/roster contract. No builds or source edits. Dirty SDK repair files are excluded.

**Engine-half source approval for proceeding to cross-layer acceptance. No remaining concrete engine source blocker was found in the bounded outstanding set. This does not approve the SDK, the unfinished integration matrix, or future Costs transactions.**

## Outstanding engine findings closed

- `LogState::effective_seat` now normalizes Shared to seat0, then checks actual roster membership. Both admission validation and fold paths use it; plugin Move/Spawn also retain their direct checks, and Spawn validates explicit/default owner before allocation. Sparse [0,7] legitimately addresses PerSeat7; absent PerSeat1 rejects; Shared supplied1 remains valid at0. The new incoming test checks `validate` refusal and the complete sequenced-refusal state with only next_seq advanced, preserving the deliberate admission/sequence distinction. Successful sparse/shared state restores exactly. This closes the last source blocker from 32ba.
- Ordered pending Reveal reconciliation is unchanged from corrected32ba: idempotent requests, subsequent PublicReveal fulfillment, final outstanding live-card/face/revealed/shown handling, then canonical Peek pruning using final All/Owner-seat/shown authorization. Required baseline/later Peek pairs are not erased merely because a public face exists. None restoration does not create global face visibility. The actual-view pending-Peek regression discriminates the previous erroneous order.
- Feature caps are enforced before capsule capture/information append and before recursive CBOR Value construction. The raw prepass now separately covers active current mechanics/disclosures and the capsule; annotation outer-map and aggregate row lengths are checked before collection metadata allocation. The actual roster-derived Peek cap is established before parsing those collections. This is a source-timing observation, not an allocator instrumentation claim.
- Current and capsule token IDs still require live cards; owner/seat/zone/counter/disclosure references and allocator ordering checks remain present. No global group-size cap was reintroduced for ordinary nongroup mechanics. Raw legacy maps still migrate only without new group/counter keys; versioned snapshots retain exact envelope requirements. Existing nongroup oversized annotation/card and zero-span/zero-step controls remain valid.

## Root boundary tests are discriminating

`complete_active_group_limits_have_exact_boundary_controls` now checks cards, counters and aggregate annotations independently in current and capsule state: the unaffected half passes its raw helper, the oversized half fails, and the complete versioned payload rejects. Exact-cap controls restore equal state. Aggregate annotation entries are distributed over individually small rows, so the test detects a per-row-only implementation.

Peek and annotation-outer-map raw-limit controls intentionally use a duplicate extra pair and a dangling extra row while keeping card count valid. Their direct raw-helper assertions isolate advertised count rejection before semantic decoding; they must not be described as otherwise legal over-cap states. For a fixed 4096-card/one-seat valid roster, a 4097th distinct valid Peek pair is impossible. The accompanying valid exact-cap controls are appropriate. af477d9d correctly changes final decode assertions to versioned snapshots, avoiding rejection merely because raw maps carried journals.

The tests preserve full refusal state with the documented sequenced next_seq consumption rather than incorrectly expecting the log sequence to rewind. No additional broad engine unit audit is required before implementing the already planned differential gate.

## Remaining acceptance scope

- Execute the shared full mechanics/disclosure/allocator matrix against authoritative DecideRequest + SDK projection + fresh native/hardened engine/plugin instances. Include both viewer audiences, saved pending requests and rollback, exact config (`despawn_any`), ABI0 guards and legacy export/restore. Existing engine unit suite counts cannot establish cross-layer equality.
- Carry the pending-Peek final sets and both audiences through fresh engine/SDK reconstruction in that matrix. Test-only source assertions do not establish private-face/cache/session behavior.
- Refresh and gate engine/plugin artifacts after SDK repairs and integration. Engine approval here is compatible with SDK source remaining blocked by its separate review. No test was run by this reviewer.

## Phase-one fixture 0d113353

The shared fixture uses real `decide::parse`, `EffectProjection::from_decide` and `project_verdict`, and emits normal SDK verdict bytes. The native PluginModule adapter decodes those bytes and advertises ABI1. The cdylib example uses the actual `export_plugin!` exports and a no-content fixture manifest. The host test performs real native fold_mediated Begin, authoritative active snapshot, fresh engine/plugin adapter construction, Commit and warm/cold final snapshot equality. The net test enters through real HostSession::intent. These are useful scaffolding, not a claimed native/wasm or disclosure acceptance matrix.

**Known fixture correction required:** 0d113353 MANIFEST encodes `version` as unsigned1, but `sim::wire::PluginManifest.version` is String. Its manual reader test repeats the wrong expectation. Root is correcting this to text "1" with the real `decode_plugin_manifest` assertion; the old manifest must not be handed to the wasm loader as accepted evidence. Root's lockfile-only1c67176e is noted separately.

Phase-one limits are explicit: native-only Begin/Commit, no actual wasm artifact loading/export test, no SDK-after versus authoritative-after state comparison, no full mechanical/disclosure workload, no private recipient delivery or restored wasm suffix. The net assertion currently checks a Game entry exists rather than decoding/asserting the active group; strengthen that assertion before crediting a HostSession group lifecycle test. The one-byte wrapping plugin command counter is deterministic fixture bookkeeping, not a production continuation schema. These points do not block neutral engine source approval; they define the remaining fixture/matrix work.

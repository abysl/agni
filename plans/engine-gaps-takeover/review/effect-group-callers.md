# Neutral effect-group journal: caller inventory

Inventory taken from the Costs checkout at `4d661781`; this is a protocol
design map only. No production changes are included.

## Authoritative fold and snapshot path

- `sim/src/log.rs:193-218` — `LogState` is the authoritative serializable
  engine state. It includes table/seats/zones, annotations/counters/tokens,
  disclosure sets (`revealed`, `owed_reveals`, `peeks`, `shown`), plugin state,
  options, and `next_seq`; it has no journal field or schema marker.
- `sim/src/log.rs:717-752` — neutral `Effect` is the seven existing serde
  variants (`Move`, `Annotate`, `Counter`, `Spawn`, `Despawn`, `Reveal`,
  `Peek`). There are no numeric effect tags; externally tagged CBOR names are
  the wire keys. The first unused operation names are
  `BeginEffectGroup`, `CommitEffectGroup`, and `RollbackEffectGroup`.
- `sim/src/log.rs:804-940` — `apply_effect` mutates the cloned next state;
  Move sheds/forgets visibility, Spawn allocates a fresh table id, Despawn
  removes card/disclosure state, and Reveal/Peek update disclosure sets.
- `sim/src/log.rs:955-998` — `fold_begin` advances `next_seq`; `fold_finish`
  currently gets same-request atomicity by cloning `LogState`, applying the
  entry/effects, then replacing the original. A saved pending replacement
  needs an explicit journal because this clone is gone across snapshot/reload.
- `sim/src/log.rs:1001-1006` — `encode_state`/`decode_state` are generic
  ciborium over `LogState`; there is no explicit snapshot version or migration
  hook. A journal addition therefore needs a versioned snapshot decision and
  old-snapshot compatibility tests.
- `sim/src/engine.rs:64-175` — `NativeEngine` owns state and exposes
  `fold_entry`, `decide_request`, `snapshot`, and `restore`.
- `sim/src/engine.rs:190-260` — native fold and shadow-fold paths compare
  engine results/deltas; journal state must remain in native/wasm snapshots
  and preserve shadow parity.
- `engine/wasm/src/lib.rs:23-79,81-157` — wasm handlers serialize fold,
  decide, snapshot, and restore through the shared ABI; no separate journal
  channel exists.
- `engine/host/src/lib.rs:298-385` — `AbiEngine` enforces
  `ENGINE_ABI_VERSION` and sends the CBOR requests/replies. Snapshot/restore
  are opaque bytes at this layer.

## SDK projection and plugin effect encoding

- `sim/src/abi.rs:10` — `ENGINE_ABI_VERSION` is `3`; adding a required
  DecideRequest/journal or snapshot shape should move this to the next ABI
  version and update the ABI mismatch tests/goldens together.
- `sim/src/abi.rs:49-65` — `DecideRequest` carries plugin state, full
  `LogState`, and entry. `PluginViewRequest` carries the same state plus a
  private `faces` overlay; the overlay is owner/peek knowledge and must not be
  used as authoritative undo data.
- `plugins/sdk/src/decide.rs:261-295` — SDK `Effect` has the same seven
  variants, represented by Rust enum values.
- `plugins/sdk/src/decide.rs:322-421` — SDK effects are manually encoded as
  externally tagged text maps (`Move`, `Annotate`, etc.), with no numeric tag
  allocation. New journal operations require matching enum/encoder/parser
  changes; they cannot be added only to sim serde.
- `plugins/sdk/src/decide.rs:424-497` — `Verdict` wraps accept/plugin_state,
  optional effects, and reason. The guest currently returns only effects; a
  journal protocol must define whether begin/commit/rollback are effects or a
  separate engine-owned request/result field.
- `plugins/sdk/src/table.rs:314-336,697-782` — SDK `Snapshot` and its parser
  project cards/zones/counters/reveals/tokens/options. Unknown map keys are
  skipped, so an additive public projection key can be tolerated, but the
  projection is not lossless enough to restore authoritative order, absent
  counter rows, faces, or disclosure transitions.
- `plugins/sdk/src/guest.rs:1-75` — guest ABI exports only `abi_version`,
  `decide`, and `view`; no transaction lifecycle export exists.

## Views, network, and replay callers

- `sim/src/view.rs:36-51,94-146` — `TableView` is a public/seat view and
  intentionally omits journal data; `table_view` derives visibility from
  `revealed`/`peeks`/zone visibility.
- `sim/src/view.rs:236-255` — `view_to_table` reconstructs a renderer table
  with hidden faces unless supplied in the private face map; this is not a
  restoration path for a journal.
- `net/src/host.rs:105-118,243-320` — `HostSession` caches `LogState`, folds
  through the engine, and builds plugin views with `PluginViewRequest::seen_by`.
  Journal bytes should stay inside engine snapshot/decide state unless a new
  host message is deliberately designed.
- `net/src/client.rs:184-194,202-392` — `ClientSession` replays entries,
  maintains a mirror, harvests Reveal/Spawn disclosures, and builds the same
  private plugin view. It has no rollback state beyond replay/snapshot.
- `net/src/proto.rs:8,179-285` — `WIRE_VERSION` is `6`; `HostMsg`/`ClientMsg`
  contain entries, faces, and module transfer but no engine snapshot or
  journal. An engine-internal journal does not require a wire bump; any new
  network-carried journal field would require `WIRE_VERSION` and golden updates.
- `net/tests/riftbound_turns.rs:240-285` — native/wasm snapshots are decoded,
  restored, and compared after each turn; this is the decisive saved-pending
  parity seam.
- `engine/host/tests/host.rs:175-245,262-356` and
  `sim/src/engine.rs:543-563,612-633` — ABI version, snapshot parity,
  restore/resume, and mismatch tests are the host/engine compatibility gates.
- `sim/tests/goldens.rs:30-41,152-226` and `net/tests/goldens.rs:22-33,167-185`
  — current pinned CBOR vectors. Internal state/ABI changes need new vectors
  and an explicit version bump; wire vectors remain unchanged unless net
  messages change.

## Compatibility constraints for the eventual design

- Keep the journal in authoritative engine state and snapshots, separate from
  plugin private state and public `TableView`; do not reconstruct it from
  `PluginViewRequest::faces`.
- Use typed before-images for cards (including exact face/owner/zone/seat/order),
  annotations/counters including absent rows, ordered zone/card contents,
  allocated ids/high-water mark, and mechanical disclosure changes. Restore
  internally rather than through ordinary Move/Spawn, which shed state and
  emit normal transitions.
- Preserve accepted Reveal/seat disclosure history. Rollback should restore
  mechanical state only: never rewind log sequence, synthesize a Reveal, or
  return a private face to an unauthorized audience.
- Keep one active group per plugin (or reject nesting/interleaving) and make
  Begin/Commit/Rollback deterministic. Plugin-only resource pools/promises
  remain in the game blob; engine state covers runes/tokens/counters/etc.
- Preserve monotonic allocation: restore the original object reference for a
  preexisting object, but never reuse an id that was committed or exposed.
- The current engine ABI is 3, plugin ABI is 0, and wire protocol is 6. A new
  snapshot/journal schema should define an explicit snapshot version or
  versioned wrapper and update ABI negotiation and fixture regeneration as one
  change.

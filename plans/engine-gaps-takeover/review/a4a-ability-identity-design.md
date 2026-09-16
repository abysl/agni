# A4a — stable ability identity and accounting

Bounded read-only plan against current integration and saved triggers `7dfaef5f`.
No production edits/builds. Depends on A1's ObjectRef/legacy policy in
`review/a1-incarnation-design.md`. Use root's `/tmp/agni-a4a-callers.txt` as the
exact saved-line index; regenerate it on the eventual merged baseline. Paths below
are relative to `games/riftbound-turns/src/` unless qualified.

## Confirmed problems

- Saved `ItemKind::Granted/Lent { holder, lender, index }` preserves more provenance
  than integration's source/index, but `activate::locate` recovers index by pointer;
  `granted_texts` flattens current copied/granted text, and `lent_at` uses the current
  holder list. Changing an earlier grant can change what an index means.
- `TurnEvent::Activate { source, ability:u8 }` is interpreted against that list
  *before* a stable ItemKind is constructed. A stale view click can activate a
  different lender. Stabilizing only serialized pending items is insufficient.
- `triggers::interchangeable` skips OrderTriggers using pointer equality plus
  controller/subject/noted/origin, ignoring source/lender provenance. Equal callback
  or definition pointers do not prove order-independent effects or identical “me”.
- Trigger collection and activation consume card-wide FLAG_ONCE_USED/seat bits.
  Two independent abilities on one holder share a quota; lenders/copies collapse.
  `once_by_seat(seat & 3)` aliases seats. Official enforced player limits may make
  seat4 unreachable, but the new ledger must use validated actual seats, not bits.
- `Once::PerTurn` also conflates “first/Nth occurrence” with “perform up to N times”.
  Core383.3.e.2 permits declining an optional once-each-turn trigger at finalization
  without spending its use. Saved collect_ordered spends on enqueue instead.
  Core383.1.b's Nth condition is a different occurrence policy; declining a later
  optional effect does not manufacture another first occurrence.

## Minimal identity types and ownership

Keep three separate identities:

1. `AbilityDefId`: a stable definition address within the pinned plugin: script
   definition key plus explicit path (printed ability slot, implicit ability kind,
   static/attached grant path and local ability slot). Immutable source paths are
   valid under a pinned module; a runtime filtered-vector offset is not. Do not
   hash Rust function addresses or use names alone to identify an instance.
2. `AbilityInstanceId`: A1 holder ObjectRef + provenance. Printed/implicit ability
   uses its definition path. A granted instance includes granting ObjectRef,
   grant-site path and deterministic GrantInstanceId; copied text additionally
   records the selected definition and copy/borrow route. Two physical lenders,
   two distinct grant sites, or two genuine copied instances remain independent
   even if they resolve to the same AbilityDefId.
3. `OccurrenceId`/chain item ID identifies one activation/trigger execution. Many
   occurrences can share one ability instance and usage quota. A delayed ability
   can outlive the original holder/lender and still retain its executable definition.

Saved Granted means a granted triggered occurrence; Lent means granted/borrowed
activation. Preserve that distinction and holder-as-“me” semantics, but make both
carry the common typed ability instance. The lender is provenance/linked-effect
identity, not automatically the target of self-costs or the current controller.
Capture the chain controller separately. Execution lookup uses captured definition,
not “whatever this lender currently lends at index i”. Source liveness controls
new availability, not continued existence of an already-generated independent item.

Grant::Ability yields definition paths directly. Change Grant::Copied's return
contract from a naked `&[Ability]` to identified definitions; Svellsongur must capture
which wearer's definition was copied. Grant::Borrowed returns stable instances/
routes; Heimerdinger must not lose the intermediate holder when borrowing an already
lent ability. Remove pointer-based `locate` and list-offset identity reconstruction.
`lent_on` may deduplicate repeated enumeration of the *same instance*, not identical
text or `(lender,index)` reached through genuinely distinct grants.

Runtime-owned CostedGrant currently owns keyword costs, not arbitrary Ability text;
keep A9 representation intact. If the new path creates owned ability grants, assign
GrantInstanceId at creation from a checked deterministic ledger. Never allocate IDs
in view, affordability probes or ordinary enumeration. Continuous grants require a
stable grant-site identity and explicit grant lifecycle; don't generate fresh IDs
on each statics scan. Genuine removal/regrant and copy replacement must be distinct
from temporary inability to activate (exhausted, timing, missing targets).

**Lifecycle assumption requiring card-specific confirmation:** loss/reacquisition
of granted text may create a fresh ability instance even when holder/lender stayed
on board. Do not equate detach/reattach, condition inactivity and source reincarnation
without reviewing that grant's rule text. Record its policy at the grant site, test
it, and do not silently reset once accounting on a view refresh. A1 already settles
non-board source/holder reincarnation; full floating-aura mechanics remain S1.

## Admission and serialization

Add a new plugin TurnEvent activation payload carrying the stable ability instance
(or a revision-bound opaque token which verifies that exact instance). Recommend
an explicit key, so removing an unrelated grant does not stale every unaffected
ability. Resolve it among *currently available* instances, verify A1 incarnations,
controller/timing/cost, then snapshot definition/provenance into the pending item.
An old click for a removed lender must refuse, never select its successor by index.

PluginView affordance.data already carries opaque plugin bytes: use it. Update
`present.rs`, activate::Offer, `engine/legal.rs` activation highlights and any Kai/AI
adapter that reconstructs Activate from source+u8. A display ordering index is fine
only if execution also validates the stable key. Generic SDK/ABI/wire needs a version
bump only if a shared shape actually changes; plugin event/schema changes need their
own explicit compatibility contract. No renderer-owned rules or spirit changes.
Keep old events for old pinned replays; reject ambiguous old activation payloads
under the new plugin instead of treating them as a new list index.

Add explicit versioned fields for identity/usage/grant creation counters to the
next blob schema after finalized Play/A1. Preserve structural readers and fixtures
for old ItemKind tags0–3 and saved Granted/Lent tags4–5; tag overlap alone does not
make unrelated saved-lane schemas compatible. Busy legacy source/index rows and
card-wide spent bits cannot reveal which independent ability was used. Follow A1:
structural decode remains available; safe new-engine import requires a documented
clean boundary, or finish under the old pin/supported replay. Don't clear ambiguous
spent bits or duplicate one old usage across all new instances while calling that
faithful migration. Capture Limit policy and accounting owner on pending occurrences
so reload never recomputes them from a changed lender list.

## Usage policy and accounting boundaries

Use an ordered ledger keyed by `(AbilityInstanceId, window, seat_scope, policy)`;
per-turn window is the actual game turn, per-seat means the unaliased specified
seat. Controller changes must not refresh a global once-per-turn use. A1 creates
new source/holder instances where the rule requires; old historical usage can be
pruned after no pending/linked reference needs it. Checked IDs/counts, no truncation.

Replace ambiguous Once metadata with explicit policies, retaining concise prelude
constructors: unrestricted; activation N-per-turn; performed-trigger N-per-turn;
Nth matched occurrence; optionally per-seat variants. Map each existing once helper
call from its printed text. Blade Twirler/Jayce “first time” and optional “once each
turn, you may” must not share one consume-on-enqueue behavior.

- Availability/quote/view reads never spend. Successful activation commits its use
  with accepted finalization/payment; cancellation/refusal before that releases any
  pending reservation. Reload and retry cannot commit twice for one occurrence.
- A limited performed trigger checks/claims quota when the player accepts its
  optional performance and finalization succeeds. Decline doesn't spend; removing
  an unpayable pending item doesn't spend. Do not refund merely because a finalized
  ability later has an ineffective instruction. Reserve/commit/release explicitly
  where multiple pending occurrences compete for the same quota.
- Nth occurrence accounting advances on qualifying inciting events, not successful
  effect resolution; store matched-count/selected occurrence separately from use
  count. A3 owns exact captured-event timing, and A4b owns controller choice among
  simultaneous qualifying instances (383.1.b). A4a supplies stable keys and batch/
  occurrence slots; it must not choose the first traversed event as a substitute.
- Replacement quotas use the same keyed ledger namespace but commit only when the
  replacement is applied (371); declining optional replacement doesn't consume.
  Expose the API now; D1/A11 own suspended replacement execution and its result.
- `reckoners_arena.rs` direct once checks/spends and all effect-generated activations
  must use the common entry point. Avoid a second card-local ledger or early spend.
  Preserve unrelated per-seat draws/cards-played/Legion counters; they count game
  events, not instances of the observing ability.

## Ordering and implementation inventory

Smallest correct ordering change: always ask when a same-controller trigger batch
has more than one item. Remove pointer-based interchangeable elision. Equal stable
definition IDs are still insufficient: holders, lenders, occurrence data and scripts
can make order observable. Any later elision needs an explicit semantic equivalence
contract including full provenance/state; it is not required for A4a. Stable sort
only sets initial presentation order, not the player's rules-required decision.

Migrate the supplied caller index plus these transitive surfaces:
- `cards/mod.rs`, `cards/prelude.rs`: Source/Once/Grant/Copied/Borrowed definitions;
  explicit Implicit identities replacing collision-prone GRANTED/IMPLICIT ranges.
- `engine/attach.rs::{granted_abilities,granted_on}`, statics::sourced_grants_on,
  activate::{granted_texts,locate,lent_on_by,lent_at,item_kind,lent_index_of}.
- targets::{ability_at,ability_of_kind,ability_of}, all ItemKind matches in cost,
  pay/self-cost, play, chain, triggers::Match/texts_of/collect_ordered/queue_delayed.
- `lib.rs` TurnEvent codec/dispatch; present activation affordances; legal highlights;
  SDK LegalKind consumers/AI activation constructors wherever an index is executable.
- state usage encoding, expiry turn reset; direct card flags/bit readers/writers,
  particularly Reckoner's Arena; actual copied/lent callers Svellsongur,
  Heimerdinger, attachment texts, Forge of the Fluft, Gardens of Becoming.
This is an implementation checklist, not a claim the lexical index is exhaustive.
Use compiler-driven types plus production-prefix searches; do not blindly convert
physical card IDs used for wire effects or immutable card-script lookup.

## Decisive acceptance and sequence

1. Two independent once abilities on one holder; then two same-text equipment
   lenders and Svellsongur copied text. Using one leaves the others available;
   save/reload between activations and verify correct holder self-cost/lender link.
2. Capture an affordance, remove an earlier grant/reorder active availability, then
   submit old key: it either invokes that still-valid exact ability or refuses.
   It never invokes another lender. Bounce/replay holder/lender invalidates old key.
3. Queue a copied/lent trigger, then change copied source/attachment before resolution;
   captured definition/provenance remains correct without reading a new list offset.
4. Optional performed-once trigger declined, then later accepted; failed payment/
   cancellation doesn't consume; successful accepted use and reload consumes once.
   Contrast first-occurrence trigger with optional suffix declined: no new first.
5. Two untargeted callbacks with identical definition but different holders/lenders
   produce OrderTriggers and honor both chosen orders. Cold native/hardened WASM
   must present identical prompt/options/effects, regardless of prior view calls.
6. Per-seat and turn expiry boundaries, controller transfer, grant lifecycle and A1
   reincarnation; test validated supported seats, plus ledger non-aliasing directly.
   Preserve legacy structural fixture decoding while rejecting ambiguous admission.

Sequence: typed definition/provenance + grant enumeration → stable activation bytes
and pending snapshot → common usage ledger/policy mapping → ordering and all bypass
callers → focused native/reload/parity plus full required gates. Coordinate mutations
with A5 rollback and A3 event capture; leave A4b simultaneous Nth selection explicit.

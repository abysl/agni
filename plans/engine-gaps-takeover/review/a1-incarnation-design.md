# A1: object incarnations — bounded implementation plan

Scope: integration `eaf3a317` production baseline (v11), anticipating Play's v12
Limited/child-continuation additions. Read-only; no production edits or builds.
Sources: `plan.md` A1; `wiki/design/architecture.md`; pinned
`games/riftbound/rules/riftbound-core-rules-v2026-07-16.txt` rules 107–108, 124,
359.3.e.2–14, 359.3.f, 392, 393–397 and 427.3. Paths below are relative to
`games/riftbound-turns/src/`. Incorporate root's `/tmp/agni-a1-mutation-callers.txt`;
its lexical candidates are not proof that all semantic transitions are covered.

## Rule findings and proposed identity contract

- **Rule:** changing zones with either endpoint Non-Board creates a new object;
  board-to-board movement does not. Temporary modifications/statuses disappear
  on the former transition (124/124.1). A target can regain legality after a
  battlefield→base→battlefield move, but cannot after hand→replay (359.3.e.3–4).
- **Proposal:** plugin-local `ObjectRef { card: u32, incarnation: u64 }`, with
  deterministic ordered ledger keyed by physical card ID. Capture explicitly;
  resolve only if that incarnation is current and the card exists. Keep physical
  IDs in SDK effects/wire/UI. Do not put Riftbound identity rules into spirit,
  generic agni-core/sim or the renderer. Use checked increments; overflow refuses
  the whole action rather than wrapping/saturating or silently retaining identity.
- Ledger survives `drop_card_state`, default-row pruning, departure and despawn;
  store it separately from temporary CardState. New physical IDs start at zero;
  retain tombstones if an ID can reappear. Reset only with a new game state that
  also clears every old reference. Never derive generations from allocations,
  card names, current zone alone, or process-local caches.
- **Rule classification:** Board = each Base, Battlefield, associated Facedown
  sub-zone, and Legend zone (107). Chain, Hand, Main/Rune Deck, Trash, Champion
  zone and Banishment are Non-Board (108). Zone identity includes seat for private
  per-seat zones; a same-zone reorder/shuffle/reveal/exhaust/control-only change
  is not a zone transition. Different players' bases remain board-to-board.
- **Engine mapping:** `zones.rune_pool` stores rune cards that rules place on the
  board; classify it Board. `SeatState.pool` is conceptual energy/power (166),
  not a zone containing object references. Hidden cards use `hidden_at`; classify
  the Facedown logical sub-zone as Board even while inactive. Legend classification
  must use zone, not card kind. `face_on_board` omits Legend; `face_in_play` admits
  legend kind irrespective of zone; `statics::face_active` also excludes hidden,
  pending and attached cards. None is the identity predicate.
- Sideboard, absent/unplaced and setup-only storage need an explicit adapter
  policy: proposed out-of-game/Non-Board, never a live target. This is an engine
  boundary assumption, not an additional zone asserted by rule 108.

## Authoritative transitions, including projections

1. Centralize a successful semantic transition observer with before/after object
   location. Increment once iff the logical zones differ and either is Non-Board;
   clear old temporary CardState, card counters/status annotations and object-bound
   effects before installing new-entry status. Preserve printed identity/ownership
   and historical records. A printed keyword is not an old granted modification;
   `Expiry::Permanent` does not make a grant survive a new object.
2. Cover admitted incoming actions (`Ctx::new` shadow), `Ctx::enter`, successful
   emitted Move/Spawn/Despawn, Clear/Reset and helper mutations. Never count both
   snapshots in `enter`, failed effects, host/view previews, or `replay_table`.
   Identity changes belong to the same verdict/checkpoint as effects. A rejected
   action restores ledger, logical position and all identity-bound cleanup.
3. **Raw movement is insufficient:** a hand unit dragged directly to base is
   physically there in `Ctx::new`, yet its Pending play is logically on the Chain.
   Conversely hidden/revealed helpers can keep physical storage while declaring a
   play. Represent this bounded overlay explicitly: begin = old zone→Chain;
   permanent finalization = Chain→Board; spell finish/cancel = Chain→destination.
   Route the physical projection through these transitions without extra bumps.
   Do not reset or finalize the object merely because a preview moved its card.
4. Captured play item identity must follow its own *authorized* Chain→Board result
   when constructing Played/Entered triggers. Other targeting references to the
   old Chain object must become stale. Cleanup happens before ready/entered flags,
   attachments or grants intentionally created for the new object are applied.
5. Establish free→enforced identity at a documented cleared-state boundary, or run
   the same observer on free-mode effects (`rules.rs` bypasses Ctx). Never resume
   old enforced references after untracked free movement. Physical reveal/peek
   transport knowledge is distinct from an object status: clearing Revealed status
   must not erase necessary face delivery or disclose a hidden face.

## Reference carrier migration inventory

| Carrier | Required treatment |
|---|---|
| `TargetRef::Card`; `ChainItem.targets` and `.subject` | Store ObjectRef, including remembered suffixes. Validate identity before type/location/controller/property checks. Keep stale positions for spec/group indexing; exclude stale objects from target-count/referent queries per 359.3.e.9.a. Do not compact the vector or refresh refs at resolution. |
| `ItemKind::{Spell,Permanent}.card`, `{Ability,Trigger}.source` | Distinguish the live played object from the historical source of an independent chain ability. Capture source incarnation; never query a replayed replacement as “me”. Do not drop an already-created ability merely because its source left. Preserve executable script selection separately; A4a later supplies full stable ability/lender identity. |
| `ChainItem.awaiting`; transient `Ctx.awaiting`, `.remembered`, `.picked` handoff | Capture incarnation at wait/selection. Physical Reveal may deliver known face data, but resumes only the matching saved item+object; an old waiter must not attach to a new incarnation. Validate response before converting physical picks into references. |
| `Delayed.source`, `.args` | Source is historical, not a liveness gate (392). Args are currently encoded u32 then blindly mapped to Card targets by `queue_delayed`; replace with typed refs/values, not “all integers are IDs”. Capture at creation, never recapture when due. |
| `CardState.id`, `.attached_to`, `.control_source` | State belongs to current incarnation; endpoints are ObjectRefs. Clear old statuses/grants/names/attachments on transition, detach/expire dependent relationships by old identity. Independent control effects may have their own duration; do not invent source-liveness requirements. |
| `Expiry::WhileAttached(u32)` in might/grants/costed grants/promises/preventions | Bind the referenced attachment object. Preserve duration semantics. `MightMod.src: u16` is an existing effect discriminator: inventory its meanings before typing it; it is not automatically a physical card ID. |
| `PromptWhy::GroupMove.unit` | Persist ObjectRef and reject a stale anchor. Other prompt item IDs and `TargetRef::Item` identify chain items, not physical cards; A4a/A6 address their identity/exhaustion separately. |
| `Death.card`, `Noted`, queued trigger subjects; transient `Event`, `triggers::Match`, `Cause::Ability(Source)` | Death/source refs identify historical incarnations; never discard history because the object is gone. Noted's zone/might/controller/alone are captured values, not live references. Capture before reset; this is identity plumbing, not A3 trigger-timing redesign. |
| `Prevention` and recovered shields | Current v11 Prevention is global, with no unit field; do not make it expire on arbitrary departure. Saved damage adds `unit: Option<u32>`: Some must become an ObjectRef, None remains global. Target-specific delayed shields must not protect a replayed card. |
| `Origin`, `Showdown`, `Staged`, control holders, pools/promises | Current zone/seat/value fields need no card generation. Origins preserve provenance, not a wildcard permission to follow a physical card. Play v12 Limited card/parent-child locals inherit the same typed-reference rules; location lists remain zones. |
| `ANNOTATION_REPLACED`, copied/runtime card links, script-registry IDs | Audit annotation payloads that refer to cards (battlefield replacement/swap-back). A runtime relation needs typed identity; immutable face/script lookup and asset IDs remain physical/content IDs. |

Compiler-driven conversion of the carrier types must be accompanied by an audit
of `card_target`, `remembered_cards`, `targets::valid`, equality/contains/count
queries, and direct `ctx.card(item.kind.source())` callers. Returning a bare ID
from a checked helper is safe only for an immediate operation; retaining it over
another movement/suspension loses the guarantee.

## Explicit linked instructions, not a universal escape hatch

A1 must provide an operation result that distinguishes prior-object reference,
optional resulting ObjectRef, executed/replaced outcome and permitted captured
values. The operation that moved the object produces that successor; callers may
use it only for a rule-authorized linked instruction. Serialize it when suspended.
Never “follow the newest incarnation of this ID” or permit all effects sharing a
source card to follow it. Banish-linked records bind the originating object/effect
instance plus resulting banished ObjectRef (427.3, 393–397).

359.3.e.13 permits looking back at characteristics of an object moved by that spell
or ability; it does not give universal last-known-information fallback for a target
that was already illegal. Hidden Blade's controller draw can use captured data
when its kill instruction executed, including replacement; Deathgrip's “if you do”
requires the kill action itself. An initially stale target does neither linked
instruction. Ordinary independent suffixes still execute. Actual choice suspension
remains A5/D1; this plan defines the identity/result contract only.

## Legacy policy and implementation order

A1 needs the next explicit blob schema after finalized Play; retain exact historical
readers. Legacy snapshots contain no transition history: assigning every reference
incarnation zero cannot prove an old target was not already bounced/replayed.
Recommended safe import: establish zero generations at a boundary with no pending
chain/queue/prompt/awaited/linked/delayed object references; preserve current local
state as that imported boundary. For busy legacy games, continue under their pinned
old plugin or reconstruct under a specifically supported migration/replay path.
Keep v7/v9/v10/v11 (and finalized Play) structural decoders and their exact
schema fixtures independently of resume admission: decoding a busy document must
not imply it can start under A1. A clean import guarantees prospective identity
safety from that boundary, not retroactive validation of its history. Do not
silently label busy snapshots identity-correct or cancel their pending effects. If compatibility
requires grandfathering busy refs, record it as an explicit weaker policy decision.

Implement serially: (1) semantic-zone adapter/ledger/atomic observer, including Play
projection; (2) typed carriers and validation/helper callsites; (3) linked-result and
legacy-boundary handling; (4) targeted acceptance plus existing full/parity gates.
No floating-aura, trigger capture, ability accounting or damage-order redesign here.

## Decisive acceptance tests

- Same physical ID: target, bounce, replay, reload, resolve. Old target stays stale;
  independent draw still occurs; old target is excluded from “chooses only” counts.
  Contrast battlefield→base→battlefield and control changes: same incarnation,
  legality reconsidered normally. Partial multi-target spell still affects survivors.
- Transition matrix: Hand→Chain→Board, Facedown→Chain, rune deck→board rune storage,
  Legend→Banishment, attachment board moves, same-deck reorder/reveal, cancel return,
  successful spawn/despawn. Check exact increments and cleanup/new-entry statuses.
- Hidden selected card leaves/reenters before its saved face response: no old resume;
  valid unchanged-object reveal resumes once across a reconstructed request. Include
  Play's saved Limited child/parent boundary when its final API lands.
- Shield/delayed target/control-link on old incarnation never affects its replay;
  independent delayed ability still executes after its source leaves. Death history
  keeps both distinct incarnations of the same physical card.
- Linked kill/look-back and banish→play use captured result only; stale initial
  target cannot activate them. A second same-name/source reincarnation cannot take
  an earlier banish link. Cover Hidden Blade versus Deathgrip replacement outcomes
  using existing synchronous replacements; deferred-choice behavior stays D1/A5.
- Cold native/hardened replay compares blob/effects/prompts across all boundaries;
  view/affordability previews and `replay_table` change no generation. Rejection,
  callback rollback and failed effect restore identities with table state. Add an
  overflow refusal and legacy clean/busy import fixture; no wrap or silent rebinding.

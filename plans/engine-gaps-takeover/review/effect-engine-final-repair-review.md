# Engine repair 9c24b873 source review

Reviewed only committed `2aaca6d2..9c24b873` core/sim/engine changes in `/tmp/agni-effect-groups`; dirty SDK/game files excluded. No builds or production edits. Prior source and frozen-protocol findings are the comparison baseline.

**Disposition: changes required before engine-half acceptance.** The reported disclosure-debt, transient-history and marker fixes are substantially improved and have useful tests. Remaining blockers are bounded protocol issues below. Whole-mechanics/native-SDK-hardened/audience acceptance belongs in the already planned integration gate rather than another full-game corpus.

## Closed or materially improved

- `validate_group_information` now processes ordered pending RequestReveal IDs as a set: duplicate requests coalesce, and PublicReveal clears only preceding debt. `repeated_reveal_requests_coalesce_and_round_trip_at_each_boundary` reaches one fulfillment after two requests and then Commit; it distinguishes the prior count bug.
- Rollback tracks later pending requests independently of restored revealed state. It clears inappropriate revealed/shown/face for the still-owed surviving card, retains its debt in a revealable Owner placement, and refuses None/absent restoration. The surviving-card test checks the later face can actually be fulfilled by its restored owner. The old transient test now also fulfills its latest debt and successfully retries rollback.
- Historical RequestPeek targets need only be allocated IDs with valid seats, not currently live. The transient Peek/Despawn test checks snapshot equality and subsequent rollback.
- First active marker ID is checked before incoming action/PublicReveal append. Later wrong-ID terminals now produce indexed BadEffect; first membership failures remain typed. The active-admission test now calls native_decide_request on the actual active state.
- Decoder checks now include card player/owner membership, counter declarations/scopes/targets/duplicate keys/values, aggregate annotations, before/current allocator relation and more disclosure shape rules. They are useful checks, but their scope and execution order still need correction.

## Must fix: accepted Peek audience is lost on rollback

`rollback_effect_group` still skips a recorded RequestPeek when candidate.revealed is true, and PublicReveal clears every Peek for a visible restored card. Neither is a sufficient audience test.

Admitted sequence: baseline hidden Owner card c owned/held by seat0; Begin; Move to All; normal PublicReveal; Move through None; Peek(c,1); Rollback. The Peek was actually granted while unrevealed and is recorded. Rollback installs the earlier public face, sets revealed, then skips the later RequestPeek. Returned Owner c has no shown or seat1 Peek. `sim/src/view.rs::table_view` sets still_public=false for Owner/None and grants an Owner face only to its seat, shown recipients or explicit Peek recipients. Seat1 therefore loses its accepted authorization. A baseline seat1 Peek is similarly lost by the PublicReveal arm's blanket clearing when Reveal happened in All but restoration is Owner without shown.

Preserve accepted baseline/recorded grants for viewers not already authorized by the restored All/Owner/shown policy; revealed alone is insufficient. The new `validate_disclosures` check `!revealed.contains(id)` for every Peek must also allow legitimate retained Owner grants after reconciliation. Test baseline grant and later recorded grant, both viewers, reload, and same-placement Owner/All redundancy controls. Ordinary unrelated Peek-on-already-revealed semantics are not being changed. Root confirmed this is the intended authorization contract; the frozen plan's overbroad skip-revealed shortcut has a clarification appended.

## Must fix: feature-specific limits still run after allocation

`SnapshotScanner` is a real byte/depth prepass now, but it permits a million elements at every array/map. `cbor_value` still allocates the complete journal before the 4096-card/information, 16384-counter/annotation and seated-count Peek limits run in bounded_group_value/validate_mechanical. For example, a complete, correctly shaped active capsule with 4097 cards passes the generic scanner and allocates its Value tree before rejection. The new huge test instead puts a wrong-shaped/truncated array where effect_group should be; decode(None) does not distinguish feature-limit rejection.

Bounded correction: locate the versioned state, seats and effect_group byte spans without constructing their payload Values, allowing arbitrary map-key order. Then scan the recognized group/before/table/information paths with their exact frozen counts; sum annotation rows across all card maps before allocating them; derive Peek count from the bounded actual roster. Only after that pass may the Value/typed decoder allocate the journal. Reject duplicate semantic keys before serde normalization. Replace new-journal prefix `.any` duplicate scans with bounded deterministic sets/sorted key metadata rather than permitting million-element quadratic scans. This is not a request to optimize unrelated old generic payloads.

Tests must assert the feature-aware scanner rejects complete valid-shape over-limit inputs, then assert decode rejects too; a plain decode(None) is insufficient to prove ordering. Include valid count-at-limit controls. Use a valid active group with 4097 unique cards; information length4097; 16385 unique counter keys under otherwise valid declarations; one or several valid cards totaling16385 annotation entries; and a seat-derived Peek limit. Separately keep header-only huge-length and depth tests to prove early rejection of truncated input. No large wasm stress corpus is needed; representative export rejection plus bounded unit vectors suffice.

## Must fix: current dangling tokens are no longer rejected

`validate_state` dropped its old current tokens live-reference check. validate_disclosures has no tokens parameter; only `before.tokens` remains checked. An otherwise valid snapshot with current tokens containing a nonexistent ID now decodes. Restore current token-reference validation and test a valid existing token control versus one absent ID. The earlier duplicate-token test had nonexistent IDs and could not isolate duplicate detection; keep a separate duplicate-real-token vector.

## Must fix: new decoder-wide limits/semantics exceed the group feature's scope

The global 16MiB byte cap, million-element collection cap, unconditional current annotation cap and unconditional `4096 * seats` Peek cap apply even with no group. Existing ordinary fold paths do not enforce those bounds, so accepted nongroup states can fail their next restore. New `zone.span > 0` and counter step constraints likewise reject configurations accepted by current Genesis validation. The journal feature must not silently redefine unrelated persisted state validity.

Current-source controls which require no game content:

- Genesis/Deal a public card, then ordinary Annotate with one value of 16MiB+1 bytes: the native fold admits it; old snapshot decode could restore it; new prescan rejects solely on bytes. Use a native compatibility unit case rather than an expensive wasm stress case.
- Ordinary annotations on one real card with 16385 distinct keys are admitted; no group exists. Decode must remain compatible; subsequent Begin should reject LimitExceeded without mutation. The same oversized aggregate in the group capsule must reject before allocation.
- Ordinary Peek grants on 4097 unrevealed real cards to one seated viewer are admitted with no group; their snapshot must remain compatible, while Begin's 4096-card capsule constraint rejects. This separates baseline admission from feature limits.
- A nonempty public face whose domain Vec contains 1,000,001 short strings is accepted by ordinary Reveal without game-specific domain validation; a generic collection cap must not newly reject that nongroup state. A declared zone with span0 or counter step0 is also accepted by current Genesis. Do not label those rows malformed solely from new unstated constraints.

Scope revision: apply frozen group counts to the journal/capsule/information and its creation, keeping existing nongroup structural compatibility. If a new global resource/configuration limit is desired, it needs its own explicit admission/migration contract; do not smuggle it into effect-group restore. For an active ordinary operation that could exceed a newly required current-state constraint, either keep the constraint scoped to the capsule as frozen or enforce an explicit agreed refusal before accepting that operation. In every case, an accepted state must round-trip. Keep duplicate/dangling-reference protections required by the new contract; do not solve these compatibility cases by accepting invalid group journals.

One additional source-alignment case belongs to that same boundary: PublicReveal journal validation now rejects a hidden/empty face, while current validate_action(Reveal) can admit that face in a revealable zone. A group can therefore accept an entry its saved journal rejects. Align the new group's admission/record requirement, with a focused case, rather than changing unrelated legacy Reveal behavior silently.

## Remaining acceptance after these source fixes

The new coalescing, surviving/None/transient later-debt tests are meaningful source regressions. Most use one mutable state and check decoder success; they do not replace genuinely fresh engine/plugin continuation. The unsolicited-public test commits while preserving ordinary owed state, so the ordered-validator source reading remains relevant; integration should include the rollback branch too.

Still required in the planned cross-layer gate: complete token/face/order/counter/annotation restoration and failed-clone allocator behavior; baseline owed-hidden preparation and the Move(None)/Despawn/token-shed refusal/success matrix; actual Owner/None/All views with baseline/later Peek grants; native/SDK/hardened per-step and cold-state equality; ABI0 mediated/direct capability checks; independent old raw migration and malformed restore through exports. Those may land as the dedicated synthetic integration corpus. Existing empty-group goldens remain format coverage, not authorization coverage.

Sparse roster `[0,7]` and count256 projection are separately documented in `/tmp/agni-effect-group-roster-review.md`; engine checks must continue to use actual membership, while the SDK repair handles its immutable projection. No fourth-Costs, A1 or D1 completion is implied. Worker gate counts were not independently rerun here.

The exact Peek policy correction and baseline/later/control assertions are frozen in `/tmp/agni-effect-group-peek-authorization-addendum.md`. It specifies canonical redundancy removal only on cards with a reconciled PublicReveal and preserves unrelated baseline pairs, so implementations cannot diverge by optionally keeping redundant metadata.

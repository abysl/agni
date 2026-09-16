# Neutral engine repair review

Reviewed committed core/sim/engine delta `40a69aa7..2aaca6d2` in `/tmp/agni-effect-groups` against the original engine review and frozen protocol/addendum. Dirty SDK files are excluded. No builds or production edits.

**Disposition: changes required.** Admission, allocator preflight and golden coverage improved, but reachable disclosure/snapshot failures and required decoder validation remain. Reported suite counts do not establish the missing protocol vectors.

## P1: duplicate requests are incorrectly counted as separate debts

`sim/src/log.rs::apply_group_effect` appends RequestReveal whenever the card is unrevealed, including when it is already owed. Ordinary `apply_effect` correctly coalesces the obligation in a BTreeSet. `validate_group_information` instead compares whole-journal RequestReveal and PublicReveal counts.

Concrete admitted sequence: Begin; Reveal(c); Reveal(c) while c is unrevealed/revealable; then one normal LogAction::Reveal(c) with first-effect Continue. The one public entry fulfills the owed set, but the journal has two requests and one fulfillment. Continue does not validate this, so the accepted active snapshot immediately fails decode; Commit or an ordinary grouped effect also rejects. Repeated Reveal requests are idempotent, and one fulfillment must clear their coalesced debt. Use ordered obligation state, not counts. Earlier unsolicited/redundant public entries must not act as credit against later requests either.

Decisive test: round-trip after every accepted boundary in that sequence, then mutate/Commit successfully after the single public Reveal. Add an unsolicited Reveal before a later request to distinguish chronological fulfillment from counts.

## P1: rollback still loses a later debt on a surviving card

`rollback_effect_group` now tracks transient requests with an ordered pending set, fixing the previously reported absent-card case. However, RequestReveal still skips a card once `candidate.revealed` is true after an earlier PublicReveal.

Concrete sequence: baseline c unrevealed in Owner zone; Begin, move c to All, request and fulfill its face; move c through None to All, request its face again, then Rollback without the second fulfillment. Reconciliation restores the Owner card, processes the first PublicReveal and sets candidate.revealed, then silently skips the later RequestReveal. It drops the current owed obligation and accepts rollback. The new transient-only test does not cover this surviving-card case.

Track latest outstanding obligations independently of the restored candidate's revealed flag. They must survive in a fulfillable restored state, or rollback must wait; no earlier public record can erase the later obligation. Test both the surviving Owner case and rollback-to-None, then fulfill the latest request and retry successfully. The existing new transient test should also test successful retry after fulfillment, not only rejection.

## P1: accepted Peek/despawn histories cannot reload

`validate_state` requires every historical RequestPeek target to exist in the current table. The frozen contract explicitly permits allocated historical IDs absent from before/current.

Concrete sequence: Begin; Spawn a transient card into a None Aux zone; Peek(c, seated viewer); Despawn(c). Peek has no public owed obligation, Spawn creates a token, and the operations are accepted. The active snapshot then fails decode because c is no longer live. Existing unrevealed token Peek→Despawn is another instance. Validate historical IDs against retained allocation high-water and audiences against the roster; require live references only for current/capsule disclosure sets. Test save/reload and subsequent Rollback, including restoration of a baseline token and dropping a transient token's Peek record.

## P1: decoder is still neither bounded before allocation nor sufficiently validated

`decode_state` first constructs an entire recursive `ciborium::Value` through `cbor_value`, then calls `prescan_value`. All new journal vectors, nested maps and strings have already been allocated before limits/depth/duplicate checks. This does not implement the required advertised-length/depth guard. Reject before collection construction using a bounded reader/visitor or a genuine byte prescan; preserve duplicate detection before serde normalizes sets/maps. Quadratic duplicate scans over the current million-element allowance also do not supply a practical deterministic bound for hostile snapshots.

`validate_table`, `validate_mechanical` and `validate_state` additionally omit:

- Card owner/player seat validity; duplicate zone/counter declarations and their valid definitions.
- Counter declaration/scope/value checks, card/seat target validity, and duplicate `(target,counter)` rows (different values must still count as duplicates).
- Capsule allocator not exceeding current retained allocation high-water. Otherwise rollback can restore references from IDs the current allocator may already allocate again before rollback.
- Aggregate annotation-entry limits: both bounded_group_value and validate_mechanical count outer card maps, not the sum of annotation rows used at Begin.
- Peek bound based on actual seated count; the decoder hardcodes 64. Capsule shown/revealed/owed placement consistency and impossible outstanding-hidden baseline also need the agreed semantic checks.

Use focused malformed fixtures with one defect each, plus a valid control. Include huge advertised group lengths, deep nesting, duplicate set elements referencing a real card, duplicate counter keys with different values, invalid owner/seat/counter/zone, before allocator beyond current and one card with 16,385 annotations. Verify rejection through the actual NativeEngine/wasm restore boundary. Existing duplicate-token fixture uses a nonexistent card, so it rejects even if duplicate normalization remains unnoticed. The raw compatibility fixture is self-written by the current encoder; retain an independently authored old raw input as requested.

`effect_group: None` is omitted by ordinary serialization, so the explicit-null rejection in bounded_group_value_or_state is not itself an ordinary round-trip regression. Raw snapshots with either new key must remain rejected.

## P2: membership/error contract remains only partly aligned

`fold_finish` checks the first marker's variant before apply_action, but validates its ID only when applying the effect after the incoming action and PublicReveal append. A wrong ID on a Reveal when the information journal is full returns LimitExceeded before WrongGroup. Validate the captured active ID with the first marker before that action/append.

Only AlreadyOpen/InvalidLifecycle map through indexed_group_error to BadEffect. Other invalid indexed lifecycle operations still escape as unindexed WrongGroup/NotOpen (for example valid Continue followed by Commit with the wrong ID). The frozen contract distinguishes pre-entry membership errors from invalid later indexed lifecycle effects; apply that distinction consistently or explicitly revise both engine and SDK contract before integration. Add exact error assertions for a first wrong ID and a later wrong-ID terminal.

## Closed findings and remaining acceptance

- Active Admission checks now occur in validate, preventing unsupported structural actions from generating a normal decide request or consuming sequence. Sequenced rejection retains its separate existing behavior. The new test's native_decide_request assertion uses a fresh empty state rather than the active state, so that assertion should be corrected even though the source path is sound.
- ABI-0 mediation now obtains one request and passes the same bytes to the plugin. Its call-count test distinguishes the former duplication.
- Active Spawn refuses an exhausted physical allocator before mutation. Begin checks capsule sizes before mechanical capture. Existing accepted rollback retains allocator high-water through Table::restore_cards_from.
- The new four-effect, active request and snapshot-v1 golden files pin the requested basic shapes under the version bump. They are useful empty-group format vectors, not disclosure or restoration tests.
- Missing substantive acceptance from the first review remains: full token/face/order/counter/annotation restoration after reload; failed final effect consuming no tentative IDs; prefix-before-Begin and post-terminal effects; baseline owed-hidden/owed-revealable guard matrix; request→Move(None)/Despawn/token-shed refusal followed by successful public-fulfillment retry; actual Owner/None/All audience views after rollback; original Peek audience preservation. The new suite adds one transient disclosure rejection, not this matrix.

SDK repairs and cross-layer projection parity remain separate. These engine-half failures must close before that parity can establish the shared protocol. Fourth Costs still requires its own payment cursor, rules undo and reveal/replacement continuation integration; this engine review does not claim it complete.

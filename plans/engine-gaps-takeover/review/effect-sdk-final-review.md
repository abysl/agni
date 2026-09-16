# SDK 4d710e35 — final bounded repair checklist

Compared committed SDK/game production with the seven-area 0be7 review and approved engine4d292547; fa2c8570 fixture changes are separate. No builds or edits. **Source approval withheld.** Implement discriminating failing tests first, then close the ordered items below in coherent commits. This is the existing repair scope, not a new protocol audit.

## 1. Ordinary Move disclosure parity

`effect_group.rs:apply_effect_inner` always clears shown after Snapshot.apply, although an Effect::Move can return success without relocating; engine returns before touching disclosure on that no-op. It also forgets token disclosure solely from destination kind, without checking whether the token actually disappeared. `apply_action_disclosures` identifies shedding only via declared `to` zones, missing built-in Hand (`to=None`, kind Hand).

Failing vectors: (a) publicly shown card in Owner Aux, same-slot Effect::Move: shown and both viewers remain unchanged; compare a real relocation, which clears shown. (b) token in a shedding destination created by Spawn, same-slot effect Move: no-op preserves card/disclosure. (c) token with Peek moved by incoming Action::Move to built-in Hand, hidden=false: engine removes token and all disclosure; SDK currently leaves dangling Peek. Use pre/post move outcome or shared mechanical facts, not unconditional cleanup. Extend `action_disclosures_follow_clear_hidden_and_token_shed`; its current custom-zone move is not the Hand/no-op case.

## 2. Ordered pending debt must survive validation and block rollback

`rollback` now orders Request/Public records, but its final pending loop silently continues if the restored card is absent. Engine rejects OutstandingDisclosure. `from_decide/validate_information` still does not reconcile ordered pending requests against current live cards/owed/revealable placement.

Failing legal sequence: Begin→Spawn in None→Move to All→RequestReveal→saved request→Rollback before logged Reveal. SDK must reject rollback atomically, preserve the live transient/current owed debt and allocator; after its actual logged PublicReveal, rollback may discard the transient. Separately feed a complete malformed active request with latest RequestReveal but no current owed card, and require parse/from_decide rejection; current production accepts it.

**Existing wrong fixture:** `rollback_reconciles_ordered_transient_and_latest_reveal_requests` builds its `latest` journal ending with RequestReveal but leaves `latest.disclosures.owed_reveals` empty. The engine rejects that current state. Make the current request valid with live revealable owed card and correct hidden/revealed state, and retain a separate malformed-input rejection control. Its fulfilled transient half is useful but cannot detect the outstanding-transient bug.

## 3. Finish canonical Peek pruning after final debt handling

The repaired loop removes All pairs and the Owner's own pair, but ignores final shown. For a PublicReveal at the same restored Owner zone/seat, shown authorizes everyone; the frozen addendum requires removing every redundant pair for that publicly reconciled card. Current SDK retains other-seat pairs, unlike engine; those stale grants may later survive a hidden move.

Failing vector: baseline Owner card with Peek(card,1), Begin→same-Owner logged PublicReveal→Rollback: shown=true and zero Peek pairs. Paired control adds a hidden round trip, actual Peek(card,1), and a fresh RequestReveal before rollback: final shown=false, owed=true, required Peek(card,1) survives. Include the already specified All, Owner/no-Peek, baseline/later Peek and None single-viewer controls using actual engine audiences. Do not delete needed Peek merely because revealed is set.

## 4. Complete active preallocation guards, retaining nongroup compatibility

`decide::preflight_state` now bounds some current fields before allocation, but Peek remains hardcoded4096*256; tokens/revealed and outer annotation map rows are unbounded. Capsule Peek parsing still uses MAX_PEEKS before actual-roster validation. The prepass assigns latest cards/counters lengths, so duplicate fields can hide an earlier oversized value until the allocating parser rejects the duplicate.

First pass must obtain actual immutable roster and active group regardless of top-level key order, then cap both current and capsule collections before allocation: 4096 cards/tokens/revealed/owed/shown, 16384 counters/aggregate annotations, at most4096 annotation outer rows, and4096*actual seat count Peeks. Check each field as encountered or reject duplicate relevant fields before allocation; do not let a later small duplicate mask an earlier huge array.

Tests: complete correctly shaped active requests at/exceeding each limit independently in current/capsule, both state/group orders, 1-seat versus256-seat Peek capacity, aggregate across small annotation rows, and oversized-first/small-duplicate cards/counters. Direct borrowed preflight assertions isolate count rejection from later malformed-reference/duplicate checks. Exact-cap controls must parse; otherwise-identical valid nongroup over-feature-cap requests must remain accepted. No such preflight test was added in this repair.

## 5. Remove active-game Join bypass

`validate_roster_request` returns Ok immediately for Join whenever GameBlob is playing, bypassing requested-ID and overflow checks. All three public entry paths now call the helper, but the helper still accepts dense[0,1]→Join7 and255→256 in a running game. Apply the same proposed-seat==current dense count and checked successor rules before any playing-state special case.

Failing tests: call top-level decide and direct engine::decide with a started blob, Join7 on[0,1], and Join255 on255 seated IDs; both reject without changing plugin/table state. Keep valid next-dense Join and dense2/3/4 controls, and unsupported sparse/256 ordinary-request refusals. The new helper guards start/decide before projection, Ctx restores prior max(blob,dense) behavior, and present::contested no longer fabricates a battlefield for an unsupported roster; credit those repairs. No new game-boundary tests accompany this commit.

## 6. Align counter validation with the approved engine

The new `validate_snapshot` rejects CounterBounds.start outside min/max even when no current value exists. The engine permits such a declaration and clamps when materializing a counter value; engine validate_counters checks min<=max and actual stored values, not start. This extra SDK restriction newly rejects otherwise valid authoritative nongroup/group requests.

Failing control: Genesis with Table counter start0,min1,max5, no stored row, then a Game request; engine snapshot restores and SDK must parse/from_decide it. Materialize by Counter delta0 to clamp to1, then Begin/modify/Rollback and compare absence/value semantics. Mirror approved declaration/value validation; do not alter engine semantics to make an unnecessary SDK restriction true. Existing strict duplicate declaration and stored-value bound checks are improvements.

## Credited closures and required evidence

- Strict ABI1 keys/protocol, authoritative disclosures/despawn_any, allocator high-water and before<=current checks remain; effective PerSeat/Shared destination and owner validation now match the common engine helper.
- Terminal→Begin is now blocked and actually asserted by the expanded post-terminal test. Per-effect active group limits are now enforced. Ordinary owed-hidden Game is permitted; Begin refuses without consuming a group ID. Clear/hidden/custom-zone token-shed cases were added.
- Unknown annotation references, duplicate annotation rows, malformed current counter fields and invalid tagged zones now reject instead of normalizing. Some semantic checks improved, but ordered journal/current consistency in item2 remains absent.
- Add a discriminating Begin-at4096→Spawn4097→Commit test and both incremental/project_verdict atomic-error assertions; the corrected source is not covered by a new cap test. Add independent malformed allocator/disclosure/roster tests rather than relying on the full library pass count.
- All reported57 SDK /4494 game/plugin/clippy results remain worker evidence, not reproduced here. Coherent repair commits should report the named pre-fix failure and post-fix pass for each item. The upcoming authoritative native/SDK differential and hardened/session matrix is still required after source repair, and must not treat two implementations agreeing on an invalid hand-built fixture as proof.

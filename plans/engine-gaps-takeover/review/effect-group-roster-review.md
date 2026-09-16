# Exact roster prerequisite for neutral projection

Read-only source/design check in `/tmp/agni-effect-groups`, committed engine `2aaca6d2` plus the current SDK repair source. No builds or production edits. This is a bounded prerequisite to the existing effect-group protocol/equality gate, not a new gameplay foundation or rules expansion.

## Confirmed source facts

- `sim/src/log.rs::validate_genesis` explicitly rejects `entry.seat != 0` with NotTheHost, in both the committed candidate and current source. Therefore `{2,7}` is not a normal Genesis/Join history. Do not use it as the admitted regression.
- `validate_action(Join)` accepts any unused u8 entry.seat; `LogState::seated` checks actual roster entries. Genesis0 then Join7 gives the fully legal roster `{0,7}`. Joining every remaining ID 1 through 255 gives 256 distinct seats.
- `plugins/sdk/src/table.rs::parse_state_with_disclosures` currently skips each roster entry and converts its array length into `Snapshot.players: u8`. It loses membership and rejects 256 entries. Snapshot's Counter and Peek checks use `seat < players`.
- The new effect_group validators repeat that assumption for current/capsule card owner/seat, counter targets, Peek audiences and PublicReveal seat_at_reveal; their peek limit uses the same count. They cannot certify engine/SDK equality on the neutral engine's existing roster domain.

Concrete mismatch with a card owned by seat0: on roster `{0,7}`, Begin→Peek(card,7) and a declared Seat counter for seat7 are valid engine operations but SDK refuses; Begin→Peek(card,1) is accepted by the count-based SDK but engine returns UnknownSeat. A valid card owned/held by seat7 also makes the repaired constructor reject before any operation. After 256 admitted seats even a no-op Game request fails the SDK parser's count conversion.

## Chosen bounded representation

Add `Snapshot.seats: Vec<u8>` as immutable configuration, preserving engine roster order. It contains exact IDs, not names, counts or a dense renumbering. Bound length to 256 before allocation; validate each required seat ID as u8, reject duplicate IDs even if names differ, and validate the existing Seat map's required fields/types/duplicate keys before discarding name text. Preserve legal nonmonotonic join order such as `[0,7,2]`; do not demand sorted input.

Provide:

- `Snapshot::seated(&self, seat: u8) -> bool`: membership in seats, never a comparison to count.
- `Snapshot::seat_count(&self) -> u16`: exact 0..256 count.
- `Snapshot::dense_player_count(&self) -> Option<u8>`: checked count conversion and membership equal to all IDs in `0..count`; returns None for sparse rosters and count256. Empty pre-Genesis roster returns Some(0).

Widen SDK `Snapshot.players`, `decide::Request.players` and `view::Request.players` to u16, keeping their meaning as the exact count and their existing names. Parsed values are derived from seats.len; a manually built Snapshot must maintain the same invariant. This is a bounded Rust source-compatibility migration, not a wire or game-blob schema change. Do not use saturation, max-seat-plus-one, modulo, a zero/255 sentinel for 256, or silently populate a dense roster when parsed IDs are missing. The engine already transmits the full roster in state.seats, so no new CBOR field or ABI version beyond the pending ABI1 work is needed.

Keeping a u8 field as the authoritative count cannot represent the existing 256-seat domain. An explicit wider count plus a checked legacy game adapter is preferable to an ambiguous compatibility sentinel. Existing ordinary dense games retain the same numeric count and internal u8 turn types; their turn logic does not need a neutral 256-seat rewrite.

## Exact caller changes and immutable boundary

- SDK table parser reads the seats array; Snapshot.count/Peek use seated. Update current/capsule reference checks in effect_group::validate_snapshot, validate_disclosure_shape and validate_information to use seated for card owner/seat, Seat counters, RequestPeek and PublicReveal placement audiences. Compute maximum peek pairs from checked `seats.len() * MAX_CARDS`.
- `EffectProjection::from_decide` copies the current roster and exact count into the before Snapshot alongside zones/counter declarations/options. Capsule CBOR still has no roster field: it inherits current immutable configuration. Rollback cannot replace the roster from a forged capsule. Test Begin/rollback preserves the current roster and configuration exactly.
- Current neutral entry/effect callers must use actual roster membership wherever the engine does. Do not convert entry.seat or a card's controller to an ordinal. Shared-zone physical seat0 handling remains the engine's existing separate placement rule. This patch does not imply every older ordinary Move/Spawn helper already validates its player references; coordinate the new group's invariant with the engine repair rather than silently accepting a state the repaired restore rejects.
- Generic SDK `turns::TurnOrder` and `dice::Roll` remain u8/dense-seat utilities; they are game-policy consumers, not neutral membership validators. Do not renumber or expand them in this patch. SDK apply_entry's preexisting partial Join/Deal projection is not the setup oracle: build rosters through the actual engine, then parse the next authoritative request. Active groups already prohibit Join.
- Add an explicit test fixture constructor/helper for a dense roster; update literal Snapshots and tests that directly assign `.players = 3/4` to update seats as well. No production fallback that invents `[0..players)` when seats is empty. Existing valid CBOR fixtures already encode seat maps; parse them, do not alter their wire bytes merely because the projection gains a field.

## Existing game count compatibility

Keep Riftbound GameBlob, TurnCore, TurnOrder and Roll player counts u8. Add one checked dense-count adapter in the Riftbound plugin boundary and reuse it at conversions; source examples needing updates are lib.rs GameBlob::apply/roll/Join handling, present.rs lobby/points/with_seats/Roll::new, engine/ctx.rs::players, rules.rs::winner, and the plugin TableOptions test. Most test integer literals remain unchanged in value; direct Request/Snapshot builders gain exact roster fixtures.

For parsed decide/view requests outside the game's currently representable dense domain, return an explicit plugin refusal or a passive unsupported-roster presentation before count-based turn logic. Do not invent a neutral core player cap or reject such requests in SDK. For Join, validate the proposed actor against the game's dense next-ID convention before using count+1; Genesis/empty pre-state remains accepted. These are representation guards preserving the game's supported inputs, not implementation of sparse or 256-player Riftbound. Audit direct exported game helper entry points before using an infallible cast in Ctx; use the checked adapter rather than `as u8` or `.min(255)`.

The neutral synthetic fixture uses seated/seat_count directly and supports the full engine roster. MTG's current accept-all stub need not gain gameplay behavior, but its ABI1 parser/view/test builders must compile with the widened count. If widening exposes another consumer, adapt its checked boundary explicitly rather than reintroducing a dense membership test in SDK.

## Decisive acceptance vectors

1. Actual admitted native engine log: Genesis seat0, Join7; declare a Seat counter, Deal a card to seat7's Owner zone and a second card to seat0. Parse the engine-generated DecideRequest/from_decide. Assert seats exactly `[0,7]`, players=2, seated(7), !seated(1). Begin; Counter(Seat7); Peek(seat0-card,7); save/reload; Rollback. Native engine, SDK projection and hardened engine must agree, including the seat7 card and Peek audience.
2. Same valid baseline: Counter(Seat1) and Peek(card,1) refuse in SDK and engine without partial mechanics. Both are below the numeric count2, so they distinguish membership from range checks. Do not use an invalid card/reference that would reject first for another reason.
3. Genesis0, Join7, Join2: preserve roster order `[0,7,2]`; accept member2/7 and reject absent1/3. Add a dense `{0,1}` control showing legacy dense_player_count returns Some(2) with unchanged normal game behavior.
4. Build 256 seats by actual engine Genesis0+Join1..255 without a game-policy plugin. The next Game request parses with players=256 and roster length256; member255 is accepted, peek bound uses 4096×256, and no count wraps. Run one Begin/Counter(Seat255)/save/reload/Rollback through native/SDK/hardened paths. dense_player_count is None. This is a short neutral setup loop, not a 256-player card-game scenario.
5. Independently authored malformed roster arrays: advertised length257, duplicate ID with different names, duplicate seat/name keys, missing seat or name, out-of-range seat256, wrong field types. Parser rejects each before a group constructor; include valid controls. Current/capsule card or counter references to absent1 and information audiences absent1 reject under roster `{0,7}`. An unsorted unique roster is a positive control, not malformed input.
6. Game boundary tests preserve existing dense 2/3/4-player behavior and reject unsupported sparse/full256 game-count conversion explicitly. Engine/SDK itself still admits those rosters. Re-run ABI1 consumer compilation and existing library/plugin tests; no saved game-blob version changes or regenerated wire goldens are implied.

Add vectors 1/2 and one count256 checkpoint to the planned neutral host differential gate. Detailed malformed roster vectors belong in SDK/sim unit tests; only a representative malformed restore/request needs the wasm boundary. No new audit number or broader game support is claimed.

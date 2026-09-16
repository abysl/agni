# Play recovery checklist

**Baseline update:** Read `play-final-baseline-review.md` first. Its final-Economy review supersedes the stale schema, cancellation, private-candidate, and slot-conflict instructions below. In particular, Limited needs an explicit schema migration, saved resolving stages must remain resumable, child-play choices must wait until the parent finishes all repeated executions, and pricing must use the complete item context.

This is a read-only comparison of the Play lane at `origin/gaps/play`
(`1bad9b25`) with the reviewed integration at `1b517611` and the saved
Economy lane at `1c790ef1`. No source changes or builds were made for this
note. The Play commit is a porting specification: its `Origin::Revealed`
repair is not present in the integration tree, and the Economy tip is based
on an older, structurally different engine line.

## What the five Play commits must restore

The recovery unit is all five saved commits, not only the final origin patch:

* `a5ab6c3c` adds `Static::PlayLocations` grants for open battlefields. The
  grant is the union of the card's own grant and active board sources, while
  the base and held-battlefield destinations remain available. It also keeps
  Rockfall Path and other restrictions out of the returned list, marks a play
  there as a contest, and preserves the unit's normal timing.
* `0299b894` extends that static location surface to enemy-held battlefields
  where the enemy is alone, including the sorcery-timing and friendly-unit
  grant cases. The source's controller and whether the source is face-active
  are part of the permission; a generic “all battlefields” union is wrong.
* `2ca60f1f` adds replacement restrictions such as `OnlyPlayLocations` and
  the opponent base lock. Location grants, restrictions, enemy-held
  destinations, and the base-only veto must have an explicit precedence in
  `Resolved`, `Ctx`, `legal`, and the PlayLocation prompt.
* `6525ffeb` makes `LimitedPlay` an engine path for plays begun inside a
  resolution. It supplies `Origin::Hand`/the chosen `Price`, uses the engine's
  location prompt, and changes the hand candidate list and `Ability.viable`
  view behavior. It also touches the same Play, prompt, state, and card
  registry surfaces as Economy.
* `1bad9b25` adds the deck-reveal origin and its cancellation behavior below.

The first three commits conflict directly with the statics movement work:
their `engine/ctx.rs`, `engine/legal.rs`, and `engine/statics.rs` changes must
be reconciled with per-card movement-source checks and locks, rather than
wrapped in a helper that drops source/controller information. The resulting
legal destination list is also a public client contract. `present::legal`
feeds the SDK `Legal.zones`; Kai's `Rims.destinations` and `drop_plan` light
and accept only those zones. A location that the engine accepts but the view
does not list, or a visible destination inferred by Kai rather than returned
by the fold, is an integration defect. Add checks for open, enemy-held,
restricted, and base-locked destinations in both the plugin view and the
authoritative action path.

The Play commit adds `Origin::Revealed { from: RevealedFrom::Deck }` at
reserved origin tag 5. It uses that origin for Dazzling Aurora, Rek'Sai -
Swarm Queen, Void Rush, and Promising Future's queued play. It keeps Wild
Claw on `Origin::Banishment`, because Wild Claw first banishes the selected
card, and leaves Rek'Sai - Void Burrower's identical reveal as a separate
open gap. The engine consequences are coupled:

* `origin_cost` treats a revealed play like a banished play for the base
  cost, while the card script supplies its reduced or Power-only cost.
* `onto_the_chain` must recognize a deck-revealed card that is still in the
  deck or chain, and `cancellable` must allow the pending play to be taken
  back.
* Cancelling a revealed deck play must move it to the top of its owner's main
  deck. The origin currently records only `Deck`, so the implementation must
  preserve the invariant that the controller passed to `play::begin` owns the
  revealed deck card; otherwise the cancel path can return a card to the
  wrong seat's deck. This is an additional recovery acceptance condition,
  not a reproduced cross-seat card bug.
* `Event::Played` and any origin-sensitive trigger must see `Revealed`, while
  Wild Claw and other true banishment plays must continue to see
  `Banishment`. Search every `Origin` match after the port so a wildcard or
  old helper does not silently collapse the distinction.

The four Play replacements are behavioral coverage, not proof that every
revealed-play path is fixed. In particular, the existing generic
`rek_sai_void_burrower::play_revealed` path remains a named follow-up. The
port must not mark its old ignored test enabled merely because the new origin
exists.

## Schema and Economy reconciliation

The current integration serializes the statics/A9 v10 schema. Its
`ChainItem` is a fixed 13-field row and already persists `origin`, `stage`,
dynamic `picks`, and `awaiting`; `Origin` tag 5 is reserved by the Play
commit, so adding the enum is wire-compatible for existing v10 rows. The
decoder must still reject unknown origin codes and retain explicit v7/v9/v10
readers. Old v7/v9/v10 fixtures are independently authored and must decode
through their selected reader, migrate into the final state, and re-encode
through one canonical v11 writer with expected v11 bytes. There is no old-v10
writer to preserve as a second canonical output, and a v10 input must not
manufacture Economy fields before migration.

Economy `1c790ef1` replaces the v10 seat row's numeric `PlayLock`, readiness
flags, chosen champion, and `next_discount` with a different 11-field row
containing `no_spells`, gear counters, promises, and a pool. It also changes
the card row and the Play cost pipeline. The economy recovery checklist
selects v11 for that schema. Therefore the Play port must land against the
final v11 reader/writer and preserve `Origin::Revealed` in the v11 chain item,
while retaining the explicit old readers. Do not resolve the conflict by
copying Economy's old `state.rs` or inferring a row from its length.

The Economy port also changes `Cost`/payment APIs (`promises`, owned
`CostedGrant`, `cost::of_grant`/`of_parts`, and `Paying` context). Port the
Play stages to those APIs instead of restoring `floating`, tuple grants, or
the removed interner. In particular, `Price::PowerOnly` for a revealed play
must not pay energy twice or consume a promise before the play is accepted.
The optional cost sequence must retain the Economy promised-repeat stage, but
slot allocation needs an explicit repair before merge. Current integration
uses `SLOT_MODE = 6` and stores one named mode per execution at
`SLOT_MODE + execution`; Economy uses slot 6 for `SLOT_PROMISED_REPEAT`.
Those meanings collide in the same persisted `ChainItem.picks` vector. Choose
distinct stable ranges for promised-repeat and per-execution modes, preserve
the old mode decoding policy, and add a round-trip test with both a named-mode
choice and a paid promised Repeat in the same chain item. The test must answer
both prompts after reconstruction and assert the mode and repeat survive; do
not describe “retain slot 6” as sufficient.

## Limited-play and private-hand boundary

The Play lane's Rift Herald replacements are deliberately view/fold tests:

* `the_seats_own_view_greys_the_hand_cards_it_sees_are_no_payable_unit_and_the_play_reports_the_hand`
* `the_knell_withholds_only_a_public_face_that_is_no_payable_unit_and_reports_a_hand_play`

The first proves a seat can use its own private hand faces to grey an
unpayable unit. The second proves a fold without that private face cannot
invent its kind or cost and must withhold the candidate. This boundary must
remain intact when Economy pricing and promised discounts are added:

* authoritative candidate enumeration must use replay-reconstructible state;
  a seat-specific view may additionally use that seat's private faces, but
  those unlogged faces cannot change the fold's decision or resulting blob;
* the public log and other seat's view must carry no hand face merely because
  a limited play prompt was opened;
* `Origin::Hand` must be used for an actual limited play from hand, while a
  deck reveal uses `Origin::Revealed`; do not use Banishment as a generic
  “play from somewhere” origin;
* `Price::PowerOnly`, Free-for-Power promises, and static discounts must be
  evaluated against a face the acting seat is allowed to know. A hidden
  opponent card must remain an unavailable/grey candidate rather than a
  speculative affordable play.

The current public/private tests around `Rell Magnetic` and `Here to Help`
show the intended pattern: the requesting seat's view can grey a candidate
using a private face, while a replica without that face sees a conservative
subset. This is presentation information, not an unlogged authority input.
`HostSession` builds a seat-specific view from `faces_seen_by`. Its hand-play
precheck may project the face revealed; when accepted, the host logs that
reveal before the move so replicas reconstruct the same decision state.
Refusal must not leak that face. A client-only private face must never alter
the authoritative blob without this logged information boundary. Preserve
that contract while adding any limited-play location prompt, and test both
the richer owner view and the conservative other-replica view.

## Pending prompts, reconstruction, and object identity

`Ctx::await_faces` stores the temporary list in `Ctx.awaiting`, reveals or
owes the faces, and `engine::chain::run` copies that list into the serialized
`ChainItem.awaiting` before returning an `Ask(Resume)` flow. A fresh
`Ctx::new` does not reconstruct `Ctx.awaiting`; `chain::face_arrived` instead
uses the persisted chain row. This is correct only if every new Play/Economy
path follows the same handoff. Acceptance should therefore:

1. stop at a hidden-face prompt, encode the blob, reconstruct a fresh context,
   deliver the owner reveal, and assert that the same item resumes exactly
   once;
2. stop at the limited-play location or promised-repeat prompt, reconstruct,
   answer it, and assert the persisted stage/slot and resulting payment;
3. cancel after reconstruction and assert the card returns to the correct
   home zone with no stale pending item or promise consumption;
4. replay the same sequence from the saved snapshot/blob boundary and compare
   effects, origin, prompt, chain item, and view visibility.

This is also an A1 object-identity boundary. The current engine uses a bare
physical card id in `ChainItem.awaiting`, `TargetRef::Card`, prompt picks,
delayed arguments, and chain sources. A revealed card that moves from deck to
chain and is later cancelled/replayed must not satisfy an old awaited face or
target merely because its id is reused. The Play port should retain the
`ChainItem.awaiting`/`ItemStatus::Resolving` guard and avoid introducing a new
card-id-only pending reference. Full incarnation invalidation remains the A1
follow-up; this checklist records the Play dependency rather than expanding
that primitive.

## Cost multiplicity and named test mapping

The Economy independent review records two existing A10/C3 debts that Play
must not hide:

* `Ctx::flow_of`/`granted_cost` still select the first cost rather than
  offering the Rule 829.1.c.3 controller choice among distinct Flow costs;
* multiple Repeat promises are coalesced by the saved Economy implementation,
  and promise indexes above 255 alias. Rule 820 requires each Repeat instance
  to be independently optional and payable.

Play's separate printed-repeat and promised-repeat prompts must therefore be
tested with one of each, and the checklist/report must continue to label
multiple Flow and multiple Repeat instances as A10/C3 debt until the bounded
multiplicity work lands. Do not describe the Play prompt port as fixing that
rules gap.

When the inventory audit is rerun, preserve the explicit replacement mapping:

* the saved Play source removes/replaces the old Rift Herald test; the
  immutable inventory entry is absent from that source and maps one-to-many
  to the two Play tests above. Do not report it as an active ignored test, and
  do not call the original covered until both replacements are present and
  enabled;
* the old Herald of Scales test remains the contradictory fixture; Economy's
  `the_engine_prices_each_seats_dragons_by_its_own_herald_and_pays_the_discount`
  is the corrected replacement, and must not be counted as Play coverage;
* Fallen Feline's old self-controller test remains the rules-question entry;
  its active named-lock replacement remains separate.

Enabled source tests are only inventory evidence. The final report must show
the full turn suite, focused Play/origin/prompt tests, v7/v9/v10/v11 blob
round trips, wasm, clippy, formatting, and the hardened replay gate.

## Concrete recovery order and limits

1. Port the three `PlayLocations`/restriction commits and `LimitedPlay` onto
   the reconciled statics movement and client destination contracts.
2. Allocate distinct persisted ranges for named modes and promised Repeat,
   then add the combined round-trip/prompt continuation test.
3. Port `Origin::Revealed` and its four card callers onto the reconciled
   Economy/A9 APIs, leaving Wild Claw and Void Burrower semantics explicit.
4. Add whole-blob and request-boundary tests for revealed cancellation,
   owner-seat deck return, private limited-play candidates, and fresh-context
   prompt continuation.
5. Port the promised-repeat stage without changing printed-repeat behavior,
   then run the mapped Rift Herald tests and inventory audit.
6. Run the required scoped/full gates and inspect the final diff for stale
   `Origin::Banishment`, `next_discount`, `floating`, tuple grant, and
   interner callers.

This review did not run tests or prove gameplay semantics. It identifies the
actual integration dependency that the current tree has no `Revealed` origin
at all, the concrete prompt reconstruction contract, and the controller/owner
invariant required by the proposed deck-return encoding. It does not claim
that the A1 identity or A10 multiplicity debts are resolved.

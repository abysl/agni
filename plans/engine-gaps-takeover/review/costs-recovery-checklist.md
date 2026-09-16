# Costs recovery checklist

## Current acceptance corrections

The baseline comparison below predates Play recovery. The current Play candidate
writes blob v12 with a 14-field chain row: field 14 is `Limited`. Costs must
preserve that field and explicitly add its own persisted choice/payment fields
under a later version, retaining the v12 reader alongside v7/v9/v10/v11.
Mode prompt tag 14 and Name tags 15–16 are occupied. Promised Repeat uses slot
6, and per-execution modes begin at slot 7. Do not reuse any of these tags or
slots from the isolated Costs branch.

The saved lane conflates choosing an additional cost with paying it. In
`d0639152`, `settle_additional` calls `additional::pay` and then advances to
location/target choices. Core 355.1.a chooses whether to pay an optional cost;
355.2/355.5 choose locations/targets; 356 calculates costs; 357 pays them; and
358 checks legality. Preserve that distinction during recovery. A discard,
kill, or exhaust must not execute merely because its cost option was selected.
In particular, 357.3 forbids sacrificing the chosen friendly buff target when
another legal sacrifice exists. Test the selected target, the selected cost
object, payment, and final legality across separate saved requests.

Costs helpers must quote and plan with the complete `ChainItem`, including its
actual origin, Limited price, optional choices, qualified resource sources,
promises, and targets. Saved `additional::play_item` synthesizes `Origin::Hand`,
and several saved helpers call generic `pay::affordable`; those shortcuts must
not replace the integrated item-aware pipeline. `Price::Ignored` suppresses
non-standard mandatory costs too (356.5.a); `Price::Free` only changes base
costs. New cost selection must preserve this difference.

This is a read-only comparison of integration `c0128e66` with the five saved
Costs commits `ec0719fe`, `c2979850`, `f21bddbb`, `c5939791`, and `d0639152`.
The saved tip is not one cherry-pick: it is four behavioral clusters plus the
design-row correction. Port behavior and its original tests onto the current
statics/A9 line, alongside Economy `1c790ef1` and Play `1bad9b25`.

## Cluster map

- `ec0719fe`: `engine/ctx.rs` changes counter eligibility from board-only to
  in-play, so a legend in its zone can be Empowered; card regressions cover
  Akali, Ambessa, Jayce, Mel, Profiteer, Yordle Kennen, and Zed.
- `c2979850`: `SelfCost::Disempower` is offered only while empowered and pays
  at activation in `engine/activate.rs`; it exhausts and disempowers before
  the chain resolves. `f21bddbb` only corrects the corresponding
  `wiki/design/rules-engine.md` row; it is not an implementation commit.
- `c5939791`: `Event::Empowered` gains `by`; `Event::Banished` gains `by` and
  `token`, with `banish_by`/`empower_by` and `YouEmpower`/`YouBanish` trigger
  matching across `engine/ctx.rs`, `engine/triggers.rs`, and card scripts.
- `d0639152`: `engine/additional.rs` and the Play/prompt/state changes make
  non-resource costs a resumable additional stage, including discard, kill,
  empower/disempower, optional runes, discounts, candidate picks, and the
  persisted `paid_with` data.

## Concrete reconciliation hazards

1. **Event union and actor semantics.** Integration `engine/ctx.rs` still has
   actorless Empowered/Banished constructors. Port every constructor and match
   in `engine/ctx.rs`, `engine/triggers.rs`, `engine/activate.rs`, `engine/chain.rs`,
   `cards/prelude.rs`, and affected cards. Preserve the distinction between
   owner, controller, and actor: `banish()` derives the controller, while a
   card effect may call `banish_by`. Capture `token` before despawn so token
   banishment cannot be treated as an ordinary owned-card move. Keep existing
   `Event` variants and tests; do not drop actor data in a compatibility helper.

2. **Legend counters and activation payment.** Merge `ec0719fe` with the
   current `statics::in_play`/face-active checks, rather than restoring a
   generic `on_board` test. A legend in its own zone may be Empowered. Check
   actual kind/zone eligibility; attachment alone does not move gear off the
   Board. The existing `face_in_play` accepts a legend kind without checking
   its zone, which must join A1's logical Board predicate audit. `SelfCost::Disempower`
   in `engine/activate.rs` must validate before mutating, then exhaust and
   disempower during activation finalization, before the item reaches the chain. Preserve
   `Reason::NotEmpowered`, the offer/pick text in `engine/prompts.rs`, and the
   Ambessa/Glowstone/Hextech Disc/Mel/Questionable Tome/Kennen/Zed regressions.
   A refusal or invalid continuation must not leave the source exhausted or
   disempowered.

3. **Additional-stage suspension and rollback.** `d0639152` can suspend at
   `PromptWhy::CostPick`, but pays non-resource costs prematurely as described
   above. Port the behavior with separate declaration and settlement.
   Preserve `ChainItem`'s additional choice,
   paid state, picks, and `paid_with` through `state.rs`, `engine/play.rs`,
   `engine/additional.rs`, `engine/legal.rs`, and `engine/prompts.rs`. Extend
   the A5/A6 checkpoint contract in `engine/chain.rs` to restore all transient
   choices/effects/death queues on refusal; test discard/kill/empower and
   optional-rune paths across snapshot/reconstruction. Do not make a paid
   additional cost cancellable merely to simplify rollback.

4. **Privacy and target ordering.** Additional discard/kill candidates must
   use only faces the acting seat may know. A private view may grey a choice;
   an unlogged private face cannot change the authoritative verdict or blob.
   Declare optional costs before location and target selection, then settle
   payment at the correct step. Ensure unpaid side effects cannot influence candidates, target legality, or
   `Static::PlayLocations`/`LimitedPlay` from Play `1bad9b25`. Test both owner
   and conservative replica views, including hidden hand cards.

5. **Costs, targets, and persisted slots.** `d0639152` adds `CostPick` tag 14
   and a 14-field chain row, while integration's `PromptWhy::Mode` already
   uses tag 14 and `ChainItem` is the statics/A9 13-field row. Economy
   `1c790ef1` also reserves slot 6 for promised Repeat while integration uses
   `SLOT_MODE = 6` for per-execution named modes. Allocate distinct stable tags
   and slot ranges, then test one chain with a named mode and paid promised
   Repeat through reconstruction. If adding the fourteenth chain field,
   advance beyond the integrated version and retain its explicit legacy
   readers, including Economy's v11; retain `Origin::Revealed` and A9 owned
   `CostedGrant` bytes. Never infer a layout from row length or copy Economy's
   older v10-looking seat/card rows.

## Original coverage to carry forward

Keep the saved tests enabled or explicitly mapped while porting: the legend
Empower/disempower cases in the `ec0719fe` card files; the activation/refusal
test in `c2979850`'s activation fixture; actor-sensitive legend, trigger, and
token cases in `c5939791`; and all additional-cost fixtures in `d0639152`
(`Legion Quartermaster`, `Meditation`, `Nami`, `Pyke`, `Rampage`, `Ruthless
Strike`, `Sacrifice`, `Sea Monkey`, `Stalking Wolf`, `Zaun Punk`, and `Zed From
the Shadows`). Existing ignores must remain visible until their replacement
asserts the same state transition, prompt continuation, privacy boundary, and
final effects; an enabled test is not proof of rule correctness.

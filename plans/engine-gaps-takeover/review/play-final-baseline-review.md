# Play port review against final Economy

Read-only source review: Economy `21c73e78` against the five saved Play commits
`a5ab6c3c` through `1bad9b25` (delta from `0dd32ba0`). No builds or tests run.
Use this with `review/play-recovery-checklist.md`; that checklist's Economy
baseline and several implementation steps are now stale. Paths below are
relative to `games/riftbound-turns/src/`.

## Already supplied by Economy — retain, do not reimplement

- `state.rs`: canonical v11, explicit statics v7/v9/v10 migration, A9 owned
  `CostedGrant`, promises/pool, numeric play locks and readiness fields.
  Ambiguous saved Economy-v10 is deliberately not an accepted legacy schema.
- Promised Repeat uses slot 6; named modes start at 7. Old modes shift only
  when reading pre-v11 rows. `OPTIONAL_COST_SLOTS` includes promised Repeat.
- Printed/owned/static first Repeat plus one promised Repeat can execute three
  times. Mode/target grouping handles each execution's actual specification
  count. Rocket Barrage's three-mode gameplay and third-relative-target tests
  reconstruct the actual saved items (`rocket_barrage.rs:334`,
  `engine/targets.rs:1751`). These satisfy the old slot-conflict acceptance.
- `engine/chain.rs:158,273`: execution views retain staged remembered targets
  after all declared target groups and clear memory/awaiting between repeats.
  `here_to_help.rs:389` exercises two hidden selections and two locations across
  saved requests. Port this regression to the new location flow; do not drop it.
- `engine/play.rs:298,1431,1478` and `engine/cost.rs:606`: optional costs quote
  the complete hypothetical item before discounts and pool, with item-qualified
  payment sources. Pool-only/excess-discount Additional and qualified Accelerate
  regressions already exist. A9 owns runtime power; restore no interner/floating.

## Remaining concrete port requirements

1. **Schema and old callback continuations (blocking acceptance).** Saved Play
   adds `ChainItem.limited` as field 14; final v11 writes/requires 13 fields
   (`state.rs:1456`). Origin tag 5 alone is additive, but Limited is not. Prefer
   explicit v12 with a v11 13-field reader defaulting `limited=None`; retain old
   migrations and shift modes only for versions below 11. Another encoding is
   possible only with a documented semantic contract; do not overload unrelated
   picks/targets/memory to avoid the field. Test chain and queue rows, not just an
   isolated origin. Saved Play also removes Here to Help stage `LOCATED=3` and
   Swarm Queen `LOCATE=4`. Old v11 resolving items can still contain those stages:
   preserve legacy handlers or migrate those continuations, otherwise the new
   callback's fallback restarts selection. A version bump alone does not fix it.

2. **Defer child finalization until the parent finishes (corrected acceptance).** Core 354.3 says a child card moves to the Chain as Pending, then the currently resolving effect continues before further play steps. Core 158.3/158.3.a forbids finalizing or resolving another item until that effect finishes; Repeat executes within the same resolution. The earlier requirement to finish child location/payment before the next parent Repeat was incorrect. Limited plays begun during resolution must enqueue without advancing; parent repeats and parent face/pick prompts continue normally. `proceed` must not advance pending children while a resolving parent is suspended. Only after the parent completes should child location, optional-cost, payment and finalization proceed. New Here to Help paths queue the allowed locations; LOCATED remains a legacy resume handler. Test two hidden unit selections across parent repeats and saved requests, both children still Pending with resources unspent, then both child declarations/arrivals after parent completion. Preserve failures, queue entries, private reveals and normal later priority. No CHILD_WAIT sentinel or parent-waits-for-child links are needed for this behavior.

3. **One item-aware price path (blocking acceptance).** Saved
   `cards/prelude::unit_priced_in_hand` and limited callers use raw `cost::priced`
   plus generic `pay::affordable`; that loses final Economy's discounts, pool,
   promises and `Paying::Item` sources. Build a hypothetical item carrying the
   actual controller/origin/Limited price and use the common final quote pipeline
   for viability, optional choices and payment. Apply base overrides before
   optional additions, LessEnergy to the combined cost, and Ignored to the final
   total; retain granted Accelerate. **Corrected:** Ignored preserves optional
   choices and makes their physical payment zero; the earlier forced-decline
   instruction was wrong. See `play-ignored-optional-correction.md`. Test a limited PowerOnly/LessEnergy play affordable
   solely through banked resources or a qualified source, with no pre-consumption.
   Audit Promising Future's manually prepaid `queue_play`: the saved source still
   pays Power before targets are accepted and queues a free-base origin. Its
   no-legal-target return test alone proves neither atomic payment nor discounts.

4. **Correct the cancellation/private-face wording.** Saved `play::cancellable`
   explicitly excludes Revealed and Limited plays: these are not user-cancellable;
   automatic failed declaration still uses `cancel` and returns Revealed to deck
   top. Test that distinction and owner/controller mapping. The Rift Herald saved
   tests keep unknown hand faces offered in stable order, grey using the owner's
   private view, and remove only publicly known ineligible cards. The checklist's
   suggestion that every unknown candidate is withheld is inaccurate. Preserve
   index alignment and log the selected reveal before face-dependent authority.

5. **Location port remains outstanding.** Port grants/restrictions with current
   statics source/controller/activity checks intact, then compare view zones,
   classify and pending location options for open/enemy-held/OnlyPlay/base-lock
   cases. Saved statics deduplicates scripts by pointer; prefer stable traversal
   and destination deduplication, not new pointer identity in admission. This
   source observation alone does not prove allocation-dependent outcomes here.

## Implementation sequence and disposition

1. Freeze the accepted Economy tests, select/document the next schema, and add
   old v11 staged-item fixtures alongside retained v7/v9/v10 migrations.
2. Port the three location commits, reconciling statics and destination contracts.
3. Add Limited/Price and item-aware quotes; establish deferred child finalization until the current parent resolution completes before migrating repeated card callers and their tests.
4. Port Revealed and the four named callers; retain true Banishment for Wild Claw.
   Preserve the two Rift Herald replacement tests and legacy-stage continuations.
5. Run focused new request-boundary/payment/location tests, full turns, schema
   fixtures, scoped lint/format, wasm and fresh hardened replay gates.

The above are Play integration acceptance items, not blockers in the approved
Economy production. A1 incarnation identity, A10 arbitrary Flow/Repeat instances,
A6 index/work limits, and the named Void Burrower gap remain separate follow-ups.

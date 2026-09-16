# Stable Play port review — WIP against e9235605

Read-only review of `/tmp/agni-takeover-play`; no builds/tests. Current child
scheduling loop and net fixture are intentionally excluded while Luna fixes them.
Paths below are relative to `games/riftbound-turns/src/`. Findings describe the
source inspected, not a claim that execution reproduced them.

## Blocking findings

1. **High — actual Move admission loses open-field grants.**
   `Ctx::new` applies the incoming Move before `legal::classify`→`from_hand`→
   `play_to`. `ctx::units_at:816` excludes Pending cards, but the incoming card
   does not yet have a Pending row. `prelude::is_open_battlefield` therefore sees
   that unit occupying the destination; Sneaky Deckhand's open-field grant becomes
   empty. On an initially unheld empty field: holds=false, Ambush=false, no grant,
   hence NotHeld. This is established by the source path, not merely speculative
   allocation/timing reasoning. Enemy-alone grant predicates have the analogous
   incoming-unit hazard. The saved Sneaky Deckhand test classifies *before*
   manually applying Move, then calls act directly, bypassing real decide order.
   Fix the admission projection for location checks while preserving the revealed
   face and existing statics/control/movement semantics. Require engine::decide
   Move tests plus view-zone equality for own and external open/enemy grants.

2. **High — Limited and Revealed lose Accelerate charging.**
   `engine/cost.rs:513` returns `priced(...)` immediately for Limited; priced's
   Accelerate test is `can_accelerate(card)`, not Economy's `can_accelerate_item`.
   Static::GrantsAccelerate can therefore offer a yes prompt while its cost is
   absent. Concrete registered pairing: Rek'Sai Breacher grants Accelerate to a
   non-Hand unit played through Swarm Queen/another Limited non-Hand origin.
   Separately, non-Limited Revealed is now explicitly excluded from the later
   Accelerate addition, unlike saved Play's Revealed|Banishment branch. Dazzling
   Aurora's direct Revealed begin can offer printed Accelerate without charging
   it. Separate price-adjusted base from common item-qualified optional addition;
   add native saved-prompt tests asserting both energy and domain power consumed.

3. **High — Swarm Queen v11 stage4 does not resume.**
   The new card has CONFIRM=1, REVEALED=2, PICK=3 and no LOCATE=4 handler or
   stage4 candidates. Its fallback restarts CONFIRM. The new compatibility fixture
   deliberately retains a v11 resolving item at stage4 with remembered choice,
   but only checks structural migration. Restore a legacy-only LOCATE handler
   and matching options, or migrate that exact continuation explicitly. Test a
   real decoded v11 table+blob, answer its pending location, and verify the saved
   selected card is queued once without new deck confirmation. Here to Help's
   retained LOCATED=3 handler needs the same gameplay acceptance, beyond row bytes.

4. **High — Swarm Queen selection still bypasses Economy prices.**
   `playable_revealed:78` delegates to Void Burrower's raw printed-price callback;
   `play_revealed_here:98` separately uses generic `pay::affordable` on raw priced
   cost. Eligible cards affordable through discounts/promises can be excluded;
   the generic second check also loses item-qualified sources. Banked pool is
   handled by pay's planner, so a pure pool-only example alone is insufficient
   to expose every shortcut. Use a complete hypothetical Limited item with the
   actual Revealed origin/controller/location scope and common quote pipeline.
   Do not blindly reuse `priced_item`, which hardcodes Origin::Hand and would
   wrongly qualify Hand-only FreeForPower promises. Test a genuine discount and
   qualified source, plus a Hand-only promise that must not cover Revealed.

5. **Medium — Price::Ignored only removes base/optional costs.**
   Bone Skewer now uses Limited Price::Ignored for “any and all costs”. The new
   price match returns zero base and begin_limited declines the four optional
   slots, but `base_of_item` still adds surcharges and `of_item` adds Deflect.
   There is no final Ignored override. A Vaults of Helia unit surcharge therefore
   still charges/can reject this instructed play. Core356.5.a requires the final
   total, including non-standard costs, be zero. This is a carried-over limitation
   exposed in the new Price contract, not evidence Economy newly broke ordinary
   payment. Add a Bone Skewer + unit-surcharge regression and distinguish Free
   (base only) from Ignored (all costs); preserve legality/location restrictions.

## Preserved source behavior and remaining acceptance

- Existing statics movement helpers/locks remain in place. New play restrictions
  filter location grants and Ambush via playable_at/OnlyPlay; base lock takes
  precedence over battlefield choices. Source pointers are still used to dedup
  scripts, but this review does not establish divergent outcomes from that alone.
- A9 owned CostedGrant and Economy slot6 promised Repeat/slot7+ modes remain;
  full quote discounts/pool and existing three-group execution code are retained.
  `prelude::unit_priced_in_hand` now uses of_item + Paying::Item, correctly fixing
  the *Hand* helper. Do not generalize its hardcoded origin to revealed/trash plays.
- The two Rift Herald visibility tests are present. Offered unknown hand cards
  stay in stable order; only owner view greys private ineligible faces. Present
  applies enabled flags for card affordances without changing Pick indices.
- Revealed cancellation explicitly uses ctx.owner(card), correcting owner-seat
  deck return. Revealed/Limited remain non-user-cancellable; test automatic failed
  declaration and correct owner destination after save/reload, not a Cancel button.
- v12 ChainItem field14 and explicit pre-v12 field13 reader preserve old shapes;
  old mode shift remains version<11. Independent fixture additions cover chain
  and pending rows, reject wrong-version row lengths, and retain A9/statics history.
  This proves structural compatibility only, not the missing stage4 continuation.
- Promising Future now queues Limited PowerOnly instead of prepaying Power. Keep
  that correction and test automatic failure without payment/promise consumption.

Disposition: repair the five bounded findings before full Play gates. Final
approval also requires the separately reviewed corrected parent-before-child
schedule, real action/view tests, legacy resumed gameplay and fresh parity/full
checks. A1, A10, A12 and remaining Void Burrower semantics stay separate follow-ups.

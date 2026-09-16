# Play cost/location recheck

Read-only current WIP in `/tmp/agni-takeover-play`, compared with `e9235605`.
No builds/tests run. Swarm/child lifecycle remains under the main worker's active
ownership and is not re-reviewed here. This narrows findings 1/2/5 from the earlier
stable-port review; it is not final approval of the complete Play recovery.

## Remaining cost blocker

**High: Limited LessEnergy applies too early.** `cost::origin_cost` now calls
`priced(ctx, card, limited.price, false)` and *then* adds item-qualified Accelerate.
For a unit with printed Energy2, Here to Help's LessEnergy3 and paid Accelerate1,
this produces Energy1 instead of0. Core356.4.f permits discounts to reduce additional
costs. The old priced(...,true) applied this reduction to the combined energy.
The same structural issue affects any other additional cost added after origin_cost.

Implement LessEnergy as a discount on the complete item cost before pool settlement,
not as the base-ignore behavior used by PowerOnly/Free. Preserve additional cost
choices even when their monetary amount becomes zero. Keep A12 floor/ordering work
separate: this example needs no ordering choice. Add a saved-prompt regression with
printed2/LessEnergy3/paid Accelerate1 and its Power cost, asserting zero Energy and
one Power; include a printed/granted Accelerate variant if reusing the same test.

## Fixes accepted by source inspection

- Limited now uses can_accelerate_item for actual optional charging, preserving
  Breacher's granted Accelerate; non-Limited Revealed reaches the normal Accelerate
  addition again. This resolves the earlier free-Accelerate paths, subject to the
  remaining LessEnergy placement above.
- `of_item` applies the Ignored override after surcharges/Deflect/discounts, clears
  energy/power/XP/burn and pooled consumption, while preserving matching promise
  metadata. Free retains surcharges. A9 owned costs and Economy slots/pool remain.
- The new cost tests quote a complete non-Hand Limited item with Breacher, pause at
  the actual optional prompt, save table+blob, reconstruct, answer yes and inspect
  resulting location/rune domains. The Revealed printed-Accelerate case is also
  present. The Limited quote asserts2Energy/2Power; spent-rune count alone cannot
  independently prove both Energy and Power were charged, since recycling a rune
  can pay both. The explicit quote assertion provides additional evidence.
- Ignored test contrasts Free with a Vaults surcharge, checks zero Ignored cost and
  matching promise metadata, then plays and asserts no ready-rune loss. It checks
  real final payment, though it is helper-level rather than Bone Skewer gameplay.

## Location projection disposition

`engine::decide` now sets projected_move_card only around classify and clears it
before act. `Ctx::units_at` excludes that card, so open/enemy-alone grants no longer
count the incoming unit as an existing occupant. This fixes the original concrete
Sneaky Deckhand admission path without altering authoritative table/effect storage.

The new `decide_projects_incoming_units_for_open_and_enemy_grants` test calls real
engine::decide on a Move request for Sneaky Deckhand and Dauntless Vanguard; it also
calls present and asserts the exact legal zones plus one rejected destination each.
This is materially stronger than saved tests that classify before manually moving
and calling act. The test only checks successful verdict presence, not applying it
and asserting arrival/contest/payment; those final outcomes remain worth asserting.

Remaining bounded acceptance: add one external grant source (Miss Fortune Buccaneer
or the equivalent enemy-held grant), plus OnlyPlay/base-lock view-versus-real-Move
cases. Current new test covers self grants only. Do not claim every source-sensitive
statics interaction is covered by it. Existing per-card movement restrictions remain
in source; the temporary exclusion currently runs for *all* Move classification,
including board moves, so retain the ordinary movement/view parity gate as well.
`units_at` repeats the identical projected-card filter twice; remove the duplicate
as a small cleanup, not a correctness blocker.

Disposition: earlier Accelerate/Ignored faults are repaired; LessEnergy placement
still blocks cost acceptance. Original incoming-unit grant rejection is fixed on
source inspection and now has real-request tests. Require their actual passing
results, the external/restricted location acceptance, and final fresh parity/full
gates. No new claim is made about active child-loop or legacy-stage fixes.

## Addendum: frozen permission must still obey current prohibition

**High, newly confirmed candidate regression:** current limited_locations returns
stored zones without the earlier ctx.limited_play_locations filtering, and
location_still_open returns true for every Limited item. Queue a Limited battlefield
play before an enemy Warden is active; a parent suffix then moves that Warden to a
battlefield. The saved list still includes the battlefield and finalization admits
it despite the new only-base prohibition. This follows the source path; no test was
run. Core358.3/a requires checking the current legal outcome, with the explicit Here
to Help/Warden example; permission captured earlier does not override a prohibition.

Keep captured allowed destinations even if their granting source later leaves.
Recheck the intersection with current prohibitions/OnlyPlay and battlefield capacity
when presenting/choosing locations and before payment/finalization. Do not regenerate
ordinary legal::locations_for (which would reimpose timing/current held/grant-source
permissions on an effect-initiated play). Test both directions: vanished grant still
permits its captured destination; newly active Warden prevents it with no payment or
stale pending entry. Normal play_to already lacks an explicit destination_capped
check, so the three-player issue is a broader existing limit gap, not demonstrated
as a regression uniquely caused by this frozen-list edit. Its shared legality check
should nevertheless be reused when defining the Limited finalization contract.

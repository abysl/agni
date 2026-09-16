# Economy documentation reconciliation

This bounded docs pass reconciles `wiki/design/rules-engine.md` after the
recovered economy lane (`1fd89235`) removed conflict markers but left both
alternatives in several tables. It is based on the integrated statics/A9
checkpoint (`1b517611`), the economy recovery notes, and the pre-repair
alternatives.

The M10 gap table now has one Hidden rules row and one discounts row. The
Hidden row retains both the `Filter::Facedown` Pack of Wonders behavior and
the `targets::in_universe` candidate-universe detail. The discounts row uses
the landed `Static::PlayDiscount(ItemDiscount)` path and `cost::of_item`
wording.

The Vendetta Groups table now has one row per group. Stargazer and Applied
Researchers use the integrated `Static::PlayDiscount` representation. The
Vendetta engine-gap table keeps the recovered economy behavior in one row per
mechanic: landed `Static::AbilityDiscount`/`Static::Surcharge` with the
remaining Risen Altar pay-stage choice, the current counter and prompt rows,
and the detailed target-filter row. The prompt row records the landed named
modes and Fallen Feline name prompt; Stargazer's trash Flow now explicitly
uses the same `cost::of_item` path as its activation pricing. The repeated Unleashed summary prose was
collapsed to retain the landed group filters and banked `SeatState.pool`
without repeating contradictory alternatives.

A9's owned costed-grant model and the current play-discount vocabulary remain
the documented interfaces; no old interner or `Static::SpellDiscount`
alternative was restored.

Validation was documentation-only: `git diff --check` passed; a Node scan of
the three affected tables found no duplicate first-column keys; and a marker
scan found no `<<<<<<<`, `=======`, or `>>>>>>>` lines. No Cargo build or
cache was used.

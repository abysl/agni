# Triggers recovery checklist

The saved tip is `7dfaef5f`; recover its five commits after Economy, Play,
Costs and Damage. This is a read-only review of the saved source, not a claim
that the recovered behaviors or remaining twenty clusters are complete.

## Saved steps

- `e980b53a`: `UnitDies(Who)` with friendly/enemy watchers, `Noted.buffed`,
  departed token faces, and seven original tests. The dying source's own
  trigger remains Death; friendly unit deaths exclude the source itself.
- `3fa5ba42`: attachment Ability/Copied/Mirror grants and wearer text, fifteen
  original tests. Its intermediate gear-as-source representation is corrected
  by the fourth commit; do not stop recovery at this intermediate behavior.
- `57f1f8ac`: aura/borrowed activations, Legend scope and holder-paid costs.
  Its dynamic offset lookup at resolution is likewise corrected below.
- `786200d1`: per-seat/per-battlefield excess-damage records, six original
  tests; Granted/Lent items with separate holder and lender addresses; pending
  abilities survive detach, source death and changing battlefield control.
- `7dfaef5f`: Who::Any for movement, attacks, chosen-friendly and ready events,
  including Smoke and Mirrors/Lillia and battlefield watchers.

## Reconciliation and required evidence

1. Preserve the integrated versioned seat/card readers and Economy mode-slot
   migration. The saved lane's version 7 means a different historical schema:
   Noted grows from four to five fields, Granted/Lent add four-element item
   variants under tags 4/5, and excess rows use `xd`. Dispatch old Noted rows
   by the containing schema and retain chain **and queued Pending** migration.
   Add independent old-input fixtures and current canonical round trips;
   reject malformed lengths and conflicting/unknown variants.
2. Reconcile holder/lender classification across payment, additional costs,
   prompts, discounts, narration, targeting and resolution. A Lent item is an
   activation, not a spell or triggered ability; a Granted item remains a
   trigger. Holder pays/exhausts and supplies “me”; lender supplies granted
   text and “this”. Preserve A9 owned runtime costs and all recovered costs.
3. Keep static scope/ready/lock logic from Statics. The saved Unit/Legend scope
   repair must not erase newer predicates. Aura Ability, Copied, Borrowed and
   attachment lists must agree on their deterministic address order.
4. Preserve Damage's attribution and immunity/multiplier/mark-aware lethal
   calculation when inserting `record_excess` before damage is dealt. Record
   assigned excess per assigner and battlefield, distinguish None from zero,
   replace the previous attack's record and clear at Expiration. Do not copy
   the older saved combat::lethal over Damage's implementation. A11 replacement
   ordering and its effect on assignment/excess remain explicit D2 debt.
5. Preserve captured death controller/buffed state and departed token faces.
   Killing a token in the same request must still distinguish it from a real
   unit; killing gear must not raise a unit-death watcher. A1 must later bind
   these snapshots to object incarnations, and A5 must restore departed/death
   queues when a resolving segment faults.
6. Merge Who::Any mechanically at every producer/consumer, preserving move
   cause, origin and subject. Retain the exact persisted Mask/Lillia regression
   and total 4+3=7. Deferred matching and the illegal cross-action ordering
   choice remain A3 debt; broadening Who does not fix event chronology.
7. Saved `triggers::interchangeable` uses `std::ptr::eq` to elide a prompt and
   does not compare source/lender identity. A4a must remove pointer-dependent
   prompt decisions and use stable ability-instance provenance; any permitted
   elision needs explicit semantic equivalence of the complete payload.
   Equal Rust addresses do not establish this. Add native/Wasm prompt parity
   for printed/copied abilities with different holders/lenders and independent
   once limits. Do not claim the current helper proves determinism.
8. Saved granted indices and order counts use u8 casts. Audit them under A6
   before accepting large dynamic grant lists; never alias abilities or accept
   a truncated mandatory ordering batch.

Run the full engine suite, independent blob fixtures, native/hardened replay
with a pending Granted and Lent item restored after its lender leaves, scoped
fmt/clippy/wasm checks, original inventory audit and Kai prompt/brain gates.
Keep historical test substitutions visible in the immutable inventory mapping.

# A9 costed grant recovery

The recovered engine represented granted `Equip`, `Repeat`, `Empower`, and
`Flow` costs with process-global `Power::intern` and `Cost::interned` caches.
That made a persistent wasm instance's work depend on prior view and decide
calls. The old reverse conversion was structurally lossy for arbitrary runtime
costs such as `AnyOf`, XP, burn, and floating costs; the only recovered caller
supplied printed costs, so no historical wrong Kennen price is established.

This repair keeps the existing static card vocabulary and the existing costed
grant row shape. `CardState` now owns `CostedGrant { kind, energy, power,
until }`; its encoded row remains `[keyword-code, energy, power-codes,
expiry]`, so existing bytes need no version bump. Runtime costs are built from
owned grant symbols through `engine::cost::of_grant`, while static printed
costs continue to use the static card API. The global intern tables and the
general `to_script` conversion are gone. Unsupported dynamic `Equip` and
`Empower` grants remain refused.

Kennen's Flow grant deliberately maps the printed card fields at its call
site: no domains become Rainbow, one domain becomes that domain, matching
domain and power counts remain explicit, and other multicolor costs become
the owned symbol. This keeps arbitrary runtime `AnyOf` values out of the
compact serialized grant representation while preserving printed-before-grant
Flow precedence and expiry.

The repair adds owned grant round-trip coverage, expiry and keyword-instance
coverage, Kennen's table-driven printed-cost mapping, and a hardened-plugin
history test. The latter compares native and cold wasm view/decision results,
then repeats the same valid `EndTurn` decision after unrelated costed views
and a reset view. The existing native/hardened replay corpus remains the
broader parity check. Legacy v7/v9/v10 whole-blob fixtures are supplied by the
separate inventory worker commit and are intentionally not duplicated here.

Remaining A9 debt: none identified for the current costed-grant wire shape.
The broader statics compatibility and economy follow-ups remain outside this
checkpoint.

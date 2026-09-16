# Economy recovery checklist

This is a read-only comparison of `origin/gaps/economy` at `1c790ef1` with
the reviewed statics checkpoint (`35f3fe34` / `takeover/reconcile` at
`871e7e45`) and the active A9 owned-cost work in that worktree. No source
changes or builds were made for this note.

The economy tip is not a clean cherry-pick target for the current takeover. It
is on the older engine-gaps line and carries a different state schema. Treat
its behavior and tests as a porting specification. The relevant recovery
sequence is the six economy commits from `60fab162` through `1c790ef1`, then
the owned-cost repair recorded in `7333a859` and currently being edited in
`/tmp/agni-takeover-reconcile`.

## Schema and API reconciliation

The current statics writer emits v10. Its seat row has eleven fields in this
order: setup, draws, played-main, numeric `PlayLock`, facedown-look count,
card count, spell count, `next_discount`, the two readiness flags, and the
optional chosen champion. Its card row has thirteen fields, including
ordinary grants, costed grants, attachment/hidden/control state, and `named`.
The decoder deliberately keeps v7 and v9 compatibility as well as v10; the
two different v9/v7 twelve-field card layouts must remain explicit.

The economy tip also writes an eleven-field seat row, but it is a different
row: setup, draws, played-main, boolean `no_spells`, facedown-look count,
card count, spell count, gear count, gear-ability count, `promises`, and
`pool`. Its v10 reader is fixed-layout and does not retain the current
statics `PlayLock`, readiness, or chosen champion. Its card row is eleven
fields and has neither costed grants nor `named`. Applying that row or its
reader would silently reinterpret current v10 bytes and discard statics.

The economy `promises` replacement is also incompatible with the current v10
`next_discount` slot: an old v10 row contains a two-unsigned discount pair,
where the economy reader expects an array of promise records. Keep v10 as the
current statics schema and introduce an explicit new version for the promise
and pool schema (v11 is the straightforward choice). The v10 reader must
continue to read old statics blobs; v7/v9/v10 whole-blob coverage must remain
green. Do not infer a layout from array length when two versions use the same
length.

Before implementation, write down and test the v11 seat order. A minimal
port can retain the current v10 prefix and replace only the versioned
`next_discount` value with a promise array, then carry the readiness flags and
champion and append the economy gear counters and pool. The rules lock should
remain the current numeric `PlayLock`; economy's `no_spells` behavior maps to
its spell bit rather than reintroducing a parallel boolean. Whatever order is
chosen, it must be an explicit v11 reader/writer and preserve all of the
following values: lock bits, readiness, chosen champion, per-seat counters,
promises, and pool.

The A9 repair changes `CardState::granted_costed` from
`Vec<(Keyword, Expiry)>` to an owned `CostedGrant` representation. It keeps
the serialized row exactly `[kind code, energy, power codes, expiry]` and
removes the process-global `Power::intern`/`Cost::interned` cache. Its runtime
API changes `Ctx::granted_cost` and `Ctx::flow_of` to return engine costs and
adds `cost::of_grant`/`of_parts`; Kennen receives an explicit owned grant.
Port economy on top of that API. Do not restore tuple destructuring, static
power slices, `cost::to_script`, or the interner to make a conflict compile.

The legacy blob tests in this branch intentionally assert public decode and
exact independently authored bytes rather than the Rust representation of
`granted_costed`. Retain that property after the A9 merge, and add v11 cases
for promise/pool rows. A9's costed-grant byte compatibility does not make the
economy seat row compatible.

## Behavior to recover

Recover the economy lane in these bounded pieces, keeping each piece behind
round-trip and request-boundary tests.

1. Add the owned `Pool`/`Pooled` state and its deterministic serialization.
   `[Add]` effects must bank energy and resolved power, `pay::from_pool` must
   spend the pool before runes in the established deterministic order, and a
   payment must record exactly what it consumed. Preserve the economy expiry
   behavior: empty pools at the start of Main and at expiration, with the
   existing narration and turn reset ordering.
2. Add per-seat `Promise` state with the economy encoding: kind code, effect
   (`Discount(pool)`, `RepeatForCost`, or `FreeForPower { max_energy }`), and
   expiry. Matching is play-only: promises do not cover abilities or
   triggers. Matching discounts stack, all matching promises are consumed by
   the covered play even when an optional cost is declined, and permanent
   promises survive a turn while turn-scoped ones expire.
3. Port the cost pipeline without changing current statics precedence. A
   printed Flow/Repeat must keep its existing precedence over a granted cost;
   a promised Repeat is a separate optional instance and slot. Keep printed
   and promised Repeat stages distinct, add the promised-repeat prompt/status,
   and make `ChainItem::repeats`, target-spec cycling, execution slicing, and
   replayed target selection count every paid instance.
4. Port the energy-free gear promise with the printed-energy threshold. It
   applies only to a qualifying gear from hand, zeroes energy before other
   optional costs/surcharges, leaves power payable, and is spent by the first
   covered play. Preserve existing `PlayLock` enforcement and static
   discounts around it.
5. Port card scripts and assertions from the economy lane: Astral Heron's
   stacked next-card discount, Jayce's turn-scoped gear promise, Raging
   Firebrand's next-spell discount and expiry, Temporal Portal and The
   Academy's cost-equal Repeat, plus Nasus/Jhin assertions that now inspect
   promise or pool state. Keep card scripts using the public prelude helpers;
   their state effects must be persisted and folded rather than narrated only.

The `Need::AnyOf` warning in the A9 audit is material here. Do not use a
general conversion from runtime needs back into static `Power`; it turns
alternative domains into `Own` and loses meaning. Resolve static `Power::Own`
through the source domains only where the pool representation requires it, and
keep owned costed grants as their encoded power list.

## Original test coverage to restore

The economy commit removes ignores and changes assertions in the following
areas. Preserve the active statics replacements and reapply the economy
expectations to the current fixtures rather than copying old surrounding
lines.

- Astral Heron: trigger only at a battlefield, after the appropriate played
  spell resolution, no consumption by an ability or a countered first play,
  stacking two discounts, and permanent persistence.
- Jayce: only an in-hand gear within the printed energy limit is eligible;
  its power remains payable, the promise is consumed once, and an ineligible
  kind/location/energy does not consume it.
- Raging Firebrand: only the next spell is discounted, multiple Firebrands
  stack, a unit does not consume the spell promise, and the turn-scoped row
  expires.
- Temporal Portal/The Academy: a printed Repeat and promised Repeat are
  separate optional costs; declining the printed one still asks the promised
  one; paying both yields three executions with the right target groups; the
  promise is spent whether its optional Repeat is paid or declined.
- Pool/payment tests from `60fab162` through `ee6794ad`: reaction and delayed
  Add sources, source-domain/Own/Rainbow conversion, pool-first payment,
  multi-power selection, ability pricing, pool reset, and no double pricing.
- State and API tests: v11 promise/pool roundtrip, old v7/v9/v10 rows,
  costed Flow with Own/Chaos/Rainbow bytes, expiry, duplicate promise kinds,
  printed-before-granted precedence, and unsupported Equip/Empower refusal.

The original statics recovery review explicitly leaves Rift Herald and Herald
of Scales for the play/economy replacement lane. Keep those mappings visible
in the inventory audit and do not count the old ignored tests as recovered
until their current fixtures exercise the promise/economy behavior. Likewise,
the Towering Pairofant and Fallen Feline exceptions remain separate statics
coverage decisions.

## Integration hazards and gates

The economy files overlap the statics/A9 seam in `state.rs`, `engine/cost.rs`,
and `engine/ctx.rs`; they also touch `prelude.rs`, `pay.rs`, `play.rs`,
`chain.rs`, `prompts.rs`, `targets.rs`, and the affected card scripts. The
active A9 edits additionally touch `cards/mod.rs`, `cards/kennen_storm_of_shuriken.rs`,
`engine/legal.rs`, and the net replay test. Resolve by API intent and rerun a
repository-wide symbol search for `next_discount`, `floating`,
`granted_costed`, `Cost::interned`, `Power::intern`, and `to_script` after each
merge; no stale caller should survive behind a compatibility shim.

The economy optional slot is numbered 6 while the normal pick vector starts
with four entries. The current dynamic `ChainItem::set_slot` behavior must be
retained and its serialized picks, prompt resume, `begin_ignoring_any_and_all_costs`,
and native/wasm replay must all handle the extended vector. A prompt at the
promised-repeat stage must reconstruct with the same slot and status after a
snapshot restore.

The v11 change must be reflected in module pins and plugin replay fixtures.
The decoder should reject unknown versions, retain v7/v9/v10 compatibility,
and refuse malformed/ambiguous row lengths. Reconstructing a v10 state must
not manufacture an empty promise/pool row with different semantics, and
re-encoding a legacy v10 state must remain byte-stable for its existing
fields.

Required gates after the implementation are the complete Riftbound turn test
suite, warnings-denied all-target clippy, scoped formatting and diff checks,
Riftbound wasm compilation, and the real hardened-plugin parity replay. The
parity corpus must include a persisted promise/pool state, a promised-repeat
prompt continuation, and reconstruction between entries. This note records
source-level risks only; it does not claim any of those gates passed.

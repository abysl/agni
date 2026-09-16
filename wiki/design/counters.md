# Counters — plugin-declared, player-changed, host-sequenced

> Status: design, 2026-09-05. Nothing here is built yet.

## What exists today

Nothing that names a counter. Two substrates carry the weight, unevenly:

| Substrate | Reaches | Missing |
|---|---|---|
| `LogAction::Annotate { card, key, value }` → `LogState.annotations` → `ViewCard.badges` | the renderer, already | any declaration of what a badge *is*, or a UI to change one |
| `LogAction::Game { data }` → `LogState.plugin_state` (via `Verdict.plugin_state`) | the fold only | `plugin_state` is not in `TableView`, so nothing seat-level ever reaches the screen |

So a per-card counter has a complete path to the screen and no way to declare or
edit one; a seat counter — life, points, XP — has no path at all.

## The shape

Counters copy the zone table, because that pattern is already proven end to
end: the plugin declares, the manifest carries, the view mirrors, and the
client lays it out knowing nothing about the game.

```rust
pub enum CounterScope { Seat, Card, Table }

pub struct CounterDecl {
    pub id: u16,
    pub name: String,          // "life", "points", "xp", "infect"
    pub label: String,         // "Life"
    pub scope: CounterScope,
    pub default: i32,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub step: i32,             // what one press of +/- moves
    pub place: CounterPlace,   // seat plate, card badge, table centre
}
```

`PluginManifest` gains `counters: Vec<CounterDecl>` beside `zones` and
`hotkeys`.

## Changes are log entries

```rust
LogAction::Counter { target: CounterTarget, counter: u16, delta: i32 }
```

Deltas, not sets. The host sequences everything, so both would converge, but a
delta keeps the log a readable history — "Rae −3 life" — and replays as what
actually happened rather than as a series of destinations. A correction is just
a delta the other way.

`CounterTarget` is `Seat(u8)`, `Card(u32)` or `Table`, matching the decl's
scope; a mismatch is a fold error. The fold clamps to the declared `min`/`max`
so a client that sends nonsense cannot desync the table, and holds the result in
`LogState.counters: BTreeMap<(CounterTarget, u16), i32>`, seeded from `default`
when a seat joins.

## Free-form by default

agni's constitution says the shipped engine enforces no rules, and counters do
not change that. Any seat may move any counter; the declared range is the only
constraint the fold applies. A plugin that *wants* to enforce something already
has the hook — every entry passes through its `decide`, so a plugin can reject a
counter change exactly as it rejects an illegal move. Nothing here makes the
neutral table less neutral.

## Reaching the screen

- **Seat and table counters** — `TableView` gains
  `counters: Vec<CounterValue>`, and `ViewDelta::Counters` carries changes, so
  the mirror updates the same way zones and cards do.
- **Card counters** — reuse `ViewCard.badges`, which already arrives. The decl
  is what turns an opaque badge into a labelled, steppable counter.

kai renders from the decls alone: a seat plate row of `label −  n  +`, a badge
on the card, a centre strip for table counters. No game knowledge in the client,
exactly as with the zone table.

## What the games declare

- **Riftbound** — `points` (seat, 0..=8, step 1) and `xp` (seat, from 0).
- **MTG** — `life` (seat, default 20, step 1, unbounded below for the formats
  that allow it), `poison` (seat, 0..=10), and card-scoped `+1/+1`, `-1/-1` and
  `loyalty`.

## Cost

This is a wire change on four fronts, so it lands with a version bump and
regenerated goldens:

| Surface | Fixture |
|---|---|
| `LogAction::Counter` | `sim/tests/fixtures/log_v1.hex` |
| `TableView.counters` | `sim/tests/fixtures/view_request_v0.hex` |
| `PluginManifest.counters` | plugin ABI — both guests rebuild |
| `WIRE_VERSION` | `net/tests/fixtures/{client,host}_v1.hex` |

Old logs still fold — the new variant is additive — but an older client cannot
read a log containing counter entries, which is what the `WIRE_VERSION` bump is
for.

## Open questions

- **Per-seat defaults.** MTG's starting life is 20 in constructed and 40 in
  Commander. Either the plugin varies the decl by table option, or the decl
  carries a default the host may override at genesis. The second is simpler and
  keeps the decl static.
- **Who may change another seat's counter.** Free-form says anyone; a table
  option could restrict it to the owner plus the host.
- **Card counters on a hidden card.** A badge on a face-down card leaks
  information if it is visible to everyone; scope it to the card's existing
  visibility rules.

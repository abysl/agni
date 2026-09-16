# Agni

> "Form is a duel."

Agni is a deterministic card-game engine: table state, replayable simulation,
network sessions, plugin hosting, and user-run card importers. It is built on
top of [Spirit Library](https://github.com/abysl/spirit-library), the generic
content mesh underneath it.

Clients handle everything else. [Kai](https://github.com/abysl/kai) does UX,
rendering and QR pairing; other front ends can use the same simulation boundary.
Front ends never mutate game state:
dropping a card emits a request that agni's session stack orders and folds.

agni ships **neutral**: no game's rules are enforced by shipped code, and no
card content is distributed with the repo or the builds. Rules belong to
user-installed plugins binding through `plugins/`; card content arrives via
user-run importers against third-party public services (Scryfall and
Riftcodex today) into the user's own spirit store. This is the Cockatrice
posture, on purpose.

## Structure

| Crate | Role |
|---|---|
| `core/` | game-agnostic table state — cards, zones, ownership, seeded RNG, `Intent` |
| `sim/` | the deterministic action log — entries, validation, the fold every replica runs; the wire face/zone types, the mirrored `TableView`, the engine ABI and the `ModuleCall`/`AbiEngine` hosting seam, genesis module pins |
| `engine/wasm/`, `engine/host/` | the engine.wasm guest and its wasmi host |
| `net/` | multiplayer over spirit — the host sequencer and client sessions (`Result`-returning, never panicking on a peer's input), the versioned CBOR wire protocol, the `spirit-table/1` transport, the bridge clients drain |
| `importers/` | user-run card importers: the game-neutral art journal and deck-list machinery, the MTG (Scryfall) and Riftbound (Riftcodex) importers behind cargo features, and their bins |
| `plugins/` | the trait surface user-installed card scripts bind to, keyed by card identity; `plugins/harden` mints wasm modules; `plugins/sdk` is the dependency-free guest toolkit (CBOR, request/verdict/view codecs, turn-order and pass-window primitives, the export macro); `plugins/{riftbound,mtg}` are the two guest plugins |
| `games/{riftbound,mtg,abysswalker}/` | per-game zone tables, deal plans and deck-shape types for importers and deck models; `games/riftbound/rules` holds the Core Rules text and `games/riftbound-turns` the turn machine the Riftbound plugin runs |

## Design

- [`wiki/design/architecture.md`](wiki/design/architecture.md) — determinism,
  replay, plugins over spirit, front-end boundaries, the crate map
- [Kai deterministic log](https://github.com/abysl/kai/blob/main/wiki/design/deterministic-log.md)
  — the action log agni-sim implements
- [`wiki/design/plugins.md`](wiki/design/plugins.md) — plugins as pure deciders
  in the fold, the SDK, verdict effects, and the Riftbound rules as built
- [`wiki/design/deck-import.md`](wiki/design/deck-import.md) — how a deck link
  resolves against Riftcodex and the store catalogue, and the retry and
  `unresolved` report that replaced the silent drop
- [`plans/implementation-plan.md`](plans/implementation-plan.md)

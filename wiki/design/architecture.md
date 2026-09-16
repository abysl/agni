# agni — Architecture

> Status: design sketch, written at bootstrap. Nothing here is implemented yet.
> Update it in the same change that alters behaviour, not afterwards.

agni is a framework for building **deterministic card games**: replayable,
peer-to-peer, extensible with custom cards, and shared by every card game
project rather than rewritten per front end.

Consumers, all three real and all three different:

| Consumer | Nature of the boundary |
|---|---|
| `abyss-walker` (Godot, abysl) | in-process FFI or local IPC; rich UI, full rules |
| mechanist e-ink cards (`agni/paisho`) | tiny, intermittent, power-constrained |
| paisho, eventually | physical/digital hybrid |

Three consumers with nothing in common but the rules is the whole reason the
engine is a library and not part of a game.

---

## 1. Determinism is the load-bearing property

Everything else — replay, peer-to-peer play without a referee, desync
detection, spectating, bug reports that reproduce — is downstream of one
guarantee:

> The same initial state plus the same ordered inputs produces the same final
> state, on every machine, every platform, and every version that claims
> compatibility.

This is far cheaper to design in than to retrofit, and a violation is close to
undebuggable after the fact — the symptom appears on someone else's machine,
many turns after the cause. So the rules are constraints on the simulation
crate, enforced by review and (where possible) by lint:

- **Seeded RNG only.** One explicit PRNG, seeded from state, advanced through
  the state. Never a thread-local or OS source. Shuffles are a function of the
  seed and nothing else.
- **Ordered collections in state.** No `HashMap`/`HashSet` anywhere a traversal
  order can reach a decision. Iteration order of the std hash maps is
  randomized per process by design, so it differs between two peers running the
  same build. Use `BTreeMap`/`BTreeSet`, or `IndexMap` when insertion order is
  what matters.
- **No floating point in the simulation.** Different platforms, different
  results. Integers and rationals only.
- **No wall-clock, no locale, no environment.** Time is a value in the input
  log, supplied by the caller, never read from the machine.
- **No pointer identity or address-dependent behaviour.** Entities are
  identified by stable ids assigned in the log, never by allocation order.

The determinism boundary is the simulation crate, and *only* that crate.
Rendering, audio, animation and input handling live above it and are free to be
non-deterministic — that separation is what keeps the rules testable and lets
the same rules drive a Godot scene and an e-ink card.

**Test it, don't hope.** Determinism erodes silently, so the property needs a
test that would catch the erosion: replay a corpus of recorded games and assert
the final state hash matches. Cross-platform (x86_64 and aarch64, Linux and
macOS) is the version that catches real bugs.

## 2. Replay: log the inputs, not the states

A game is an initial state plus an ordered log of player actions. Persisting
the log rather than snapshots keeps replays small, makes them diffable, and
means a replay is the same computation as the original game — so a replay that
diverges is itself a determinism failure the engine can detect.

Open questions, to answer before the format is fixed:

- Periodic state hashes in the log — cheap desync detection, and they bound how
  far back a divergence has to be hunted. Cost is log size and a compatibility
  commitment.
- Snapshots as an *optimization* (seek without replaying from turn one), never
  as the source of truth.
- What a rules-version change does to old replays: refuse, or migrate.

## 3. Assets and plugins over spirit

Custom cards are the point, and they are what makes a peer-to-peer card game
hard: two peers must agree on what a card *does* before they can agree on what
happened. Fetching a card by URL is exactly the wrong shape — the same URL can
serve different bytes to different players.

spirit addresses this directly: content identity is separate from the bytes
that represent it, so a table can agree on the identity of a card set and each
peer can source the bytes from whoever has them. Trust is a signed attestation
from a group you chose, not a server's say-so.

So: agni depends on `spirit-sdk` and identifies every card set, rule module and
art asset by content identity. What still needs deciding:

- **Are plugins data or code?** Data (a declarative effect language) is
  sandboxed by construction and portable to firmware, but limited. Code (wasm)
  is expressive and much harder to make deterministic and small. This choice
  constrains the e-ink consumer hardest and should be made with it in mind.
  Decided in [plugins.md](plugins.md): both, layered — declarative card
  definitions, game rules as a wasm decider module inside the fold, per-card
  scripts as data the game plugin interprets.
- Whether a rule plugin's identity is part of the replay's compatibility key —
  almost certainly yes, or replays silently change meaning when a card errata's.

## 3a. Zones are game-defined

`agni_core::Zone` is an open enum: `Hand` and `Board` for the free-form table,
and `Plugin(u16)` for everything a game declares. A game crate pins its zones
as `agni_sim::wire::ZoneDecl` data — id, name, `ZoneKind`, `ZoneOwner`,
`ZoneVisibility`, `ZoneLayout`, `ZonePlace` band and `span` weight — and the
table's genesis entry carries that zone table in its `TableConfig`, so every
replica folds the same declarations and a renderer lays out from them without
knowing the game. Magic's exile and command zones and Riftbound's legend, rune
deck and shared battlefields are all `Plugin(n)` with different declarations.

The fold uses only the declaration: visibility decides who may `Reveal`,
`ZoneKind::Deck` decides what a re-deal clears, and ownership decides which
seat a per-seat zone belongs to. Legality inside a zone stays with plugins.

## 4. Front-end boundary

### Manual match recovery

The SDK's optional `manual::Command` protocol travels inside the existing opaque
`Game` action. Riftbound handles it before entering its rules engine: `Disable`
sets the additive `GameBlob.manual` field, keeps turn and table facts, and clears
pending prompts, priority, chain bookkeeping, delayed effects and a reported win.
All subsequent physical actions bypass automatic rules; new-game reset restores
the normal lobby. The presenter exposes capabilities through hidden affordances
so clients need no Riftbound dependency to show the controls.

Private deck looks use `Peek` effects. Manual reveals use `Reveal`, including
showing a deck card in place. A plugin-created reveal debt lets the owner-attributed
reveal entry cross an otherwise fully hidden zone; direct reveals there remain
invalid. `Conceal { card }` clears public/private visibility grants and the
replicated face without changing location, counters or marks.
Shuffle conceals the whole deck before applying an explicit-seed permutation;
the seed is supplied in the logged command, never read from the environment by
the fold. Only the requesting seat's deck is inspected or shuffled. Token removal
and individual reveal/conceal are restricted to the owner.

This adds an engine effect but no session wire message. Engine and Riftbound
plugin artifacts must both be rebuilt; genesis-pinned module hashes keep peers
on the same semantics. An already-running match pinned to an older plugin does
not gain the capability merely by updating its renderer.

The simulation should be a pure state machine: `apply(state, action) -> state`,
with no I/O, no rendering, no networking. Front ends drive it and observe it.

The consumers pull in different directions, and this is where the design is
least settled: Godot wants a rich in-process API; the e-ink firmware wants
something that fits in very little memory and syncs intermittently, and may
only be able to hold a view of the game rather than run it. A plausible answer
is that the firmware is a *client of a peer* rather than a peer itself, but
that is a decision, not a conclusion.

---

## Crate layout

| Crate | Role |
|---|---|
| `agni-core` | game-agnostic table state: cards, zones, seeded RNG |
| `agni-sim` | the deterministic action log: `LogEntry`/`LogAction`, `validate`, `fold`, plus the wire face types the log embeds; see kai's [deterministic-log.md](../../../../agni/kai/wiki/design/deterministic-log.md) |
| `agni-net` | the multiplayer stack over spirit: `HostSession` (the sequencer) and `ClientSession`, the CBOR wire messages, the `spirit-table/1` transport, and the net-to-game bridge kai's systems drain |
| `agni-importers` | user-run card importers against third-party public services; today the Scryfall ingester and its `ingest-scryfall` bin |
| `agni-plugins` | the binding surface for user-installed card scripts: `CardScript` keyed by card CIR, `ScriptRegistry`; no interpreter |
| `agni-riftbound` / `agni-mtg` / `agni-abysswalker` | per-game deck-shape types for importers and deck models — data, not enforcement |

The Bevy 3D card table is its own app,
[kai](../../../../agni/kai/README.md); see its `wiki/design/table.md`.

`agni-core` must never depend on Bevy. The e-ink firmware consumer cannot take
that dependency and the rules have to run there too, so the Bevy `Resource`
wrapper lives in kai as a newtype instead. `agni-sim` inherits the same rule.
`agni-net` must keep compiling on `wasm32` exactly as this code did inside
kai: spirit-node with `default-features = false`, timers and spawns from
`n0-future` in shared paths, `tokio` limited to `sync` + `macros` on wasm —
the browser peer at kai.rae.blue rides it.

Randomness is hand-rolled (xorshift64*) rather than taken from `rand`. A
dependency-free generator means there is no ambient thread-local generator to
reach for by accident, and no version bump that silently changes the sequence
and invalidates every stored replay. It is xorshift64* — fine for shuffles, **not
cryptographically secure**; never use it where predicting the sequence profits a
player.

The shipped engine is deliberately neutral: no game rules are enforced by any
crate here, and no card content ships in the repo or the builds. Rules
enforcement, when it exists, lives in user-installed plugins behind
`agni-plugins`; card content arrives through user-run importers behind
`agni-importers`. Anticipated, once earned: `agni-ffi` (the Godot boundary).

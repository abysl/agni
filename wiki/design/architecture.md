# Agni architecture

Audience: developers new to Agni who understand functions, data structures, and
tests. Read the [project introduction](../../README.md) first.

## What Agni owns

Agni is the game framework between an interface and the services that exchange
content. A client asks to perform an action; Agni decides whether to accept it,
records the accepted input, and produces a view for the client to display.

A **fold** applies an ordered sequence of actions to a starting state.
A **session** connects player requests to that sequence. A **plugin** adds the
rules of a particular game. A **view** is the information a client may render,
including actions and prompts appropriate to a seat.

## Follow one action

1. A client submits an intent, such as moving a card.
2. The host validates it using the current state and selected plugin.
3. An accepted change becomes an ordered log entry.
4. The deterministic fold applies the entry.
5. The client receives the result and updates its presentation.

A rejected request must not alter state or reveal information as a side effect.
Keep validation and visibility changes together when reviewing a hand play.

## Crate map

| Crate or directory | Responsibility |
|---|---|
| `core/` | Shared card and table types |
| `sim/` | Action log, deterministic fold, engine interface, and views |
| `net/` | Host/client sessions, messages, and transport integration |
| `engine/wasm/` | Portable engine executable |
| `engine/host/` | Run the engine and plugin modules |
| `plugins/sdk/` | Game-neutral helpers used inside plugins |
| `plugins/harden/` | Validate modules and add execution/stack limits |
| `games/deck/` | Shared deck and snapshot types |
| `games/*`, `plugins/mtg/`, `plugins/riftbound/` | Existing game-specific implementations |
| `importers/` | Optional parsers, resolvers, and content acquisition |

The separate [agni-rfb repository](https://github.com/abysl/agni-rfb) contains
a copy of the Riftbound implementation. Existing dependencies have not all
moved there. Removing the copies here requires coordinated changes to
importers, clients, and tests.

## Why determinism is a boundary

Replay is meaningful only if the result does not depend on the machine
running it. Keep clock reads, network access, environment access, and unseeded
randomness outside `core` and `sim`. Use ordered data structures where
iteration affects a decision. Pass any random seed through the recorded inputs.

A renderer can animate differently on each device; it cannot independently
decide where a card ends up.

## Modules and compatibility

The engine and plugins exchange encoded data through an application binary
interface (ABI). Hardening validates a compiled module and adds resource limits.
The resulting bytes determine the module's identity. A session pins its module
identities at creation, so an update does not silently alter a match in progress.

Wire protocol versions, plugin state versions, and application versions describe
different boundaries. Review and test the affected boundary explicitly.

## Deck shuffle randomness

The host obtains fresh system randomness for each shuffled deck deal, including
new tables, new games, and deck reloads. Native hosts use the operating system's
random source; browser hosts use Web Crypto through `getrandom`. The seed mixes
that private entropy with the seat, log position, and deck contents. It is not
derived from the deck list alone, and is never sent to other players.

Randomness is obtained before dealing or clearing a deck. If it is unavailable,
the operation fails rather than falling back to a predictable seed. Groups that
do not require shuffling retain their supplied order.

This happens in the host's dealer, outside the deterministic fold. Replay uses
the recorded card operations and the appropriate private face records, not a
new shuffle. No wire or game-plugin state change is needed. This does not make
a host trustworthy; the host still knows the deck order.

## Session undo

Host sessions retain at most 64 pre-intent checkpoints, including deterministic
state, private dealer faces, hidden-play ownership, the card ID allocator, and
the log boundary. One intent can append several entries; undo counts the intent
and all its automatic effects as one action. Setup mutations (genesis, join,
deal, clear, reset) establish a new history boundary.

Undo requests carry a monotonic session revision, not a rewindable log length.
Only one proposal can be active. Every other seated player must be connected
and approve the proposal ID. Rejection, disconnection, or accepted play cancels
the proposal. An unopposed host can restore immediately. Snapshots are local
host state and are never broadcast.

Restoration uses the engine's existing state-restore ABI and the native shadow;
ordinary intents do not call the engine snapshot ABI. A client receives the
retained log boundary and only its currently owed private faces. It replays
the retained prefix, clears optimistic moves and stale face caches, then resumes.
Reconnecting clients receive the retained log through the normal welcome flow.
The protocol cannot make a player forget previously revealed information.

## Storage and security boundaries

Wire version 8 adds session chat requests and host-authenticated sender IDs.
Chat is ephemeral transport data, not part of the deterministic game log.
The shared text validator bounds messages at 2000 bytes; applications also
bound histories and rate-limit accepted messages per seated sender.

The Riftbound importer exposes fixed-origin public deck search for Piltover
Archive and RiftDecks. Search returns at most 20 titles and source URLs per
page, with queries limited to 160 bytes and pages 1–10. The deck gateway
accepts `search_site=piltover|riftdecks`, `q`, and `page` through its existing
deck resolver. Results are untrusted data, not instructions or imported decks.
Website blocking and transport errors are reported without bypassing access
controls. Use synthetic markup for parser tests, never downloaded pages.

Spirit Library stores content by hash and exchanges it between devices.
Agni adds game-specific schemas and protocols; Spirit must not depend on Agni.
A content hash verifies bytes, not the honesty of whoever supplied a rule
module or its right to redistribute artwork.

The session host orders actions. This architecture is not proof against a
malicious host and should not be described as one.

Continue with [development](../development.md), or use the
[reference index](../README.md) for module, counter, and importer details.

## Riftbound 0.8.1 state and presentation

The Riftbound plugin uses blob version 15. Each seat persists an
`equipment_played` flag when a played Equipment event is raised; Expiration
clears it. Azir's condition reads that history, not the current request's
temporary event list. Versions 13 and 14 remain readable and default the new
field to false; their snapshots do not contain earlier Equipment history.

After accepted enforced actions, the plugin publishes computed Might minus
printed Might through the existing Might counter. It does not add another
modifier to the rules calculation. This keeps conditional effects such as
Steel Paws' Empower visible, including when the condition stops applying.
Free-table manual counters and hidden faces are not overwritten.

Token declarations include print identifiers; renderers can resolve artwork
at runtime without bundling card images. These changes do not require a wire
protocol change. Kai's maintenance release pins a wire-7 revision in Cargo.lock.

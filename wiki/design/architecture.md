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

## Storage and security

Spirit Library stores content by hash and exchanges it between devices.
Agni adds game-specific schemas and protocols; Spirit must not depend on Agni.
A content hash verifies bytes, not the honesty of whoever supplied a rule
module or its right to redistribute artwork.

The session host orders actions. This architecture is not proof against a
malicious host and should not be described as one.

Continue with [development](../development.md), or use the
[reference index](../README.md) for module, counter, and importer details.

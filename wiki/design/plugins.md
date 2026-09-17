# Building and integrating game plugins

Audience: developers familiar with Rust who want to understand Agni's plugin
boundary. Read [architecture](architecture.md) first.

## What a plugin supplies

A plugin receives the relevant state and a request, then returns a verdict,
updated plugin state, and effects for the engine to apply. Its view output
describes available actions and prompts for the renderer. It does not draw UI,
open sockets, or fetch card images.

The game-neutral helpers live in `plugins/sdk/`. Existing plugin entry points
under `plugins/mtg/` and `plugins/riftbound/` show the export pattern.
They are examples of the current ABI, not guarantees of complete game rules.

## Compile, harden, then identify

Compile the guest for `wasm32-unknown-unknown`. Run `agni-harden` on the
raw module to validate its ABI and add resource limits. The hardened bytes,
not the raw compiler output, are the artifact whose hash a session pins.

The host runs the engine and plugin through defined encoded requests/replies.
Keep limits and deterministic failure behavior consistent across hosts.
Do not add an unbounded escape hatch when a plugin exceeds its budget.

## Lifetime and compatibility

Installing a module makes it available for a future session. It does not
replace the module inside a running match. The session's initial state records
which engine and plugin bytes it uses.

An ABI version, game-state encoding version, wire version, and application
version are different compatibility signals. Update the one whose contract
changed and test older data where compatibility is intended.

Engine ABI 4 and plugin ABI 1 add the public-face `Transform` effect used by
copies. Hosts accept plugin ABI 0 as a compatible older subset, but reject
unknown newer plugin ABIs before a table starts. A new plugin that emits this
effect must be paired with the updated engine and host; transferring a pinned
plugin alone cannot teach an older host how to decode its verdict.

Wire 9 is the compatible maintenance protocol for these effects without chat
messages. All players must update together; wire 7 clients cannot replay copy
effects. Wire 8 separately introduced chat messages and is not interchangeable
with the maintenance protocol.

## Test before integration

Use pure rule tests for decisions and refusals, host tests for ABI behavior,
and session tests for module pinning and replay. Check malformed replies,
resource exhaustion, and unavailable modules.

The [development guide](../development.md) provides the engine build commands.
The separate [agni-rfb](https://github.com/abysl/agni-rfb) repository is the
game-specific contribution surface, though Agni still contains legacy copies
used by current consumers.

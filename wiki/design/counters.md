# Game counters

Audience: contributors adding a score, resource, or other numeric value to a
game. Read [architecture](architecture.md) first.

A counter is game-declared state, not a number owned by a UI widget.
The game descriptor defines what the counter means and the renderer displays
the corresponding view.

Player changes must pass through the host's ordered request path and become
log entries. Do not let a local button update the displayed total as if that
were the authoritative game state.

A free-form game may allow manual adjustment. A rules-enforced game may impose
limits or change the same counter through effects. Keep those policies in the
game's decision logic rather than hard-coding them in the renderer.

Test declaration defaults, allowed and refused adjustments, replay, and view
visibility. A counter change must reach the same result on every replica.

# Riftbound rules implementation

Audience: developers reviewing game-rule decisions. Read
[architecture](architecture.md) and [plugins](plugins.md) first.
This is a code guide, not an official rules publication.

## Repository status

The game implementation is separately available in
[agni-rfb](https://github.com/abysl/agni-rfb). This workspace still contains
copies used by current importers, tests, and clients. Identify which revision
the consumer builds before changing a rule.

The copied implementation lives in `games/riftbound-turns/`, with deck
construction in `games/riftbound/` and the guest entry point in
`plugins/riftbound/`.

## Decision model

The turn machine consumes a snapshot, its serialized game state, and an action.
It validates the request, computes effects, and returns a new state or refusal.
The engine applies accepted effects in the ordered log.

Turn phases, priority, combat, costs, prompts, and cleanup are connected parts
of one transition. A partial update followed by a refusal is a bug.

Deck-construction legality is separate from in-game legality. Knowing a card's
metadata or listing it in a catalog does not implement its effect.

## Where to work

Start with `state.rs` for persisted state, `engine/` for transition logic,
`cards/` for individual scripts, and `present.rs` for the player's available
actions. Reuse the existing projection and effect helpers rather than writing
direct table mutations inside a script.

Prompts must preserve who is allowed to answer and what information they may
see. A hidden face cannot become public merely because a requested action was
considered and rejected.

## Rule changes

Describe the intended interaction in your own words and cite the official rule
edition you consulted. Add a small regression state that demonstrates the
decision. Test refusal, payment, targets, resulting effects, cleanup, and
visibility as applicable.

Do not infer full rules coverage from the existence of a card registry entry.
Use tests for implemented behavior and record known deviations explicitly in
the relevant issue or pull request.

## Compatibility

Changes to serialized game state need version and restore/replay tests.
Changes to prompts need checks that clients can answer every offered choice.
Changes to effect encoding need framework integration tests.

Pool Markdown is also machine-readable input for tests and consumers.
Treat it as data, not ordinary prose to reflow. See the
[reference-data guide](../../games/riftbound/rules/README.md).

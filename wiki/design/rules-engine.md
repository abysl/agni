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

## Copies and abilities after a token leaves

A Reflection uses the copied face's registered script for keywords, activated
and triggered abilities, and statics. Transforming an existing token also
invalidates the request-local static-source candidates, so copied auras and
play restrictions apply immediately rather than after the next request.
Copying does not play the copied unit again or duplicate its damage, buffs,
attachments, or separately granted abilities. LeBlanc grants Temporary to the
Reflection after copying.

Triggered and activated chain items save the canonical name of their printed
ability's script when queued. Resolution uses that identity plus the ability
index, not the source's current face or continued existence. A despawned token's
last face is available while its death triggers are collected. No vanished
card is kept on the board or made targetable to preserve its ability.

The rule source is the publisher's [Core Rules, July 16, 2026](https://cmsassets.rgpub.io/sanity/files/dsfx7636/news_live/e9ac8e3d33e0f78cef296f5945aba7bc1313b086.pdf):

- 477.1.b.1: copyable traits include rules text, and copying a copy uses its
  current copyable traits.
- 808.1.c–d: Deathknell triggers on death, retaining the details needed before
  the source leaves the board.
- 816.1.b: Temporary kills its permanent in its controller's Beginning Phase,
  before scoring. This is a kill, not a banish or bounce, so Deathknell applies.

Blob version 16 appends an optional script identity to pending and finalized
chain rows. Previously supported versions remain readable and default to the
old live-source lookup. They cannot recover an identity that was never saved
for an already vanished token. The framework wire protocol is unchanged;
peers must still agree on the hardened plugin hash. Plugin 0.9.1 ships this
change in Agni, which Kai currently consumes; the extracted agni-rfb copy is
not the runtime source for this fix.

The Reflection regression tests serialize state and rebuild the script cache
between choices and priority passes. They cover simultaneous combat deaths,
Temporary before Hold scoring, multi-stage Deathknell prompts, movement and
activated abilities, changing a copy while its trigger waits, immediate aura
changes, copying a copy, and bouncing without Deathknell.

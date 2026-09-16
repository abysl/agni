# A1 design notes: object identity across zone changes

This is a read-only audit of the reconciled `riftbound-turns` tree at
`2932deb5` (rules merge plus the repair checkpoint). It proposes the shape of
the next change; it makes no tracked repository changes.

## Rule boundary

Core Rules 124 says that an object becomes a new object whenever it changes
zones to or from a Non-Board Zone. The SDK's `ZoneKind` is a renderer and
transport classification, so it is not the Board test. Riftbound's logical
Board includes each Base, each Battlefield, the Legend Zone, and each
Facedown Zone (107.1–107.4). In this engine, the `rune_pool` presentation
zone holds the Runes that logically reside in a player's Base (107.1.c); it is
therefore Board for identity purposes even though its generic kind is `Aux`
and the rules' Rune Pool is a conceptual payment resource (166). The engine's
`hidden_at` field records a Facedown subzone while the card remains in its
associated Battlefield presentation zone.

The logical Non-Board zones represented here are the hand, chain, trash, Champion
Zone, Main Deck, Rune Deck, sideboard and banishment. The Legend Zone is Board;
the Champion Zone is Non-Board. Base and shared battlefields are therefore
the same object across a board move. The boundary is the effective
`(zone, seat)` for per-seat zones, not just the numeric zone id. A move within
the same effective zone is a reorder, not a new object.

The engine currently has physical card ids but no incarnation. `Ctx::forget_index`
only invalidates its lookup cache. `Snapshot::apply` sheds display counters and
annotations for Hand/Deck/Discard, while several `Ctx` helpers drop the blob
`CardState`; neither mechanism makes a retained `TargetRef::Card(id)` stale.
`Prevention` in this reconciled tree is also table-wide and has no protected
card field, so it cannot yet distinguish an old object from a fresh object
with the same physical id.

## References that need identity or explicit invalidation

The persistent references are:

* `TargetRef::Card(u32)` in `ChainItem.targets` and `subject`, and the card
  ids in `Prompt.picked`, `PromptWhy::GroupMove`, and `ShowdownStage::Damage`
  assignments. These are selections that can outlive a reaction and must
  fail closed when their object incarnation changes.
* `ItemKind::{Spell,Permanent,Ability,Trigger}` source/card ids. A pending or
  resolving item refers to the source object that created it. Its source id
  must not silently resolve against a later play of the same physical card.
  `Pending.item`, queued `engine::triggers::Match`, and the copied
  `Event` subjects all carry this relationship.
* `Delayed { source, args }`, which is serialized in `GameBlob`. `args` are
  card references and `source` is a source reference. Delayed-trigger data
  captured at creation must retain the captured object/zone information;
  source-independent delayed abilities are a deliberate exception described
  below.
* `ChainItem.awaiting` and `Ctx.awaiting`, which wait for particular hidden
  card faces. A card leaving and re-entering while a face is awaited must not
  satisfy the old wait merely because its physical id matches.
* `CardState.attached_to` and `Expiry::WhileAttached(card)`. Attachments and
  their materialized grants belong to the old object on either side of the
  link. `CardState.control_source` is a source reference for effects such as
  “you control it until I leave the board.” `MightMod.src` is an item id
  (`u16`), not a physical card id, and should remain an item provenance
  field; it must not be treated as a card incarnation by accident.
* `Noted` and `engine::triggers::Match` are snapshots associated with a
  referenced card even though `Noted` stores only zone/might/controller.
  Their owning `subject`/source needs the same identity. `Event` and
  `Ctx.remembered` are entry-local, but still need the same checks while one
  projection is resolving. `Effect::Counter` entries and SDK card counters
  must be shed or rejected when their card object is no longer current.
* The current `GameBlob.preventions` (`Prevention`) is missing the object it
  protects. A1 should restore a per-unit reference (the earlier damage lane
  used `unit: Option<u32>` and `Amount::Next`) with an incarnation, or use an
  equivalent invalidation hook. A shield must never protect a fresh object
  that reuses the same physical id.

References that are only zones, seats, chain item ids, or battlefield control
rows (`Control`, `Staged`, `Showdown.zone`) do not need card incarnations.
`Cause::Item(u16)` and `TargetRef::Item(u16)` name chain items, not cards.

## Zone-change boundaries to centralize

The identity transition belongs at the single projection boundary for every
`Effect::Move`, before/with `Ctx::emit` applying it. Determine the old and new
effective zones from the projection, and advance the card incarnation (or
invalidate all non-linked references) exactly when either side is Non-Board.
`Effect::Despawn` invalidates every reference to a token; `Effect::Spawn`
creates a new identity. This central hook is needed because direct `Move`
effects exist alongside the convenience helpers.

The concrete paths audited are:

* `Ctx::new`'s incoming `Action::Move`, `deal`, `channel`, `burn_cards`,
  `reveal_top`, `recycle_to_bottom`, and roll/mulligan sinking: all deck,
  hand, chain and discard transitions are Non-Board transitions.
* `play`'s hand/champion/facedown/trash/banishment to chain and chain to a
  destination; `counter_item` returning a chain card to hand/trash; and
  finalization returning a chain spell to trash, banishment or a deck.
* `banish`, `file_in_trash`, `trash`, `discard`, `bounce`, `kill`, and the
  `Flow`/recycle paths. These include board-to-Non-Board and Non-Board-to-
  Non-Board changes.
* `replace_battlefield` moving the replaced card to banishment and
  `swap_back` moving the original out of banishment; token replacement also
  has a despawn/spawn boundary. An incoming hand/champion-to-Battlefield move
  that is then marked hidden enters a Board Facedown subzone and is a
  Non-Board-to-Board identity boundary. Marking an already in-board card
  hidden, lifting it, and moving it between Board locations do not themselves
  create a new object; ordinary `move_unit`/`recall` retain identity, although
  their designation and attached-state rules still apply.

Do not infer this from `ZoneKind::Battlefield` or
`ZoneKind::sheds_state()`. The former misses Legend, Base-as-Rune-Pool and
Facedown semantics; the latter covers display-state shedding for
Hand/Deck/Discard and misses Stack, Champion, sideboard and Banishment. The
transition must use an explicit Riftbound logical-zone mapping and must handle
a per-seat zone's effective owner.

An incarnation counter can live in serialized `GameBlob` state keyed by
physical card id, with card-bearing references carrying the captured value.
An equivalent design may invalidate/prune every stored reference at the
central transition hook. Either design must have explicit read compatibility
for old blobs and must not guess from a bare card id. Adding identity only to
`TargetRef` is insufficient because sources, delayed args, awaiting faces,
attachments, control sources, prompts and shields are separate fields.

## Linked-instruction exceptions to preserve

These are rules semantics, not permission to keep a stale physical id alive:

* Rule 359.3.e.13 permits an effect that moves an object to look back at the
  object's pre-move characteristics. Capture a pre-move snapshot or an
  action binding before invalidation; do not make the new incarnation inherit
  temporary state.
* Rules 359.3.e.14 and 359.3.e.14.a–c bind later instructions to the earlier
  instruction's object/action. If the earlier instruction is ignored because
  its target changed zones, later linked instructions are ignored. If the
  action is replaced, later instructions continue unless they directly say
  “if you do” or otherwise reference that action. Hidden Blade and Deathgrip
  are the concrete distinctions.
* Rule 359.3.f.3 captures trigger-condition information when the trigger is
  fulfilled, and delayed triggers capture it when generated. Lillia's
  “moved from” location must survive a later move of Lillia; it must not be
  read from the current physical id at resolution.
* Rules 390.5–390.5.c scope delayed linked abilities to the affected
  object's/source's appropriate zone. Rule 392 is the opposite explicit
  exception: ordinary delayed abilities are not associated with their source
  object and still execute after that source leaves the board.
* Rules 390.3.a and `chain::finish`'s “then recycle/banish it” refer to the
  specific chain item leaving the chain, not to any later object represented
  by the same card id. Keep the chain item binding and the `held.zone ==
  chain` guard.
* Rules 394–397 make linked abilities provenance-sensitive. Zero Drive may
  replay only units banished by its linked deathknell/activation set; a card
  independently banished with the same physical id is not a match. The
  existing ignored Zero Drive and Cursed Sarcophagus tests are the clearest
  implementation guard.

Attachments, control-until-source-leaves effects, and `replace_battlefield`
state transfer need dedicated action semantics. They are not a general
exception to 124: a fresh object does not inherit damage, stun, counters,
grants, attachment or control merely because its physical id is unchanged.

## Regression cases for implementation

1. **Stale target after bounce/replay.** Put a spell targeting a unit on the
   chain; in response bounce that unit and replay the same physical card.
   Resolution must treat the original target as illegal and must not affect
   the replayed incarnation. A board-to-board march in response must retain
   identity and remain legal. Extend the existing Portal Rescue/Arcane Shift
   and `thrill_of_the_hunt::the_replayed_unit_is_a_fresh_play_whose_play_trigger_fires_again`
   coverage with the target assertion.
2. **Every non-board boundary sheds temporary state.** Exercise board →
   chain → board, board → hand, board → trash, board → banishment → board,
   and hand/deck reorders. Damage, stun, granted keywords, attachments,
   control-until-source, designations and shield state from the old object
   must not leak; base ↔ battlefield must retain the object. Verify both the
   serialized blob and SDK projection after each transition.
3. **Shield follows the object, not the id.** Register an `Amount::Next` or
   numeric shield for one unit, move that unit to a Non-Board zone, then
   return/replay it. The old shield must not prevent damage to the fresh
   incarnation; a shield on a different unit must remain. This also catches
   the current table-wide `Prevention` shape.
4. **Captured delayed source/location.** Generate a delayed trigger whose
   condition captures a source or “moved from” location, move/replay the
   physical card before it is due, and verify the captured old object and
   location are used. Separately verify an ordinary source-independent
   delayed ability still fires after its source leaves, while a delayed
   linked ability stops outside its allowed zone.
5. **Linked instruction action result.** Cover Hidden Blade when the target
   changes zone before resolution (both instructions ignored), then cover a
   replacement of the kill where the non-direct later instruction still
   executes. Add the direct “if you do” variant where replacement suppresses
   the later instruction. This prevents a generic invalidation pass from
   incorrectly suppressing all linked follow-ups.
6. **Linked banishment provenance.** Activate the Zero Drive/Cursed
   Sarcophagus path with one unit banished by its linked ability and another
   unit with the same physical identity history banished independently. Only
   the linked unit is offered/replayed, and the replay is a fresh object with
   a new incarnation. The existing ignored tests named
   `the_wearers_death_banishes_it_and_the_reclaim_replays_it_free_through_the_engine`
   and `the_activation_offers_the_units_it_banished_and_plays_one_for_its_printed_cost`
   should become the provenance gate after the identity work.

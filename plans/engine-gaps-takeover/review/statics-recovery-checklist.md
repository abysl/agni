# Statics recovery checklist

Compared `takeover/integration` (`6bc26db3`, reviewed rules recovery) with
`origin/gaps/statics` (`f8322afb`, seven statics commits from the shared base).
The merge tree reports 27 changed-in-both files, with 134 statics-touched
files overall. This is a read-only review; no tracked files were changed.

## Schema and wire reconciliation

- `BLOB_VERSION` is 7 in the rules tip and 9 in statics. The combined writer
  needs an explicit final version and the decoder needs a deliberate policy for
  old rows. `GameBlob` must retain the rules `wn` field and add statics' `dt`
  death-record field without changing map-key meanings.
- `SeatState` has incompatible rows. Rules writes nine elements in this order:
  setup, draws, played-main, `PlayLock`, facedown count, card count, spell
  count, discount pair, chosen champion. Statics writes ten elements from the
  pre-lock shape: the old boolean spell lock followed by the two ready flags.
  The combined row is eleven elements with both ready flags before the chosen
  champion. Preserve the rules decoder's eight-element legacy row and decide
  explicitly whether version-9 ten-element rows remain readable; test all
  accepted lengths and the legacy boolean lock.
- `CardState` is a harder collision: rules' length-12 row appends `named` at
  the end, while statics' length-12 row inserts `granted_costed` before the
  attachment and hidden fields. A length alone cannot identify the schema.
  Use the blob version or an explicit migration parser, then write a combined
  length-13 row containing both fields. Round-trip `named`, ordinary grants,
  costed Flow/Repeat grants, attachments, hidden state, control, and default-row
  elision.
- Statics' `Death` records live on `GameBlob`, not `SeatState`; `kill` appends
  card/controller/unit/phase and expiration clears the vector. Preserve that
  cross-request lifetime and test immediate death, prior-request death, gear
  exclusion, phase capture, and expiration.
- Preserve rules' `PromptWhy::Mode`/`Name`, `NameKind`, `CardState.named`, and
  `ChainItem` mode slots while adding statics' costed-keyword wire data. Update
  the design document's version and row descriptions together with the final
  schema decision.

## Semantic overlap to resolve

- Merge the complete `Static` enum and every matcher arm. Statics adds
  `EntersReady`, `ReadySuppressed`, `NoUnitsMoveToBase`, `NoMoveByEnemy`, and
  granted Repeat/Accelerate; rules adds play locks, scoring, counter, token,
  control, deflect, and named-spell behavior. A side-winning match arm will
  silently drop one lane's rules.
- Keep the rules `legal::unlocked` plus `spells_locked` path in
  `activate::flow_playable`; the statics ancestor still reads `no_spells`
  directly and would bypass both kind locks and Fallen Feline's named lock.
  Costed grants must remain visible to `flow_of`, Repeat pricing, and the
  statics granted-Accelerate surcharge.
- Statics changes `prelude::move_unit` and `move_to_location_of` to accept the
  source `Item` and return `Option<Moved>`, so `NoMoveByEnemy` can inspect the
  moving item. Update every remaining integration caller and test; do not add a
  compatibility wrapper that loses item provenance. `march::effect_move`,
  route/candidate filters, and group-move prompts must all use the item
  controller. Preserve rules' `set_controller_in_place` behavior when merging
  control-taking effects.
- Preserve the union of the `Event` enum. Statics currently raises `Event::Entered`
  after `Played` and before exhaustion to queue implicit triggers; this is an
  implementation observation, not a correct trigger-capture boundary. A3 must
  capture only after the inciting entry, including exhaustion, completes. The rules
  tip also has later `Activated`, `Burned`, `Banished`, and `TurnQueued` events.
  Check trigger ordering and ensure an entry is not emitted twice.
- `Ctx::ready`, `awaken`, `spawn`, and play finalization must combine static
  suppression, static/per-seat enters-ready predicates, and the rules' token
  faces. Test aura-granted readiness, Confront/Sun Disc flags, spawned tokens,
  and a vetoed ready versus Awaken.
- Keep `IMPLICIT_VISION` and `IMPLICIT_WEAPONMASTER` distinct from the rules'
  implicit indices. They fire once per printed, granted, or projected keyword
  instance on `Entered`; remove explicit Weaponmaster abilities from migrated
  cards so cards do not double-trigger. Cover Gemcraft Seer/Forecaster and
  Azir's granted Weaponmaster, plus the Vision recycle prompt and face routing.
- `Static`/`Filter`/`Rel`/`TargetSpec`/`Ability`/`Card` all grew on both lanes.
  Combine fields such as `min_at_level`, named modes, `Card.names`/`kind`,
  zone filters, tags, and static predicates, then run every constructor and
  `same_kind`/target matcher test. Keep Brush and Baron Pit token lookup from
  the rules tip.

## Immutable inventory exceptions and test mappings

- Towering Pairofant's old ignored
  `played_after_a_unit_died_this_turn_it_enters_ready` was replaced in
  statics by `played_after_a_unit_died_this_request_it_enters_ready` and
  `played_after_a_unit_died_in_an_earlier_request_this_turn_it_enters_ready`.
  The latter is the original cross-request behavior; the former is additional
  same-request coverage. Map the inventory entry to the latter and retain both.
- Rift Herald's statics-era
  `the_knell_offers_only_affordable_units_and_reports_a_hand_play` remains
  ignored because limited-play support belongs to the play lane. The play lane
  replaces it with
  `the_seats_own_view_greys_the_hand_cards_it_sees_are_no_payable_unit_and_the_play_reports_the_hand`
  and
  `the_knell_withholds_only_a_public_face_that_is_no_payable_unit_and_reports_a_hand_play`.
  Keep the statics checkpoint from claiming this debt; map both replacements
  when the play lane is integrated.
- Herald of Scales keeps the old ignored
  `playing_a_dragon_with_the_herald_in_play_costs_two_energy_less`, whose
  opponent fixture places a live Herald under seat 1 but expects printed cost.
  Economy replaces it with the active
  `the_engine_prices_each_seats_dragons_by_its_own_herald_and_pays_the_discount`
  and an explicit reason for the legacy ignore. Preserve the corrected
  seat-local fixture and do not count the contradictory test as unexplained
  statics debt.
- Fallen Feline's prompt lane keeps the old ignored
  `playing_her_names_a_spell_and_the_opponents_copies_are_refused_while_she_stands_afield`
  because its final self-controller Defy assertion conflicts with the empty
  counter target. The active replacement is
  `her_controller_names_a_spell_and_the_opponents_copies_are_refused_while_she_stands_afield`.
  When resolving the statics file conflict, retain the active replacement and
  its rules-question ignore, applying only the new movement-source signature.

## Required verification after resolution

- Run the full turn suite and confirm the expected counts plus no newly hidden
  tests; run clippy with denied warnings, scoped format, and the Riftbound
  wasm check.
- Run focused regressions for blob round trips and old-row decoding; readiness
  and death records (Pairofant, Shadow Watcher, Spoils of War, Sun Disc);
  implicit Vision/Weaponmaster and aura grants; NoMoveByEnemy/NoUnitsMoveToBase;
  granted Flow/Repeat/Accelerate (Kennen, Syndra, Rek'Sai); Fallen Feline's
  named lock and Flow path; and the corrected Herald of Scales fixture.
- Re-run the kai prompt-answerability check after any prompt or implicit
  trigger changes, then update inventory dispositions for the three replaced
  tests and the two intentional rule-question ignores.

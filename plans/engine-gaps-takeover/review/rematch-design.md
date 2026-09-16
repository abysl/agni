# Authoritative rematch design

Read-only Astra design against `/tmp/agni-takeover`, `takeover/integration`, 2026-09-14. No production edits. The integration branch is still recovering engine lanes; line numbers and the current blob version are observations, not reserved schema assignments.

## Decision

Put the match ledger and next-game rules in the Riftbound plugin, under a new `games/riftbound-turns/src/match_state.rs` module. Store it as one nested field of `GameBlob`. Reuse the existing generic `LogAction::Reset` and `ClientMsg::NewGame` transport. The plugin already receives the pre-reset table and can return a fresh per-game blob retaining the match ledger. Kai and its AI consume the replicated ledger and existing plugin affordances; neither computes the authoritative winner, starting-player entitlement, or battlefield exclusions.

This does not require a new agni-sim match concept, a spirit change, or a wire change merely to carry the ledger. It does require small serialized-state hooks after the engine recovery merges. A completely separate authoritative store outside the blob would create a second replay/snapshot authority and is the wrong way to avoid a state.rs conflict.

## What actually exists

- `sim/src/engine.rs::native_decide_request` validates and advances only the preview sequence; it does not apply Reset before the plugin decides. `sim/src/log.rs::fold_finish` applies the action and then installs the verdict's plugin bytes. `apply_action(Reset)` clears cards, faces, counters, annotations, tokens, peeks and other table information, while retaining plugin bytes unless the verdict replaces them. Its existing test explicitly establishes this contract.
- Both Riftbound Reset paths erase all plugin bytes: the free/lobby dispatcher in `games/riftbound-turns/src/lib.rs`, and `games/riftbound-turns/src/engine/mod.rs`. They are the immediate loss of match history.
- `GameBlob::apply_in_lobby` always uses a dice winner, and `StartGame` constructs a completely fresh `GameBlob::start`. Merely preserving a ledger in Reset would still lose it on StartGame and would not change who is authorized to start.
- `net/src/host.rs::reset` appends a host Reset before clearing dealer caches and optional re-dealing. Host-only reset validation is already generic. NewGame from a connected peer currently queues a host reset in kai. No game-over check currently guards it.
- `games/riftbound/src/lib.rs::TableOptions` contains only victory score and battlefield count. Its genesis encoding also carries the enforced flag. Duel and Match therefore become identical options in the actual log, despite distinct `MODES` rows. Match scope must first become a pinned option; two players plus two battlefields is insufficient to infer it.
- `kai/src/deck/battlefield.rs` keeps `record.battlefield`, `battlefield_played`, and `BattlefieldPrompt.placed` locally. `watch_new_game` detects an inactive-to-active transition, not Reset during an active session. `redeal_after_new_game` responds to `DealGeneration`, which also changes on Clear. These are rendering/dispatch caches, not game identities.
- The desktop sends body and battlefield `DealDeck` groups separately. AI `driver::deal` sends a whole plan and checks the locally held battlefield choice first. The AI's Reset handler clears `battlefield_played` but keeps `battlefield`; its next tick auto-deals that same battlefield. `deal` also sets `dealt` when it sends, not when accepted.
- A chosen battlefield reaches authority as a physical card's revealed `CardFace`. Neither core `CardFace` nor the SDK `Face` has a Riftcodex print ID. Canonical full card name is already the semantic identifier shared by these paths. Rules 103.4.c prohibit duplicate same-name battlefields; different print/art IDs must not evade reuse restrictions.
- Tokens can themselves have kind Battlefield (Brush and Baron Pit). Capture the actual presented battlefield names at game start, rather than scanning every battlefield-kind card at game end.

## Rules and exact scope

The pinned Core Rules 486.5–486.6 remove **both players' presented battlefields** after a game with a winner. They are not just the winner's battlefield, not controlled battlefield zones, and not all copies globally by ownerless name. Each player's usage history is keyed by that seat and canonical battlefield name. A draw adds no win and consumes no battlefield. Best of three ends at two wins; draws do not use up the best-of-three win budget.

The already-verified Tournament Rules PDF adds:

- 407.4: the previous loser chooses first or last; after a draw preserve the prior starting player.
- 406.1.b: after a draw the players **must use the same battlefields**, stronger than Core 486.5.a's permission to reuse them.
- 403.10: no sideboarding after a draw.
- 410.1 permits mutually agreed game draws, but this code currently has no recorded game-draw outcome or event.

Necessary now: represent a recorded draw distinctly, implement and test its rematch transition, never infer a draw from an unfinished game or from clicking Reset, and expose the unchanged-deck/same-battlefield policy to clients. Adding a negotiated draw proposal/confirmation UI is an optional product extension, not a prerequisite for fixing existing winner-to-next-game flow. With no legal draw producer, Reset during an unfinished enforced Match must refuse instead of silently inventing a draw. A test fixture may exercise the pure transition with a recorded Draw, but that does not count as an end-to-end production draw feature.

Keep Duel, free-form, MTG, and multiplayer variants out of 1v1 Match rules. Do not generalize “the loser” to a 3/4-seat table. Best-of-five battlefield reuse and tournament clocks are separate optional scope; do not advertise best-of-five until 486.6.a's usage-count rules are implemented.

## State and transitions

Suggested shape (names can change):

- `MatchState`: match/game generation, wins per seat, previous completed game, current presented battlefields, current recorded result, and per-seat battlefield usage.
- `CompletedGame`: result `Won(seat)` or `Draw`, actual starting seat, and presented battlefield names by owner.
- A pure derived `StartPolicy`: first game's real dice winner chooses; subsequent `Won(winner)` gives the other seat the choice; `Draw` fixes the previous first seat. Do not forge a dice outcome to give the loser an existing start UI.
- A pure derived selection policy: unused names after a win; exactly that seat's previous battlefield after a draw. A read-only summary exposes game identity, score, previous result, start policy, remaining/required battlefield policy, and whether the match is complete.

Use a stable ordered collection representation (small sorted vectors suffice). Store only public results and presented battlefield identities. No full hidden deck, secret shuffle state, or unrevealed choice belongs in public `plugin_state`.

1. **Genesis/lobby:** a pinned numeric match option, e.g. `match_wins_required = 2`, enables this behavior explicitly for supported two-seat Match. Absence means existing non-match semantics. Add it to the game crate options' encode/decode/clamping and the SDK rule-options reader; preserve it through kai's option reconstruction and customized score/count controls. Never rediscover it from a display label.
2. **Start:** use one backend helper for both legality and presentation. Check the actor and requested first seat against StartPolicy, verify both decks are present, and verify exactly the expected presented battlefield for each participant is public and legal under the ledger. Capture those semantic names and `TurnCore.first` before setup effects. Carry the existing ledger into the newly started blob. Gate the action itself; disabling a button alone is insufficient.
3. **Finish:** record a result once at the same authoritative terminal boundary that recognizes a point win, concession win, or script win. Existing winner detection is split across `blob.won`, point counters, and concessions, so do not inspect only `blob.won`. The finish/result helper must be idempotent and use the captured starting battlefield selection. The current game's outcome should be visible immediately so the winner banner can show correct game and match scores before Reset.
4. **Next game:** the Reset handler builds a fresh lobby from defaults, preserving only the ledger and intended mode/options. Clear every per-game card reference, roll, prompt, setup/mulligan state, counter-like plugin field, priority/chain/queue, trigger/delayed/prevention state, conceded list and winner. Do not maintain a hand-written growing list of fields to clear. Copy the completed result into previous-game history, increment game identity once, clear current result/selection, and derive the next start policy. Do not award a second win while doing this.
5. **Refusal:** an unfinished enforced Match and an already-reset lobby refuse next-game Reset. Duplicate NewGame requests therefore cannot consume another game or erase the new lobby. Generic HostSession preserves the error and clears no private caches when the Reset append is refused. Kai must increment/reset its caches only on an actually applied transition.
6. **Match completion:** do not start an imaginary game four or carry forbidden battlefields forever. For this bounded task, expose match completion and make “next game” unavailable; the existing leave/create-table route can begin a fresh match. If “new match on this same connection” is desired, it needs an explicit separately reviewed transition, not silent clearing of the completed score when next-game Reset is clicked.

Only running matches track a recorded terminal result. Free-table escape must not accidentally erase an already recorded result or change the previous game's winner after the fact. Decide the exact free-mode policy explicitly: continuing a rules-enforced Match after freeing the table must retain its existing terminal result; ordinary free tables keep their current reset behavior.

## Battlefield authority and the bounded ingress solution

For the requested rematch behavior, the smallest authoritative checkpoint is StartGame: read the revealed faces already in the game state, match ownership, reject reused/incorrect names, and capture accepted selections. Kai filters its chooser with the same backend summary and shared semantic-name helper. The AI chooses only from that filtered list. A modified client can still *submit/place* a prohibited battlefield through existing generic DealDeck, but cannot start a valid next game with it. Do not claim the backend rejected the placement itself if only StartGame rejects it.

Do not make a new rejection in `Action::Reveal` the only validation: `HostSession::deal_groups` currently appends multiple entries incrementally; a rejection after Deal can leave a partially placed hidden card and old caches. Placement-level refusal requires an atomic planned deal/preflight or a dedicated validated setup command, and is a separate bounded backend extension if the desired UX requires it.

There are two pre-existing rule gaps beyond the requested reuse fix:

- Sequential public DealDeck reveals let the later player see the first choice; Core486.5 requires simultaneous presentation. Correcting it needs selection commitments/private staging, not putting unrevealed choices in the public blob. Track it explicitly if claiming full setup compliance; it is not solved by a next-game ledger.
- The engine does not hold or verify the original three-battlefield registration, or full original deck/sideboard composition. Checking against used semantic names prevents reuse but cannot prove a new name came from the original set-aside pool. Likewise a UI lock after Draw is not backend proof that no hidden-deck sideboarding occurred. Full registration/commitment validation is separate scope. Do not silently add a public full-deck list to solve it.

The rematch acceptance must distinguish these from the guarantee actually delivered: no valid enforced next game starts with a used battlefield, and ordinary host/joiner/AI clients offer only allowed selections. Draw transition policy is specified and tested, but no new negotiated-draw flow is invented.

## Kai and AI consumption

Read the authoritative summary from the already mirrored plugin bytes using a small public read-only match accessor exported by agni-riftbound-turns. Kai already directly depends on this game crate. This avoids a new generic PluginView schema solely for game-specific fields. Keep visible start choices in PluginView's existing opaque `Game` affordances, generated from StartPolicy; the UI and brain should press those exact bytes.

Key ephemeral UI reset state by the log/session identity plus authoritative game generation. `DealGeneration` remains useful for card entities but is not the match identity. Joining mid-game should initialize from the live summary without treating the welcome replay as a new-match request; Clear/reload must not advance match history or automatically reset every player's selection.

On a confirmed next-game transition, clear the local battlefield index, `battlefield_played`, placement deduplication key, open tray selection, stale winner-dialog dismissal, and per-game AI decision/hold/roll caches. Preserve the held deck content for sideboarding and repeat play. After a win the chooser opens on allowed names; after a recorded draw the previous battlefield is fixed and sideboard edits are disabled by policy. Reorder/alternate-print tests must use semantic names, not persisted array indices.

The desktop may deal its body before choosing the battlefield, following its existing split path. AI must not set `auto_deal = false` after `deal` reports “choose a battlefield first”; select/await a permitted choice, then send the appropriate plan and track acceptance/refusal. The simplest random/auto AI fallback picks the first legal candidate in stable deck order. LLM state lists only allowed battlefield choices and the previous result/start entitlement; keep game-level memory reset separate from these match facts. A held `--battlefield` value from game one is a preference at most, never an override of the next game's exclusions.

Use “next game” while a match continues, display the score and next chooser, and display match completion distinctly. Concede and card-script wins must lead to the same path. Existing `ClientMsg::NewGame` still routes through the host; user-facing requests do not themselves mutate client state.

## Batches and ownership

### R0: isolated rules/state module foundation

Own only new `games/riftbound-turns/src/match_state.rs`, a new test file, and the review note. Implement pure MatchState transitions, serialization, StartPolicy, name-based battlefield rules, and a documented hook contract. Do not touch state.rs, lib.rs, present.rs, or engine files while recovery is editing them. A completely unreferenced module cannot run in the crate: either integrate its one module declaration at a serialized checkpoint or use a temporary isolated harness and explicitly report that limitation. Do not call an uncompiled module a completed backend.

Separate disjoint work can prepare `games/riftbound/src/lib.rs` match option and its tests, but its public TableOptions changes affect kai literals and should land as a coherent checkpoint.

### R1: serialized authoritative plugin integration

After recovery, own `state.rs`, `lib.rs`, `rules.rs`, `present.rs`, the necessary small `engine/mod.rs` / `engine/cleanup.rs` terminal hooks, and tests. Reserve one unused nested CBOR key, bump from the **then-current** blob version, preserve the ledger through StartGame/Join/Reset paths, and replace both erasing Reset handlers with one helper. Finish match option propagation, actual-start validation, immediate idempotent result recording, and authoritative next-player offers. This is the minimal backend product checkpoint; no kai behavior should be sold as authoritative before it passes.

### R2: session/replica contract and reset regressions

Own new agni-net integration tests and only necessary generic reset fixes in `net/src/host.rs`/`client.rs`. Drive the real Riftbound plugin through HostSession reset, replay, and reconnect. Keep all Riftbound rules out of generic session code. Verify dealer caches, owner faces and new card IDs are correct, refusals do not clear valid state, and Reset entries reach every replica.

### R3: kai and AI consumers

Own `kai/src/net/mod.rs`, `src/deck/battlefield.rs`, `src/deck/import.rs`, `src/table/winner.rs`, affected table-menu/sideboard consumers, `src/ai/driver.rs`, `src/ai/brain.rs`, and `src/ai/local.rs` tests; shared backend accessor must be settled first. Bind option selection, cache resets and chooser/AI filtering to the actual ledger. This worker can avoid the play-location and Firefox files, but no two workers should edit shared plugin_ui/driver modules without explicit ownership.

## Required validation

- Pure serialized transition: 0-0 -> seat0 win -> game2 1-0, seat1 may choose either first/last; seat0 cannot choose. Repeat with seat1 winner, point/script/concession routes, and both choices.
- Win is credited exactly once before Reset; replay, repeated view calls, repeated terminal cleanup, Reset and duplicate NewGame cannot add another win.
- Recorded Draw preserves score and first seat, consumes neither player's battlefield, requires those same names next game, and produces the no-sideboard policy. Unfinished Reset is refused rather than labeled Draw. No production-draw claim without a real draw producer.
- After a won game both owners' names are excluded. The opponent's copy of the same named battlefield does not corrupt ownership accounting. Reordered decks/changed artwork or print IDs cannot bypass the restriction. Created Brush/Baron Pit cannot contaminate usage history.
- StartGame itself rejects the wrong actor/first seat, missing deck, missing/extra battlefield, hidden/unverified chosen face, and reused/wrong battlefield; returns no setup/mulligan/draw/channel effects on refusal. A manually constructed client intent must fail identically to a UI intent.
- One dirty blob containing all current transient queues/references resets to exactly a fresh game's state plus ledger and retained options; round-trip after reset/start and compare real wasm/native bytes/effects/views. Extend this test when recovery adds state fields.
- First game still genuinely rolls; game2 does not ask for another opening roll. A draw's fixed first seat offers no opportunity to alter it. Setup retains the correct `TurnCore.first` for the extra first-channel rule.
- Match wins at two; next-game is not offered beyond completion. Duel/free-form/MTG/multiplayer behavior is not accidentally turned into 1v1 Match.
- Host and joiner replay the exact reset/redeal/start log; reconnect after Reset and during game2 restores match score, previous result and used names. Clear/reload does not advance game identity. Rejected Reset leaves host dealer and client faces intact.
- Extend `kai::ai::local::tests::the_seat_deals_its_deck_again_after_the_hosts_new_game`: its current unconditional mid-game reset expectation is inappropriate for enforced Match; retain it as a non-match lifecycle test and add a genuinely completed game -> legal new battlefield -> loser start choice -> second game test, using the hardened plugin. Assert BF identity changed, not just that a second DealDeck was sent.
- Human-host/AI-loser and AI-host-equivalent/human-joiner-loser choices; AI does not deadlock before choosing, reuse its game1 index, or mark a refused deal successful. Winner dialog and per-game tray state reset on actual game transitions only.

Run the affected engine suites, scoped formatting, warnings-denied clippy and wasm check per the project plan; use the existing real-plugin parity lane for cross-request coverage. No golden rewrite or ABI/WIRE bump is necessary for an opaque plugin-state addition alone. Bump the blob version and rebuild/harden/pin the plugin. If implementation expands generic view/action/face/transport fields, reassess ABI/WIRE and golden versions at that point rather than pretending the expansion is schema-free.

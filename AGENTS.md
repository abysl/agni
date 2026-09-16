# agni — Agent Rules

Deterministic card game framework over spirit. Read
[`wiki/design/architecture.md`](wiki/design/architecture.md) before writing
simulation code. The Bevy renderer lives in its own app,
[`kai`](../../agni/kai/README.md) — renderer rules are in its
`AGENTS.md`, not here.

## The constitution

agni is the core engine: logic, networking, plugins, card importers and game
simulation for multiple card games (Riftbound, MTG, abyss-walker). Clients
(kai today) handle UX, rendering, QR codes — anything not directly part of the
card game framework. spirit stays the generic content mesh substrate UNDER
agni (identity, blobs, gossip, refs, gateway) and carries no game knowledge;
agni registers its own protocols on spirit's router.

The shipped engine is neutral: no rules enforcement and no card content ship.
Rules live in user-installed plugins (the `plugins/` surface); card content
arrives via user-run importers against third-party public services
(`importers/`). Every fetched asset lands in the spirit store under a journal
(`art::ASSET_JOURNALS`) and is advertised to the mesh through the node's
`assets` index (`asset_gateway::publish_index`); a consumer asks the mesh
first and fetches a URL only when no node holds the blob — the deck gateway's
`asset` resolver (`importers/src/asset_gateway.rs`, allowlist
`art::ASSET_HOSTS`) does that for browsers. Do not commit card assets to the repo or bake them into
builds.

## Current scope

Table state, the deterministic action log, the session/transport stack, the
Scryfall importer, and skeleton crates for plugins and per-game deck shapes.
There is no rules engine and no opponent. The game crates hold data shapes
only — grow them when an importer or deck model needs them, not before.

Spirit is implemented incrementally, as agni needs it. Do not build spirit
features ahead of demand.

## The rule that matters

The simulation must be reproducible: same initial state plus same ordered inputs
produces the same result everywhere. In `agni-core` and `agni-sim` that means
no `HashMap`/`HashSet` iteration reaching a decision, no floating point, no
wall-clock or environment reads, no unseeded randomness, no
allocation-order-dependent behaviour.

If a change makes determinism harder to guarantee, say so in the review summary
rather than absorbing it quietly.

## Layout

| Crate | Path | Role |
|---|---|---|
| agni-core | `core/` | game-agnostic table state; no Bevy dependency, ever |
| agni-sim | `sim/` | the deterministic action log (`log`), the wire face/zone types it embeds (`wire` — `WireFace` and `WireZone` are `agni_core::CardFace`/`Zone`), the mirrored `view`, the CBOR `abi`, the `engine` hosting seam (`ModuleCall` → `AbiEngine`/`AbiPlugin`, gas budgets) and genesis module `pins`; pure, no I/O, no Bevy. Golden CBOR vectors live in `sim/tests/fixtures` and `net/tests/fixtures`; regenerate with `AGNI_GOLDEN_WRITE=1` only alongside a version bump |
| agni-engine-wasm | `engine/wasm/` | the engine.wasm guest: agni-sim's fold, Decider dispatch, snapshot/restore and view builder behind zero-import CBOR exports (`abi_version`/`alloc`/`dealloc`/`fold_entry`/`fold_log`/`decide_request`/`snapshot`/`restore`/`view`); shared ABI types live in `agni_sim::abi`, the mirrored view in `agni_sim::view`, the host traits and `fold_mediated` trampoline in `agni_sim::engine` |
| agni-engine-host | `engine/host/` | wasmi 1.x hosting: `WasmEngine`/`WasmPlugin` speak the ABI, write per-call gas budgets into the hardened `gas_left` global (sentinel trap = deterministic reject for plugins, fault for the engine), and `ModuleSource` is the loading seam — `FileSource` for bundled assets, `StoreSource` resolving a module out of the `modules/<name>` version collection in a spirit store: the newest version whose content attestation is signed by a DGID trusted at cache level or above, filtered by `abi_version` |
| agni-net | `net/` | multiplayer over spirit: `proto` (the versioned CBOR wire messages — `WIRE_VERSION`, `ClientMsg`/`HostMsg`, `WireIntent`), `host` (the `HostSession` sequencer; every mutation returns `Result<_, SessionError>` and `intent` decides a hand play on a preview with the face revealed, then appends reveal and move, so a plugin refusal never leaks a face and the decider sees the card's cost; `owed_faces` sends a seat every face it is owed for unrevealed cards in its owner-visible zones, draws by plugin effect included, plus every face the fold's `peeks` grant it; `reveal_surfaced` pays the fold's `owed_reveals` with a `Reveal` entry from the card's owner; `serve_module`/`module_frames` answer a joiner's `NeedModule` with the genesis-pinned engine and plugin bytes in `MODULE_CHUNK_BYTES` `Module` frames, or a `NoModule` refusal for any other hash), `client` (`ClientSession`, and `ModuleInbox` which reassembles and blake3-verifies those frames before the join folds under them), `pins` (genesis module pin checks), all re-exported through `session`; `table` (the `spirit-table/1` ALPN transport), `bridge` (the net-to-game event queue and pump loops kai drains). `HostSession::join_as` seats a joiner by its authenticated endpoint id, so a node that already holds a seat reclaims it with no second `Join` entry (a duplicate would fold to `SeatTaken`) and `seat_faces` re-sends only that seat's owner-visible faces; `table::HostEvent::Joined` carries `Connection::remote_id()` to make that identity available. Compiles on `wasm32` — spirit-node with `default-features = false`, tokio limited to `sync`+`macros`, timers/spawns from `n0-future` in shared paths |
| agni-deck | `games/deck/` | the deck shapes every game shares: `CardName`, `DeckEntry<C>` with `push`/`total`/`expand`/`flatten`, and the history `Snapshot` (`SnapshotZone`/`SnapshotCard` with art, kind, stats, domain, `tags` and `signature` as serde defaults; `identity()` hashes zone, key and count only, so tags and art never change a deck's identity); game crates alias their own `DeckEntry = agni_deck::DeckEntry<ResolvedCard>` over it |
| agni-importers | `importers/` | user-run card importers against public services. `naming` (shared name normalisation), `art` (the source-agnostic art core: the content-addressed journal, `fetch_one` for a single card, `fetch_deck_art` for a batch) and `deck` (the game-neutral deck-list machinery: the `Game` trait with its section/identifier/card/deck types, one `TextVocabulary`-driven `parse_text`, one `CardLookup<G>` + `NameIndex` + `Cached`, one `resolve`) are game-neutral; a new game adds a `Game`/`TextVocabulary` impl, never a copy of the parser stack. The MTG importer behind `mtg` (decklist parser, name-normalised catalog with `Cached`/`Layered`) and `mtg-native` (the Scryfall `cards/named` resolver at 100 ms, the `refs/mtg` manifest, `ingest::ingest_deck_art`); the bulk Scryfall set ingester behind `scryfall` (`ingest-scryfall` bin). The Riftbound importer behind `riftbound` (pure parsers/resolvers: PA deck codes v1–v5, text lists, code lists, link extraction — and beside each parser its renderer: `text_list::render` (community `N Name` sections, names merged per print, main sorted by kind/energy/name), `code_list::render` (`SET-NNN-COUNT`, legend/champion/main/runes/battlefields, reprint sets folded to the base print because a bare `OPP-183` is ambiguous, refused with a reason when a sideboard exists), `deck_code::encode_deck` (the PA code, reprints folded through `base_print_of_id`, an unfoldable print named in the error), `link::piltover_url`, and `snapshot::{snapshot, deck}`, the `agni_deck::Snapshot` round trip history and the gateway share; `catalog::CatalogCard` carries `tags`, `signature` (supertype `Signature`), `set_id` and `text`, and `CatalogCard::resolved()` is the one `ResolvedCard` constructor), `riftbound-native` (Riftcodex client with a 1 req/s throttle, `ingest::ingest_deck_art`, and the `ingest-riftbound` and `resolve-riftbound` bins — the bulk/headless path; `resolve-riftbound --text|--code|--code-list|--link` prints the resolved deck in that form instead of JSON; the store manifest's `RiftboundCard.tags` is a serde default, so a manifest ingested before tags loads with empty tags and the legality report says `unverified` until the set is downloaded again) and `riftbound-gateway` (the deck `Resolver` agni registers on spirit's generic gateway as `/gateway/resolve/deck`, owning the deck-site allowlist spirit used to carry) |
| agni-plugins | `plugins/` | `CardScript` + `ScriptRegistry`, keyed by card CIR per spirit's `identity.md` phase 3 — the surface user-installed rules bind to; no interpreter, none ships |
| agni-harden | `plugins/harden/` | the minting pass: gas metering, stack limiting, ABI validation, blake3 identity, and the spirit-store publish leg — `--store DIR --name NAME` mints a `wasm-module` CIR, a `wasm-harden` TDR recording the input hash and every limit (the reproducibility claim: same input plus same config yields the same bytes), and a content attestation signed by the store's DGID, then appends the version to `col:modules/<name>` |
| agni-plugin-sdk | `plugins/sdk/` | the game-agnostic guest toolkit, zero dependencies and no floats so any plugin built on it passes the third-party hardening: `cbor` (an integer-only reader/writer), `decide` (parses the engine's `{plugin_state, state, entry}` into seat, seat count and action kind; encodes the verdict map), `view` (parses the view request with a zone summary; `PluginView`/`Affordance` builders and encoder), `turns` (`TurnOrder` and the pass-in-sequence `PassWindow` that MTG priority passing and Riftbound focus passing both reduce to), `dice` (the commit-and-reveal `Roll`: commitments, verified reveals, replica-identical dice, ties re-rolled), `guest` (`export_plugin!` — the export set, the request/reply buffers); `table` (a `Snapshot` of the engine's state — cards with zone, seat, owner, face stats and the exhausted mark, zones, counters — shared by `decide` and `view`), `decide::Effect` and `Verdict::with_effects` (the engine-applied `Move`/`Annotate`/`Counter`/`Spawn`/`Despawn`, and the information effects `Reveal` — a face owed to the whole table — and `Peek` — a face owed to one seat) |
| agni-riftbound-turns | `games/riftbound-turns/` | the Riftbound turn machine on the SDK: `state::GameBlob` (the versioned CBOR document, `v` 4, written with the SDK `blob` helpers — mode, the lobby roll, `TurnCore` with turn/player/phase, control slots (holder, scored mask, contester), the open showdown with its focus window, the open `Prompt`, the free-table proposal, the last four narration lines; unknown keys skipped, any other first key is a fresh lobby), `TurnEvent` (start game, end turn, commit/reveal roll, pass, pick, activate, set mode, free table — one tag byte plus a little-endian payload) with the refusals the Core Rules imply, each carrying a reason into `Verdict.reason`; `decide` and `present` (the per-seat status lines, affordances and prompt summary the client renders). Free mode is bookkeeping with light enforcement: cards still move freely; the beginning-phase choreography runs inside the entry that starts the turn (`StartGame`, the previous seat's `EndTurn`), a unit's arrival opens a showdown, passes in sequence close it, and `EndTurn` settles every battlefield before handing over; `rules` holds the Core Rules beyond turn order — awaken, hold scoring, channel, draw, rune payment for plays, contest → showdown on arrival, control settled and conquer scored — as pure functions from a `Snapshot` and the blob to effects. Enforced mode is chosen by the roll winner in the lobby ("switch to …" toggles `SetMode` beside the start affordances), stored in the blob and shown on the strip; every entry then goes through `engine/` (`ctx` — the projection that applies the incoming entry to the `Snapshot` and mirrors every effect it emits; `legal::classify` — the free-form Move to intent-or-reason table; `cost`/`pay` — printed cost plus Accelerate and the rune planner; `play` — hand and champion plays with location and optional-cost prompts, units entering exhausted, gear to base, spells resolving at once until the chain lands in M2; `march` — standard moves, the group-move prompt, the two-other-seats cap and contest marking; `cleanup` — the rule-322 pass after every action (win check, lethal kills, control settled, contests staged, the next showdown opened or `PickStaged` asked) plus establish/Conquer/Hold with the once-per-battlefield-per-turn and final-point rules; `showdown` — the four turn states, staging, focus, `Pass` walking the ring with the empty-hand auto-pass, close → establish (a closed combat returns the attackers home until damage lands in M3); `phases` — Setup with the mulligan, Awaken → Beginning (Hold) → Channel → Draw → Action and Ending/heal/Expiration inside one `EndTurn`; `roll` — the in-game commit-reveal roll (a `Shuffle` prompt holding the cards plus `blob.roll`, `CommitRoll`/`RevealRoll` routed to it while open, the pooled secret seeding xorshift64 for a replica-identical permutation sunk to the deck bottom; the two-card mulligan recycle is its caller); `prompts` — one numbered option list shared by decide and present, the status line per `PromptWhy`, `Pick` resolution and the single-option auto-answer, plus `answer_words(why)`, the words a brain's tool text must name per prompt kind, iterated over `PromptWhy::each()`, which decodes one sample of every tag the writer knows, so kai's brain test is derived rather than hand-typed) and `cards/` (the static `Card` script vocabulary of the design doc, the registry with the per-kind generic fallback, `Rockfall Path`); `FreeTable` (proposed by the turn player, confirmed by another seat, the one event still accepted once a seat has won) drops back to free and clears any open prompt; the enforced strip shows the prompt and its options (or `pass` for the focus holder, `end turn` in Neutral Open), `waiting for {seat N}: <what>` for everyone else, held/contested per battlefield, the chain, the showdown line and the last four narration lines, while the free strip stays as M0 left it. Decks must be dealt before the winner starts: the first turn's draw and channel run inside `StartGame` |
| agni-riftbound-plugin, agni-mtg-plugin | `plugins/riftbound/`, `plugins/mtg/` | the two guest wasm plugins on the SDK's `export_plugin!` macro — a `build.rs` bakes the manifest CBOR from the matching game crate; riftbound's `decide` and `view` run the turn machine in `games/riftbound-turns`, mtg's answer accept-all and an empty view until its rules land. Built for `wasm32-unknown-unknown`, hardened by agni-harden, bundled into kai and seeded as versions of the `modules/riftbound` / `modules/mtg` collections |
| agni-riftbound | `games/riftbound/` | Riftbound deck anatomy as types (legend, chosen champion, 40 main, 12 runes, 3 battlefields, sideboard), the pinned zone table (`zone_table()`) as agni-sim `ZoneDecl` data — hand, base, rune pool, both decks, legend, champion, trash, sideboard and the shared battlefields, each carrying the `ZonePlace` band and `span` weight a renderer lays out from — the `deal_plan` choreography, the resolved deck shapes importers emit (`ResolvedCard` with riftbound_id, image URL, kind, stats, domain, Riftcodex `tags` and `signature`, `Default` so literals say `..Default::default()`; `DeckEntry`, `ResolvedDeck`), and `legality`: the deck-construction rules of Core Rules §103 as a pure `check(&ResolvedDeck, Mode) -> Report` with no I/O — `Verdict` (Legal / Broken(n) / Unverified), the `Meter` counts, and one `Finding` per broken rule carrying the `Rule` variant, the rule number as printed in `rules/riftbound-core-rules-v2026-07-16.txt`, a `Grade` (Break, Unverified when tags are missing, Advisory for kai conventions such as an oversized main deck or a sideboard card that would break on swap), the zone and the riftbound ids to point at. Helpers: `identity` (the legend's domains), `fits_identity` (empty or Colorless always fits, a multi-domain card needs every domain), `champion_tags` (the legend's tags found in its name stem, else all its tags), `champion_unit` (a non-signature unit tagged with its own name stem — exact on every print of the 2026-09 dump, `None` without tags), `rune_split` (12 runes across the identity, remainder first). This is deck construction only; play stays free-form |
| agni-mtg | `games/mtg/` | MTG deck anatomy as types (commander, 60-card main deck, sideboard), the pinned zone table (hand, library, graveyard, exile, battlefield, command), and the `deal_plan` choreography (commander to `command`, library shuffled with a seven-card draw) |
| agni-abysswalker | `games/abysswalker/` | abyss-walker deck-shape placeholder |

`agni-core` and `agni-sim` must not depend on Bevy — the e-ink firmware
consumer cannot take that dependency, and the rules have to run there too.
Renderers (kai today, Godot and e-ink later) depend on agni, never the
reverse. Game crates hold deck-shape data for importers and, for Riftbound,
the pure deck-construction check (`agni_riftbound::legality`) the deck editor
and the import step read; they never enforce play: the table stays free-form
during a game, and in-game legality belongs to plugins a user chooses to
install.

## Dependency on spirit

`spirit-node` (via agni-net and the importers' gateway feature) is an in-repo
Cargo **path dependency**, so a breaking change to spirit's surface breaks
this build the moment it lands. `agni-core` itself depends on nothing but
serde: `spirit-sdk` is a dev-dependency there, and `core/src/lib.rs` has a
test that only compiles if that dependency really links — do not delete it
to quiet a build. Clients that need spirit's store (kai) depend on
`spirit-core` directly rather than reaching it through agni.
spirit must never grow a dependency on agni; extra protocols reach spirit's
router through the `serve_with`/`serve_mesh_with`/`serve_in_memory_with`
closures, with the caller supplying the handler.

## Dev Environment

devenv provides the toolchain; there is no system-wide cargo.

```
direnv allow          # or: devenv shell
build | unit-test | clippy | fmt | fmt-check
```

- The test script is `unit-test`, never `test` — bash resolves `test` to its
  builtin, so a script by that name is unreachable.
- `fmt` and `fmt-check` format the WHOLE monorepo, from the root `treefmt.toml`.
- The end-to-end iroh smoke test is `#[ignore]`d; run it with
  `cargo test -p agni-net --test net_smoke -- --ignored`.

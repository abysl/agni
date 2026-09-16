# Code review — agni + kai, September 2026

> Scope: every crate under `agni/agni/` (core, sim, net, engine/wasm,
> engine/host, importers, plugins/* including harden and guests, games/*)
> and all of `agni/kai/src/`. Reviewed at
> `acf6599d` (card-art format sniffing), re-merged onto `bb5b6788`.
> Lenses, per Rae, all complete: DRY (§3),
> single-responsibility architecture (§4), file size/tangle (§1–2), Rust
> best practices (§5), test architecture (§6), dependency and build
> weight (§7), plus the executable kai reorg map (§8). Both workspaces
> were linted with `clippy::pedantic` and the result triaged in §5G — the
> raw 1,061 warnings are ~85% noise for this codebase, and the residue is
> named. Severity scale:
> **structural** (wrong shape, will hurt growth), **hygiene** (worth
> fixing, not urgent), **nit** (fix when passing). Effort: S (<1h),
> M (half a day), L (a day+).
>
> Two agents are landing work concurrently with this review, so several
> findings describe a target shape rather than the tree as it stands:
>
> - **[MTG⚡]** — an MTG plugin, a game selector, and a source-agnostic
>   art core with on-the-fly per-card fetching, touching kai
>   `import.rs`/`net.rs` and agni `importers/`, `games/`, `plugins/`.
>   Findings so marked intersect it directly; coordinate before
>   executing them, and read §3.4 first — it is far cheaper before that
>   branch merges than after.
> - **[ZONES⚡]** — riftbound base/rune-pool zones, a riftatlas-matching
>   layout, and a sideboard popup, adding `kai/src/sideboard.rs` and
>   reworking `kai/src/zones.rs`. It lands inside the §8 reorg's blast
>   radius; §8.0's ordering is written to survive it.

## Top 10 — do these first

Ranked by cost of delay first, then by leverage over other findings, then by
effort. Six of the ten get materially more expensive once the two in-flight
branches merge.

> **Before any of it, a fifteen-minute fix that is costing real time now.**
> The three copies of the wasm test bootstrap (`engine/host/tests/host.rs:26-38`,
> `net/tests/wasm_session.rs:25-37`, `net/tests/riftbound_mvp.rs:39-51`) shell
> out to `cargo build --release` with **no `-j` cap** whenever
> `AGNI_ENGINE_WASM`/`AGNI_RIFTBOUND_WASM` are unset. A bare local
> `cargo test -p agni-net` therefore fires up to three nested unbounded release
> builds. CI sets both vars so CI is safe; local development is not, and this
> is the most likely cause of the machine falling over mid-review twice.
> Export both in `devenv.nix` and make the fallback panic with the fix rather
> than build. §6.5.

| # | Do | Findings | Why first | Effort |
|---|---|---|---|---|
| 1 | **Build the game registry — one `Game` trait (`id`, `zone_table`, `deal_plan`) plus the shared `ResolvedCard`/`DeckEntry`/`DeckFaces` family — and route riftbound through it.** | §3.4, §2.2, §8.2, §6.4, §7.7.1 | Highest cost of delay in the review. `agni-mtg` clones agni-riftbound's whole type family *this week*; abyss-walker is third. It is also the single seam five other findings are waiting on: net.rs's four hardcoded `agni_riftbound::` references, the import panel's by-name import, the acceptance suite's inability to take a second game, and the empty game stubs. Landing it **before** the MTG branch merges doubles nothing; landing it after triples it. | M |
| 2 | **Stop letting a misbehaving module kill the client.** Thread `EngineFault` out of `append`/`intent`/`apply` as a `Result`, and stop discarding `append`'s existing `FoldError`. | §2.3, §5A.3, §5B.3 | Eight `.expect("engine answers …")` and ten `.expect("… folds")` in `net/src/session.rs` turn a sandboxed guest's misbehaviour into a process abort — in kai, the whole window. That is precisely the failure the sandbox exists to contain, and the plugins design already specifies the correct response ("stop folding and declare the plugin broken"). Also the prerequisite for ever telling a player *why* a drop bounced. | M |
| 3 | **Check in golden CBOR vectors and a golden RNG sequence.** | §6.3.3, §6.3.4, §6.3.5, §6.3.7 | Every byte-stability test in the tree is a round-trip, so a `#[serde(rename)]`, a field reorder, a variant insertion or a ciborium bump changes both sides together and every test still passes — while every recorded log becomes unreadable and every genesis pin shifts. `core/src/rng.rs:44` asserts `from_seed(42)` equals itself, so changing the xorshift constants silently reshuffles every deck ever dealt. Seven `.cbor` files, eight literal RNG outputs, one pinned permutation, one pinned fuel count. **The cheapest high-value work in this review**, and it protects the project's entire premise. | S |
| 4 | **Collapse net.rs's broadcast and refresh duplication, and make `refresh` compare before assigning.** | §2.2, §5C.1, §8.0 step 1 | The broadcast loop appears eight times and the refresh pair eleven; two helpers delete ~90 lines. It must land *before* §8.2 cuts `net.rs` into five files, or the duplication is moved into five new homes instead of deleted. The compare-before-assign half restores meaning to every downstream `Res<Mirror>::is_changed()` guard. | S |
| 5 | **Quantize `tint` to `[u8; 3]` and delete `art_path`, in one log-format revision.** | §4.2, §3.7, §5D.3, §6.3.6, §4.13 | Four findings collapse into one change. It is the *sole* reason `HardenConfig::engine()` must set `allow_floats: true`, so the engine module currently fails the float ban agni imposes on every third party. It makes `CardFace`/`Card`/`Table`/`WireFace` `Eq + Ord + Hash` — today a deterministic log's central type cannot be a map key. `art_path` is never set to `Some` anywhere yet serializes as a permanent `null` into every log and wire message. Add `CardFace::named` + `Default` first so this is a one-line edit rather than a 20-site sweep. Cost of delay: format revisions get dearer as recorded logs accumulate. | M |
| 6 | **Hoist `seat_center` and `seat_yaw` (lib.rs:424, :441) plus `dim` into `render/layout.rs`, in their own commit.** | §8.0 step 2, §4 cross-lens | The blocking prerequisite for the entire kai reorg, and it is small and additive. `zones.rs` — the one exemplary file in the client — computes seat math against them; if `zones.rs` moves first it either breaks or grows a `crate::` back-reference that defeats the move. It also makes the in-flight zones/layout branch *easier*, so do it now and tell that agent. | S |
| 7 | **Swap `std::sync::Mutex` for `parking_lot::Mutex` across the six static-holding modules.** | §5B.1 | 73 `lock().unwrap()` sites, 55 of them on process statics, every one guarding a global — so a single panic inside any lock poisons it permanently and every subsequent frame panics on the same line, killing a Bevy app one frame later with a stack trace pointing at the wrong place. One dependency, mechanical, removes the whole class. §3.9's status board and §4.1's `NetBridge` then delete most of the statics outright; the two are complementary, not alternatives. | S |
| 8 | **Add a `cargo check --target aarch64-linux-android` lane to `.woodpecker/kai.yml`.** | §5E.2, §6.7.3 | `sync.rs` plus the android arms of `telemetry.rs`, `app.rs`, `lib.rs`, `node.rs`, `identity.rs`, `import.rs`, `modules.rs` and `net.rs` are compiled only by `agni-artifacts.yml`, which runs *after* merge to main and is skipped entirely when the `kai_android_signing` secret is absent. **A commit that breaks the android build merges green today.** The check needs no signing key. The Rust and test lenses reached this independently. | M |
| 9 | **Collapse the two wasm engine hosts and the four pin state machines.** | §3.1, §3.2, §6.3.2 | `kai/src/engine_web.rs` and `engine/host/src/lib.rs` maintain the fold path's ABI call sequence twice, with all seven `impl Engine` methods byte-identical — on the one code path that must agree byte-for-byte between desktop and browser. `modules.rs` holds four hand-maintained copies of the pin→resolve→load decision that gates whether a peer may enter a table at all. ~300 lines, and **nothing would catch the drift**: there is no native ↔ browser determinism test. A `ModuleCall` trait in `agni_sim::engine` plus one generic `resolve_pin` driver. | M |
| 10 | **Make `agni-importers`' default `[]`, and drop `spirit-sdk` from `agni-core`'s normal dependencies.** | §7.3.1, §7.1.1, §7.4 | Two one-line manifest edits with outsized effect. The importers default enables `riftbound-gateway` for a single call site, pulling the 307-crate iroh stack and taking the crate from 28 to 322 crates — it sets the `cargo build` cost floor for the whole workspace. `agni-core`'s `spirit-sdk` is an unused re-export behind an empty-bodied test canary that costs 18 crates, reaches 8 of 12 agni crates, compiles `cpufeatures` and blake3's build script **for wasm32** inside a zero-import guest, and gives `spirit-sdk` the same rebuild blast radius as `agni-core` itself. kai takes `spirit-core` directly at its four use sites instead. | S |

**Just below the line**, in order: the transport layer is untested in CI
because `net_smoke.rs:41` is `#[ignore]`d and nextest is not given
`--run-ignored` (§6.7.1, M) — it is the only test touching `net/src/table.rs`,
`net/src/bridge.rs` and real iroh; the `agni-testkit` crate (§6.5, M), which
items 1 and 3 both eventually want; §3.9's `StatusBoard` collapsing five
status cells in three storage idioms (M); §3.5's three manifest-decode and
two seeded-deal copies in kai (M), best written against the incoming art
core rather than before it; §5D.2's missing wire-protocol version, which is
cheap now and impossible to retrofit politely later (M); and §4.4's two
independent implementations of the hidden-face mask, whose parity test turns
out to exist but to be weaker than the invariant it audits (§6.3.1, M).

## 1. File census

Line counts at review time, worst first — with the code/test split, because
for three of these files the test bulk is the *entire* reason they appear
here. Verdicts per file below; the code column is what the §2 split
proposals are sized against.

| File | Total | Code | Test | Verdict |
|---|---:|---:|---:|---|
| kai/src/lib.rs | 1806 | 1629 | 177 | **split** — 6+ responsibilities |
| agni/net/src/session.rs | 1713 | **886** | 827 | **split** — 4 responsibilities, but only 886 lines of them |
| agni/sim/src/log.rs | 1103 | **496** | 607 | OK — single responsibility; the census number is tests |
| kai/src/modules.rs | 1005 | 442 | 563 | **split** — two full platform impls + panel in one file |
| kai/src/net.rs | 972 | 837 | 135 | **split** — host+client+routes+panel+platform shims |
| kai/src/import.rs | 952 | 732 | 220 | **split** — parsing+faces+art+dispatch+panel [MTG⚡] |
| importers/riftbound/deck_code.rs | 769 | ~430 | ~340 | OK — one codec, v1–v5 |
| kai/src/telemetry.rs | 577 | 577 | **0** | borderline — see layering lens; and §6.1 |
| plugins/harden/src/lib.rs | 496 | 496 | 0 | OK — covered by 15 tests in `tests/harden.rs` |
| importers/riftbound/ingest.rs | 472 | ~330 | ~140 | OK |
| games/riftbound/src/lib.rs | 465 | ~370 | ~95 | OK — data shapes + deal plan |
| kai/src/zones.rs | 456 | 196 | 260 | OK — pure, tested, exemplary |
| engine/host/src/lib.rs | 405 | 342 | 63 | OK |
| kai/src/engine_web.rs | 404 | 404 | **0** | OK on shape (duplication findings apply); zero tests, §6.1 |
| everything else | <360 | | | OK |

Two corrections to the numbers this table carried in the earlier draft:
`session.rs` is **886** lines of code, not 723, and `log.rs` is **496**, not
497 — measured from each file's `mod tests` boundary (§6.2). The three
`Code = 0`/`Test = 0` rows are the ones §6.1 flags: `telemetry.rs` and
`engine_web.rs` are the two largest untested files in either tree.

## 2. The four split proposals

### 2.1 kai/src/lib.rs (1806) — structural, effort L

Grown all week into six distinct jobs: plugin/app wiring, resources +
messages, scene/camera setup, card-entity lifecycle + materials, layout
math, pointer/hotkey input, and five egui overlays. Concrete split
(names align with the queued `render/` folder, section 8):

- `render/mod.rs` — `CardTablePlugin` wiring only (the `build()` body),
  re-exports. ~150 lines.
- `render/state.rs` — `GameTable`, `Mirror`, `ViewSeat`, `MySeat`,
  `PlayerCount`, `SessionInfo`/`SessionRole`, `HandScroll`,
  `DealGeneration`, `Held`, and the messages `CardDropped`,
  `ExhaustToggled`, `Redeal`. The `dim` consts ride along (or a
  `render/dim.rs`).
- `render/scene.rs` — `setup_scene`, `camera_pose`, `zoom_camera`,
  `apply_zoom`, `sync_seats`, `seat_color`, lights.
- `render/layout.rs` — `seat_center`, `seat_yaw`, `deal_origin`,
  `hand_slot`, `board_slot`, `fan_back_slot`, `rotation_for`,
  `layout_cards`, `Facing`, `Slot`. `zones.rs` becomes
  `render/zones.rs` beside it.
- `render/cards.rs` — `sync_cards`, `FaceKey`/`face_key`,
  `wants_label`, art decode, foil spawn path, `sync_hand_backs`,
  `CardArt`/`FoilArt`/`FoilBody`/`OpponentHand` components.
- `render/animate.rs` — `animate_cards`, `apply_foil_alpha`,
  `hide_viewed_hand`.
- `render/input.rs` — the pointer observers (`on_hover_card`,
  `on_unhover_card`, `on_click_card`, `on_drag_*`, `on_drop_*`),
  `hotkeys`, `guarded_from_me`, `my_hand_ids`.
- `render/overlays.rs` — `card_label_ui`, `label_lines`,
  `zone_overlay_ui`, `preview_hud`, `hand_scroll_ui`,
  `seat_buttons_ui`.

The facing tests split along the same lines (layout tests with layout,
mirror test with state). Nothing crosses a crate boundary; this is a
pure file move plus visibility tightening (most of these systems can
drop from crate-visible to module-private once the wiring lives beside
them).

Local findings while reading:

- **DRY, hygiene, S**: `sync_hand_backs` (lib.rs:1599–1607)
  re-implements `fan_back_slot`'s arc math (lib.rs:773–784) —
  `x*x*0.012` droop, `HAND_NEAR`, `HAND_TILT - 0.5` — inline. Call
  `fan_back_slot` and apply the transform.
- **DRY, hygiene, S**: `on_drop_on_surface` (lib.rs:1088) and
  `on_drop_on_zone` (lib.rs:1387) share the axis-projection
  index-counting block verbatim (~10 lines). Extract
  `drop_index(slots, table, to, seat, axis, position) -> usize`.
- **Perf, nit, S**: `Mirror::rotated` is a linear scan over
  `view.cards`, called once per card inside `layout_cards` — O(n²) per
  relayout. Same for `kept.contains(&card.id)` in `sync_cards`. Fine at
  60 cards; a `BTreeMap` index in `Mirror` fixes both if tables grow.
- **Hygiene, S**: `eprintln!` for art-decode failure (lib.rs:580) —
  the rest of the app logs through `tracing` into telemetry; use
  `warn!`.

### 2.2 kai/src/net.rs (972) — structural, effort M

Five jobs in one file: platform shims, the `drain_net` dispatcher,
host-side session handling, client-side join/fold handling, the intent
routing systems, and the multiplayer panel. Split (section 8's `net/` +
`panels/`):

- `net/mod.rs` — `drain_net` (dispatcher only), system wiring,
  re-exports.
- `net/platform.rs` — `player_name`, `keep_awake`, `start_host`,
  `close_table`, `start_join` (all the cfg-forked shims — one home for
  the gates).
- `net/host.rs` — `HostState`, `handle_peer`, `send_private_faces`,
  `deals_legacy_hands`, `deck_already_dealt`, plus the two helpers the
  DRY finding below creates.
- `net/client.rs` — `ClientState`, `PendingWelcome`, `try_finish_join`,
  `handle_host_msg`.
- `net/routes.rs` — `route_drops`, `route_redeal`, `route_annotations`,
  `route_deck_deals`.
- `panels/multiplayer.rs` — `net_ui`, `host_controls`,
  `discovered_tables`, `TableChoice`.

Local findings:

- **DRY, structural, S**: the broadcast loop
  (`for entry in &entries { for &conn in host.conn_seats.keys() {
  bridge::send_to(conn, HostMsg::Entry{..}) } }`) appears **eight
  times** — net.rs:358, 405, 430, 590, 661, 721, 763, plus the roster
  variant at 271. The refresh pair `table.0 = session.table();
  mirror.view = session.view().clone();` appears **eleven times** —
  net.rs:224, 393, 415, 442, 499, 546, 557, 601, 682, 730, 772. Two
  helpers — `HostState::broadcast(&self, entries: &[LogEntry])` and a
  free `refresh(table, mirror, session)` — delete ~90 lines and make
  every future host action three lines. Do this *before* the file
  split; the split gets smaller.
- **DRY, hygiene, S**: the find-conn-by-seat scan is duplicated
  (`send_private_faces` net.rs:115 vs `route_redeal` net.rs:674). One
  `conn_for_seat(&self, seat) -> Option<u64>`.
- **Layering, structural, M [MTG⚡]**: riftbound is hardcoded into the
  client's session flow — `TableChoice { riftbound: bool }` (net.rs:96),
  `agni_riftbound::ZONE_NAME_MAIN_DECK` (net.rs:127),
  `agni_riftbound::zone_table()` fallback (net.rs:191),
  `agni_riftbound::deal_plan` (net.rs:702). The plugins design says the
  manifest is the zone source of truth; the compiled fallback and the
  game toggle should become a game-registry lookup (id → zone table +
  deal-plan fn) so the MTG selector slots in as data, not another
  boolean and another `use agni_riftbound`. `deck_already_dealt` should
  key on `ZoneKind::Deck` occupancy, not a riftbound zone-name string.
- **Hygiene, S**: `handle_host_msg`/`try_finish_join` take 8 `ResMut`
  parameters each; after the split, bundle them in one
  `#[derive(SystemParam)] struct SessionView<'w>` and pass that.
- **Nit**: the `#[cfg(all(test, ...))] mod tests` sits mid-file between
  `host_controls` and `net_ui`; file-order hygiene resolves itself in
  the split.

### 2.3 agni-net/src/session.rs (1713 total, 886 code) — structural, effort M

886 lines of code, 827 of tests, four jobs: module-pin helpers, the
CBOR wire messages, `HostSession` (including the whole deal executor),
and `ClientSession`. Split inside agni-net:

- `net/src/pins.rs` — `engine_blob_ref`, `genesis_engine_pin`,
  `genesis_plugin_pin`, `verify_engine_pin`, `verify_plugin_pin`,
  `pin_hash`, `module_matches_pin` (+ their tests). These are
  module-identity concerns, not session mechanics — kai's modules.rs
  imports exactly this subset today.
- `net/src/proto.rs` — `SeatInfo`, `WireIntent`, `ClientMsg`,
  `HostMsg`, `encode_/decode_client/host` (+ round-trip test). The wire
  vocabulary is what `table.rs`/`bridge.rs` and kai's platform shims
  actually share; today they pull it through `session`.
- `net/src/host.rs` — `HostSession` minus the deal executor: genesis,
  join/disconnect/roster, `append`/`refresh_state`, `intent` (+ the
  auto-reveal pairing), `reset`, `private_faces`, `table`/`view`.
- `net/src/deal.rs` — the deal executor: `deal`, `deal_to`,
  `deal_groups`, `deal_faces`, `draw_dealt`, `deal_seed`, `occupancy`,
  `hand_decl`, `zone_named` — as `impl HostSession` blocks or a
  `DealExec<'_>` borrowing the session. This is the piece that grows
  per-game choreography; isolating it keeps host.rs stable when MTG's
  deal plan lands. **[MTG⚡]**
- `net/src/client.rs` — `ClientSession`.
- `session.rs` stays as a thin re-export module for one release so kai
  and tests don't churn (`pub use` the lot), then callers migrate.
- The 827 test lines split with their subjects; the riftbound-flavored
  deal tests (`synthetic_deck`, battlefield spread, seeded shuffle)
  belong beside `deal.rs` or in `net/tests/` with the other
  integration suites.

Local findings:

- **Error handling, structural, M**: `HostSession::intent` returns
  `Option<Vec<LogEntry>>` — the refusal reason (`FoldError`) is
  discarded, so kai can never tell the player *why* a drop bounced;
  the optimistic overlay just snaps back. Return
  `Result<Vec<LogEntry>, FoldError>` (callers that want the old shape
  use `.ok()`), and give `FoldError` a `Display`. Same for
  `deal_groups` → the kai status line currently guesses ("this table
  has no riftbound zones to deal into") where the session knew exactly.
- **Robustness, structural, M**: `append`, `refresh_state`,
  `from_welcome_with` and `apply` all `expect("engine answers …")` on
  `fold_mediated`/`snapshot`. Per the plugins design, a host-level
  engine fault should "stop folding and declare the plugin broken" —
  today it aborts the process (in kai: the whole app) instead of ending
  the session with a status. Thread `EngineFault` out through a
  `Result` on `apply`/`intent` and let kai's drain loop turn it into
  `SessionRole::Ended` + status. This is the one place `expect` is
  load-bearing wrong rather than merely inelegant.
- **Pub-surface, hygiene, S**: `ClientSession.seat`/`roster` are `pub`
  fields and kai *writes* `session.roster` directly on
  `HostMsg::Roster` (kai net.rs:537). Make them read-accessors plus a
  `set_roster`; a client that can scribble on its own roster mid-fold
  is an invariant leak.
- **Nit, S**: `WireIntent → LogAction` mapping in `intent()` is a
  `From` impl waiting to happen; also used implicitly by validate
  probes.

### 2.4 agni-sim/src/log.rs (1103 total, 496 code) — leave mostly alone

496 lines of code with one responsibility (log vocabulary + validate +
fold) and 607 lines of tests. This file is *fine*; the census number is
tests. Optional, effort S: move most of `mod tests` to `sim/tests/log.rs`
— §6.2 measured it at ~90% movable, with a small inline module worth
keeping for `validate_action`'s error-code coverage — or split
`entry.rs` (types + codec) from
`fold.rs` (validate/apply) if it crosses ~700 code lines when
signatures (phase B) land. Not worth churn now.

- **Nit**: `LogAction::Genesis { .. } => unreachable!()` inside
  `validate_action`'s match (log.rs:194) is correct today (genesis is
  handled above) but a refactor magnet; an early `return` pattern or a
  commentless `debug_assert` shape would survive reordering. Leave
  until touched.
- **Good**: `fold_begin`/`fold_finish` splitting admission vs sequenced
  semantics is exactly the right seam, and the visibility rules
  (face-shedding on hidden-zone entry) are engine-enforced with tests.

## 3. DRY findings

Duplication that crosses a crate or platform boundary, worst first. The
§2 file-local duplication (net.rs's broadcast/refresh, lib.rs's arc math
and drop-index blocks) is not repeated here.

### Structural

**3.1 — The two wasm engine hosts are the same file twice.** *Effort M.*
`kai/src/engine_web.rs` and `agni/engine/host/src/lib.rs` share their
entire ABI call sequence:

| kai/src/engine_web.rs | engine/host/src/lib.rs | Status |
|---|---|---|
| `WebEngine::exchange` :306-318 | `WasmEngine::exchange` :164-172 | identical |
| `impl Engine for WebEngine` :321-371 | `impl Engine for WasmEngine` :175-225 | **all seven methods byte-identical** |
| `impl PluginModule for WebPlugin` :386-404 | :251-269 | identical modulo fault enum name |
| `engine_fault` :281-287 | `fault` :141-147 | identical |
| `WebFault` :171-175 | `CallFault` :15-19 | same three variants |
| `WebEngine::instantiate` :294-304 | `WasmEngine::load` :150-162 | same ABI-version check |

Genuinely platform-specific: only `WebModule` (engine_web.rs:184-279) vs
`WasmModule` (engine/host/src/lib.rs:35-134) — JS `Reflect`/`BigInt`
versus wasmi `Store`/`Global`.

This is not cosmetic. The fold path's ABI call sequence is maintained
twice, so desktop and browser can silently drift on the one code path
that must agree byte-for-byte. Recommendation: declare

```
trait ModuleCall {
    fn call(&mut self, name: &str, request: &[u8]) -> Result<Vec<u8>, CallFault>;
    fn abi_version(&mut self) -> Result<u32, CallFault>;
}
```

with generic `AbiEngine<M: ModuleCall>` / `AbiPlugin<M: ModuleCall>` and
`CallFault` in `agni_sim::engine` — pure, no I/O, constitution-clean.
`agni-engine-host` supplies `impl ModuleCall for WasmModule`; kai supplies
`impl ModuleCall for WebModule` and drops ~150 lines.

**3.2 — Four copies of the pin→resolve→load→`JoinModules` state
machine.** *Effort M.* `kai/src/modules.rs`: native engine :369-400,
native plugin :402-430, wasm engine :826-878, wasm plugin :880-925. Same
`match genesis_*_pin(log)` → parse hash → resolve bytes → load → note /
`Refused` / `Pending` shape; they differ only in the loader, the literal
`"engine"`/`"plugin"`, the gas budget, and the byte source. Four
hand-maintained copies do not guarantee desktop and browser make the
*same* join/refuse/pending decision — and that decision gates whether a
peer can enter a table at all. Recommendation: one generic driver

```
fn resolve_pin<T>(pin: Option<String>, fetch: impl Fn([u8;32]) -> PinState,
                  load: impl Fn(&[u8]) -> Result<T, String>,
                  kind: &'static str) -> PinOutcome<T>
```

in the platform-neutral part of `modules.rs`, called four times with
closures. Deletes ~150 of that file's 1005 lines. Pairs with §8.4's
`trait ModuleSource`.

**3.3 — The gateway deck-reply JSON has two hand-written mirror codecs
in two crates.** *Effort S/M.* **[MTG⚡]** Serializer:
`importers/src/riftbound/json.rs:5-50` (`card_json`, `zone_json`,
`deck_json`). Deserializer: `kai/src/import.rs:92-155` (`card_of`,
`entries_of`, `parse_reply`), hand-indexing
`value["deck"]["chosen_champion"]`. One wire format, two hand-rolled
halves, no shared type, and no test spanning them — json.rs:59-77 tests
one side, import.rs:762-793 the other, and neither would catch a
disagreement. Recommendation: a serde `DeckReply` type pair in
`agni-importers`; `deck_json` becomes `serde_json::to_value`, `parse_reply`
becomes `serde_json::from_str::<DeckReply>`, one round-trip test, ~90
lines gone. Urgent: `import.rs:127-134` hardcodes the riftbound zone
names (`legend`, `chosen_champion`, `runes`, `battlefields`) *as* the
reply schema, so MTG doubles this rather than reusing it.

**3.4 — `agni-mtg` clones `agni-riftbound`'s entire type family, and it
lands this week.** *Effort M.* **[MTG⚡]** On `b350d77e`,
`games/mtg/src/lib.rs:99-175` defines its own `CardName`,
`ResolvedCard`, `DeckEntry`, `ResolvedDeck`, `DeckFaces`, `flatten`,
`wire`, `deal_plan`, `zone_table` — structurally parallel to
`games/riftbound/src/lib.rs:132-265`, with `wire` (:225-227) verbatim.
abyss-walker will be the third. Meanwhile `kai/src/import.rs:2` imports
`agni_riftbound::{DeckEntry, ResolvedCard, ResolvedDeck}` *by name*, so
the game selector must either duplicate the whole import panel or
generalize. Recommendation: shared generic types plus

```
trait Game {
    fn id() -> &'static str;
    fn zone_table() -> Vec<ZoneDecl>;
    fn deal_plan(faces: &DeckFaces) -> Vec<DealGroup>;
}
```

in `agni-sim` or a small new `agni-game` crate, with
`ResolvedCard { name, key, image_url }` (the incoming art core already
generalized `riftbound_id` → `key`). **This is the same registry §4/§2.2
want for `net.rs`'s hardcoded riftbound — build it once and both fall
out.** Cheapest to land *before* the MTG branch merges: merging first
triples the copies instead of doubling them.

**3.5 — Two manifest-decode paths and two deal-from-manifest routines in
kai.** *Effort M.* Three narrowing views of one CBOR manifest:

- `kai/src/app.rs:24-36` private `Manifest`/`ManifestCard {name, image}` (CBOR)
- `kai/src/bridge.rs:44-56` private `BridgeManifest`/`BridgeCard {name, image, riftbound_id}` (JSON via gateway)
- the real `agni_importers::riftbound::ingest::{RiftboundManifest, RiftboundCard}` (`importers/src/riftbound/ingest.rs:14-20`), which `kai/src/import.rs:214` already loads properly

The seeded deal is duplicated verbatim between `app.rs:160-187` and
`bridge.rs:165-196` — same `SystemTime`-nanos seed, same Fisher-Yates via
`rng.below((i+1) as u32)`, same `picks.truncate(deal)`, same
`foil_threshold = (foil_chance.clamp(0.0,1.0) * 10_000.0) as u32`. Art
patching is duplicated too: `bridge.rs:239-281` `apply_art` vs
`import.rs:379-438` `apply_store_art` — same `needs_patch` early-out,
same loop filling `art_jpeg` where `None`.

Recommendation: one kai module (§8's `platform/store.rs`, or a `cards/`
module) owning `deal_faces_from(rows, foil_chance) -> Vec<CardFace>` and
`patch_table_art(table, cache)`, with the manifest row type imported from
`agni-importers` rather than redeclared. The store-vs-gateway difference
then reduces to how rows and bytes are obtained. **Note the incoming
`importers/src/art.rs` on `b350d77e` covers only the fetch/write side —
the read/decode side named here is still unowned, so this is the right
target shape to build now, not a duplication about to be refactored
away.** This is also §4.8's fourth manifest copy.

### Hygiene

**3.6 — Seven hand-written CBOR codec pairs when the generic exists in
the same crate.** *Effort S.* `agni_sim::abi::encode`/`decode`
(`sim/src/abi.rs:9-17`) is the generic form; the identical four-line body
is reimplemented at `sim/src/log.rs:84-92`, `:94-97`, `:453-461`;
`sim/src/wire.rs:75-83`, `:155-163`; `net/src/session.rs:131-139`,
`:141-149`. Keep the named wrappers — they document the wire vocabulary
and are the byte-stability contract — but make each body
`abi::encode(value)` / `abi::decode(bytes)`. This removes seven
`ciborium::into_writer(...).expect(...)` sites, each with a differently
worded message, which is also the largest single block on §5's
expect audit.

**3.7 — `CardFace` is built as a five-field literal in 20 places, and
one of its fields is dead.** *Effort S, high leverage.* Twenty literal
constructions across 15 files: `core/src/lib.rs:65,296`;
`sim/src/wire.rs:185`; `sim/src/view.rs:144`; `sim/src/log.rs:674,747,1059`;
`net/src/session.rs:899,1453`;
`net/tests/{wasm_session.rs:64, riftbound_mvp.rs:100, net_smoke.rs:20, log_fold.rs:19}`;
`games/riftbound/src/lib.rs:362`; `engine/host/tests/host.rs:76`;
`kai/src/{bridge.rs:191, app.rs:183,207, net.rs:855, import.rs:182}`.
`CardFace` has neither `Default` nor a constructor.

Separately and more interestingly: **`art_path` is never set to `Some`
anywhere in either tree.** All 20 sites write `None`; its only two read
sites (`kai/src/lib.rs:282`, `:567`) can therefore never fire. It is
nonetheless declared at `core/src/lib.rs:27` and mirrored into `WireFace`
(`sim/src/wire.rs:112,122,134`), so it is serialized as a permanent
`null` into the deterministic log and every wire message.

Recommendation: add `CardFace::named(name)` + `Default` to collapse the
20 sites, and delete `art_path` from both `CardFace` and `WireFace`. Do
the deletion in the same change as §4.2's `tint` quantization — both are
log-format changes and should cost one format revision, not two. The
constructor is what makes that later change a one-line edit rather than
a 20-site sweep, which is why it is worth doing first.

**3.8 — Three hand-rolled hex encoders, and `blob_ref` implemented
twice.** *Effort S.* `net/src/session.rs:14` (`engine_blob_ref`),
`kai/src/modules.rs:47` (`hash_hex`), `kai/src/node.rs:192` (inline) are
all `hash.iter().map(|b| format!("{b:02x}")).collect()`, allocating a
`String` per byte. Worse, `engine_blob_ref` produces the **same
`blob:<hex>` string** as `ModuleBytes::blob_ref`
(`engine/host/src/lib.rs:283-285`), which does it correctly via
`blake3::Hash::to_hex()`. This is wire-identity duplication — module
pins — not a formatting nit. Delete `engine_blob_ref` in favour of
`ModuleBytes::blob_ref` (which §4.6 is moving crates anyway) and use
`blake3::Hash::from_bytes(h).to_hex()` at the other two.

**3.9 — Five independent global status-line cells in three storage
idioms.** *Effort M.* `node.rs:7,38-43` (`Mutex<String>`);
`bridge.rs:65,71-72,296` (`Mutex<String>`); `import.rs:16,249,254`
(`Mutex<Option<String>>`); `modules.rs:623,626-631` (thread-local
`RefCell<String>`); `engine_web.rs:22,43,48,54-55` (thread-local
`RefCell<String>`). Each has its own `set_status`/`status()` pair, and
they are recombined ad hoc — `bridge.rs:295-303` hand-joins art and mesh
with a four-arm emptiness match. Recommendation: a `kai/src/status.rs`
with a `StatusBoard` Bevy resource keyed by slot (`Node`, `Bridge`,
`Art`, `Modules`, `Engine`) and one `status_ui` joining non-empty slots
with `" · "`. Kills five statics and five poisoning sites (§5) at once.

**3.10 — The shipped stub plugin is copy-pasted, and the third copy is
landing now.** *Effort S.* **[MTG⚡]** `plugins/riftbound/src/lib.rs:1-59`
and (on `b350d77e`) `plugins/mtg/src/lib.rs:1-59` are byte-identical
except the `PLUGIN_ABI_VERSION` neighbours; their `build.rs` files differ
only in `name`/`display`/`zone_table()` and one hotkey label
(`"to-trash"` vs `"to-graveyard"`). Recommendation: an
`agni-plugin-skeleton` build-support crate exposing
`write_accept_all_plugin(out, name, display, zones, hotkeys)`, plus an
`agni_guest_abi::accept_all_exports!()` macro. Each shipped stub then
costs ~15 lines. Land before abyss-walker makes it four.

**3.11 — Guest ABI boilerplate: two real copies, not three.** *Effort S.*
`engine/wasm/src/lib.rs:82-127` (`stash_reply`, `request`, `release`,
`alloc`, `dealloc`) and `plugins/riftbound/src/lib.rs:9-40` (`release`,
`packed`, `alloc`, `dealloc`) — `release` and `alloc` verbatim.
**Correction to §4.15:** the `plugins/spike/hello-decider` guest copy is
*not* a genuine duplicate — it is `#![no_std]` with a fixed bump arena
and a deliberate `spin` export (:67-76), i.e. a hostile-input fixture the
harden tests need to stay minimal. Leave it alone. Fold only the two real
copies into the `agni-guest-abi` crate from 3.10. The packed-u64
convention `(ptr << 32) | len` is currently spelled out at
`engine/wasm/src/lib.rs:85`, `plugins/riftbound/src/lib.rs:20`,
`engine/host/src/lib.rs:112` and `engine_web.rs:274` — exactly the sort
of constant that must not drift between guest and host.

**3.12 — Test scaffolding duplicated seven ways, and every copy leaks on
panic.** *Effort S.* The scratch-store helper appears at
`plugins/harden/tests/harden.rs:302` and `:340` (inline, uniqueness
hand-managed by picking different tags), `engine/host/src/lib.rs:343-349`,
`importers/src/riftbound/gateway.rs:59-62`,
`importers/src/riftbound/ingest.rs:330-333`,
`net/tests/riftbound_mvp.rs:125-127`, `kai/src/import.rs:869-871`, plus
`importers/src/art.rs:117` on the incoming branch. All are
`temp_dir().join(format!("<tag>-{}", process::id()))` with manual
`remove_dir_all` before and after — **so every one leaks its directory
when the test panics.** Also `synthetic_deck(prefix)` exists twice with
different constants (`net/src/session.rs:1448-1466` vs
`net/tests/riftbound_mvp.rs:105-123`). Recommendation: an `agni-testkit`
dev-dependency with `scratch_store(tag) -> ScratchStore` as a `Drop`
guard — which fixes the panic-leak for free — and a shared deck fixture.
The fixture is riftbound-typed and will want an MTG twin unless 3.4
lands first. See §6.

### Nits

**3.13** — Default store-dir resolved three times:
`importers/src/main.rs:8-11`, `importers/src/bin/ingest_riftbound.rs:6-9`,
`importers/src/bin/resolve_riftbound.rs:14-17` — verbatim `HOME` +
`.spirit/store`, each with its own `expect("HOME is not set")`. One
`agni_importers::default_store_dir() -> IngestResult<PathBuf>`. *Effort S.*

**3.14 — Correct duplication, recorded so nobody "fixes" it.**
`importers/src/riftbound/gateway.rs:9-15` hand-converts
`spirit_node::gateway::DeckQuery` into
`agni_importers::riftbound::query::DeckQuery` — two identical
three-variant enums. This is the **inverted-dependency tax the
constitution requires**: spirit must never know about agni, so the two
types cannot be shared. Do not merge them. The only change worth making
is `impl From<&GatewayQuery> for DeckQuery` instead of a free `convert`.

### Refuted lead

Seat/zone math is **not** shared between `kai/src/zones.rs` and agni view
code. `sim/src/view.rs` carries no geometry at all — `ViewCard.seat` is a
bare `u8` (view.rs:12), and there is no `Vec3` and no `f32` in the file,
correctly so under the no-floats rule. The only coupling is *inside* kai:
`zones.rs:1` imports `seat_center`, `seat_yaw`, `Facing`, `Slot` and `dim`
from `lib.rs`, where `seat_center`/`seat_yaw` (lib.rs:424-448) also serve
25 call sites in `lib.rs` itself. That is the §8.0 reorg-ordering
constraint, not duplication — there is nothing here to deduplicate.

## 4. Single responsibility / layering

Read against the constitution: agni is the game-ignorant engine, kai is the
client, spirit carries no game knowledge, and the deterministic core admits
no floats, no clocks, no ambient state. Findings are ordered by how much
they distort those boundaries.

### Structural

**4.1 — The net↔game seam is four library-owned mutable statics.**
`agni-net/src/bridge.rs:17-20` holds `NET_EVENTS`, `HOST_TX`, `CLIENT_TX`
and `JOIN_REQUESTS` as crate statics. Consequences: exactly one host and
one client per process, no test isolation (two sessions in one test binary
stomp each other), and a stale-`CLIENT_TX` race when a join is torn down
and re-established. A library should not own process-global mutable state
on behalf of its caller. Recommendation: a `NetBridge` handle owning the
queue and the two senders, constructed by the caller and held as a kai
resource; `bridge`'s free functions become methods. The static can stay
behind the handle for one release to avoid a big-bang migration.
*Effort M.*

**4.2 — Render data rides inside the deterministic log.**
`core/src/lib.rs:25` puts `tint: [f32; 3]` in `CardFace`, and
`sim/src/wire.rs:110` mirrors it in `WireFace` alongside
`art_jpeg: Option<ByteBuf>`. Two distinct problems in one place:

- The floats are the *sole* reason `HardenConfig::engine()` sets
  `allow_floats: true` (`plugins/harden/src/lib.rs:63-75`). The engine
  module therefore fails the third-party float ban that agni imposes on
  everyone else, purely to carry a render tint. Quantize to `[u8; 3]`, or
  better, move tint out of the table entirely and key it client-side by
  card id — kai is the only consumer.
- JPEG bytes in the log is a known, named deferral (identity phase 1
  replaces faces with CIRs), so it needs no new decision — but it should
  be recorded as blocking, not merely pending, because every deal pays
  for it on the wire today.

*Effort M* for the tint (both crates, the harden config, and a
log-byte-stability check); the JPEG half is identity phase 1's scope.

**4.3 — `Zone` and `WireZone` are the same enum twice.**
`agni-core`'s `Zone` and `agni-sim`'s `WireZone` are byte-identical
variants with `From` impls in both directions. Delete `WireZone`; during
transition `pub use agni_core::Zone as WireZone` keeps callers compiling.
Verify the serde round-trip before and after so recorded log bytes do not
move. *Effort S/M.*

**4.4 — The hidden-face invariant has two implementations.**
`sim/src/log.rs:479` `render_table` and `sim/src/view.rs:37`
`table_view` + `view_to_table` both mask visibility, independently. This
is the single most safety-critical invariant in the engine (a divergence
leaks hidden information), and it is asserted twice in code that can drift
apart. Reimplement `render_table` in terms of `view_to_table`, or demote
it to `#[cfg(test)]` if it exists only as a test oracle. Either way one of
the two stops being load-bearing. *Effort S.*

**Corrected by §6.3.1**, which found the parity test this paragraph
assumed was missing — `sim/src/view.rs:225
the_view_matches_render_table_for_every_seat` — and a subtler problem in
its place. The two implementations are not equivalent by construction:
`table_view:69` consults `state.revealed`, `render_table:485-490` never
does. They agree only because an invariant maintained in two *other*
functions keeps `revealed` and hidden-zone membership disjoint, and the
test's 7-entry script never reaches the state where they would differ.
`render_table` also has zero production callers today. Read §6.3.1 before
acting on this finding; the fix there (run the parity assertion over a
long generated log, plus a direct disjointness invariant) is the
load-bearing half, and demoting `render_table` to `#[cfg(test)]` is the
cheap half.

### Hygiene

**4.5 — `LogState` is fully public and kai reaches through it.**
`sim/src/log.rs:121-128` declares every field `pub`, and kai walks the
internals: `kai/src/net.rs:122` does `.state().zones.is_empty()` and
`net.rs:131-132` iterates `.state().table.cards`. The engine's state shape
is now part of kai's build. Add `has_zone_table()` and a `view()`
accessor, shrink the `pub` fields to what genuinely needs external
mutation. *Effort M.*

**4.6 — Module-pin helpers live in the networking crate.**
`net/src/session.rs:13-76` holds `engine_blob_ref`, `genesis_engine_pin`,
`genesis_plugin_pin`, `pin_hash` and `module_matches_pin`. These are
module-identity utilities, not session mechanics, and their consumer is
`kai/src/modules.rs`. Move them beside `ModuleBytes` in
`agni-engine-host`, which already owns `blob_ref()` at
`engine/host/src/lib.rs:283`. Separately, `session.rs:7` re-exports
`agni_sim::wire::*`, so every wire type has two import paths and callers
pick arbitrarily — drop the blanket re-export. *Effort S.* (§2.3 proposes
`net/src/pins.rs` as an intermediate step; moving to engine-host is the
end state, and the two are compatible — do the file split first if the
crate move needs to wait.)

**4.7 — `kai/src/telemetry.rs` is five modules in one file.**
577 lines, 31 `cfg` gates, covering: the tracing capture layer, per
platform config discovery, token persistence, the Loki push/batching
loop, and an egui token panel. Split into
`telemetry/{mod,capture,config,ship,panel}.rs`; the cfg gates collapse
substantially once config and persistence are per-platform files rather
than per-platform functions interleaved. Bug found while reading:
`parse_kv` (`telemetry.rs:156`) line-splits what it calls TOML and
mis-parses any quoted value containing `=`. *Effort M.*

**4.8 — A fourth copy of manifest decoding sits in the app shell.**
`kai/src/app.rs:137-188` `store_faces` plus its private `Manifest` /
`ManifestCard` structs decode a manifest inline in the app entry point,
duplicating what bridge.rs and import.rs each do their own way (§3 owns
the consolidation). Also check the hob-ref fallback at `app.rs:146-152`
— it takes `"hob"` then falls through to the first file in `refs/`, which
may now pick up `refs/modules` entries seeded by `modules::seed_bundled`
and deal module blobs as cards. *Effort S/M.*

**4.9 — The engine gas budget is stated twice.**
`kai/src/engine.rs:4` defines `ENGINE_GAS_BUDGET` as a bare literal that
restates `agni_harden::ENGINE_GAS_LIMIT`; `kai/src/engine_web.rs:12`
repeats the same literal a third time. It belongs once in
`agni-engine-host` beside `FUEL_BACKSTOP`, imported by both kai engine
files. A silent divergence here means the web and native peers admit
different programs. *Effort S.*

**4.10 — `agni-plugins` has zero consumers.**
`plugins/src/lib.rs` (132 lines: `CardScript`, `ScriptRegistry`) is
referenced by nothing in the workspace — dead code with an unexercised
public API and no tests. It is not accidental: `plugins.md` re-scopes the
crate as the future card-pack index. Record that intent in the crate's
own docs so the next reader does not delete it or, worse, build against
the current shape assuming it is load-bearing. *Effort S, documentation.*
§7.7.1 reaches the same verdict from the dependency side, and finds two
*more* zero-consumer members — `agni-mtg` (805 bytes) and
`agni-abysswalker` (481) — which, unlike this crate, hold nothing worth
keeping.

**4.11 — The forbidden-opcode scan matches on `Debug` strings.**
`plugins/harden/src/lib.rs:244-257` decides whether an opcode is banned
with `name.contains("F32")` over a `Debug`-formatted operator. That is
fragile against any wasmparser formatting change and largely redundant
with `validate_features`, which already rejects the same programs through
the validator. Derive the rejection from the validator error, or match
`Operator` variants directly. *Effort S.*

The pinning question this finding raised is **answered and clean** —
§7.5.1 verified that `radix-wasm-instrument`, `wasm-encoder` and
`wasmparser` are all `=`-pinned to versions that match each other, and
that `harden.rs:72-83`'s golden hash would catch a change anyway. The
remaining gap is upstream of the pins: the *compiler* is unpinned and a
nightly `nix flake update` bumps it (§7.5.2).

**4.12 — `Table.cards` is public while `next_id` is private.**
`core/src/lib.rs:42`. `insert_card` maintains a duplicate-id guard that
any caller can bypass by pushing to `cards` directly. Add a `cards()`
accessor and make the field private; the id monotonicity invariant then
actually holds. *Effort S/M* (kai reads `.cards` in several render
systems, so it is a mechanical but wide change).

**4.13 — A render colour is baked into the engine.**
`sim/src/wire.rs:180-188` `hidden_face()` hardcodes
`tint: [0.32, 0.14, 0.10]`. This is kai's card-back brown living in the
deterministic layer. It falls out for free if 4.2 lands; do not fix it
separately.

### Nits

**4.14** — `net/src/bridge.rs:124`
`close_host(Option<(&Mesh, &TableProtocol)>)` takes an Option-of-tuple.
Two parameters, or a small struct, reads better at every call site.

**4.15** — Copies of the guest `alloc`/`dealloc`/packed-`u64`
boilerplate in `engine/wasm` and `plugins/riftbound`. A tiny
`agni-guest-abi` crate (or a single macro) would carry it, and would be
the natural home for the ABI contract when a third guest lands.
*Effort S.* **Corrected by §3.11**: the `plugins/spike/hello-decider`
guest is *not* a third copy — it is a deliberately minimal `no_std`
hostile-input fixture and must stay as it is.

**4.16** — `kai/src/identity.rs:15` `store_dir()` is platform path
policy living in the identity panel. It belongs in the `platform/` module
the §8 reorg creates; note that `kai/src/import.rs:202-210`,
`kai/src/node.rs:61` and `kai/src/sync.rs:24` each define their own
`store_dir` too — **four definitions, not three** (`node.rs:61` was found
by the clippy pass, §5G) — so the move also deduplicates. Two of the four
are `-> Option<PathBuf>` returning an unconditional `Some` on desktop; the
consolidated one should be infallible there and fallible only where it
genuinely is.

**4.17** — `kai/src/node.rs:193` hand-rolls hex encoding.

### Per-file verdicts

| Area | Verdict |
|---|---|
| `core/`, `sim/*`, `net/{lib,table}`, `engine/*`, `plugins/*`, `games/*` | OK — boundaries hold |
| `net/bridge.rs` | flagged — 4.1 |
| `kai/app.rs` | flagged — 4.8, and it is the app shell doing store I/O |
| `kai/telemetry.rs` | flagged — 4.7 |
| `kai/zones.rs` | **model file** — pure math, no ECS, well tested; the shape every other kai module should move toward |

### Cross-lens notes

- `Mutex::lock().unwrap()` on a poisoned-mutex-capable static appears in
  every static-holding module — `agni-net/src/bridge.rs`, `kai/src/node.rs`,
  `kai/src/telemetry.rs`, `kai/src/bridge.rs`, `kai/src/import.rs`,
  `engine/wasm`. Enumerated in §5.
- The three art-fetch/manifest paths are the largest DRY item in the tree;
  4.8 makes them four. Owned by §3.
- `kai/src/zones.rs` seat math depends on `seat_center`/`seat_yaw`, which
  live in `lib.rs`. The §8 reorg **must** land those in the shared layout
  module before `zones.rs` moves, or the move breaks the one exemplary
  file in the client.

## 5. Rust best practices

Read as: error handling, panics in non-test code, allocation and `Clone` on
hot paths, dispatch choices, feature/cfg hygiene, API guidelines, async
hygiene, and a triaged `clippy::pedantic` pass. Both workspaces were linted
at `-j 6` (`cargo clippy --workspace --all-targets -- -W clippy::pedantic`);
kai needs its devenv shell for the pkg-config deps.

### 5A. Error handling

**5A.1 — The errors that reach the player have no `Display`; the errors that
never leave a CLI have ten hand-written ones.** *Structural, effort S.*
`FoldError` (`sim/src/log.rs:101`, 16 variants) and `CallFault`
(`engine/host/src/lib.rs:15`) implement neither `Display` nor
`std::error::Error`. They are the two types that answer "why did my card
bounce" and "why did the table die" — and they cannot be rendered. Meanwhile
`agni-importers` carries **ten** hand-written `Display` + `Error` pairs
(`deck_code.rs:39/63`, `link.rs:58/68`, `card_code.rs:130/140`,
`code_list.rs:12/24`, `catalog.rs:54/60`, `text_list.rs:11/21`,
`mod.rs:66/76`, plus `harden/src/lib.rs:120/162`, `plugins/src/lib.rs:72/80`,
`sim/src/engine.rs:15/21`) for errors whose only consumer is a `main()` that
prints them. The taxonomy is inverted.

Recommendation: give `FoldError` and `CallFault` `Display` + `Error` (they
are the ones that earn it), and take `thiserror` as a dependency **only in
`agni-importers`**, where ten mechanical impls collapse to ten attributes.
Do not push `thiserror` into `agni-core`/`agni-sim`: those cross the wasm
ABI, must stay dependency-thin, and their error enums are `Serialize`d as
data rather than formatted.

**5A.2 — `Result<T, String>` is right at the ABI and wrong everywhere else it
appears.** *Hygiene, effort M.* `agni_sim::abi::Reply<T> = Result<T, String>`
(`sim/src/abi.rs:19`) is the correct call: the error crosses a CBOR boundary
into a sandboxed module, where a structured type buys nothing and costs
schema stability. Keep it. But the stringly type has leaked into **28
host-side signatures that never touch the ABI**, including
`WasmModule::instantiate` (`engine/host/src/lib.rs:36`),
`ModuleSource::load`/`engine_module` (`:315`, `:289`, `:295`, `:324`),
`verify_engine_pin`/`verify_plugin_pin` (`net/src/session.rs:32`, `:45`),
`write_frame` (`net/src/table.rs:48`), and most of `kai/src/engine_web.rs`.
The pin verifiers are the ones that matter: kai must distinguish "pin absent"
from "pin present but mismatched" from "bytes unavailable" to decide between
refusing, parking, and fetching — and today it re-derives that from substring
matching on the message. Give the pin path a three-variant `PinError` and
`WasmModule::instantiate` a `LoadError`; leave the rest until touched.

**5A.3 — `HostSession::append` returns a `Result` and ten callers throw it
away.** *Structural, effort S.* `append` (`net/src/session.rs:290`) is
correctly typed `Result<LogEntry, FoldError>`, and then
`.expect("genesis folds")`, `.expect("join folds")`, `.expect("deal folds")`,
`.expect("opening draw folds")`, `.expect("validated group deal folds")`,
`.expect("deal reveal folds")`, `.expect("imported deal folds")`,
`.expect("imported reveal folds")`, `.expect("imported move folds")`,
`.expect("reset folds")` discard it at `:206`, `:245`, `:273`, `:284`,
`:348`, `:397`, `:467`, `:505`, `:512`, `:681`. Each of those is a live
process abort — in kai, the whole window — on a fold the host itself
constructed. The messages are honest ("validated group deal folds" means "we
already validated this"), but a validate/apply divergence is exactly the bug
class that ships, and the response should be ending the session with a
status, not `SIGABRT`. This is the same defect as §2.3's `EngineFault`
`expect`s, one layer up; fix them together by threading both out of
`intent`/`apply`.

**5A.4 — An undecodable wire message is silently dropped.** *Structural,
effort S.* `decode_client`/`decode_host` (`net/src/session.rs:137`, `:147`)
and `abi::decode` (`sim/src/abi.rs:15`) all end in
`ciborium::from_reader(...).ok()`, and every caller treats `None` as
"nothing happened". A peer on a newer build sending a `HostMsg` variant this
build does not know produces no log line, no status, and no disconnect — the
join simply never completes. Return a `Result` (or at minimum `warn!` at the
call sites) so a version skew is loud. See 5F.2 for why this is reachable.

**5A.5 — `Result<_, ()>` in the gateway fetch path.** *Hygiene, effort S.*
`kai/src/bridge.rs:312`, `:335`, `:343` (`fetch_response`, `fetch_text`,
`fetch_bytes`) return `Result<T, ()>` — an error type carrying nothing. Every
caller (`:90`, `:105`, `:116`, `:130`, `:224`) therefore writes a generic
status string and the real browser error is discarded at the `.map_err(|_| ())`.
This is why the web gateway's failure mode reads "no gateway" whether the
node is down, CORS refused, or the blob 404'd. Return the `JsValue`'s message
as a `String`.

### 5B. Panics in non-test code

Counting only code above each file's `mod tests`, the tree has **150
panicking sites**: 73 `Mutex::lock().unwrap()`, 24 `expect` in
`net/src/session.rs`, 16 `ciborium::into_writer(...).expect(...)`, and the
remainder scattered. They fall into four classes with four different verdicts.

**5B.1 — 73 `lock().unwrap()` on poisonable mutexes, 55 of them on process
statics.** *Structural, effort M (falls out of §3.9 + §4.1).*

| file | sites | what is behind the lock |
|---|---:|---|
| `kai/src/bridge.rs` | 17 | `STATUS_LINE`, `BRIDGE`, `ART`, `FETCHING`, `ARRIVALS`, `RIFTBOUND` statics |
| `agni-net/src/bridge.rs` | 11 | `NET_EVENTS`, `HOST_TX`, `CLIENT_TX`, `JOIN_REQUESTS` statics (§4.1) |
| `kai/src/telemetry.rs` | 10 | `QUEUE`, `ORIGIN_DEFAULTS`, `SHIPPING` statics |
| `agni-net/src/table.rs` | 9 | `self.active`, `self.conns` — instance fields, not statics |
| `kai/src/modules.rs` | 8 | `STATE` static |
| `kai/src/node.rs` | 7 | `STATUS`, `NODE`, `PENDING_SEEDS` statics |
| `kai/src/import.rs` | 7 | `ART_STATUS`, `ART_ARRIVALS`, `RESULTS` statics |
| `kai/src/sync.rs` | 2 | `TICKETS` static |
| `engine/wasm/src/lib.rs` | 2 | `ENGINE`, `REPLY` statics |

The severity is not the `unwrap` — it is that a poisoned mutex is
*unrecoverable by construction here*. Every one of these guards a global, so
a single panic anywhere inside a lock (a `send` on a closed channel, an art
decode, a telemetry format) poisons it permanently and every subsequent
frame panics on the same line. A Bevy app then dies one frame later with a
stack trace pointing at the wrong place.

Two fixes, in order: (a) `parking_lot::Mutex` has no poisoning and no
`unwrap` — a one-dependency, mechanical change that removes all 73 sites at
once and is the cheapest real win in this section; or (b) the structural
version, §3.9's `StatusBoard` resource and §4.1's `NetBridge` handle, which
delete most of the statics outright. Do (a) now and (b) as those findings
land; they are not exclusive.

**5B.2 — 16 `ciborium::into_writer(..).expect(..)` on infallible
serialization.** *Hygiene, effort S.* `sim/src/abi.rs:11`, `log.rs:86`,
`:96`, `:455`, `wire.rs:77`, `:157`, `net/src/session.rs:133`, `:143`, and
their callers. Serializing an owned, derive-`Serialize` type into a `Vec<u8>`
cannot fail, so these are honest — but there are sixteen of them with
sixteen differently worded messages, and §3.6 already wants the bodies
collapsed into `abi::encode`. Doing §3.6 reduces this class to **one** site.
That is the right fix; do not chase them individually.

**5B.3 — `.expect("engine answers …")` on engine faults.** *Structural.*
`net/src/session.rs:300`, `:309`, `:310`, `:773`, `:780`, `:795`, `:796`,
`:828`. Owned by §2.3 — not repeated here beyond noting they are the eight
sites where a sandboxed module's misbehaviour aborts the host, which is the
one thing the sandbox exists to prevent.

**5B.4 — Justified, leave alone.** `sim/src/log.rs:194`
`LogAction::Genesis { .. } => unreachable!()` (§2.4 already flags it as a
refactor magnet, not a bug); `engine/wasm/src/lib.rs:106`, `:120`
`Layout::array::<u8>(len).unwrap()` inside the guest `alloc`/`dealloc` — a
guest trap is the correct response to a host passing a nonsense length, and
`no_std`-adjacent guest code has nowhere better to go; `harden/build.rs` and
the four `importers` binaries' `expect("HOME is not set")` (§3.13 dedupes
them anyway) — a CLI that cannot find `$HOME` should die.

### 5C. Clone, allocation, and dispatch

**5C.1 — `Mirror` is replaced wholesale on every network event, which defeats
the change detection built on top of it.** *Hygiene, effort S.*
`kai/src/net.rs` does `mirror.view = session.view().clone()` at eleven sites
(§2.2), each a deep clone of every card, zone and seat. Bevy's
`Res<Mirror>::is_changed()` then fires for all of them, so
`sync_zones` (`lib.rs:1352`) — which guards on `is_changed()` and then
*clones the whole zone table again* at `:1364` to compare against a `Local`
cache — does the comparison work on every event rather than on every actual
change. §2.2's `refresh(table, mirror, session)` helper should compare before
assigning (`if mirror.view != *session.view() { … }`); one line inside the
new helper fixes all eleven sites and makes the downstream `is_changed()`
guards mean what they say.

**5C.2 — Two O(n²) scans per relayout.** *Nit, effort S.* Already recorded in
§2.1 (`Mirror::rotated` at `lib.rs:173` is a linear `find` called once per
card from `layout_cards`; `kept.contains(&card.id)` in `sync_cards` likewise).
Restated here only because clippy will not find these and they are the only
genuine algorithmic complaints in the tree. At 60 cards both are free.

**5C.3 — `clone_from` opportunities in the view diff.** *Nit, effort S.*
`sim/src/view.rs:102-105` (`apply_deltas`) and `sim/src/log.rs:315` assign
`x = y.clone()` where `x.clone_from(&y)` reuses the existing allocation.
`apply_deltas` runs once per applied entry on every replica, so it is the one
place this is measurable rather than cosmetic. Two more at
`kai/src/net.rs:537` and `kai/src/telemetry.rs:508`.

**5C.4 — Trait-object dispatch is the correct choice and should stay.**
*No action.* `Box<dyn Engine>` / `Option<Box<dyn PluginModule>>`
(`net/src/session.rs:154-155`, `:729-730`, `kai/src/modules.rs:30-31`) look
like a candidate for an enum, and they are not: the concrete type is chosen
at runtime from a genesis pin, differs per platform (wasmi / browser
`WebAssembly` / native fallback), and the whole point of the design is that
a module the binary has never seen can be loaded. One virtual call per fold
against a wasm trampoline is unmeasurable. Recorded so nobody "optimises" it.
The dispatch that *should* become a trait is the one in §8.4 — kai's two
`mod platform` blocks in `modules.rs` with identical signatures and no
compiler-checked relationship.

**5C.5 — `HashMap` in the deterministic layer, safe today by accident.**
*Hygiene, effort S.* `LogState` is scrupulously `BTreeMap`/`BTreeSet`
(`sim/src/log.rs:120-127`), but the face maps beside it are `HashMap`:
`render_table(state, &HashMap<u32, CardFace>, viewer)` (`log.rs:479`),
`view_to_table(view, &HashMap<u32, CardFace>)` (`view.rs:111`),
`HostSession::dealer` (`session.rs:158`), `ClientSession::faces` (`:733`).
Both functions only ever `.get()`, so iteration order never leaks and there
is no bug today — but nothing structural prevents the next contributor from
writing `for (id, face) in faces` in `render_table`, and no test would catch
the resulting cross-replica divergence. Switch all four to `BTreeMap`: it
costs nothing at these sizes, makes the invariant hold by type rather than
by inspection, and incidentally silences the two `implicit_hasher` lints.

**5C.6 — A 64 MiB eager allocation per frame header.** *Hygiene, effort S.*
`net/src/table.rs:40` reads a 4-byte big-endian length, checks it against
`MAX_FRAME_BYTES` (`1 << 26` = 64 MiB, `:13`), then `vec![0u8; len]` before
reading a single payload byte. Any peer that completes the ALPN handshake can
force a 64 MiB allocation per connection with four bytes. The largest
legitimate frame is a `HostMsg::Welcome` carrying a full log; size that and
set the cap to a small multiple of it, or read in chunks. Not a
vulnerability at the current trust model (joins are ticket-gated), but the
cap is three orders of magnitude looser than the traffic.

### 5D. API guidelines

**5D.1 — Zero `#[must_use]` in either tree, and 307 candidates.**
*Hygiene, effort S.* `grep -c must_use` returns 0 across agni and kai;
clippy proposes 251 in agni and 56 in kai. Most are noise, but the subset
that is not is sharp: `HostSession::intent`, `deal`, `deal_to`,
`deal_groups` and `ClientSession::apply` all return the entries a caller
must broadcast — dropping the return value is a silent desync, and it is
exactly the mistake §2.2's eight duplicated broadcast loops are structured
to make. Annotate the ~15 methods on `HostSession`/`ClientSession` that
return entries or faces; ignore the other 292.

**5D.2 — No wire enum is `#[non_exhaustive]` and none has an unknown-variant
fallback.** *Structural, effort M.* `#[non_exhaustive]` appears zero times.
`ClientMsg`, `HostMsg` (`net/src/session.rs:103`, `:110`), `WireIntent`
(`:85`), `LogAction`, `FoldError`, `ZoneKind`, `ZoneLayout`,
`ZoneVisibility` are all plain CBOR enums. Combined with 5A.4's silent
`.ok()` drop and 5F.2's single opaque `ALPN = b"spirit-table/1"`
(`net/src/table.rs:11`) as the only version marker, adding one `HostMsg`
variant makes every older peer fail to join with no diagnostic anywhere.
Module pins fix the *engine*'s version skew; they do nothing for the
session protocol, and host and client are separately built binaries.
Recommendation: a `version: u32` in `ClientMsg::Join` / `HostMsg::Welcome`
checked at the handshake with a named refusal, which is cheap now and
impossible to retrofit politely later.

**5D.3 — `CardFace` is not `Eq`, not `Hash`, not `Ord`, because of the
tint.** *Hygiene, folded into §4.2.* `core/src/lib.rs:22` derives only
`Debug, Clone, PartialEq` — `[f32; 3]` forbids the rest, and it propagates:
`Card`, `Table` (`:40`) and `WireFace` (`wire.rs:107`) are all
`PartialEq`-only. A deterministic log's central data type cannot be a map
key or a set member. §4.2's quantization to `[u8; 3]` makes the whole family
`Eq + Ord + Hash` for free; note it as a second, independent reason to do it.

**5D.4 — `Default` is derived 35 times and absent where it is asked for.**
*Nit, effort S.* `CardFace` has no `Default` and is built as a five-field
literal in 20 places — §3.7 owns that. Two other spots: `LogState::new()`
(`log.rs:130`) is `Self::default()` under a different name (keep both, but
`new` should be `#[must_use]`), and `Table::new()` likewise.

**5D.5 — Small naming nits.** `card_code.rs:28`, `:92` `pub fn to_wire(self)
-> u8` takes a `Copy` receiver by value and returns a primitive — the
guideline spelling is `as_wire` or `impl From<Rarity> for u8`.
`kai/src/node.rs:46` `pub fn get() -> Option<Node>` is a module-level `get`
with no object — `node::current()` reads better at the 12 call sites.
`net/src/bridge.rs:124` `close_host(Option<(&Mesh, &TableProtocol)>)` is
§4.14. `net/src/table.rs:78`'s manual `Debug` omits `conns` and
`next_conn`, so a `{:?}` of a `TableProtocol` hides exactly the state you
would be debugging.

### 5E. Feature and cfg hygiene

**5E.1 — kai spells one predicate three ways.** *Hygiene, effort S.* 135
`#[cfg]` attributes across `kai/src`, in these shapes:

| predicate | count |
|---|---:|
| `not(target_arch = "wasm32")` | 58 |
| `target_arch = "wasm32"` | 32 |
| `target_os = "android"` | 17 |
| `all(not(target_os = "android"), not(target_arch = "wasm32"))` | 7 |
| `not(any(target_arch = "wasm32", target_os = "android"))` | 4 |
| `all(not(target_arch = "wasm32"), not(target_os = "android"))` | 2 |
| `any(target_os = "android", target_arch = "wasm32")` | 4 |
| `any(target_arch = "wasm32", target_os = "android")` | 1 |
| `not(target_os = "android")` | 2 |
| `test` / `all(test, not(target_arch = "wasm32"))` | 5 |

Rows 4–6 are **the same predicate written three ways, 13 times** — "desktop
only". Rows 7–8 are the same predicate twice. A reader cannot grep for
"desktop" and find them all, and neither can the next person adding a
target. Fix: a `build.rs` emitting `--cfg kai_desktop` / `kai_mobile` /
`kai_web` with `cargo::rustc-check-cfg`, or the `cfg_aliases` crate (already
in the dependency graph via bevy). Then §8.4's `platform/` folder replaces
most of the remaining per-function gates with per-target files.

**5E.2 — Only three of five target/feature combinations are ever built.**
*Structural, effort M.* The real matrix is {native-desktop, wasm32, android}
× {default, `fast-compile`}. CI builds native and wasm32; **android is built
only by `agni-artifacts.yml` on push to `main`, and is skipped entirely if
the `kai_android_signing` secret is absent** — so `sync.rs` (70 lines, the
whole JNI bridge) plus the android arms of `telemetry.rs`, `app.rs`,
`node.rs`, `identity.rs`, `import.rs`, `modules.rs` and `net.rs` can break
and merge green. `fast-compile` (`bevy/dynamic_linking`) is never built by
CI at all. Recommendation: add a `cargo check --target
aarch64-linux-android` lane to `.woodpecker/kai.yml` — it needs no signing
key and it is the single highest-value CI addition in this review. (§6.7
reaches the same conclusion from the test side.)

**5E.3 — Two dependency blocks are byte-identical.** *Nit, effort S.*
`kai/Cargo.toml:33-38` (`cfg(target_os = "android")`) and `:40-46`
(`cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))`)
declare the same `spirit-node` (native), `tokio` (rt-multi-thread + time)
and `ureq`; the second adds `gethostname`. Collapse to one
`cfg(not(target_arch = "wasm32"))` block plus a one-line desktop block.

**5E.4 — `agni-importers`' feature graph is well-formed.** *No action.*
`default = ["scryfall", "riftbound-gateway"]` with `riftbound` (wasm-safe,
no I/O) → `riftbound-native` (+ureq/spirit-core) → `riftbound-gateway`
(+spirit-node) is a clean ladder, and kai correctly takes
`default-features = false` with `riftbound` on wasm32 and `riftbound-native`
on desktop (`kai/Cargo.toml:26-31`, `:48-51`). This is the one place in the
tree where feature hygiene is exemplary; the `[features]`-less crates should
copy the pattern when they grow one.

### 5F. Async hygiene

**5F.1 — Detached tasks with no handles on the client side, aborted handles
on the host side.** *Hygiene, effort S.* `net/src/table.rs:140` retains the
writer's `JoinHandle` and `writer.abort()`s it at `:152` when the read loop
ends — correct. `join_via` (`:225`, `:234`) discards both handles; the
writer ends when `TableJoin`'s sender drops, but the **reader task runs
until the peer closes the connection**, holding a cloned `Connection`. Drop
a `TableJoin` without closing — which is what `close_join`
(`net/src/bridge.rs:187`) does by nulling `CLIENT_TX` — and the reader
leaks. This is the concrete mechanism behind §4.1's "stale-`CLIENT_TX`
race": the old reader is still pushing `JoinEvent`s into a channel nobody
reads while a new join is being established. Keep both handles in
`TableJoin` and abort them in `Drop`.

**5F.2 — The host cannot distinguish a peer leaving from a protocol error.**
*Hygiene, effort S.* `net/src/table.rs:147`
`while let Ok(Some(bytes)) = read_frame(&mut recv).await` collapses `Ok(None)`
(clean close), `Err(_)` (oversized frame, truncated read) and a decode
failure into one `HostEvent::Left(conn)`. The client side gets this right —
`join_via`'s reader (`:236-247`) matches all three arms and sends a labelled
`JoinEvent::Closed(reason)`. Mirror the client's shape onto the host so
`HostEvent` carries the reason; §3.9's status board then has something true
to display.

**5F.3 — Three bare `std::thread::spawn`s funnelling into statics.**
*Hygiene, folded into §3.9/5B.1.* `kai/src/import.rs:297`, `:349`, `:447`
spawn unjoined OS threads whose only output channel is
`RESULTS`/`ART_ARRIVALS`/`ART_STATUS` under a `Mutex`. No handle, no
cancellation, no way to tell a finished import from a hung one. Seating a
second deck while the first is still resolving races two threads into the
same `Vec`. A single `platform/art.rs` worker owning one queue (§8.4) fixes
the shape; at minimum keep the handles so `is_finished()` can drive the
status line.

**5F.4 — `agni-net`'s spawn injection is the right pattern.** *No action.*
`net/src/bridge.rs:83` takes
`spawn: impl FnOnce(Pin<Box<dyn Future<Output = ()> + Send>>)` so the
library never chooses an executor — kai supplies tokio on desktop and
`spawn_local` on wasm through one `Node::spawn` (`kai/src/node.rs:22`,
`:30`). That is exactly the inversion §4.1 asks for on the *state* side, and
it is worth pointing at as the local precedent when doing that work.

**5F.5 — `std::future::pending::<()>().await` parks the node forever.**
*Nit.* `kai/src/node.rs:110`, `:170`. Correct for "this task is the node and
outlives the app", but it means there is no shutdown path; a clean quit
cannot flush telemetry or close the endpoint politely. Worth a
`tokio::sync::Notify` when shutdown becomes a requirement, not before.

### 5G. Triaged `clippy::pedantic`

`agni` emits **574** pedantic warnings, `kai` **487** (unique file:line;
both workspaces are clean under default clippy with `-D warnings`, which is
what CI runs). Roughly 85% is noise for this codebase. The triage:

**Suppress at the workspace level — these lints are wrong here, not the code.**

| lint | agni | kai | why |
|---|---:|---:|---|
| `must_use_candidate` | 251 | 56 | see 5D.1 — annotate ~15 by hand, allow the lint |
| `missing_errors_doc` | 114 | 14 | the house rule is **no doc comments at all**; this lint demands them |
| `missing_panics_doc` | 69 | 24 | same |
| `needless_pass_by_value` | 2 | **208** | kai's 208 are almost entirely Bevy systems taking `Res<T>`/`Query<…>`/`Commands` by value, which the `SystemParam` derive *requires*. 80 in `lib.rs` alone. Unfixable by design |
| `cast_precision_loss` | 0 | 75 | `usize`→`f32` in `zones.rs` (15) and `lib.rs` (13) render math. Correct and intended |
| `pub_underscore_fields` | 0 | 6 | `foil.rs:21,23,25` `_pad0/_pad1/_pad2` are WGSL uniform padding |
| `single_match_else`, `if_not_else`, `semicolon_if_nothing_returned`, `needless_continue`, `items_after_statements` | 15 | 8 | style preference, no defect |

Add `[lints.clippy]` to both workspace manifests allowing exactly these, then
**turn pedantic on in CI** — the residue below is small enough to keep at
zero, and that is the point of triaging rather than ignoring.

**Worth fixing.**

| lint | count | where | verdict |
|---|---:|---|---|
| `cast_possible_truncation` / `_wrap` / `_sign_loss` | 39 agni + 35 kai | see below | **the only class with correctness weight** |
| `map_unwrap_or` | 23 + 12 | importers, `session.rs:236`, `deck_code.rs:406,410,441,448` | mechanical, `clippy --fix` handles it. Hygiene |
| `assigning_clones` | 15 + 4 | `view.rs:102-105`, `log.rs:315`, `net.rs:537`, `telemetry.rs:508` | 5C.3 |
| `implicit_hasher` | 6 + 2 | `log.rs:479`, `view.rs:111`, `import.rs:382` | disappears with 5C.5's `BTreeMap` |
| `too_many_lines` | 8 + 8 | `sim/src/log.rs:176` (132), `harden/src/lib.rs:259` (128), `kai/src/lib.rs:533` `sync_cards`, `kai/src/import.rs:531` `import_ui`, `kai/src/net.rs:151`, `:329` | **independent confirmation of §2's split targets** — every kai hit is a function §2/§8 already moves. The two agni hits are `apply_action` and the opcode scan (§4.11) |
| `unchecked_time_subtraction` | 2 | `riftcodex.rs:144` `THROTTLE - elapsed` | real panic on a clock step; use `saturating_sub` |
| `missing_fields_in_debug` | 2 | `net/src/table.rs:78` | 5D.5 |
| `unnecessary_wraps` | 2 | `kai/src/import.rs:202`, `kai/src/node.rs:61` | both `store_dir() -> Option<PathBuf>` returning unconditional `Some` on desktop. Note this makes **four** `store_dir` definitions in kai, not the three §4.16 records — add `node.rs:61` to that finding |
| `format_collect` / `format_push_string` | 5 + 2 | `session.rs:14`, `modules.rs:47`, `node.rs:192`, `riftcodex.rs:109` | the three hex encoders of §3.8, found independently |
| `ref_option` | 5 | `scryfall.rs:76` et al | `&Option<T>` → `Option<&T>`. Nit |
| `match_wildcard_for_single_variants` | 2 | `query.rs:48` `_ => ureq::Error::Transport(_)` | a new `ureq` variant would be silently mishandled. Worth fixing |
| `float_cmp` | 2 | `kai/src/lib.rs:1257` `if live != before` | slider change detection; use the egui `Response::changed()` the widget already returns |
| `ignore_without_reason` | 1 | `net/tests/net_smoke.rs:41` | §6.7 — this is the only transport test and it is skipped |

**On the casts.** 74 sites, and the triage matters because most are provably
safe while a few are not:

- **Safe by a guard immediately above** — `net/src/table.rs:55`
  (`bytes.len() as u32` after the `MAX_FRAME_BYTES` check at `:49`). Add
  `#[allow]` with the guard named, or use `u32::try_from`.
- **Safe by domain** — `kai`'s 35 (`lib.rs` 9 + 13 + 4, `zones.rs` 15 + 1,
  `app.rs` 3 + 1) are all card counts and seat indices into render math.
  Suppress with `cast_precision_loss`.
- **Worth converting to `try_from`** — the deterministic-path ones, because
  a truncation there is a fold divergence rather than a wrong pixel:
  `net/src/session.rs:281`, `:452`, `:560` (deck/group sizes → `u32` card
  ids and indices), `games/riftbound/src/lib.rs:117` (`usize` → `u16` zone
  id), `plugins/harden/src/lib.rs:455`, `:459`, `:467` and
  `engine/host/src/lib.rs:98`, `:103` (byte offsets into guest memory → the
  packed-`u64` ABI of §3.11 — a truncation here is a wild pointer into the
  guest). These nine are the actual finding; the other 65 are noise.
  *Structural for the guest-memory five, hygiene for the rest, effort S.*

## 6. Test architecture

223 test functions over 68 source files: 185 in agni (140 inline, 45 in
`tests/`), 38 in kai (all inline). Test code is 45% of agni's 12,894 lines
and 18% of kai's 7,543.

### 6.1 Zero-test modules

**29 source files carry no inline `mod tests` — 4,302 lines. Four (958
lines) are reached by integration tests; the remaining ~3,340 lines have no
coverage on any configuration.** Ranked by risk:

| crate/file | lines | inline | integration | verdict |
|---|---:|---:|---|---|
| `net/src/table.rs` | 257 | 0 | only `net/tests/net_smoke.rs:41`, which is `#[ignore]`d | **structural — effectively zero.** The iroh ALPN handler, conn map, `close_all`, `close_label`. The wire path |
| `kai/src/engine_web.rs` | 404 | 0 | none — wasm32-only, never compiled by a test lane | **structural — a third `Engine` impl** with its own gas/trap decoding (`:229-292`) |
| `kai/src/bridge.rs` | 349 | 0 | none — wasm32-only | structural. The whole web gateway path |
| `net/src/bridge.rs` | 200 | 0 | none | structural. `unreachable_reason` (`:69`) is pure string mapping and trivially testable |
| `kai/src/telemetry.rs` | 577 | 0 | none | hygiene. `parse_kv:156`, `sanitize:135`, `push_body:375` are pure |
| `importers/src/scryfall.rs` | 158 | 0 | none | hygiene |
| `kai/src/{peers,node,identity}.rs` | 231/196/148 | 0 | none | hygiene |
| `kai/src/sync.rs` | 70 | 0 | none — android-only, compiled by no CI check | structural-adjacent (6.7) |
| `kai/src/{app,tuning,foil,engine,main}.rs` | 211/96/47/20/3 | 0 | none | genuinely trivial |
| `engine/wasm/src/lib.rs` | 158 | 0 | indirect, via the compiled artifact in `engine/host/tests/host.rs` | acceptable, but `handle_fold_entry`/`handle_view`/`stash_reply:82` are target-independent and could be unit-tested natively for free |
| `sim/src/abi.rs` | 57 | 0 | indirect | trivial — except nothing asserts an `ENGINE_ABI_VERSION` mismatch is refused (`engine/host/src/lib.rs:154-159`) |
| `plugins/harden/src/{lib,main}.rs` | 496+140 | 0 | 15 tests in `tests/harden.rs` | **best-tested crate in the tree** |
| `plugins/spike/hello-decider/{guest,host}` | 76+82 | 0 | none — both declare their own `[workspace]`, so they are outside agni's workspace and `cargo test` never touches them | hygiene (6.5) |

Recommendations, in order: un-`#[ignore]` `net_smoke.rs` or replace its iroh
endpoints with an in-process duplex so `net/src/table.rs` gets real coverage
(*structural, M*); a native unit-test module in `engine/wasm/src/lib.rs`
calling `handle_*` against `abi::encode` — no wasm toolchain needed
(*hygiene, S*); pure-function tests for `telemetry::{parse_kv, sanitize}` and
`net/src/bridge.rs:69` (*hygiene, S*).

### 6.2 Unit vs integration placement

**Two files are majority test code, and that alone is why they read as huge
in the §1 census.**

| file | total | tests start | test lines | % |
|---|---:|---:|---:|---:|
| `net/src/session.rs` | 1713 | `:887` | 827 | 48% |
| `sim/src/log.rs` | 1103 | `:497` | 607 | 55% |
| `kai/src/modules.rs` | 1005 | `:443` | 563 | 56% |
| `kai/src/zones.rs` | 456 | `:197` | 260 | 57% |
| `sim/src/view.rs` | 270 | `:133` | 138 | 51% |
| `importers/src/riftbound/query.rs` | 349 | `:157` | 193 | 55% |

`session.rs`'s production surface is **886 lines**, `log.rs`'s is **496** —
the numbers §2.3 and §2.4 should be read against.

- **`net/src/session.rs:887-1713` — 26 tests, 827 lines, and every one
  touches only the public surface.** `use super::*` pulls in nothing
  private. The module belongs in `net/tests/session.rs`. *Hygiene, S.*
- **Its riftbound half (`:1448-1713`, 5 tests) reaches across a
  dev-dependency edge from inside `src/`.** `agni-net` declares
  `agni-riftbound` in `[dev-dependencies]` (`net/Cargo.toml:15-16`) solely so
  `src/session.rs`'s inline tests can call
  `agni_riftbound::{zone_table, deal_plan, DeckFaces, ZONE_*}`. A library's
  `src/` should not be the reason a dev-dep on a *game crate* exists — it is
  the one thing agni-net must never know about. Move to `net/tests/`.
  *Structural, S.*
- **`sim/src/log.rs:497-1103` — 15 tests, mixed.** These legitimately reach
  the `Decider` seam (`:862`, `:891`) but ~90% could move; keep a small
  inline module for `validate_action` error-code coverage. *Hygiene, M.*
- **`net/tests/log_fold.rs` (545 lines, 15 tests) is a `sim` test wearing a
  `net` costume.** 12 of 15 assert `agni_sim::log` behaviour;
  `HostSession` appears only as a script generator (`scripted():46`,
  `riftbound_script():284`). Consequence: editing `sim/src/log.rs` does not
  re-run them unless `agni-net` rebuilds, and agni-sim's test surface looks
  thinner than it is. Move to `sim/tests/log_fold.rs` once the script
  builders live in the testkit. *Hygiene, M.*
- **`kai/src/modules.rs:443` nests `mod tests` inside
  `#[cfg(not(target_arch="wasm32"))] mod native`** — a third idiom for
  target-gating tests in one crate (`net.rs:838` uses
  `#[cfg(all(test, not(…)))]`, `import.rs:865` gates one function). Pick
  one. *Nit, S.*

### 6.3 Determinism-suite blind spots

Determinism assertions live in five places: `net/tests/log_fold.rs` (15),
`engine/host/tests/host.rs` (9), `net/tests/wasm_session.rs` (4),
`plugins/harden/tests/harden.rs` (15), `sim/src/engine.rs:198-331` (4).

**Asserted today** — and this is a genuinely strong suite: same log → same
`LogState` (`log_fold.rs:66,238,411`); arrival order irrelevant
(`:85,485`); invalid entries rejected identically at every replica across 10
error codes (`:123`); native ≡ wasmi-hosted, raw and hardened
(`host.rs:170 native_and_wasm_folds_are_byte_identical`); native
`HostSession` ≡ wasm-engine `HostSession` (`wasm_session.rs:110`); batched
`fold_log` ≡ entry-by-entry (`host.rs:192`); snapshot/restore
(`host.rs:210`); gas-trapped plugin → deterministic rejection with identical
snapshots (`host.rs:267`); the hardening pipeline byte-deterministic against
a **pinned golden hash** (`harden.rs:72,83`); float and SIMD opcodes rejected
(`harden.rs:188`).

**Not asserted:**

1. **`render_table` and `table_view` *are* checked for agreement — the
   §4.4 note that they are not is wrong, and the real problem is subtler.**
   `sim/src/view.rs:225 the_view_matches_render_table_for_every_seat` folds
   `scripted_state()` (`:149` — 7 entries, 3 cards, one hidden plugin zone)
   and asserts equality for `v ∈ 0..3`. But **the two are not equivalent by
   construction**: `table_view:69` computes
   `face_visible = granted || state.revealed.contains(&id)`;
   `render_table:485-490` consults **only** `zone_visibility` and never
   `revealed`. They agree solely because of an invariant maintained in two
   *other* places — `validate_action` refuses a `Reveal` into a hidden zone
   (`log.rs:697`) and `apply_action:348-355` sheds face *and* `revealed`
   membership on a move into a `ZoneVisibility::None` zone
   (`log.rs:649`, `log_fold.rs:474`). If either regresses, a card sits in
   `revealed` while in a hidden zone, `table_view` leaks its real face and
   `render_table` does not — and the 7-entry script never produces that
   state. Note also that `render_table` now has **zero production callers**
   (only `log.rs:1063,1065` and `view.rs:235`): it is a test oracle, and one
   weaker than the thing it audits. Fix: run the parity assertion over
   `riftbound_script()` (15+ entries, deck→battlefield reveals, a re-deck
   shed) or a generated action stream, and add a direct invariant check
   `revealed ∩ {cards in ZoneVisibility::None zones} == ∅` after every folded
   entry. *Structural, M.* **This supersedes §4.4's second paragraph.**
2. **No determinism assertion crosses the browser engine host.**
   `kai/src/engine_web.rs:321 impl Engine for WebEngine` is a full third
   implementation of the fold seam with no analogue of `host.rs:170`.
   "Same log → same table across engine hosts" holds for native ↔ wasmi and
   is **unverified** for native ↔ browser. *Structural, L* (needs
   `wasm-bindgen-test` + headless chrome).
3. **Nothing pins the CBOR encoding.** Every byte-stability test is a
   *round-trip*: `log_fold.rs:105 entries_reencode_byte_identically`, `:518`,
   `sim/src/wire.rs:190,215` all assert `encode(decode(encode(x))) ==
   encode(x)`. A `#[serde(rename)]`, a field reorder, an enum-variant
   insertion or a ciborium bump changes both sides together and every test
   still passes — while every recorded log becomes unreadable and every
   genesis pin shifts. The only golden byte vector in either tree is the
   harden hash at `harden.rs:83`. Fix: check in
   `sim/tests/goldens/{genesis,deal,move,reveal,annotate,game,reset}.cbor`
   plus one 15-entry log and assert `encode_entry(e) == include_bytes!(…)`.
   ***Structural, S — the cheapest high-value test in this review.***
4. **The RNG has no golden sequence.** `core/src/rng.rs:44
   same_seed_same_sequence` asserts `Rng::from_seed(42)` equals itself.
   Changing the xorshift constants (`rng.rs:22-26`) or the multiplier
   `0x2545_F491_4F6C_DD1D` passes every existing test while silently
   reshuffling every deck ever dealt. `the_dealers_shuffle_is_deterministic_and_seeded`
   (`session.rs:1564`) asserts `run() == run()` in one process; it never pins
   *which* permutation. `HostSession::deal_seed`'s blake3 domain separation
   (`session.rs:477`) is unpinned too. Fix: assert eight literal outputs and
   one literal permutation. *Structural, S.*
5. **Gas determinism is asserted only within one host build.**
   `harden.rs:124,144,166` compare two runs under the same wasmi;
   nothing asserts the *fuel count itself*. Since gas exhaustion produces a
   rejection **verdict** (`engine/host/src/lib.rs:263-268`), a shifted gas
   boundary is a consensus divergence between peers on different wasmi
   versions. Fix: pin `observations[0].1` to a literal. *Structural, S.*
6. **Float absence is enforced for plugins and explicitly disabled for the
   engine, and nothing tests the exception.** `HardenConfig::default()` sets
   `allow_floats: false` (`harden/src/lib.rs:57`); `HardenConfig::engine()`
   sets it **true** (`:71`) — necessarily, because `CardFace.tint: [f32;3]`
   flows through the fold (§4.2). So the determinism-critical module is the
   one permitted floats, and `harden.rs:188` exercises only the default
   config. Fix: walk the hardened *engine* module's code section and assert
   every F32/F64 opcode present is a load/store/copy — no `f32.add`, no
   `f32.convert`, nothing whose result could vary; and mirror
   `harden.rs:201,215,227,243` against `HardenConfig::engine()`.
   *Structural, M.* Disappears entirely if §4.2 lands.
7. **No test pins the hardened engine hash**, which goes into
   `TableConfig.engine` at genesis (`wasm_session.rs:165`) — so any
   harden-pipeline change invalidates every joiner's pin, and only the
   *hello-decider* hash under the *default* config is pinned. *Hygiene, S.*

### 6.4 Do the acceptance tests scale to a third game?

**No. ~1,030 lines of the acceptance suite are riftbound-specific by
construction, and the copy cannot even be written today.**

| file | lines | tests | riftbound-bound |
|---|---:|---:|---|
| `net/tests/riftbound_mvp.rs` | 511 | 1 | **100%** — `use agni_riftbound::{…ZONE_*}` (`:9-12`), `MAIN=40/RUNES=12/FIELDS=3` (`:25-27`), `assert_dealt_shape:216` and `assert_opening_hand:241` hardcode `"Chronicle Warden"`, `"Emberwing Vanguard"`, `"Bastion"`, `assert_eq!(manifest.name, "riftbound")` (`:266`) |
| `net/tests/log_fold.rs` | 545 | 15 | **53%** (`:258-545`) — 8 of 15 run off `riftbound_host()`/`riftbound_script()` |
| `net/src/session.rs` inline | 230 | 5 | **100%** |
| `net/tests/wasm_session.rs` | 188 | 4 | 0% — `WireZone::Hand`/`Board` only |
| `net/tests/net_smoke.rs` | 195 | 1 | 0% — and `#[ignore]`d |
| `engine/host/tests/host.rs` | 358 | 9 | 0% — `zone_decl():59` is a generic single "deck" |

Cost of adding MTG as things stand: duplicate ~1,030 lines per game, ~2,060
once abyss-walker lands. And it **cannot be done**: `games/mtg/src/lib.rs` is
31 lines and `games/abysswalker/src/lib.rs` is 22 — neither has
`zone_table()`, `deal_plan()`, `DeckFaces` or `OPENING_HAND_SIZE`. The suite
is not merely un-parameterized; there is nothing to parameterize over.

**The parameterization.** In the testkit (6.5):

```rust
pub trait GameFixture {
    const NAME: &'static str;
    fn zone_table() -> Vec<ZoneDecl>;
    fn deal_plan(prefix: &str) -> Vec<DealGroup>;
    fn opening_hand() -> usize;
    fn expected_counts(prefix: &str) -> Vec<(u16, usize)>;
    fn public_names(prefix: &str) -> Vec<String>;   // MUST appear in encode_log
    fn secret_names(prefix: &str) -> Vec<String>;   // MUST NOT appear in encode_log
    fn hand_zone() -> u16;
    fn deck_zone() -> u16;
    fn play_zone() -> u16;
}
```

Every return type is `agni_sim::wire` or plain — no `agni_riftbound::` type
crosses the boundary, which is what lets agni-net drop its game dev-dep
(6.2). `net/tests/acceptance.rs` then becomes table-driven over
`[Riftbound, Mtg, AbyssWalker]` via a `for_each_game!` macro and **19 test
bodies become shared**: the 250-line
`two_seats_play_the_riftbound_mvp_through_the_hardened_plugin:261` splits
into seven (`the_plugin_manifest_matches_the_zone_table`,
`the_genesis_pins_engine_and_plugin_hashes`,
`a_joiner_fetches_and_verifies_the_pinned_plugin`,
`a_deal_replicates_with_faces_only_where_visibility_allows`,
`hidden_zone_faces_never_enter_the_shared_log`,
`annotations_replicate_to_every_seat`,
`a_foreign_hand_intent_is_refused`); `log_fold.rs` contributes eight
(`:359`, `:380`, `:411`, `:436`, `:454`, `:485`, `:518`, `:532`); the
session inline module contributes three (`:1485`, `:1564`, `:1630`). The
genuine riftbound residue is one test —
`a_second_deck_balances_the_shared_battlefields:1615`, shared-zone semantics
no other game has — which stays in a small `net/tests/riftbound_only.rs`.

*Structural.* Effort **L** for the refactor plus an unavoidable **L** to give
`agni-mtg` a real zone table and deal plan. **Land `GameFixture` and route
riftbound through it now (M), while there is still only one game to convert.**

### 6.5 Test scaffolding

**The scratch-store helper is duplicated 8 times, not 7 —
`kai/src/modules.rs:449` is the missed one — and every copy leaks its
directory on panic.** All are
`temp_dir().join(format!("<tag>-{}", process::id()))` with cleanup as a
statement at the end of the test body rather than a `Drop`:

| site | tag | cleanup sites |
|---|---|---|
| `plugins/harden/tests/harden.rs:302` | `agni-harden-publish` | `:335` |
| `plugins/harden/tests/harden.rs:340` | `agni-harden-noname` | `:355` |
| `engine/host/src/lib.rs:343-346` | `agni-store-source-{tag}` | `:375,393,403` |
| `importers/src/riftbound/gateway.rs:59-64` | `agni-riftbound-gateway-{tag}` | `:140,150` |
| `importers/src/riftbound/ingest.rs:330-335` | `agni-riftbound-ingest-{tag}` | `:385,386,454,470` |
| `net/tests/riftbound_mvp.rs:125-127` | `agni-riftbound-mvp` | `:510` |
| `kai/src/import.rs:869-870` | `kai-import-store` | `:919` |
| **`kai/src/modules.rs:449-452`** | `kai-modules-test-{tag}` | `:492,503,516,534,548,561,574,587,602` |

Three real consequences: a failing test leaves a `BlobStore` in `/tmp` that
the next run's `remove_dir_all` silently reuses-then-deletes, and CI runs on
a persistent `local` agent (`.woodpecker/kai.yml:8`) where `/tmp` is not
fresh; `process::id()` is constant within a test binary, so uniqueness rests
entirely on the tag string and only nextest's process-per-test saves the
untagged copies; and **`synthetic_deck` exists twice under one name with two
shapes** — `net/src/session.rs:1448` builds an 8-card main deck,
`net/tests/riftbound_mvp.rs:105` builds 40, so the shuffle-determinism test
at `session.rs:1564` exercises an 8-card permutation.

**Counted duplication beyond that:**

| helper | copies | locations |
|---|---:|---|
| `faces(prefix, n)` (byte-identical) | 4 | `session.rs:893`, `log_fold.rs:13`, `net_smoke.rs:14`, `wasm_session.rs:58` |
| `face(name)` | 4 | `view.rs:139`, `host.rs:71`, `riftbound_mvp.rs:95`, `session.rs:1449` |
| `in_area(seat, zone)` | 5 | `session.rs:905`, `:1480`, `log_fold.rs:280`, `net_smoke.rs:26`, `riftbound_mvp.rs:187` |
| wasm build-and-harden bootstrap (~45 lines) | 3 | `host.rs:17-57`, `wasm_session.rs:16-56`, `riftbound_mvp.rs:30-93` |
| `riftbound_host()`, divergent | 2 | `session.rs:1468` (no pins), `log_fold.rs:258` (fake `blob:test-*` pins) |
| scripted log builder, divergent | 3 | `sim/src/engine.rs:204` (5 entries), `host.rs:81` (9), `log_fold.rs:46` |

**The wasm bootstrap duplication is the dangerous one, and it is very
likely what crashed this box.** All three copies shell out to
`Command::new(cargo).args(["build","-p",…,"--target",
"wasm32-unknown-unknown","--release"])` with **no `-j` cap** when
`AGNI_ENGINE_WASM`/`AGNI_RIFTBOUND_WASM` are unset (`host.rs:26-38`,
`wasm_session.rs:25-37`, `riftbound_mvp.rs:39-51`). CI sets both
(`flake.nix:262-263`), so CI is safe — but a bare local
`cargo test -p agni-net` fires up to three nested unbounded release builds.
*Structural on the local-dev hazard alone.*

**An `agni-testkit` dev-dependency crate is warranted.** New workspace member
`agni/agni/testkit/`, `publish = false`, exposing exactly:

1. `scratch::ScratchDir` — RAII newtype over `PathBuf` with
   `impl Drop { remove_dir_all }`, named from `process::id()` **plus** a
   `static AtomicU64`; `scratch_store(tag) -> (ScratchDir, BlobStore)`.
   Replaces 8 sites and 18 manual cleanups, and fixes the panic-leak for free.
2. `modules::{engine_wasm, riftbound_plugin, hardened_engine, hardened_plugin}`
   — one `OnceLock` each, reading `AGNI_ENGINE_WASM`/`AGNI_RIFTBOUND_WASM`
   and panicking with an actionable message if unset, **never shelling out to
   cargo**. Have `devenv.nix` export both in the dev shell.
3. `faces::{faces, face, wire_face, tinted}`.
4. `table::{area, plugin_zone, name_of}`.
5. `script::{trivial_log, hosted}` — one canonical 9-entry log replacing the
   three divergent ones.
6. `game::{GameFixture, Riftbound}` + the `for_each_game!` macro (6.4).
7. `golden::assert_golden(name, bytes)` over `testkit/goldens/*.cbor` with
   `AGNI_BLESS=1` to rewrite — the mechanism 6.3's items 3, 4, 5 and 7 all
   need.

Dev-dep it from `agni-sim`, `agni-net`, `agni-engine-host`, `agni-harden` and
`kai`. Cargo permits dev-dependency cycles, so `agni-engine-host`
dev-depending on a testkit that depends on it is legal; keep the
wasmi/harden helpers behind a `wasm` feature. *Structural, M* for the crate
plus scratch/faces/modules migration, **L** including `GameFixture`.

*Related:* `plugins/harden/tests/fixtures/hello_decider_guest.wasm` (900
bytes, checked in, whitelisted at `flake.nix:31`) is the compiled output of
`plugins/spike/hello-decider/guest/`, which declares its own `[workspace]`
and is therefore never built by anything. Nothing regenerates or verifies the
fixture against its source, and `harden.rs:83` pins a golden hash derived
from it. *Hygiene, S.*

### 6.6 kai's test story

**38 tests over 7,543 lines, concentrated in 5 of 17 files, and every one
runs on exactly one configuration.**

| file | lines | tests | notes |
|---|---:|---:|---|
| `zones.rs` | 456 | **13** | every layout (`Pile`/`Row`/`Grid`/`Spread`/`Fan`), both rotations, exhaust rotation, `draw_move`/`trash_move`, `zone_line`. The model for the rest of kai |
| `modules.rs` | 1005 | 9 | **pin resolution is well covered** — selection precedence `:475`, store-over-bundle `:496`, loud fallback `:507`, `a_broken_selection_is_loud_not_silently_substituted:520`, `pinned_bytes_come_from_the_store_by_hash:552`, `a_tampered_store_blob_is_refused_not_loaded:578`. But all 9 sit inside `#[cfg(not(wasm32))] mod native`; the **wasm32 module (`:607-927`, 321 lines, including a second `prepare_join:824`) has zero tests** |
| `import.rs` | 952 | 7 | `parse_reply` is covered (`:762`, `:784`); `art_by_names:229`, `wanted_art:263`, `apply_store_art:379` are not |
| `lib.rs` | 1806 | 7 | layout math partly covered — `seat_yaw`/`seat_center` (`:1736`), `board_slot` (`:1750`, `:1763`), `deal_origin`+`hand_slot` (`:1777`), `rotation_for`+`camera_pose` (`:1790`). Untested: `fan_back_slot:783`, `zoom_camera:850`, `apply_zoom:878`, `animate_cards:898`, `guarded_from_me:1054`, `my_hand_ids:367` |
| `net.rs` | 972 | 2 | only `deals_legacy_hands`/`host_from_with`. `handle_host_msg:517`, `try_finish_join:449`, `route_drops:568`, `route_redeal:631`, `route_deck_deals:687`, `deck_already_dealt:125` all untested |
| `telemetry.rs`, `engine_web.rs`, `bridge.rs`, `peers.rs`, `node.rs`, `identity.rs`, `app.rs`, `tuning.rs`, `sync.rs`, `foil.rs`, `engine.rs` | 3,297 | **0** | see 6.1 |

**No kai test runs on zero configurations — but the target gates are
decorative.** All 38 run on `x86_64-linux` native via
`nix build .#checks.x86_64-linux.kai-nextest`. There is no wasm or android
test runner anywhere: `kai/Cargo.toml` has no `[dev-dependencies]`, no
`wasm-bindgen-test`, no `kai/tests/`. The gates' only real effect is
**hiding compile errors in wasm-gated test code**, because `kai-web-clippy`
runs `--lib --bins` (`flake.nix:276`), not `--all-targets`.

Recommendations: `wasm-bindgen-test` + a headless-chrome lane for
`engine_web.rs`, at minimum a fold-parity test mirroring `host.rs:170`
(*structural, L*); change `kai-web-clippy` to `--all-targets` (*hygiene, S*);
unit tests for `telemetry::{parse_kv, sanitize, push_body}` and
`net::{deck_already_dealt, try_finish_join}` (*hygiene, S/M*); standardize on
plain `#[cfg(test)]` (*nit, S*).

### 6.7 CI reality check

Woodpecker, config at `/.woodpecker/*.yml` (9 pipelines); everything Rust
routes through `orgs/andrea/projects/flake.nix`. No `.github/workflows`,
nothing under `agni/agni`.

| pipeline | trigger | runs |
|---|---|---|
| `lint.yml` | every push/PR, unscoped | `devenv shell -- fmt-check` → treefmt incl. rustfmt. The only fmt gate, repo-wide |
| `agni.yml` | `agni/agni/**`, `spirit/spirit/**`, `flake.*` | `nix build .#checks.x86_64-linux.{agni-clippy,agni-nextest}` |
| `kai.yml` | `agni/kai/**`, `agni/agni/**`, `spirit/spirit/**`, `flake.*` | `{kai-clippy,kai-nextest,kai-web-clippy}`, then `nix build .#kai .#kai-web` |
| `agni-artifacts.yml` | **push to `main` only** | native + web builds, then the android APK — **only if `kai_android_signing` is present** |

`agni-clippy` is `--workspace --all-targets -- --deny warnings`
(`flake.nix:225`); `agni-nextest` points `AGNI_ENGINE_WASM` /
`AGNI_RIFTBOUND_WASM` at the raw wasm artifacts (`:262-263`) so tests harden
in-process instead of shelling out. Both x86_64-linux only.

**What the review must not assume is covered:**

1. **`net_smoke.rs:41 host_and_client_converge_over_iroh` is `#[ignore]`d
   and nextest is not given `--run-ignored`.** It is the only test that
   exercises `net/src/table.rs`, `net/src/bridge.rs`, real iroh endpoints
   and `ClientSession::optimistic` over the wire. **The entire transport
   layer is untested in CI.** *Structural, M.*
2. **No wasm32 test execution anywhere**, and agni's wasm32 test targets
   cannot even compile: `net/tests/wasm_session.rs` and
   `net/tests/riftbound_mvp.rs` `use agni_engine_host` / `agni_harden`,
   declared only under
   `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`
   (`net/Cargo.toml:18-20`), and neither carries the
   `#![cfg(not(target_arch="wasm32"))]` guard `net_smoke.rs:1` has.
   *Hygiene, S.*
3. **No android build or check on any PR.** `sync.rs` plus the android arms
   of `telemetry.rs` (6 cfg sites), `app.rs` (3), `lib.rs` (3), `node.rs`,
   `identity.rs`, `import.rs`, `modules.rs`, `net.rs` compile only in
   `agni-artifacts.yml`, post-merge, skipped without the signing secret. A
   commit that breaks android merges green. *Structural, M* — add a
   `cargo check --target aarch64-linux-android` lane; it needs no key.
4. `flake.nix:17` declares `aarch64-linux` but every `nix build` hardcodes
   `x86_64-linux`.
5. **No doctests** anywhere (consistent with the no-comments rule) — nextest
   cannot run them, so if anyone adds one it will never run. Stated so no
   reviewer credits doc examples as coverage.
6. No coverage measurement and no `--no-tests=fail`: a test module
   accidentally `#[cfg]`-ed out drops to zero tests silently.

## 7. Dependency and build-graph audit

### 7.1 The inventory and the internal DAG

| crate | normal deps | dev / build |
|---|---|---|
| `agni-core` (`core/Cargo.toml:6-8`) | `serde`(derive), `spirit-sdk`(path) | — |
| `agni-sim` (`sim/Cargo.toml:6-10`) | `agni-core`, `ciborium`, `serde`, `serde_bytes` | — |
| `agni-net` (`net/Cargo.toml:6-38`) | `agni-core`, `agni-sim`, `blake3`, `ciborium`, `n0-future`, `serde`, `serde_bytes`, +`spirit-node`/`tokio` per target | dev: `agni-riftbound`; cfg(!wasm) dev: `agni-engine-host`, `agni-harden` |
| `agni-engine-wasm` | `agni-sim`, `serde_bytes` | — |
| `agni-engine-host` (`engine/host/Cargo.toml:6-17`) | `agni-sim`, `blake3`, `serde`, `serde_bytes`, `spirit-core`, `wasmi` | dev: `agni-harden`, `ciborium`, `wat` |
| `agni-harden` (`plugins/harden/Cargo.toml:6-16`) | `blake3`, `radix-wasm-instrument`, `spirit-core`, `wasm-encoder`, `wasmparser` | dev: `wasmi`, `wat` |
| `agni-importers` | all optional: `agni-riftbound`, `ciborium`, `serde`, `serde_json`, `spirit-core`, `spirit-node`, `ureq` | — |
| `agni-riftbound` | `agni-core`, `agni-sim` | — |
| `agni-riftbound-plugin` | **none** | build+dev: `agni-riftbound`, `agni-sim` |
| `agni-plugins`, `agni-mtg`, `agni-abysswalker` | **none** | — (7.7) |

**The DAG is acyclic, correctly layered, and the constitution's one-way rule
holds.** `grep -rn agni spirit/spirit` returns exactly two hits — a string
literal at `node/src/mesh.rs:1034` and a doc comment at `sdk/src/lib.rs:9`.
Spirit does not know about agni. *Verified, no action.*

**7.1.1 — `agni-core`, the declared pure data layer, carries a 24-crate
content-mesh SDK it never calls, and that edge has the widest blast radius
in the tree.** *Structural, effort S.* `core/Cargo.toml:8` declares
`spirit-sdk`; the only two references in the crate are
`core/src/lib.rs:1` (`pub use spirit_sdk;`) and a `#[cfg(test)]` `use` inside
`fn spirit_surface_is_reachable()` at `:203` **whose body is empty**.
`AGENTS.md:69-72` documents this as a deliberate link canary. Measured cost:

- `cargo tree -e normal -p agni-core` = **25 crates**; without spirit-sdk,
  **7**. Eighteen crates for a canary.
- It propagates to **8 of the 12** agni crates. `agni-engine-wasm` — the
  zero-import CBOR guest — compiles 28 crates, 9 of them
  (`spirit-sdk`, `spirit-core`, `spirit-index`, `spirit-routing`,
  `spirit-schema`, `blake3`, `arrayvec`, `constant_time_eq`, `cpufeatures`)
  solely through this edge. `cpufeatures` and blake3's build script are being
  compiled **for wasm32** to satisfy an unused re-export.
- The re-export is load-bearing in exactly one direction: kai reaches
  spirit-core *through* agni-core at four sites — `kai/src/app.rs:12`,
  `kai/src/import.rs:213`, `:230`, `:868`
  (`use agni_core::spirit_sdk::spirit_core::{BlobHash, BlobStore}`). kai
  already declares `spirit-node` directly but not `spirit-core`.

Recommendation: add `spirit-core` to kai's `[dependencies]` and rewrite those
four `use` lines; move `spirit-sdk` to `agni-core`'s `[dev-dependencies]` and
delete `core/src/lib.rs:1`. The canary is already `#[cfg(test)]` and CI runs
`--workspace --all-targets` plus nextest, so a broken spirit surface still
fails on the same commit.

**7.1.2 — `agni-harden`'s *library* graph contains the blob layer.**
*Hygiene, effort S.* `plugins/harden/Cargo.toml:9` declares `spirit-core`,
but the use is entirely in `main.rs:2-3` (`publish_module`, `BlobStore`).
This is the crate whose output must stay byte-reproducible; its library
graph should be as small as it can be. Make it
`spirit-core = { optional = true }` behind a `publish` feature that
`[[bin]] required-features` demands, taking the library to 5 direct deps.
(`agni-engine-host`'s `spirit-core` use is real — `StoreSource`, 4 sites —
and stays.)

### 7.2 Heavy deps relative to use

Measured as `cargo tree -e normal -p <crate> --prefix none | sort -u | wc -l`
against grepped call sites.

| dep | subtree | sites | verdict |
|---|---:|---:|---|
| `wasmi` 1.1.0 | 17 | 4 | **trim** |
| `radix-wasm-instrument` =1.0.0 | 17 | 1 | justified — that import *is* the gas-metering + stack-limiter pipeline |
| `wasmparser` =0.107.0 | 0 | 10 | justified |
| `wasm-encoder` =0.29.0 | leaf | 11 | justified |
| `blake3` | 5 | 23 | justified — cheapest possible content hash |
| `ciborium` | 15 | 28 | justified in sim/net/importers; see below for kai |
| `serde_json` | 5 | 19 | justified |
| `ureq` 2.12 | **64** | 10 agni, **1** kai | justified in importers; **trim in kai** |
| `spirit-node` | **307** | 8 agni-net, **1** agni-importers | see 7.3 |
| `n0-future` | 42 | 4 | justified — every crate in its subtree already arrives via iroh, and it is the wasm-portable spawn/timeout agni-net needs |
| `qrcode` | 1 | 2 | justified, already `default-features = false` |
| `bevy` 0.19 | **371 of kai's 622 native crates (60%)** | 32 | **trim** (7.3) |
| `bevy_egui` 0.42 | 357 | 121 | justified — this is the entire UI |

**7.2.1 — `wasmi`'s default features drag the WAT text parser into every
release build of the host.** *Hygiene, effort S.* `engine/host/Cargo.toml:12`
takes wasmi's defaults, which are `["std", "wat"]`; that pulls `wat` →
`wast` → `bumpalo`, `leb128fmt`, `memchr`, `unicode-width`,
`wasm-encoder 0.258.0` — seven crates — and is the **sole source** of
`wasmparser 0.258.0` and `wasm-encoder 0.258.0` in `agni/Cargo.lock`.
Nothing in `engine/host/src/` parses WAT; `wat` is already an explicit
`[dev-dependencies]` entry (`engine/host/Cargo.toml:17`,
`plugins/harden/Cargo.toml:16`) for the tests that do. Fix:
`wasmi = { version = "1", default-features = false, features = ["std"] }` in
both places. Cuts the host's normal graph 43 → ~36 and removes two of the
four `wasmparser` versions.

**7.2.2 — kai compiles a second HTTP client and TLS stack to send one Loki
POST.** *Hygiene, effort S.* `kai/Cargo.toml:44` declares `ureq`; the only
use is `kai/src/telemetry.rs:424`. **`reqwest v0.13.4` is already in kai's
native graph** via iroh/iroh-relay through spirit-node, with rustls 0.23
already built. ureq's marginal cost is modest (`ureq`, a
`webpki-roots 0.26.11` shim over the already-present `1.0.9`, `base64`) but
it is a redundant stack in a client that ships. Route the push through the
present reqwest, or put the native shipper behind a `telemetry-loki`
feature that is off in dev builds.

**7.2.3 — kai declares `ciborium` for two call sites that `agni_sim::abi`
already wraps.** *Nit, effort S.* `kai/Cargo.toml:19`; sites are
`kai/src/app.rs:155` and `kai/src/import.rs:897`.
`agni_sim::abi::{encode, decode}` (`sim/src/abi.rs:9-17`) is exactly those
two operations, and §3.6 is consolidating on them anyway.

### 7.3 Feature-flag hygiene

Every `[features]` table in both trees: `agni-importers`
(`importers/Cargo.toml:17-28`) — `default = ["scryfall",
"riftbound-gateway"]` over `scryfall`, `riftbound`, `riftbound-native`,
`riftbound-gateway`; and `kai` (`kai/Cargo.toml:69-70`) —
`fast-compile = ["bevy/dynamic_linking"]`, no default. **Nothing else in
either workspace has a feature table.**

**Which combinations are built:** agni CI builds the workspace with default
features, so `scryfall + riftbound-gateway` is the only agni-side
combination exercised. `riftbound-native` alone comes from
`kai/Cargo.toml:27-29` and bare `riftbound` from `:48-50`, both reached only
through `kai-clippy` / `kai-web-clippy`. **`scryfall` alone,
`riftbound-gateway` without `riftbound-native`, and kai's `fast-compile` are
never built anywhere.** *Hygiene.*

**7.3.1 — `agni-importers`' default enables the feature that pulls the
307-crate iroh stack, for one call site.** *Structural, effort S.*
`importers/Cargo.toml:18` turns on `riftbound-gateway` =
`["riftbound-native", "dep:spirit-node"]`, and `spirit_node` appears exactly
once in the crate (`importers/src/riftbound/gateway.rs:5`).
`cargo tree -p agni-importers` = **322 crates**; the pure parser layer
`agni-riftbound` = 28. A library defaulting to its own binaries' network
features is the wrong default, and kai already writes
`default-features = false` twice to escape it. Fix: `default = []` — the
three `[[bin]] required-features` entries (`:36`, `:41`, `:46`) already
drive the binaries — and add `--features scryfall,riftbound-gateway` to the
nextest invocation (`flake.nix:264`) so coverage does not regress. **This
sets the cost floor for `cargo build` across the whole workspace.**

**7.3.2 — bevy is taken with full defaults and ships four subsystems kai has
zero references to.** *Hygiene, effort M.* `kai/Cargo.toml:17` is
`bevy = { version = "0.19", features = ["jpeg", "android-game-activity"] }`
— additive on top of `default`, no `default-features = false`. Resolved: 75
bevy features, **64 `bevy_*` crates**, 371 crates. Grepped against
`kai/src`, all zero hits: `AudioPlayer`, `Gltf`, `AnimationPlayer`,
`DynamicScene`/`SceneRoot`, `Sprite`, `Gizmos`, `Gamepad`,
`Text2d`/`TextFont`, `Tonemapping`, `Smaa`/`Fxaa`, `ktx2`,
`BackgroundColor`. What kai *does* use is genuinely 3D — `Camera3d`,
`Mesh3d`, `StandardMaterial`, `PointLight`, `ExtendedMaterial`
(`lib.rs:496,517,626`, `foil.rs:6`) — plus `MeshPickingPlugin`
(`lib.rs:68`) and twelve `Pointer<…>` handlers, so `bevy_pbr` / `3d` /
`bevy_picking` must stay. Cut list:

`bevy_audio`, `vorbis` (removes `cpal`, 14-crate subtree) · `bevy_gltf`,
`gltf_animation` · `bevy_animation`, `morph`, `morph_animation` ·
`bevy_scene`, `scene`, `bevy_world_serialization` · `bevy_sprite`,
`bevy_sprite_render`, `sprite_picking` · `bevy_gizmos`,
`bevy_gizmos_render` · `bevy_gilrs`, `gamepad` (removes `gilrs` +
`gilrs-core`) · `bevy_anti_alias`, `smaa_luts`, `tonemapping_luts`,
`bevy_post_process` (also drops the embedded LUT blobs from the wasm
bundle) · `bevy_mikktspace` · `ktx2`, `zstd_rust` · `sysinfo_plugin` · and,
if `bevy_egui`'s optional `bevy_ui` is turned off, `bevy_ui`,
`bevy_ui_render`, `bevy_ui_widgets`, `bevy_text` — kai's `Node`
(`node.rs:10`) is its own struct, not bevy's.

That is ~16 `bevy_*` crates plus 5 third-party leaves out of 64/371, and it
shrinks the wasm bundle directly: on wasm32, `cpal` and `gilrs-core` are the
crates forcing the `AudioContext*`/`AudioBuffer*`/`Gamepad*` web-sys
features into the build. Verify with `cargo tree -p bevy -f "{f}"` before
and after, once per target.

**7.3.3 — `android-game-activity` is enabled for every target.** *Nit,
effort S.* It sits in the shared feature list (`kai/Cargo.toml:17`), so
desktop and wasm resolve it too. Move it into a
`[target.'cfg(target_os = "android")'.dependencies]` bevy stanza beside the
existing android block.

**7.3.4 — `web-sys`'s list is correct and one entry is dead.** *Nit.*
`kai/Cargo.toml:60-67` enables six features; `Headers` has zero
`web_sys::Headers` references. Note that on wasm32 the *resolved* web-sys
feature set is 160 features across 19 reverse dependents (`bevy_render`,
`wgpu`, `winit`, `egui`, `reqwest`, `glow`, `cpal`, `gilrs-core`, …), so
kai's six add nothing measurable either way. Drop `Headers`.

**7.3.5 — `default-features = false` on the spirit/tokio edges is applied
consistently and correctly.** *Verified, no action.* `net/Cargo.toml:23,26,34,35`;
`kai/Cargo.toml:20,27,33,40,48,51,55,56`; `importers/Cargo.toml:12`. wasm32
gets tokio with only `sync`+`macros` and spirit-node without `native`. This
is the one part of the feature story that is unambiguously right, and it is
what §5E.4 points at as the pattern to copy.

### 7.4 Rebuild blast radius and build scripts

`cargo tree -i`, all edge kinds:

| touched | agni crates rebuilt | plus |
|---|---|---|
| `agni-core` | **7** (sim, net, engine-host, engine-wasm, importers, riftbound, riftbound-plugin) | kai |
| `agni-sim` | **6** | kai |
| `spirit-core` | **14** (8 agni consumers + 5 spirit crates) | kai |
| **`spirit-sdk`** | **8 — entirely via `agni-core`'s re-export** | kai |
| `agni-riftbound` | 3 | kai |
| `agni-harden` | 2 (dev edges) | — |
| `agni-net`, `agni-importers`, `agni-engine-wasm`, `agni-riftbound-plugin` | 0 in-workspace | kai |

The suspects are confirmed — `agni-core` (344 lines) and `agni-sim` sit
under 7 and 6 downstream crates plus all of kai. **The non-obvious result is
that `spirit-sdk` has the same blast radius as `agni-core` itself**: any edit
anywhere in `spirit/spirit` rebuilds 8 agni crates and kai, purely through
7.1.1's unused re-export. Same fix, same effort.

**7.4.1 — The one build script in the tree is correct.** *Verified, no
action.* `plugins/riftbound/build.rs:1-32` is pure Rust — it calls
`agni_riftbound::zone_table()` and the `agni_sim::wire`/`abi` encoders and
writes three CBOR blobs to `OUT_DIR`, consumed by `include_bytes!` at
`plugins/riftbound/src/lib.rs:3-5`. It emits
`cargo:rerun-if-changed=build.rs` (`:31`), which correctly narrows the
default whole-package watch to the one file whose content matters: edits to
`plugins/riftbound/src/` do not re-run it, and edits to
`agni-sim`/`agni-riftbound` still do because cargo rebuilds the build-script
binary when a build-dependency changes. No missing directive, no
`Command::new`. Its host graph is 32 crates to emit three small blobs — so
cross-compiling the plugin to wasm32 first requires a full host build of
agni-sim → agni-core → spirit-sdk → spirit-{core,index,routing,schema} →
blake3 → ciborium. 7.1.1 takes that 32 → ~23 for free.

**7.4.2 — No dev-only crate is declared as a normal dependency.**
*Verified.* Every declared dep has at least one non-test use site in its own
crate. `agni-riftbound-plugin` is the model case: `agni_riftbound`/`agni_sim`
appear only in `build.rs` and a `#[cfg(test)]` module, and the manifest
declares them only as `[build-dependencies]` and `[dev-dependencies]`.

**7.4.3 — kai's `crate-type = ["lib", "cdylib"]` links the client twice on
any bare `cargo build`.** *Nit, effort S.* `kai/Cargo.toml:81`. The cdylib
exists for the android JNI entry point and crate-type cannot be cfg-gated.
CI dodges it (`flake.nix:163`, `:216` both pass `--bin kai`), but
`cargo clippy --workspace --all-targets` and every local `cargo build` pay
for a second link of a 371-crate graph. Add `[alias] b = "build --bin kai"`
to `kai/.cargo/config.toml`, which currently holds only the mold config.

### 7.5 Pinning and reproducibility

**7.5.1 — The harden pins are correct, complete, and test-enforced.**
*Verified, no action.* `plugins/harden/Cargo.toml` pins every crate that can
influence output bytes with `=`: `radix-wasm-instrument = "=1.0.0"` (`:8`),
`wasm-encoder = "=0.29.0"` (`:10`), `wasmparser = "=0.107.0"` (`:11`) —
matching radix-wasm-instrument 1.0.0's own declared versions exactly, which
is what keeps `Operator`/`Payload`/`ValType` type-compatible across the seam
at `plugins/harden/src/lib.rs:8`. `blake3 = "1"` is correctly left on a
caret (BLAKE3 output is spec-fixed). `wasmi`/`wat` are dev-only and cannot
reach the output. Backing this, `plugins/harden/tests/harden.rs:72-83`
asserts the hardened digest of the committed fixture equals a literal
`f30864…96c9d`. **A patch bump in any of the three cannot silently change
instrumented bytes** — it either fails to resolve or fails that test. This
answers §4.11's pinning question: the pinning is already right.

**7.5.2 — The gap is upstream of the pins: the compiler is not pinned, and a
nightly job bumps it.** *Hygiene, effort S.* `flake.nix:24` is
`pkgs.rust-bin.stable.latest.default`, there is no `rust-toolchain.toml`
anywhere under `orgs/andrea/projects`, and `.woodpecker/lockfiles.yml` runs
`nix flake update` nightly against `rust-overlay` and pushes to main. A
rustc bump changes the bytes of `agni_engine_wasm.wasm` and
`riftbound_plugin.wasm` *before* harden sees them, so published module
content hashes rotate overnight with no code change. The harden pins protect
the transform, not its input. No module hash is hardcoded in source (the only
64-hex literals outside lockfiles are the harden golden, whose input is a
committed `.wasm`, and two spirit node ids at `kai/src/bridge.rs:21,26`), so
this is churn and auditability rather than breakage. Fix: a
`rust-toolchain.toml` with an explicit channel, and point `flake.nix:24` at
`rust-bin.fromRustupToolchainFile`, so hash rotation becomes an intentional
commit.

**7.5.3 — Duplicate versions.** agni's lock has 491 packages / 31 duplicated
names; kai's has 843 / 47. Top offenders:

| crate | versions | source | dedupable |
|---|---|---|---|
| `wasmparser` | 0.107.0, 0.121.2, 0.239.0, **0.258.0** | harden pin / radix's `wasmprinter` / wasmi / `wast` ← wasmi's default `wat` | **0.258.0 yes**, via 7.2.1; the others no |
| `wasm-encoder` | 0.29.0, **0.258.0** | harden pin / `wast` | **yes**, same fix |
| `syn` | 1.0.109, 2.0.119, 3.0.4 | three upstream generations | no |
| `indexmap` | 1.9.3, 2.14.1 | `wasmparser 0.107.0` (the pin) / everything else | no — accepted cost of the pin; also drags `hashbrown 0.12.3` |
| `webpki-roots` | 0.26.11, 1.0.9 | `ureq 2` / `iroh-relay` | **yes** via 7.2.2, small win |
| `windows-*` (11 names), `thiserror`, `getrandom` | — | transitive | no |

kai adds a winit-generation split worth naming but not actionable from this
repo — `rustix 0.38/1.1`, `calloop 0.13/0.14`,
`smithay-client-toolkit 0.19/0.20`, `objc2 0.5/0.6` and four siblings,
`linux-raw-sys 0.4/0.12` — all from `winit 0.30.13` and `sctk-adwaita`
holding the old line. It resolves when bevy bumps winit.

The single actionable dedupe is 7.2.1's wasmi trim, which removes
`wasmparser 0.258.0`, `wasm-encoder 0.258.0`, `wast`, `wat`, `bumpalo`,
`leb128fmt` and `unicode-width` from the normal graph.

### 7.6 Workspace structure

**Keep the split — it is correct — but the drift it causes is unchecked.**
kai depends on agni by path but is deliberately not a member of the agni
workspace, and the reason holds: `agni/AGENTS.md:60-63` requires agni-core
and agni-sim to build for an e-ink firmware consumer that cannot take bevy.
Merging would put a 371-crate bevy graph in the same workspace as the pure
layers, so `cargo test --workspace` would build bevy to run `agni-core`'s
unit tests, and feature unification would risk leaking bevy-driven feature
choices into the deterministic crates. The Nix build depends on the split
too (`flake.nix:64-70` vs `:147-153` are separate `cargoLock`/`cargoToml`
roots with separate `buildDepsOnly` artifacts).

Measured costs:

- **444 shared crates are compiled twice.** Separate target dirs and
  separate Nix `cargoArtifacts` (`agniDeps` vs `kaiDeps`) mean serde,
  ciborium, blake3, tokio and the whole 307-crate spirit-node/iroh stack are
  built twice in CI. Inherent to the split; the attic binary cache
  (`.woodpecker/agni.yml:34-40`, `.woodpecker/kai.yml:38-46`) is the right
  mitigation and is already in place.
- **7.6.1 — Four crates have genuinely drifted between the two lockfiles.**
  *Hygiene, effort S.* `flate2` agni 1.1.10 / kai **1.1.9**; `miniz_oxide`
  0.9.1 / **0.8.9**; `indexmap` 2.14.1 / **2.14.0**; `n0-error` 1.0.1 /
  **1.0.0**. `n0-error` is the one that matters — it is part of the
  iroh/spirit-node error surface both workspaces link, so agni's tests and
  kai's runtime exercise two different builds of the same shared transport
  dependency. Nothing enforces agreement:
  `.woodpecker/lockfiles.yml` refreshes only `flake.lock` files
  (`git ls-files '*/flake.lock'`), never `Cargo.lock`. Fix: a CI step that
  diffs the shared-crate version sets of the two lockfiles and fails on
  disagreement — ~15 lines of awk, and it would have caught all four.

**7.6.2 — kai has no `[workspace]` table at all.** *Nit, effort S.*
`kai/Cargo.toml` opens at `[package]` and relies on no parent directory up
to `/` containing a `Cargo.toml`. A manifest added at `apps/desktop/`,
`apps/`, or `orgs/andrea/projects/` would silently absorb kai and invalidate
`kai/Cargo.lock`. The spike crates get this right
(`plugins/spike/hello-decider/{guest,host}/Cargo.toml:1` both open with a
bare `[workspace]`). Add `[workspace]` + `resolver = "2"`.

### 7.7 Dead weight

**7.7.1 — Three workspace members have zero dependencies, zero dependents,
and zero consumers anywhere in the monorepo.** *Hygiene, effort S.*
`cargo tree -i` returns 0 for `agni-plugins`, `agni-mtg` and
`agni-abysswalker`; a repo-wide grep hits only their own manifests plus four
documentation lines (`agni/AGENTS.md:55,57,58`,
`wiki/design/architecture.md:138-139`). All three are nonetheless built and
tested on every `agni-clippy`/`agni-nextest` run.

- `agni-mtg` — 805 bytes: `CardName(String)`, `MtgDeck`, two consts, one test.
- `agni-abysswalker` — 481 bytes: `CardName(String)`, `AbyssWalkerDeck`, one test.
- `agni-plugins` — 132 lines, 50 of them tests (§4.10).

`AGENTS.md:57-58` and `architecture.md:139` describe mtg and abysswalker as
deliberate "placeholder establishing the pattern" crates — intent, not
accident. But the pattern is three type aliases and a `Vec`, it is already
established by `agni-riftbound`, and §6.4 shows what actually blocks a
second game is that these stubs have no `zone_table()` or `deal_plan()`.
Either give them real content (which §3.4's shared `Game` trait is the
prerequisite for) or delete them and re-add in three minutes when a game
lands. **Keep `agni-plugins`**: unlike the other two it holds a real design
surface (`CardCi::parse` validating a `ci:`-prefixed 64-hex BLAKE3;
`ScriptRegistry` with a deterministic `BTreeMap` order tested at
`plugins/src/lib.rs:113-131`) that `wiki/design/plugins.md:74` commits to —
which is exactly §4.10's recommendation, restated from the dependency side.

**7.7.2 — `plugins/spike/hello-decider/host` is a dead 106-package wasmtime
spike.** *Nit, effort S.* Its manifest pins `wasmtime = "48"` with
`cranelift` + `runtime`; it is its own workspace, is not a member of the
agni workspace, is referenced by no `flake.nix` or `.woodpecker` file, and
answers a question — wasmtime or wasmi — that the shipped code already
answered in wasmi's favour. Delete it. The **guest** half is not dead: it is
the source of `plugins/harden/tests/fixtures/hello_decider_guest.wasm`, the
input to the golden-hash test, and is wired into the Nix source set at
`flake.nix:32` (see §6.5 — nothing regenerates or verifies it against its
source, which is the finding that matters there).

**7.7.3 — No dependency in either workspace has zero use sites in the crate
that declares it.** *Verified.* Nearest misses are `web-sys`'s `Headers`
(7.3.4) and `serde` in `agni-core`/`agni-net`, which have one site each but
are load-bearing derive macros.

## 8. The queued kai/src reorg, file by file

This is the executable map. Every current file and symbol group has a
destination. Nothing crosses a crate boundary and no behaviour changes —
the reorg is moves plus visibility tightening plus one plugin split.

### 8.0 Order of operations (this ordering is load-bearing)

1. **Land the DRY collapses first, not after.** §2.2's `broadcast` /
   `refresh` helpers (~80 lines out of net.rs) and §3's art-path
   consolidation shrink the files before they are cut up. Moving first
   means moving duplication into four new homes.
2. **Move `seat_center` and `seat_yaw` (lib.rs:424, lib.rs:441) into
   `render/layout.rs` in their own commit, before anything else moves.**
   `zones.rs` — the one exemplary file in the client — computes seat math
   against them. If `zones.rs` moves first it either breaks or grows a
   `crate::` back-reference that defeats the point of the move.
   `render/dim` must move with them; `zones.rs:7,11` use `dim::CARD_H`.
3. Split `CardTablePlugin` (§8.5) *before* moving systems, so each folder
   gains its own plugin and the moves are then mechanical.
4. Then the per-folder moves, one folder per commit, `cargo check` on all
   three targets (native, wasm32, android) between each — per the kai
   AGENTS.md trap, these modules do not share one cfg gate.

### 8.1 `render/` — the Bevy table

Source: all of `lib.rs` except the module declarations, plus `zones.rs`
and `foil.rs`.

| Destination | Symbols |
|---|---|
| `render/mod.rs` | `RenderPlugin` (the `build()` body's render half), re-exports. ~150 lines |
| `render/dim.rs` | the `dim` module, lib.rs:37-62 (`CARD_W`…`SAVE_DEBOUNCE_SECS`) |
| `render/state.rs` | `GameTable`, `Mirror` (+ `impl`), `ViewSeat`, `MySeat`, `PlayerCount`, `SessionRole`, `SessionInfo` (+ `impl`), `HandScroll`, `DealGeneration`, `Held`, `CardView`; messages `CardDropped`, `ExhaustToggled`, `Redeal` |
| `render/layout.rs` | **`seat_center`, `seat_yaw`** (move first — see 8.0), `deal_origin`, `hand_slot`, `board_slot`, `fan_back_slot`, `rotation_for`, `layout_cards`, `Facing`, `Slot` |
| `render/zones.rs` | all of today's `zones.rs`, unchanged, moved after layout.rs exists |
| `render/scene.rs` | `setup_scene`, `camera_pose`, `zoom_camera`, `apply_zoom`, `sync_seats`, `sync_zones`, `sync_player_count`, `seat_color`, `CardMesh`, `SeatDecor`, `ZoneDecor`, `DropSeat`, `DropZone` |
| `render/cards.rs` | `sync_cards`, `sync_hand_backs`, `FaceKey`, `face_key`, `wants_label`, `label_lines`, `card_shown`, `my_hand_ids`, `CardArt`, `FoilArt`, `FoilBody`, `OpponentHand`, `Hovered`, `SnapToSlot` |
| `render/animate.rs` | `animate_cards`, `apply_foil_alpha`, `hide_viewed_hand` |
| `render/input.rs` | `on_hover_card`, `on_unhover_card`, `on_click_card`, `on_drag_start`, `on_drag_over_surface`, `on_drag_end`, `on_drop_on_surface`, `on_drop_on_zone`, `hotkeys`, `guarded_from_me`, and the extracted `drop_index` helper (§2.1) |
| `render/overlays.rs` | `card_label_ui`, `zone_overlay_ui`, `preview_hud`, `hand_scroll_ui`, `seat_buttons_ui` — in-world HUD, distinct from `panels/` windows |
| `render/foil.rs` | all of today's `foil.rs` (`FoilMaterial`, `FoilExtension`) |
| `render/tuning.rs` | the `Tuning` resource + `Default` from `tuning.rs`; `tuning_path()` goes to `platform/paths.rs`, `tuning_ui`/`save_tuning` to `panels/tuning.rs` |

Tests split with their subjects: the `facing_tests` mod (lib.rs:1620)
divides between `render/layout.rs` (slot/facing math) and
`render/state.rs` (the `Mirror` test); `label_lines` tests go to
`render/cards.rs`.

Most of these systems can drop from crate-visible to `pub(super)` once
the wiring lives beside them — do that in the same commit, it is how the
reorg pays for itself.

### 8.2 `net/` — session plumbing

Source: `net.rs` (972 lines), minus its panel.

| Destination | Symbols |
|---|---|
| `net/mod.rs` | `NetPlugin`, `drain_net` (dispatcher only), re-exports |
| `net/platform.rs` | `player_name`, `keep_awake`, `start_host`, `close_table`, `start_join` — every cfg-forked shim, one home for the gates (net.rs:13-91) |
| `net/host.rs` | `HostState`, `handle_peer`, `send_private_faces`, `deals_legacy_hands`, `deck_already_dealt`, plus the new `broadcast` and `conn_for_seat` helpers (§2.2) |
| `net/client.rs` | `ClientState`, `PendingWelcome`, `try_finish_join`, `handle_host_msg` |
| `net/routes.rs` | `route_drops`, `route_redeal`, `route_annotations`, `route_deck_deals`, and the free `refresh` helper (§2.2) |
| `net/discovery.rs` | `discovered_tables`, `TableChoice` — or fold into `net/mod.rs` if it stays under ~40 lines |

The `#[cfg(all(test, ...))] mod tests` at net.rs:838 currently sits
mid-file between `host_controls` and `net_ui`; it splits with its
subjects.

**Do not move `TableChoice { riftbound: bool }` unchanged.** §4/§2.2
flag it as the game-registry seam and MTG is landing on it — resolve the
hardcoded `agni_riftbound` references (net.rs:96, 127, 191, 702) into a
registry lookup either just before or just after the move, but decide
which, because doing it during the move makes the diff unreviewable.

### 8.3 `panels/` — the egui windows

Each panel is a resource + a refresh system + a `*_ui` system. They are
uniform enough that `panels/mod.rs` can carry a `PanelsPlugin` that
registers all of them in one place.

| Destination | Source |
|---|---|
| `panels/mod.rs` | `PanelsPlugin` — the registration currently scattered through `CardTablePlugin::build` |
| `panels/multiplayer.rs` | `net_ui`, `host_controls` (both cfg arms) from net.rs:803-972 |
| `panels/identity.rs` | `IdentityPanel`, `qr_image`, `short_id`, `watch_refs`, `identity_ui` from identity.rs — **minus `store_dir()` (identity.rs:15) which goes to `platform/paths.rs`** (§4.16) |
| `panels/peers.rs` | all of peers.rs (`PeerPanel`, `toggle_peer_panel`, `refresh_peers`, `state_color`, `ref_color`, `roles`, `field`, `peer_row`, `peer_ui`) — moves clean, no changes |
| `panels/modules.rs` | `ModulesPanel`, `refresh_modules`, `row_line`, `modules_ui` from modules.rs:928-1005 |
| `panels/import.rs` | `ImportPanel`, `import_ui` from import.rs:531-732 |
| `panels/tuning.rs` | `tuning_ui` (lib.rs:972), `save_tuning` (lib.rs:1022) |
| `panels/telemetry.rs` | `TelemetryPanel`, `telemetry_ui` (telemetry.rs:512-560) |
| `panels/gateway.rs` | `bridge_ui`, `mesh_line` (bridge.rs:283-349), wasm32 only |

### 8.4 `platform/` — everything target-forked

This folder is where the cfg gates concentrate. The win is that a
per-target file replaces a per-function `#[cfg]`, which is what makes
kai's cfg-combination count tractable (§5E).

| Destination | Source |
|---|---|
| `platform/mod.rs` | `PlatformPlugin`; re-exports the per-target impls behind one name each |
| `platform/paths.rs` | **the three `store_dir()` definitions** — identity.rs:15, import.rs:202/207, sync.rs:24 — plus `tuning_path()` (tuning.rs:55) and telemetry's `config_file()` (telemetry.rs:172/211). One module owning path policy per target |
| `platform/node/{mod,native,web}.rs` | node.rs — `Node`, `status`, `get`, `add_peer`, `set_status`; native `start`/`run` (node.rs:66-116) to `native.rs`, wasm `start`/`seed_when_ready`/`browser_secret`/`PENDING_SEEDS` (node.rs:117-196) to `web.rs`. Replace the hand-rolled hex at node.rs:193 (§4.17) |
| `platform/engine/{mod,native,web}.rs` | engine.rs → `native.rs` (`session_engine`); engine_web.rs → `web.rs` (`WebModule`, `WebEngine`, `WebPlugin`, `boot`, `install_engine`, `compile`, …). `mod.rs` exposes one `session_engine()`. **Delete both `ENGINE_GAS_BUDGET` literals** (engine.rs:4, engine_web.rs:12) in favour of the agni-engine-host const (§4.9) |
| `platform/modules/{mod,native,web}.rs` | modules.rs — shared `ActivePlugin`, `ModuleRow`, `JoinModules`, `short_hex`, `hash_hex`, `PLUGIN_GAS_BUDGET` in `mod.rs`; `mod platform` native (modules.rs:51-606) and wasm (608-928) become the two sibling files verbatim |
| `platform/gateway.rs` | bridge.rs minus its panel — `GatewayConst`, `GATEWAYS`, `boot`, `gateway_base`, `faces`, `riftbound_art`, `request_art`, `bridge_redeal`, `apply_art`, the statics |
| `platform/android.rs` | all of sync.rs (`request_scan`, `set_keep_awake`, `drain_tickets`, `TICKETS`) minus `store_dir` |
| `platform/art.rs` | import.rs's native art half — `art_bytes`, `art_by_names`, `wanted_art`, `start_full_ingest`, `set_art_status`/`art_status_line`/`art_idle` and their statics (import.rs:211-445). **Shape this against §3's source-agnostic art core** — the MTG branch is moving the substance of this into agni-importers, so this file should end up thin |
| `platform/store.rs` | the §3.5 consolidation: one `deal_faces_from(rows, foil_chance)` replacing the duplicate seeded deals (app.rs:160-187, bridge.rs:165-196), and one `patch_table_art(table, cache)` replacing `bridge::apply_art` + `import::apply_store_art`. Manifest row types come from `agni-importers`; the three private manifest structs (app.rs:24-36, bridge.rs:44-56) are deleted |
| `status.rs` (top level) | §3.9's `StatusBoard` resource + `status_ui`, replacing the five per-module status cells in node.rs, bridge.rs, import.rs, modules.rs and engine_web.rs |

**The two `mod platform` blocks in modules.rs already expose a parallel
API** (`status`, `engine_note`, `select_engine`, `select_plugin`,
`selected_engine`, `selected_plugin`, `active_plugin`, `rows`, `reload`,
`refresh_platform`, `prepare_join` — identical signatures in both arms).
That is a trait waiting to be declared. Writing it as a
`trait ModuleSource` in `platform/modules/mod.rs` makes the two files
provably in sync at compile time instead of by inspection; today a
signature can drift on one target and only the other target's CI catches
it. *Structural, effort M* — worth doing as part of the move.

### 8.5 Splitting `CardTablePlugin`

`lib.rs:66-161` registers everything: render systems, net systems, four
panels, the import panel, android drain, and the wasm boot calls. It is
the single place where all six subsystems touch, and it is why almost
every kai module is `pub`.

```
CardTablePlugin
  ├── render::RenderPlugin      messages, table resources, Startup + Update chain
  ├── net::NetPlugin            HostState/ClientState/TableChoice, drain + routes
  ├── panels::PanelsPlugin      every *_ui and its resource + refresh system
  └── platform::PlatformPlugin  node::start, android drain, wasm boot()s, gateway systems
```

The ordering constraints currently expressed inline must survive
explicitly: `net::drain_net` and the routes run `.before(sync_player_count)`,
and `import::apply_store_art` runs `.after(route_deck_deals)`. Give them
named `SystemSet`s (`NetDrain`, `TableSync`, `ArtApply`) in the split so
the constraint is stated once in `CardTablePlugin` rather than reaching
across plugin boundaries by function name.

### 8.6 What stays at the top level, deliberately

| File | Why |
|---|---|
| `lib.rs` | module declarations + `CardTablePlugin` composition only. Target: under 60 lines |
| `main.rs` | unchanged, one-line shim |
| `app.rs` | the app shell — `main`, `bevy_main`, `primary_window`, `deal`, `redeal_on_request`, `starting_hand`, `starting_faces`. **`store_faces` and its private `Manifest`/`ManifestCard` (app.rs:137-188) do not stay** — they go wherever §3 puts the shared manifest decoder, and the hob-ref fallback bug (app.rs:146-152, §4.8) is fixed on the way |
| `deck.rs` | `SeatedDeck`, `SeatedDeckRecord`, `DealDeckRequested`, `deck_faces`, `face_from`, `face_map`, `all_cards`, `parse_reply`, `deck_size`, `card_of`, `entries_of`, `encode_component` from import.rs. This is deck data feeding the deal, not render, net, panel or platform — forcing it into one of the four would be miscategorising it. Its natural long-term home is agni-importers (§3) |
| `telemetry/` | its own folder — `{mod,capture,config,ship}.rs` per §4.7, with `panels/telemetry.rs` holding the window. It is already a self-contained `Plugin`, a peer of `render/` and `net/`, not a subordinate of any of them |

### 8.7 Expected shape after

| Folder | Files | Approx lines |
|---|---|---|
| `render/` | 12 | ~2250 (from lib.rs 1796 + zones 456 + foil 47 − panels/platform extractions) |
| `net/` | 6 | ~750 |
| `panels/` | 10 | ~1400 |
| `platform/` | 12 | ~2350 |
| `telemetry/` | 4 | ~500 |
| top level | 4 | ~450 |

No file over ~450 lines; the two 1000+ line files and the 1796-line one
are gone. The count of files goes from 17 to ~48, which is the trade
being made deliberately: more files, each answering one question.

### 8.8 Intersections with in-flight work

Two branches are editing files this map moves. Neither invalidates the
map, but both change the order in which it should be executed.

**[ZONES⚡] — base/rune-pool zones, riftatlas layout, sideboard popup.**
This branch reworks `zones.rs` and adds `kai/src/sideboard.rs`.
Consequences for the reorg:

- **Let it land before §8.1 moves `zones.rs`.** Moving a file that
  another branch is rewriting produces a conflict that git resolves
  badly (rename plus heavy edit). §8.0 step 2 — hoisting
  `seat_center`/`seat_yaw` out of `lib.rs` into `render/layout.rs` — is
  the exception: it is a small, additive change that makes the zones
  rework *easier*, so it is worth doing first and telling that agent
  about.
- `kai/src/sideboard.rs` maps to **`panels/sideboard.rs`** — it is a
  popup window, so it belongs beside `panels/import.rs`, not in
  `render/`. If it carries slot geometry as well as UI, the geometry
  splits into `render/layout.rs` with everything else.
- A riftatlas-matching layout means more per-game geometry. If it
  arrives as riftbound-specific constants inside `zones.rs`, that is the
  same hardcoding §3.4 and §2.2 flag elsewhere — the game registry
  should own the layout table, and `zones.rs` should stay the pure
  interpreter of a `ZoneDecl` list that makes it the model file it is
  today. Worth raising with that agent now rather than after.

**[MTG⚡] — MTG plugin, game selector, source-agnostic art core.**
Consequences for the reorg:

- `platform/art.rs` and `platform/store.rs` (§8.4) should be written
  *against* the landed art core, not before it. If the reorg reaches
  `platform/` before that branch merges, do those two files last.
- The game selector rewrites exactly the `net.rs` lines §8.2 warns not
  to move blind (`TableChoice`, the `agni_riftbound` references). Prefer
  landing the selector first, then splitting `net/`; the split is
  mechanical either way, but sequencing it second means the registry
  shape is settled before it is distributed across five files.

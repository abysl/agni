# Code review — agni + kai, 3 September 2026

> Scope: every source file under `agni/agni/` (core, sim, net, engine/wasm,
> engine/host, importers, plugins/*, games/*) and `agni/kai/src/`,
> plus the manifests, `devenv.nix`, `web/index.html`, `MainActivity.java`,
> both `AGENTS.md`s, both READMEs and the design wiki. Reviewed at
> `f5dbf3d7` (the kai `net/ engine/ deck/ os/` reorg). Read against the
> Rust book's guidance on error handling (ch. 9), traits over duplicated
> code (ch. 10), test organisation (ch. 11), shared-state concurrency
> (ch. 16) and `unsafe` (ch. 20), and against the two constitutions
> (`agni/AGENTS.md`, `kai/AGENTS.md`).
>
> This is a follow-up to [code-review-2026-09.md](code-review-2026-09.md).
> That review is still the reference for DRY, layering, dependency weight
> and the kai reorg map — none of it is repeated here except as a status
> line. What is new here: three build breaks on `main`, five bugs the last
> review could not have seen (they arrived with the MTG, sideboard,
> reconnect and reorg branches), a reassessment of the "JPEG in the log"
> deferral, and the smell map.
>
> Verification, not inspection: `cargo clippy --workspace --all-targets
> -- -D warnings` and `cargo test --workspace` were run in both trees, and
> one throwaway integration test was written and run to confirm §1.3.
> Severity: **break** (CI is red / the app dies), **bug** (wrong
> behaviour), **structural**, **hygiene**, **nit**. Effort S/M/L as before.

## Outcome — fixed 3 September 2026, same day

Every item below was implemented in one working tree (uncommitted at the
time of writing), and both trees were re-verified: agni `cargo clippy
--workspace --all-targets --features $AGNI_IMPORTER_FEATURES -- -D
warnings` + `cargo test --workspace`, kai `cargo clippy --workspace
--all-targets -- -D warnings` + `cargo test --workspace` + `web-clippy`
+ `cargo ndk … check --lib` from the android devenv. Where a finding was
resolved differently from the recommendation, the row says how.

| Item | Status | How |
|---|---|---|
| 0.1–0.3 | fixed | `ZoneSpec` const tables in both game crates; `HostEvent::Joined(conn, _)`; `serving.shutdown().await?` |
| 1.1 | fixed | `deck/sideboard.rs` returns a "riftbound only" notice for an MTG seat; no static empty deck, no `unreachable!` |
| 1.2 | fixed | art left `CardFace`; `apply_store_art` writes the table only when a face actually changes |
| 1.3 | fixed | the `Reveal` arm authorises on zone visibility + owner, same rule as `guards_card`; pinned by `a_hidden_card_played_onto_another_seats_public_zone_is_revealed_too` in `net/tests/session.rs` |
| 1.4 | fixed | the double-click detector is a `Local<(Entity, f32)>` keyed on the clicked entity |
| 1.5 | fixed | `intent` appends the `Move` first and the `Reveal` after; a plugin refusal leaks nothing, a refused reveal surfaces as `SessionError::Partial` |
| 1.6 | fixed | `SessionError { Refused, Engine, UnknownSeat, NoDealTarget, NoFace, Partial }`; every `HostSession`/`ClientSession` mutation returns `Result` and is `#[must_use]`; kai shows the reason in the status line and ends the session on an engine fault |
| #1 / 3.2 | fixed, structurally | `agni-deck` (`games/deck/`) holds the shared `CardName`/`DeckEntry<C>`; the importers gained a game-neutral `deck` module (`Game` trait, one `TextVocabulary`-driven `parse_text`, one `CardLookup<G>`/`NameIndex`/`Cached`, one `resolve`) and `mtg`/`riftbound` became `Game` impls over it, with the shared behaviour tested once. Honest accounting: `importers/src` went from 5,308 to 5,501 lines — the generic module and its tests cost more than the deleted copies, so the "roughly a third" shrink did not happen; what did happen is that a third game is now a vocabulary, not a copy. Zone tables, deal plans and `ResolvedCard` stay per game on purpose |
| #2 | fixed | see 1.6; `EngineFault` is a value, never a panic |
| #3 | fixed | `core/tests/rng_golden.rs` pins the raw, bounded and shuffle streams; `sim/tests/goldens.rs` + `net/tests/goldens.rs` pin the log, ABI request and wire encodings against hex fixtures (`AGNI_GOLDEN_WRITE=1` regenerates, only alongside a version bump) |
| #4 | fixed | one `Conns::broadcast`/`broadcast_roster`/`deliver` path and one compare-before-assign `refresh` in `kai/net/mod.rs` |
| #5 / 3.4 / 3.1(2) | fixed | `CardFace { name, tint: [u8; 3], foil }` deriving `Eq`/`Ord`/`Hash`; `art_path`/`art_jpeg` gone, art lives in kai's `ArtCache`; `LogEntry { seq, seat, action }`; `WIRE_VERSION = 1` on `Join`/`Welcome` with a mismatch refusal; `MAX_FRAME_BYTES` down to 8 MiB. The engine module is still hardened with `allow_floats` because serde/ciborium's float visitors leave `f32`/`f64` in the guest's function signatures even though no agni type carries one — the harden pass rejects the module otherwise (verified) |
| #6 | done earlier | unchanged |
| #7 | fixed | `parking_lot::Mutex` in every kai static and in `agni-net`'s `bridge`/`table`; zero `lock().unwrap()` left in kai. The wasm guest keeps `std::sync::Mutex` (single-threaded, two sites) |
| #8 | fixed | `kai.yml` runs `cargo ndk -t arm64-v8a -P 28 check --lib` from the android devenv after the nix checks; the same command passes locally |
| #9 | fixed | `agni_sim::engine::{ModuleCall, AbiEngine, AbiPlugin}`; wasmi and the browser `WebAssembly` are two `ModuleCall` impls; one `resolve_pin`/`join_modules` driver in `kai/engine/modules.rs` |
| #10 | fixed | `agni-importers` `default = []`, features passed explicitly by the devenv scripts and the flake's `agniArgs`; `spirit-sdk` is a dev-dependency of `agni-core`; kai depends on `spirit-core` directly |
| 3.1(1) | fixed | no per-action snapshot: `fold_shadowed`/`fold_log_shadowed` fold a native `LogState` beside the engine with the same verdict and raise an `EngineFault` if the two disagree; pinned by two tests in `net/tests/session.rs` |
| 3.3 | fixed | `ViewCard.rotated` gone (kai reads the `exhausted` badge); `CardFace::hidden()` is the empty default with no tint; the seat palette moved to `kai/table/colors.rs` |
| 3.5 | fixed | `From<WireIntent> for LogAction` + `TryFrom<LogAction> for WireIntent` |
| 3.6 | fixed | `send_wrapper::SendWrapper` instead of `unsafe impl Send + Sync`; every JNI caller goes through `os::android::with_activity` |
| 3.7 | fixed | both `AGENTS.md` layout tables, both READMEs, `architecture.md` §3a, `plugins.md`, the kai wiki paths |
| 3.8 | fixed | `kai/os/paths.rs` |
| 3.9 | moot | `WireFace` is `agni_core::CardFace`; the comparison is a plain `!=` |
| 3.10 | unchanged | as noted |
| 3.11 | fixed | `refresh` compares before assigning; the UI sliders keep the `bypass_change_detection` pattern |
| 3.12 | fixed | `riftbound/ingest.rs` writes through `art::Journal` and waits `THROTTLE` |
| 3.13 | fixed | one `NameIndex::find_name` with the unique-prefix `take_while` rule for both games |
| §4 tests | fixed | `session.rs` inline suite moved to `net/tests/session.rs`; `agni-riftbound`/`agni-mtg` stay dev-deps of `agni-net` only for the MVP suites; `net_smoke` builds again but is still `#[ignore]` (it needs two live endpoints) |
| §4 API | fixed | `#[must_use]` on the entry-returning session calls; `Table`, `ClientSession` and `HostState` fields private with accessors |

Left open on purpose: `net/tests/log_fold.rs` still lives in `agni-net`
(it drives `HostSession`, so it is a session test after all), and the kai
`target/debug/deps` directory had grown to 113 GiB during this work —
a `cargo clean` there is the owner's call.

## 0. `main` does not pass its own CI gates — three fixes, fifteen minutes

`agni`'s CI runs `clippy --workspace --all-targets -- -D warnings` and
nextest. Both are red at `f5dbf3d7`; kai's are green (clippy clean, 69
tests pass). Every agni test that *builds* passes — 229 across the
workspace inside the devenv shell, including the wasm-hosted MVP suites —
so the breakage is entirely the three items below, not behaviour.

| # | What | Where | Fix |
|---|---|---|---|
| 0.1 | `too_many_arguments` (8/7) — clippy `-D warnings` fails on the first game crate it reaches, so nothing after `agni-mtg` is even linted | `games/mtg/src/lib.rs:27` `fn zone(...)` | `agni-riftbound`'s twin already carries `#[allow(clippy::too_many_arguments)]`; better, both should take a `ZoneDecl { ..default }` struct literal and lose the helper |
| 0.2 | `net_smoke.rs` no longer compiles: `HostEvent::Joined(conn)` matches one field, the variant grew a second (`Joined(u64, String)`) in the reconnect commit `66315891`. Because the test is `#[ignore]`d nobody ran it, but `cargo test -p agni-net` fails to **build**, so every agni-net integration test is skipped locally | `net/tests/net_smoke.rs:70` | `HostEvent::Joined(conn, _)`. Then un-ignore it or give it `--run-ignored` in nextest — the last review's §6.7.1 said the transport layer was untested; it is now un-compilable |
| 0.3 | `unused_must_use`: `serving.shutdown().await` returns a `Result` since spirit changed its signature; under `-D warnings` this fails the `deck-gateway` bin | `importers/src/bin/deck_gateway.rs:90` | `serving.shutdown().await?` |

All three are drift between crates that share a path dependency —
exactly the failure mode `agni/AGENTS.md` warns about under "Dependency on
spirit". None would have merged if `agni.yml` had run on the commits that
introduced them; check whether the Woodpecker path filters fired for
`66315891` and `2ac61c80`.

## 1. Bugs

### 1.1 — Opening the sideboard with an MTG deck seated panics the client. *bug, S*

`kai/src/deck/sideboard.rs:286-302`: after the window closes,
`sideboard_ui` unconditionally does
`let deck = riftbound_deck_mut(&mut record.deck);`, and
`riftbound_deck_mut` (`:319-326`) is `unreachable!("sideboard is riftbound
only")` for `ImportedDeck::Mtg`. The read-side helper `riftbound_deck`
(`:309-314`) quietly substitutes a static empty deck so the panel *renders*
for an MTG deck — and then the write-side helper aborts. Repro: import any
MTG list, "seat this deck", click "sideboard · 0". The whole Bevy app goes
down (on android, the activity).

Fix: early-return from `sideboard_ui` when the seated deck is not
riftbound (or show the "riftbound only" label the read helper is pretending
to), and delete the `EMPTY_RIFTBOUND: OnceLock` stand-in — a static empty
deck used to make a mismatched enum variant look like data is the smell that
produced the panic. Longer-term the panel should read the deck through the
game registry the last review's §3.4 asked for.

### 1.2 — `apply_store_art` marks `GameTable` changed every frame once any art has landed. *bug, S*

`kai/src/deck/import.rs:490`:
`crate::render::art::patch_table(&mut table.0, &cache)` runs
unconditionally every `Update`. `&mut table.0` goes through
`ResMut::deref_mut`, which sets the resource's changed tick whether or not
`patch_table` writes anything (`patch_table` only early-outs when the
*cache* is empty, and the `cache` `Local` is never evicted). So from the
first art arrival onward, on every native frame:

- `sync_cards` (`table/sync.rs:19`) re-runs its keep/despawn scan (O(n²)
  via `kept.contains`),
- `layout_cards` (`table/layout.rs:15`) recomputes every anchor and slot,
- **`sync_hand_backs` (`table/sync.rs:308-313`) despawns and respawns every
  opponent hand-back entity** — entity churn plus mesh/material handle
  clones, sixty times a second, for the rest of the session,
- every downstream `Res<GameTable>::is_changed()` guard is meaningless.

Fix: compute whether anything is missing first and only take the `&mut`
when it is (`if table.0.cards.iter().any(|c| c.face.art_jpeg.is_none() &&
cache.contains_key(&c.face.name)) { patch_table(&mut table.0, &cache); }`),
or have `patch_table` take `ResMut` and call `bypass_change_detection()` /
`set_changed()` on its own `patched` result. The wasm twin
`net/gateway.rs:187-228` `apply_art` already does this correctly
(`needs_patch` before any `&mut`); mirror it. This is the same
compare-before-assign discipline the last review asked for on
`mirror.view = session.view().clone()` (§5C.1), which is still at 14 sites.

### 1.3 — A card played from a hidden zone straight onto an opponent's per-seat public zone stays face-down forever. *bug, M* — verified with a probe test

`agni-net/src/session.rs:763-830` `HostSession::intent`. When a `Move`
leaves a `ZoneVisibility::None` zone for a public one, the reveal is
appended *after* the move (`after_move = true`, `:811-818`), because a
`Reveal` inside a hidden zone is refused. But `validate_action`'s `Reveal`
arm (`sim/src/log.rs:275`) authorises on the card's **current seat**
(`current.seat.0 != entry.seat → ForeignHand`), and after the move the
card's seat is the *target* seat. If the target is another seat's
`PerSeat` + `All` zone — MTG `battlefield`, riftbound `base`, `legend`,
`trash` — the reveal probe fails and `intent` silently drops it
(`:814-818` — `if validate(...).is_ok()` / `if let Ok(entry)`), returning
`Some(vec![move])`. The card now sits in a public zone, `revealed` does not
contain it, no replica has its face, and nothing can ever reveal it: the
mover is no longer its seat-holder and the seat-holder never had the face.

kai reaches this from the UI: `table/interaction.rs:145-153`
`on_drop_on_zone` only refuses drops onto *private* foreign zones, so
dragging from your library onto the opponent's battlefield is allowed.
Confirmed by a throwaway `net/tests` probe against `agni_mtg::zone_table()`:
seat 1 deals to `library`, moves the top card to seat 0's `battlefield`;
`intent` returns one `Move`, the card lands at seat 0 with an empty face
and `revealed` is empty. (The riftbound MVP test does not catch it because
battlefields there are `Shared`, where the owner check is skipped.)

Root cause: **`Reveal` authorisation is keyed on the card's seat, `Move`
and `Annotate` on its owner via `guards_card`, and the two disagree** the
moment a card crosses seats. Fix options, cheapest first:
(a) in `validate_action`'s `Reveal` arm, also allow `current.owner.0 ==
entry.seat` (the dealer-owner can always reveal what they brought);
(b) make `intent` emit the reveal first for hidden-zone sources too, by
revealing *into* the move — i.e. carry an optional face on `Move`; or
(c) have `intent` return `Err` instead of `Some(vec![move])` when the
follow-up reveal is refused, so at least the move never lands half-done.
(a) is one line and closes the hole; (c) should land regardless — a
half-applied intent that reports success is worse than a refused one.

### 1.4 — The double-click exhaust detector is global, not per card. *bug, S*

`kai/src/table/interaction.rs:202-226` `on_click_card` keeps one
`Local<Option<f32>>` for the last click time and never records *which*
entity was clicked. Clicking card A then card B within 350 ms toggles
exhaust on B. Store `(Entity, f32)` and compare the entity.

### 1.5 — Reveal-before-move leaks a hand face if the plugin rejects the move. *latent bug, M*

Same function, the other branch (`session.rs:819-825`): for an
`Owner`-visibility source the `Reveal` is appended first, then the `Move`.
`validate` was run on the move *before* the reveal, but a
`PluginModule::decide` verdict is only asked at `append` time — so a plugin
that accepts the reveal and rejects the move leaves the face in the shared
log while the card stays in hand; the function returns `Some(vec![reveal])`
and reports success. Both shipped plugins are accept-all today, so this is
latent, but it is the first real rule a plugin would want to enforce
("you can't play that now"), and the leak is unrecoverable (the log is
append-only). The fix is the same shape as 1.3(b): the reveal should not be
a separate admission from the move it exists to serve.

### 1.6 — `HostSession::intent`, `deal_groups`, `reload_groups` return `Option` and discard the reason. *structural, M — carried from §2.3, restated because 1.3 and 1.5 are consequences*

Every refusal collapses to `None`, every half-success to `Some(partial)`,
and the twenty-four `.expect(...)` sites in `session.rs` (`:254-983`)
still turn an engine fault or a validate/apply divergence into a process
abort. Threading `Result<Vec<LogEntry>, IntentError>` (with
`FoldError`/`EngineFault` inside) out of these three is the single change
that lets kai *say* why a drop bounced, lets the session end instead of the
window, and forces the half-applied cases above to be decided rather than
defaulted.

## 2. Where the September top 10 stand

| # | Item | Status at `f5dbf3d7` |
|---|---|---|
| pre | cap the nested wasm builds in the test bootstrap | **done** (`15f41fbb`, `--jobs`) |
| 1 | game registry (`Game` trait, shared `ResolvedCard`/`DeckEntry`/`DeckFaces`) | **open — and the cost was paid**: `games/mtg` now clones the riftbound type family (`lib.rs:115-195`), `importers/mtg/` clones `catalog.rs`/`resolve.rs`/`text_list.rs`, and kai's `ImportedDeck` enum (`deck/import.rs:24-115`) dispatches on game at 9 sites. §3.2 below |
| 2 | thread `EngineFault`/`FoldError` out of `append`/`intent`/`apply` | **open** — 24 `expect`s in `session.rs`; see 1.6 |
| 3 | golden CBOR vectors + golden RNG sequence | **open** — zero `.cbor` fixtures, `rng.rs:44` still asserts `seed(42) == seed(42)` |
| 4 | collapse net.rs broadcast/refresh duplication, compare-before-assign | **open and worse** — `bridge::send_to` at 19 sites, the refresh pair at 14, now spread through `net/mod.rs`'s 1,393 lines |
| 5 | quantise `tint` to `[u8;3]`, delete `art_path` | **open** — 20 five-field `CardFace` literals, `art_path` still never `Some`, engine still hardened with `allow_floats: true` |
| 6 | hoist `seat_center`/`seat_yaw` + `dim` before moving `zones.rs` | **done differently** — they live in `table/mod.rs:389-411` and `zones.rs` imports them from there. Works; `table/mod.rs` is now the hub every submodule glob-imports with `use super::*` |
| 7 | `parking_lot::Mutex` on the static-holding modules | **open** — 91 `lock().unwrap()` sites now (was 73) |
| 8 | `cargo check --target aarch64-linux-android` CI lane | **open** — `kai.yml` unchanged; the IME and clipboard JNI code (`os/ime.rs`, `os/clipboard.rs`, `os/android.rs`) merged with no android compile on any PR |
| 9 | one `ModuleCall` trait behind the two wasm engine hosts; one `resolve_pin` driver | **open** — `engine/web.rs:321-371` and `engine/host/src/lib.rs:412-462` are still the same seven methods; `engine/modules.rs` still holds four hand-rolled pin state machines (`:375-448`, `:835-932`) |
| 10 | `agni-importers` `default = []`; drop `spirit-sdk` from `agni-core` | **open** — `default = ["scryfall", "riftbound-gateway", "mtg-native"]` (grew), `core/src/lib.rs:1 pub use spirit_sdk;` intact, kai still reaches spirit-core through it at 3 sites |

The kai file split (§2.1, §8) happened and is good — no file over 1,400
lines, `table/` mirrors the proposed `render/` map, `zones.rs` is still
the model file. What did *not* happen was §8.0 step 1: the DRY collapses
were supposed to land *before* the split, and instead the duplication was
moved intact into `net/mod.rs`.

## 3. New findings

### Structural

**3.1 — JPEG bytes ride every fold step, not just the wire.** *structural,
M for the mitigation, identity-phase-1 for the cure.* The last review
recorded "JPEG in the log" as a known deferral. Reading the data path end
to end, it is broader than the log:

- `kai/deck/import.rs:282-295` `face_from` puts the decoded image into
  `CardFace.art_jpeg`; `deal_plan_for` → `DealGroup.faces` (`WireFace`
  with the bytes) → `HostSession.dealer`.
- Every `Reveal` (`session.rs:806-810`) serialises `WireFace::from(&face)`
  — bytes included — into the log entry, the `HostMsg::Entry` broadcast to
  every seat, and `LogState.table` on every replica.
- `refresh_state` (`session.rs:356-359`, `:949-952`) runs after **every**
  `append`/`apply`: `engine.snapshot()` CBOR-encodes the entire
  `LogState`, then `decode_state` decodes it. On the wasm engine that is
  guest-encode → host-copy → host-decode of every revealed card's JPEG,
  per action. A 60-card MTG game with a dozen cards revealed at ~60 KB each
  is ~0.7 MB of image bytes re-serialised per move, twice.
- `native_decide_request` (`sim/src/engine.rs:106`) clones the whole state
  again per entry when a plugin is loaded, and encodes it into the plugin.
- `HostSession::table()` / `ClientSession::table()` (`view_to_table`)
  clone every face on every call; kai calls them after every net event
  (14 sites).
- `HostMsg::Welcome` ships the full log — every reveal's JPEG — to each
  joiner, and `MAX_FRAME_BYTES` is 64 MiB because it has to be.
- `validate_action`'s `Reveal` arm (`log.rs:266`) clones the incoming face
  to compare it (`face.clone().into()`), bytes included.

Two mitigations that do not wait for content identities: (1) stop snapshotting
per action — `refresh_state` exists because `state_cache` is read for
validation probes and `seat_faces`; keep a `LogState` folded natively
alongside the engine (it is the same deterministic fold) and drop
`snapshot()` from the hot path, or add an `Engine::state_hash()` and only
re-snapshot on mismatch; (2) move `art_jpeg` out of `CardFace` into a
kai-side `HashMap<name, bytes>` keyed by the face name/key, so `WireFace`
carries `name` + `tint` + `foil` and art travels once, via the store or the
`Faces` message, never in the fold. (2) is the same log-format revision
as top-10 #5 and should land with it.

**3.2 — The MTG branch doubled the importer, not just the game crate.**
*structural, M.* Beyond §3.4 of the last review, `importers/src/mtg/` and
`importers/src/riftbound/` are parallel modules: `text_list.rs`
(`section_header`, `split_count`, `TextError`, `parse_text` — same
shape, different vocabularies), `catalog.rs` (`LookupError`,
`CardLookup`, `StaticCatalog`, `Cached` — riftbound's has `by_code`/
`by_id`, mtg's has `Layered`), `resolve.rs` (`Unresolved`, `Resolution`,
`push`, the section→zone match), `ParsedEntry`/`ParsedDeck` in both
`mod.rs`. `encode_component` exists three times (`mtg/scryfall_named.rs:48`,
`riftbound/riftcodex.rs:102`, `kai/deck/import.rs:204`), and `scryfall.rs`
(the bulk ingester) still has its own `USER_AGENT`, `ImageUris` and image
fetch loop beside `art.rs`'s `fetch_image`/`Journal`. A `Game` trait with
associated `Section`, `Identifier` and `Card` types, and a generic
`text_list<G>` / `catalog<G>` / `resolve<G>`, deletes roughly a third of
the 5,300-line crate. Do it before abyss-walker triples it.

**3.3 — Game knowledge in the neutral engine.** *structural, S.*
`sim/src/view.rs:65` derives `ViewCard.rotated` from the literal badge key
`"exhausted"`; `sim/src/wire.rs:193` `hidden_face()` bakes kai's card-back
brown; `net/src/session.rs:77-109` owns the seat colour palette
(`SEAT_COLORS`, `contested_color`) that only kai renders. The constitution
says agni holds no game knowledge and clients own UX. `rotated` belongs
to the plugin manifest (a `HotkeyDecl`-style `BadgeDecl { key, rotates }`)
or to kai; the palette is render data and can move to `kai/table/colors.rs`
with `SeatInfo.color: u8` staying an opaque index on the wire.

**3.4 — Three permanent nulls in every log entry.** *hygiene, S (format
revision).* `LogEntry { table, prev, actor }` (`sim/src/log.rs:65-72`) are
never `Some` anywhere in either tree — `LogEntry::unsigned` is the only
constructor. They are phase-B signature fields serialised as CBOR `null`
into every entry, every `HostMsg::Entry` and every genesis pin. Same class
as `art_path`; drop them in the same format revision as #5 and re-add when
signatures land (a `#[serde(default)]` on the new fields will keep old logs
readable — which is also the moment to add the wire `version` §5D.2 asked
for).

**3.5 — `WireIntent` is a third copy of the `LogAction` subset.**
*hygiene, S.* `session.rs:128-144` mirrors `LogAction::{Move, Annotate,
Game}` field-for-field and `intent()` maps them by hand. `impl
TryFrom<LogAction> for WireIntent` + `From<WireIntent> for LogAction` makes
the "which actions may a client request" rule explicit and removes the
match. (Not the same as the `Zone`/`WireZone` twin, which is pure
duplication — §4.3, still open.)

**3.6 — `JsHandle` lies to the type system.** *hygiene, S.*
`kai/src/engine/web.rs:166-169` `unsafe impl Send/Sync for JsHandle<T>`
exists only because `Engine: Send + Sync` (`sim/src/engine.rs:41`) and JS
objects are neither. It is sound today because wasm32 Bevy is
single-threaded, and it will be UB the day it is not. The honest shape is
`pub trait Engine: MaybeSend` with a cfg-gated alias
(`#[cfg(target_arch = "wasm32")] pub trait MaybeSend {}` / native
`MaybeSend: Send + Sync`), which is what iroh/n0 do for the same
constraint. The JNI `JavaVM::from_raw` / `JObject::from_raw` sites
(`os/android.rs:34-35`, `os/clipboard.rs:131-133`, `os/ime.rs:93-95`,
`telemetry.rs:223`) are four copies of the same three unsafe lines; one
`os::android::with_activity` already exists in two of the files — use it
in all four.

### Hygiene

**3.7 — Both `AGENTS.md`s and both READMEs describe trees that no longer
exist.** *hygiene, S, but it is the house rule.* The no-comments rule puts
the explanation in the docs, so stale docs are stale comments.
`kai/AGENTS.md`'s layout table names **18 paths that do not exist** after
`241d5746`/`f5dbf3d7` (`src/lib.rs` as the plugin, `src/zones.rs`,
`src/sideboard.rs`, `src/clipboard.rs`, `src/colors.rs`, `src/import.rs`,
`src/art.rs`, `src/bridge.rs`, `src/engine.rs`, `src/engine_web.rs`,
`src/modules.rs`, `src/node.rs`, `src/net.rs`, `src/sync.rs`,
`src/ime.rs`, `src/identity.rs`, `src/peers.rs`, `src/tuning.rs`,
`src/foil.rs`); `kai/README.md:34,77,111` cite `src/bridge.rs` and
`src/clipboard.rs`; `agni/README.md` still says importers are "the
Scryfall ingester"; `agni/wiki/design/architecture.md` §3a still says
`Zone` is `Hand` and `Board` and that zones "have to become game-defined"
— they did (`Plugin(u16)` + `ZoneDecl`). The reorg commits should have
carried the table with them.

**3.8 — Six `store_dir` definitions.** *hygiene, S.* Was four; now
`net/identity.rs:15`, `os/android.rs:24`, `deck/import.rs:309`, `:314`,
`net/node.rs:56`, `:61`. One `os::paths::store_dir()`.

**3.9 — `Reveal` compares faces by cloning them.** *nit, S.*
`log.rs:266` `current.face != face.clone().into()` — `impl
PartialEq<WireFace> for CardFace`.

**3.10 — The android IME buffer diff is right; its seed is a magic width.**
*nit.* `os/ime.rs:8-9` `PAD_WIDTH = 64` / `PAD_FLOOR = 16` with the
reasoning only in the README. Fine as is; noted because the diff
(`ime.rs:26-55`) is the one piece of new platform code with a real test
suite, and it is the model for the rest of `os/`.

**3.11 — `hand_scroll_ui` / `tuning_ui` get change detection right; the
pattern is not applied where it matters.** `bypass_change_detection()` +
compare + `set_changed()` (`table/ui.rs:67-111`, `:191-213`) is exactly
what 1.2 and the 14 `mirror.view = ...` sites need. Lift it into one
`fn assign_if_changed<T: PartialEq>(res: &mut ResMut<T>, next: T)`.

**3.12 — `riftbound/ingest.rs::ingest` re-implements `art::Journal`.**
*nit, S.* `:176-180`, `:217-222` open the journal file and `writeln!`
by hand, with a hard-coded `sleep(1s)` instead of `THROTTLE`; `Journal::put`
is three lines away in the same crate.

**3.13 — `mtg/catalog.rs::find_name` is O(n) on a miss and returns the
first prefix match; the riftbound twin is O(log n) and refuses ambiguity.**
*nit, S.* `.range(..).find(...)` scans to the end of the map;
`riftbound/catalog.rs:101-116` uses `take_while`. Same function, two
semantics — one more reason for 3.2.

## 4. Read against the Rust book

- **Ch. 9, `Result` vs `panic!`.** The book's rule is that `panic!` is for
  states the caller cannot recover from and `Result` for everything a
  caller might reasonably handle. `agni-net/src/session.rs` has it inverted
  in 24 places: every fold the *host constructed* is `.expect`ed, and
  every engine fault — the one thing the sandbox exists to contain — aborts
  the process. `HostSession::intent` then returns `Option`, throwing away
  the `FoldError` it just computed. kai's 91 `lock().unwrap()` on statics
  are the same pattern one layer down (ch. 16 — a poisoned `Mutex` on a
  process static is unrecoverable by construction). Both were in the last
  review; the counts went up.
- **Ch. 10, traits over duplicated code.** Three places where two
  implementations of one contract exist with no compiler-checked
  relationship: the two wasm engine hosts (`WebEngine`/`WasmEngine`), the
  two `mod platform` blocks in `engine/modules.rs` (identical eleven-function
  API, no trait), and now the two importer stacks (3.2) and two game
  crates. Each is a `trait` waiting to be declared; each drifts silently
  on the target CI does not build.
- **Ch. 11, test organisation.** `session.rs` carries ~1,000 lines of
  tests that touch only its public surface and pull `agni-riftbound` in as
  a dev-dep of the *networking* crate; `net/tests/log_fold.rs` is an
  `agni-sim` test wearing an `agni-net` costume; the only transport test
  is `#[ignore]`d and now uncompilable (0.2). Move the inline suites to
  `tests/`, drop the game dev-dep, and make the ignored test build.
- **Ch. 20, `unsafe`.** Every `unsafe` block in both trees was read. The
  guest `alloc`/`dealloc` (`engine/wasm`, both plugins) and the JNI
  `from_raw` calls are justified and localised. `JsHandle`'s `unsafe impl
  Send + Sync` (3.6) is the one that asserts something false about the type
  rather than about a pointer.
- **API guidelines.** Still zero `#[must_use]`; `HostSession::intent`,
  `deal_groups`, `reload_groups`, `ClientSession::apply` return entries a
  caller must broadcast and are exactly the functions where dropping the
  value is a desync. `LogState`, `Table.cards`, `ClientSession.seat/roster`
  remain `pub` fields kai writes through.

## 5. Smell map

Ranked by what it would cost to leave.

| Smell | Where | Severity |
|---|---|---|
| Static empty deck substituted for a wrong enum variant, then `unreachable!()` on the write path | `kai/deck/sideboard.rs:309-326` | bug (1.1) |
| `&mut` through `ResMut` on a no-op path | `kai/deck/import.rs:490` | bug (1.2) |
| Two authorisation rules for one card (`guards_card` vs the `Reveal` arm) | `sim/log.rs:165-169` vs `:275` | bug (1.3) |
| `if let Ok(_) = self.append(..)` swallowing a refusal after a mutation | `net/session.rs:814-818`, `:821-824` | bug (1.3, 1.5) |
| `Option` where the reason was in hand | `session.rs:763`, `:672`, `:693` | structural |
| Per-action full-state CBOR round trip | `session.rs:356`, `:949` | structural (3.1) |
| Image bytes in the deterministic state type | `core/lib.rs:26` | structural (3.1) |
| Broadcast loop ×19, refresh pair ×14 | `kai/net/mod.rs` | structural (#4) |
| Seven-method `impl Engine` duplicated across two hosts | `kai/engine/web.rs:321`, `engine/host/lib.rs:412` | structural (#9) |
| Four pin→resolve→load state machines | `kai/engine/modules.rs` | structural (#9) |
| Parallel game crates and parallel importer modules | `games/mtg`, `importers/mtg` | structural (#1, 3.2) |
| `"exhausted"`, card-back tint, seat palette in agni | `sim/view.rs:65`, `sim/wire.rs:193`, `net/session.rs:77` | structural (3.3) |
| `unsafe impl Send + Sync` on a JS handle | `kai/engine/web.rs:168` | hygiene (3.6) |
| 91 `lock().unwrap()` on process statics | kai `net/`, `deck/`, `os/`, `telemetry.rs`; agni `net/bridge.rs` | hygiene (#7) |
| `HashMap` as a face map inside the deterministic crate | `sim/log.rs:514`, `sim/view.rs:111` | hygiene (§5C.5) |
| `pub cards` beside private `next_id` | `core/lib.rs:41-44` | hygiene (§4.12) |
| Permanent-`None` fields in the wire type | `sim/log.rs:66-69`, `core/lib.rs:27` | hygiene (3.4, #5) |
| `[f32; 3]` in the fold, engine hardened with `allow_floats` | `core/lib.rs:25`, `harden/lib.rs:71` | hygiene (#5) |
| `Vec<(u8, String)>` for seats, `Vec<(String, ByteBuf)>` for badges | `sim/log.rs:126`, `sim/view.rs:16` | nit |
| `LogAction::Genesis { .. } => unreachable!()` after an early return | `sim/log.rs:197` | nit |
| Six `store_dir`, three `encode_component`, three hex encoders | kai `net/`, `os/`, `deck/`; importers | nit |
| Global double-click timer | `kai/table/interaction.rs:207` | bug (1.4) |
| Docs naming 18 nonexistent files | `kai/AGENTS.md`, READMEs, `architecture.md` | hygiene (3.7) |

## 6. Order of work

1. **Today:** §0 (three one-line fixes so CI is green), 1.1, 1.2, 1.4 —
   four small edits, all S.
2. **This week:** 1.3(a) + 1.6 together — `Reveal` authorises on owner,
   `intent` returns `Result` and never reports a half-applied intent as
   success; add the probe test from 1.3 to `net/tests/` permanently.
   Top-10 #4 while `net/mod.rs` is open, using the `assign_if_changed`
   helper from 3.11 so 1.2's class cannot recur.
3. **Before the next game lands:** top-10 #1 as extended by 3.2 — one
   `Game` trait covering zone table, deal plan, deck shapes *and* the
   importer's section/identifier vocabulary. abyss-walker is the third
   copy otherwise.
4. **One format revision, once:** #5 + 3.4 + 3.1(2) — quantise tint, drop
   `art_path`/`table`/`prev`/`actor`, move `art_jpeg` out of `CardFace`,
   add a wire `version`; land golden CBOR vectors (#3) in the same change
   so the new format is pinned from day one.
5. **Then:** #9, #7, #8, #10, 3.3, 3.6 — none blocks a feature, all get
   cheaper the earlier they land.
6. **Doc sweep (3.7)** rides with whichever of the above touches each
   file; the kai layout table should be rewritten now, it is wrong today.

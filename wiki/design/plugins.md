# Plugins — Hot-Installable Game Rules as Wasm Deciders

> Status: design, approved for execution, with a running spike
> (`plugins/spike/hello-decider/`). Answers the ask: "a hot plugin system
> that allows players to download card packs or custom game modes in a safe
> way, probably with wasm sandboxing," with MTG and Riftbound as the dogfood
> plugins — and incorporates the approved MVP (a Riftbound table with free
> card movement, tap/exhaust, and deck import), the engine-as-wasm
> architecture directive, and the concluded runtime research (wasmi-hosted
> core modules, metered-module-hash identity). Builds on [architecture.md](architecture.md) (the
> neutral-engine doctrine and the open "are plugins data or code?" question —
> this doc closes it), kai's
> [deterministic-log.md](../../../../agni/kai/wiki/design/deterministic-log.md)
> (state = fold(log); the plugin slots into that fold), spirit's
> [identity.md](../../../spirit/wiki/design/identity.md) (card CIRs, the
> records card packs are made of) and
> [attestations.md](../../../spirit/core/wiki/design/attestations.md)
> (how a plugin gets signed). The workstream breakdown at the end is sized
> for direct execution.

## The three decisions, stated up front

**1. Native/Bevy-level extensions, or only sandboxed wasm?** Wasm only, as
the single third-party trust boundary. A native or Bevy-level plugin is
unsandboxable — it is arbitrary code with the user's privileges, which kills
the "download safely" promise outright, and it cannot exist at all on the
browser peer, which is a first-class replica of the same fold. Under the
engine-as-wasm architecture (below) the argument closes completely: plugins
are sibling wasm modules beside the engine module, identical bytes on every
platform, and there is no native seam left for a native plugin to plug
into. Everything a game legitimately needs from the renderer arrives as
**declarative descriptors** the client interprets (zones, layouts, prompts,
affordances — the view vocabulary below), never as code the shell executes.
The residue that genuinely needs native code — custom shaders, novel input
schemes, a 3D minigame between turns — is first-party territory: a curated,
in-tree, compile-time path (cargo features in kai), reviewed and shipped
like any other kai code, not downloaded. One boundary, no second-tier
"trusted plugin" tier to socially engineer around.

**2. Can a locked-down wasm API express whole new games, or only cards?**
Whole games — because the API is designed so the plugin *is* the game. A
game plugin is a pure decider inside the fold: `(plugin state, public table
state, proposed entry) → verdict + effects + next plugin state`. Turn
structure, phases, priority, resources, win conditions, the stack — all of
that is deterministic state transition, which is exactly what a gas-metered
pure function over CBOR expresses with nothing more than memory and math.
The engine keeps only the game-agnostic substrate (cards, zones as data,
seats, annotations, the log); everything Rae would call a "gameplay system"
lives in the plugin's own state blob and verdict logic. What the ceiling
excludes is not rules but *presentation* (answered by descriptors and the
curated path) and real-time/physics gameplay, which a card table is not.
The prerequisite is honest: today's `LogAction` vocabulary must grow a
generic `Game { data }` action, a card-annotation action, and plugin-declared
zones, and the fold must carry an opaque plugin state blob — that engine
work is a named workstream, and without it the answer to this question
would be "cards only." The MVP proves the same claim from the other side: a
plugin with **no decider logic at all** — a manifest of zones, visibility
and hotkeys over the free-form table — is already a shippable game mode.

**3. Card scripts, or declarative TOML definitions with logic pushed to
plugins?** Both, layered — but the engine never runs a card script.
Declarative definitions (stats, types, costs, keywords — TOML authored,
compiled to canonical CBOR, attested to card CIRs per identity.md) cover the
overwhelming majority of cards in both dogfood games. The **game plugin**
interprets keywords: "flying" is not logic on a card, it is one branch in
the plugin's combat-legality check, and a thousand cards share it. Bespoke
behaviour beyond the keyword set ships as a small effect DSL that the game
plugin defines and interprets *inside its own sandbox* — data riding the
card's rules record, never a second wasm boundary against the engine.
MTG's known nightmares (the layers system, replacement effects, state-based
actions) settle this decisively: they are global, cross-card rule systems
that no per-card script can express in isolation, so per-card logic as the
primary mechanism was never viable for the hard game. The existing
`CardScript`/`ScriptRegistry` surface in agni-plugins survives as the
host-side index mapping `ci:` identities to their definition and script
blobs — it becomes the plumbing that feeds card packs into the plugin, not
an execution surface of its own.

## The architecture: engine-as-wasm, plugins as sibling modules

Adopted by directive, and it simplifies more than it costs: **the whole
core — fold, sim, plugin surface — compiles to one `engine.wasm`.**
Desktop and Android are thin native shells that start a lightweight wasm
runtime (wasmi, per the settled runtime decision below) and instantiate
the engine plus the pinned game plugin as sibling modules; the web shell
has the browser itself instantiate the same modules via JS glue. There is no wasm-in-wasm anywhere — the host environment is
always the runtime, and the engine calls the plugin through imports the
shell bridges between the two instances. The same crates keep building
natively for `cargo test`, so the determinism suite runs both ways
(native and through the wasm boundary) and cross-checks itself.

What this buys:

- **One engine artifact, every platform.** The divergence surface between
  desktop, Android and browser collapses to the shell glue. The
  determinism guarantee stops being "same source, three compilers'
  opinions" and becomes "same bytes."
- **Genesis pins both hashes.** A table's genesis names the engine module
  *and* the game plugin module by content hash. Replay compatibility is
  total: a log names everything that gave it meaning, and a joiner fetches
  both blobs from the mesh by hash before folding entry zero.
- **The plugin API is a guest-side interface.** Plugins link against the
  engine's declared imports/exports, not a native host trait — so the
  contract is identical on every platform by construction, and the
  runtime choice (the runtime section below) cannot leak into plugin
  semantics.

What it forces into the open: the **renderer↔engine boundary becomes a
first-class deliverable**, equal in rank to the plugin API. Bevy cannot
reach into wasm memory per frame. The pattern is a mirrored table view:
the engine emits state deltas across the ABI **once per folded action —
never per frame** — the shell applies them to a native render-side mirror,
and Bevy reads only the mirror. This is not a new seam: it is
`drain_net`/fold made load-bearing — kai's systems already drain an event
queue into resources once per frame and render from those. The delta
vocabulary is specified below, unified with the plugin's presentation
vocabulary, because they turned out to be the same document viewed from
two sides.

Networking stays in the shell: iroh endpoints, QUIC and the mesh cannot
live inside a wasm module, so agni-net's pump loops run shell-side and feed
received frames across the ABI as engine inputs, exactly as the bridge
queue feeds Bevy today. The engine is pure compute: frames and intents in,
deltas and outbound messages out.

## A plugin is a pure decider inside the fold

The deterministic log is the keystone, and it hands this design most of its
answers for free. Today `state = fold(log)`: every replica — sequencer
included — derives the table by folding the same byte-identical entries
through `validate` + `fold_entry` (`sim/src/log.rs`). A game plugin slots
into that fold as the rules half of `validate`:

```
fold_entry(state, entry):
  engine checks    seq, seats, zone ownership, hidden-face invariants
  plugin decides   (state.plugin_blob, public table, entry) → verdict
  on accept        apply plugin effects + entry to table, store next blob
  on reject        deterministic no-op, seq slot consumed (as today)
  on gas trap      same deterministic no-op — the meter is in the hashed bytes
```

Because wasm execution is fully deterministic — the spec defines every
result, floats included, and the config below closes the deliberate escape
hatches — every peer running the *same module bytes* over the *same log*
folds to the same state. "Same module bytes" is enforced the same way
everything else in this stack is enforced: content addressing.

```toml
[genesis]
engine      = "blob:<blake3 of engine.wasm>"
game_plugin = "blob:<blake3 of the HARDENED plugin module>"
config      = "<CBOR: game options against the plugin's manifest, gas budget included>"
```

The hashes are part of the table's identity. A joiner who lacks either blob
pulls it over the existing mesh machinery (a blob under a ref, replicated
and hash-verified like card art), instantiates both, and folds from entry
zero. The p2p "download card packs and game modes" story is not new
infrastructure — it is spirit's existing blob/ref/gossip layer carrying two
more content kinds. Replays pin both hashes by construction, which answers
architecture.md's open question about rule-module identity in the replay
compatibility key: it is in the genesis, so it cannot not be pinned.

The decider is **pure**: plugin state is an opaque CBOR blob passed in and
returned, not memory retained across calls. That costs a serialization
round-trip per entry (hundreds of bytes at card-game rates — irrelevant)
and buys exactly the properties the log design already paid for elsewhere:
the plugin state is hashable for desync detection, snapshottable for seek,
and a fresh instantiation can never smuggle stale guest memory into a
verdict.

## The engine's log grows three things

The current `LogAction` vocabulary (Genesis/Join/Deal/Move/Reveal/Reset) is
the free-form table's alphabet. Plugins — and the MVP before any rules
exist — need three additions, all engine work in agni-sim/agni-core with
native tests, no wasm involved:

- **Plugin-declared zones.** `WireZone` grows a `Plugin(u16)` arm indexing
  the zone table the plugin's manifest declares (id, kind, owner,
  visibility, layout, place, span — pinned via the genesis plugin hash).
  `Move` entries carry it; layout stays client-side. `place` and `span` are
  the plugin's say in where a zone sits and how wide it is, so a renderer
  never has to know a zone by name; `place = offstage` declares a zone that
  exists in the log and replicates like any other but is never placed on the
  table at all.
- **Per-zone visibility as an engine invariant.** Whether a face may enter
  the log (`visibility = all`), rides the owner's private `Faces` channel
  (`owner`), or never leaves anyone's store (`none` — face-down decks) is
  decided by the zone table, enforced by the same fold code that enforces
  hand privacy today. This must be engine-enforced data, not plugin
  behaviour: a hostile plugin must not be able to opt hidden faces into
  the shared log.
- **Card annotations.** `LogAction::Annotate { card, key, value }` — a
  replicated, foldable key/value on a card, stored in an ordered map in
  `LogState`. Tap/exhaust is `key = "exhausted"`; counters, damage and
  markers ride the same action later. This is a wire change (new entry
  variant) but a backward-clean one: old logs contain none.

The engine-level validation for all three stays permissive by default
(free-form table posture: any seat may move or annotate public cards);
the plugin's decider tightens it when rules arrive — in the same plugin,
with no re-architecture, because the decider was always in the fold path
and the MVP plugin simply answered accept-all.

## Runtime: wasmi-hosted core modules, metered-hash identity

The runtime research concluded; this is the settled substrate. Modules are
**plain core wasm** — `wasm32-unknown-unknown` cdylibs built with plain
cargo, no component model, no WIT, no WASI. Native shells (desktop,
Android) host engine and plugin in **wasmi** (1.0.x now, 2.0 when it
lands); the web shell has the browser instantiate the same modules as
siblings via JS glue, copying CBOR across memories (~µs per crossing, and
crossings are per-action, never per-frame). wasmtime remains a documented
drop-in alternative — the contract forecloses nothing that speaks core
wasm. Three shapes were weighed for the contract itself:

| Option | What it buys | What it costs | Verdict |
|---|---|---|---|
| **Component model** (WIT-typed API) | Typed, versionable interface; generated bindings; the ecosystem's direction | Components do not instantiate in a browser without `jco` transpilation — a Node toolchain bolted onto the platform where plain wasm already runs natively; interpreter-class runtimes don't speak components either, so it would pre-empt the runtime decision; canonical-ABI copying gives back the zero-copy elegance; WASI 0.3 churn is not done | Not now. A WIT file may still serve as documentation IDL |
| **Extism** | Batteries-included plugin DX | A convenience layer whose batteries point the wrong way — its kernel and stock host functions (vars, config, HTTP) are exactly what this sandbox must *not* expose; another dependency between the engine and its modules | Rejected |
| **Raw core-module exports, canonical-CBOR payloads** | Runs on every candidate runtime and the browser's own `WebAssembly` API; CBOR is already the house codec (log entries, records, wire messages), so the ABI speaks the same bytes the log does; smallest determinism surface | Hand-rolled ABI discipline (an `abi_version` export and one `(ptr, len)` multi-value convention) | **Chosen** |

The second half of the settlement is **what gets hashed**. The pinned
identity is the *metered* module hash, not the source build's: a
publish-time hardening pass (finite-wasm / wasm-instrument style) rewrites
the module to inject deterministic gas accounting, stack-height checks and
a declared memory maximum — and, for third-party plugins, validates that
the module contains **zero float and SIMD opcodes** (agni-core already
bans floats in the simulation; the ABI keeps the ban structural: wire
types are integers and bytes only). The transformed module is what enters
the spirit store and what genesis pins, so the meter travels inside the
content hash and every runtime — wasmi, wasmtime, the browser's own
engine — traps at the same instruction on the same inputs. Performance
notes that survive the decision: wasmi is interpreter-class (order
10–50× slower than cranelift), which is irrelevant for the MVP's
accept-all decider and fine at card-game rates generally; budget the MTG
layers pass against it when that plugin lands, with wasmtime the
drop-in escape hatch if it ever matters.

The guest-side interface, in full — what a plugin module exports to the
engine (bridged by the shell between sibling instances):

```
exports:
  abi_version() -> u32                          today: 0
  alloc(len: u32) -> u32                        request buffers in guest memory
  dealloc(ptr: u32, len: u32)
  manifest() -> (u32, u32)                      (ptr, len), CBOR, static
  decide(ptr: u32, len: u32) -> (u32, u32)      CBOR request in, CBOR reply out
  view(ptr: u32, len: u32) -> (u32, u32)        CBOR in, CBOR out; optional

imports: none.
```

`manifest` returns the static self-description: plugin name and version,
the zone table, hotkey-able action templates, and game-config schema. It is
read once at install and its content is pinned by the module hash.
`decide` takes `{ state, table, entry }` and returns
`{ verdict, effects, state }`. `view` takes `{ state, table, seat, faces }`
and returns dynamic presentation (prompts, affordances, narration) — the
MVP plugin omits it entirely. No WASI, no host functions, not even logging
in release. Everything the plugin knows arrived in its arguments;
everything it can do rides its return value.

Effects are engine-applied primitives, so engine invariants stay enforced
by the engine rather than trusted to the plugin:

```
Effect = MoveCard { card, zone, seat, index }
       | SpawnCard { face, zone, seat }         tokens, emblems
       | DespawnCard { card }
       | Annotate { card, key, value }          exhausted, counters, markers
```

A plugin cannot forge a reveal or read a hidden face through effects,
because faces are not in the fold state at all — see the hidden-information
section.

## The spike, and what it proved

`plugins/spike/hello-decider/` is two crates deliberately outside the
workspace (each roots its own `[workspace]`, so `cargo build --workspace`
never sees them). The guest is a ~900-byte `no_std` module built for
`wasm32-unknown-unknown` exporting the full required export set (with a toy
FNV fold standing in for rules, plus a deliberate infinite loop `spin`);
W2 extended it from the original 593-byte decide-only spike so it doubles
as the hardening pipeline's real-rustc-output test fixture, checked in at
`plugins/harden/tests/fixtures/hello_decider_guest.wasm`.
The host is wasmtime 48.0.1 with `default-features = false, features =
["cranelift", "runtime"]`, configured with `consume_fuel(true)`,
`cranelift_nan_canonicalization(true)`, `relaxed_simd_deterministic(true)`,
and a `StoreLimits` memory cap.

Observed, asserted, and reproducible:

- the module has **zero imports** — instantiation with an empty import list
  succeeds, so "no WASI, no host functions" is real, not aspirational;
- two runs of `decide` on fresh stores return **identical bytes and
  identical fuel** (406 units for the toy input);
- the infinite loop **traps deterministically** at fuel exhaustion with the
  memory cap holding — the sandbox contains a hostile module with no
  timeouts, threads or watchdogs involved.

Run it: build the guest with
`cargo build --release --target wasm32-unknown-unknown` in `guest/`, then
`cargo run --release -- <path to guest.wasm>` in `host/`. (On NixOS the
guest link needs `wasm-ld` on the linker path; kai's devenv already
provides it for its own wasm builds.) The spike hosts in wasmtime as the
strictest determinism reference; the shipping host is wasmi per the
runtime decision, and the workstream-zero verification bar repeats these
assertions there — host-level fuel becomes a backstop once publish-time
gas injection is the semantic meter.

## Gas lives in the module, so a trap is a verdict

The subtlety the fold forces into the open: if exhaustion counted as
"entry rejected" while metering lived in the *host*, gas accounting would
become part of game semantics that no two runtimes — and no browser —
compute identically. The metered-module-hash decision dissolves this: the
gas counter is injected into the module bytes at publish time, the hash
pins the transformed module, and therefore every runtime executes the
identical trapping instruction at the identical point. So the rule is:

> Gas exhaustion during a **sequenced** entry's decide call is a
> deterministic in-band trap — the entry is a deterministic no-op at every
> seat, exactly like any other rejected entry. Stack-overflow and
> memory-max traps behave the same way, because their checks are also in
> the hashed bytes.

Two boundaries remain host-side and are deliberately *not* semantic:
per-call gas budgets set by the engine (generous — DoS containment, not a
resource plugins tune against) must be part of the genesis config so all
seats grant the same allowance, and any *host-level* backstop a shell
adds (wasmi/wasmtime fuel around the whole call, watchdogs on the
advisory `view` path) is a fault detector only — a replica that trips one
stops folding and declares the plugin broken rather than inventing a
verdict, since host-level limits are exactly the thing peers cannot count
identically.

The e-ink consumer (paisho) never runs deciders at all — per
architecture.md it is a client of a peer, consuming the mirrored view; if
that ever changes, MCU-class interpreters (wamr, wasm3) exist, and the
pure-CBOR ABI is exactly the shape that ports.

## The hardening pipeline as built (W2)

`agni-harden` (workspace member at `plugins/harden`; nothing depends on it
yet) is the minting pass. Where reality diverged from the sketch above:

- **Instrumentation library**: `radix-wasm-instrument =1.0.0` — the parity
  `wasm-instrument` lineage rewritten on wasmparser/wasm-encoder, so it
  digests modern rustc output (bulk memory, sign-ext, reference types),
  MIT/Apache. finite-wasm ships analyses but calls its own transformation
  pass test-grade; `gwasm-instrument` 0.3 is still on the dead parity-wasm
  parser; `gear-wasm-instrument` 2.0 is GPL-3.0 and Gear-shaped. The exact
  version pin is deliberate: the injected counting must be version-stable,
  and a frozen dependency is a feature when the output hash is the identity.
- **Gas is a mutable global, not a host function**: the meter adds zero
  imports. An exported mutable i64 global `gas_left` is decremented by an
  injected local function per metered block; exhaustion writes the sentinel
  `u64::MAX` and executes `unreachable`. The configured limit (default
  100_000_000) is baked into the global's init value, so it is part of the
  hashed bytes; hosts apply the genesis budget by writing `gas_left`
  before a call. `gas_left` is a reserved export name — input modules
  claiming it are refused.
- **Pass order**: stack limiter first, then gas — the reverse order bloats
  (the limiter would instrument every injected gas call), and this way the
  limiter's checks are themselves gas-charged.
- **Stack default is 512 value-slots**, chosen so any recursion whose
  frames cost at least one slot traps in-band before wasmi's default
  1000-frame recursion backstop. Frames the limiter prices at zero slots
  (no params, no locals, empty value stack) can still only be caught by
  the host backstop, which per the fault-detector rule means "stop folding
  and declare the plugin broken", never a verdict.
- **Validation is stricter than the sketch**: exactly one memory, exported
  as `memory`; declared maximum clamped to the cap (default 256 pages =
  16 MiB), minimum above the cap refused; `view` is a required export in
  practice (the accept-all MVP plugin exports a stub) alongside
  abi_version/alloc/dealloc/manifest/decide; float and SIMD refusal names
  the offending opcode; imports refuse with the named import.
- **Custom sections are stripped** — the pinned identity carries no debug
  names, producers or target-feature notes, and stripping also sidesteps
  the instrumenter's unimplemented name-subsection rewriting.
- **Surface**: lib `harden(module: &[u8], &HardenConfig) ->
  HardenedModule { bytes, hash, report }`; bin `agni-harden input.wasm
  output.wasm [--gas-limit N] [--stack-height-limit N]
  [--memory-max-pages N]` printing exactly the blake3 hex of the output on
  stdout (report goes to stderr), which is the line W5's publish flow
  consumes before minting the blob + CIR + attestation.
- **Verification so far**: byte-determinism (harden twice, compare bytes),
  semantic preservation and identical gas counts across independent wasmi
  executions, deterministic gas trap on the spike's `spin` and a looping
  guest, deterministic stack-bomb trap distinguishable from a gas trap by
  the sentinel. The cross-runtime leg of the bar (same trap point under
  wasmtime and a browser engine) rides W0's harness, which owns those
  hosts.
- **Engine-module hardening (the open question below): W2's call is
  plugins-only.** The pipeline validates the plugin ABI's export set, which
  the engine module does not have; the engine is trusted first-party code
  and gets the hash pin, not the meter.

## The engine.wasm boundary as built (W0)

W0 landed the engine-behind-wasm critical path. Where reality diverged from
or sharpened the sketch above:

- **Crates**: `agni-engine-wasm` (`engine/wasm`) compiles agni-sim's fold,
  Decider dispatch, snapshot/restore and the view builder into a
  `wasm32-unknown-unknown` cdylib with zero imports; `agni-engine-host`
  (`engine/host`) hosts it in wasmi 1.1. The engine ABI's shared types live
  in `agni_sim::abi`, the mirrored-view schema in `agni_sim::view`, and the
  host-facing `Engine`/`PluginModule` traits plus the `fold_mediated`
  trampoline in `agni_sim::engine` — one mediation algorithm for native,
  wasmi and browser hosts.
- **Engine exports** (all CBOR request/reply over the packed `(ptr << 32) |
  len` u64 convention, replies `Result<T, String>`): `abi_version` (0),
  `alloc`, `dealloc`, `fold_entry` (entry + optional verdict + fold mode +
  viewer → fold result + view deltas), `fold_log` (a whole welcome log in
  one crossing), `decide_request` (validate + build the plugin's
  `{plugin_state, state, entry}` request), `snapshot`, `restore`, `view`.
  The engine instance retains its `LogState` between calls — it is the
  authoritative fold state; plugins stay pure.
- **Fold modes**: `Sequenced` is replica semantics (an invalid entry burns
  the seq slot as a deterministic no-op); `Admission` is the sequencer's
  probe-and-append (refusal mutates nothing). Both reduce to the same
  `fold_begin`/`fold_finish` split of `fold_entry_with` in `agni_sim::log`,
  so the native and wasm paths share one validation/mutation body.
- **The trampoline**: the engine has zero imports, so the host drives
  decide — `decide_request` crossing to the engine, `decide` crossing to
  the sibling plugin instance, `fold_entry` crossing back with the verdict.
  A plugin gas/stack/memory trap or an undecodable reply is a deterministic
  reject verdict; only host-level faults (fuel backstop, missing exports)
  stop the fold as "plugin broken". The same shape runs under wasmi and
  under the browser's own instantiation.
- **The mirror**: `TableView` carries zones, seats, revealed set, next_seq
  and per-card `{id, zone, seat, owner, face_visible, rotated, badges}` —
  **no face bytes ever cross the boundary**; the shell overlays faces it
  legitimately holds (own deals via `Faces`, public faces from `Reveal`
  entries) onto the mirror. Deltas are section-granular
  (`Zones/Seats/Cards/Revealed/NextSeq`), emitted once per folded action,
  applied to a session-owned mirror; Bevy renders from the mirror-derived
  `Table` and crosses nothing per frame. Finer per-card deltas are W3's
  call if layout wants them.
- **Sessions**: `HostSession`/`ClientSession` no longer own a `LogState` —
  they own a `Box<dyn Engine>` (native or wasm or browser), the mirror, and
  an engine-truth `state_cache` refreshed by a snapshot crossing after each
  fold (the `state()` accessor and the sequencer's auto-reveal planning
  read it; whatever the plan produces still passes the engine's admission
  fold). Join is one `fold_log` crossing. Native `NativeEngine` remains the
  default so pure tests never touch wasm.
- **Hardening**: the engine module passes the same pipeline with
  `HardenConfig::engine()` — engine export set, floats permitted (the
  first-party engine serializes `f32` tints; the third-party plugin ban is
  unchanged), 512 stack slots, 4096-page memory cap, 10^10 baked gas.
  Hosts write `gas_left` per call; engine exhaustion is a fault ("stop
  folding"), never a verdict — engine gas is DoS containment, not
  semantics. Genesis pins the blake3 of the hardened module via
  `TableConfig.engine`, set by `HostSession::with_engine` from the module
  actually loaded; `verify_engine_pin` refuses joins under a different or
  missing engine with an honest error.
- **Transport for now**: the hardened `engine.wasm` ships as a bundle
  asset — `assets/engine/engine.wasm` on desktop/android (env override
  `AGNI_ENGINE_WASM`), `./engine.wasm` fetched and compiled once at boot in
  the browser, instantiated fresh per session. The loading seam is
  `agni_engine_host::ModuleSource`/`ModuleBytes`; W5 replaces the file/
  bundle sources with the spirit store without touching the hosts. A shell
  that finds no module falls back to the native fold loudly (status line
  says so) rather than silently.
- **Verification**: `engine/host/tests` and `net/tests/wasm_session.rs`
  fold scripted logs natively and through the hardened module and assert
  byte-identical snapshots, logs and views; mediation parity against native
  deciders; deterministic plugin gas-trap rejection; per-call budget
  override; pin verification. The flake builds and hardens the module
  (`checks.agni-engine-wasm`) and feeds the raw module to `agni-nextest`
  via `AGNI_ENGINE_WASM`; dev runs build it on demand with the wasm32
  target.

## The descriptor renderer as built (W3)

kai renders any plugin-declared zone table; hand+board hardcoding is the
degenerate empty-table case, unchanged. Where reality diverged from or
sharpened the sketch:

- **The mirror reaches Bevy as a resource clone, not a delta stream.**
  Sessions own the delta-applied `TableView`; kai's `Mirror` resource takes
  a clone at every point a session refreshes (once per folded entry — the
  per-frame crossing count stays zero). Finer per-card deltas remain
  available if profiling ever wants them; at card-game rates the clone is
  noise.
- **Layout** (`kai/src/zones.rs`, pure): each `ZonePlace` band is laid out
  left to right in declaration order, each zone taking a share of the width
  proportional to its `span` — `Inner` is the per-seat row nearest the
  table center, `Outer` the row nearest the player's own edge, both spun by
  seat yaw so opposing rows read facing each other; `Center` zones line the
  seam between the seat quads at yaw 0, in a deeper band so they straddle
  both halves. Fan = the existing hand fan (a plugin fan
  zone merges with built-in `Hand` into one fan and one scroll; foreign
  fans render as back arcs, hidden while viewing that seat). Pile stacks
  in place with the count in its overlay label; Row spaces at board
  spacing compressed to fit; Spread overlaps at half-card spacing; Grid
  wraps at √n columns stepping toward the center. Labels are a subtle
  egui overlay projected from each zone's near edge. Exhausted
  (`rotated`) turns a flat card 90° in place.
- **Interactions**: every zone placement spawns a translucent drop quad
  routing `WireIntent::Move` through the existing `CardDropped` path;
  the seat plane keeps the hand-strip/board fallback. Exhaust/ready =
  double-click or `E`; draw = drag the top of a deck pile or `D` with the
  deck hovered (deck→fan move; works on main and rune decks alike);
  `T` sends the hovered card to the seat's discard zone. Shuffle from the
  MVP hotkey list is deferred: the log has no shuffle action yet — it
  arrives with the W6 plugin work that needs it. Foreign private zones
  refuse pickup and drop client-side so the optimistic overlay can never
  ghost a move the sequencer will refuse.
- **Draws out of hidden decks stay honest**: `HostSession::private_faces`
  computes which seat is owed a face after a fold (unrevealed card now in
  an owner-visibility zone) and kai's host loop sends it on that seat's
  private `Faces` channel — the same channel a deal uses.
- **Hosting a zoned table**: the multiplayer panel grows a
  "riftbound zones" toggle; hosting then pins
  `agni_riftbound::zone_table()` into the genesis `TableConfig`.
- **Deck import** (`kai/src/import.rs`, every platform): a paste box
  sniffed by `agni_importers::riftbound::parse_any` plus a URL field.
  Native and android resolve through the importer query path — the store
  catalog when ingested, the live Riftcodex API otherwise, link fetching
  included; the browser sends both forms to the gateway's
  `/gateway/resolve/deck` and shows resolver errors verbatim (including
  the paste-fallback guidance). "Seat this deck" stages the
  `ResolvedDeck` plus a riftbound_id→`CardFace` map in the `SeatedDeck`
  resource; W6's deal choreography consumes it. Faces resolve by
  riftbound_id against the store's `riftbound` manifest natively and the
  gateway bridge's riftbound manifest on web (spirit's gateway manifest
  endpoint now passes manifests through as generic JSON so the id field
  survives); art out of reach renders the honest tinted placeholder.
- **Prompt/affordance surfaces are not rendered** and their vocabulary is
  not yet in the `TableView` schema — W0 shipped zones/cards/seats only,
  no plugin emits `view` output yet, and the MVP renders none; the
  sections land when the first rules plugin produces them.

## Module distribution as built (W5)

W5 made the engine and plugins spirit-store runtime artifacts. Where reality
diverged from or sharpened the sketch above:

- **The ref convention**: a module lives in the store as two blobs — the
  HARDENED module bytes and a CBOR `ModuleManifest { name, kind
  (engine|plugin), abi_version, display, version, module: <blake3 hex of the
  module blob> }` (`spirit_core::modules`) — plus a ref file
  `refs/modules/<name>` holding the manifest hash, exactly the card-set
  shape one level down. `local_refs` advertises these as `modules/<name>`
  with completeness `(1, held)`, so gossip, `best_provider`, converge and
  `pull_ref` replicate modules over the mesh with zero new sync machinery;
  `pull_ref` pulls whatever a record's `{kind, refs}` envelope declares —
  spirit no longer decodes any typed manifest. Ref names from peer adverts
  pass `safe_ref_name` before touching the filesystem, and a name is only
  followed at all if the advertising peer is trusted at cache level or
  above.

  As of 2026-09-04 the manifest is no longer the identity carrier: a module
  version is a `wasm-module` CIR, its bytes are bound to it by a signed
  content attestation, and `refs/modules/<name>` points at the head of a
  collection holding every version. The genesis pin still enforces
  integrity for a table join, which is why joins needed no protocol change;
  what the collection adds is *which* version a fresh client picks, and on
  whose signature.
- **Publish flow**: the agni-harden bin grew the store leg —
  `agni-harden in.wasm out.wasm [--engine] --store <dir> --name <ref>
  [--module-version V] [--abi-version N]` hardens, writes the output, then
  publishes the module CIR, a `wasm-harden` TDR (input blob hash plus every
  limit — same input and same config yields the same output bytes), a
  signed content attestation and an `add` op on `col:modules/<ref>`. Stdout
  stays exactly the pinned blake3 hex. Role derives from `--engine`;
  `--display` is gone, because a display string is not identity — two nodes
  spelling it differently would mint two CIs for one module.
- **StoreSource**: `agni_engine_host::StoreSource { dir, name, abi_version }`
  implements the `ModuleSource` seam over a spirit store. `load()` resolves
  the collection — newest version, trusted signer, matching ABI — and
  returns that `Version` plus its bytes; `engine_module()` refuses a plugin
  role; `versions()` lists everything for a UI. The web peer has no local
  store and no trust registry, so the node that *has* both does the
  resolving: `/gateway/modules` serves the folded versions as JSON with
  signer and trust, and module bytes still come from
  `/gateway/blob/{hash}`, verified blake3-against-the-listing before
  compile.
- **Resolution order in kai** (status line always names source and hash):
  explicitly selected module ref → store ref `modules/engine` → the W0
  bundled asset (`AGNI_ENGINE_WASM` / `assets/engine/engine.wasm`,
  `./engine.wasm` on web) → loud native fold. A broken selection refuses
  loudly instead of silently substituting.
- **First-run seeding**: before the node serves, kai seeds the bundled
  hardened engine (and `assets/plugins/riftbound.wasm` once W6 mints it —
  the seam reads that path plus `AGNI_RIFTBOUND_WASM`) into the local store
  as blob + manifest + `refs/modules/{engine,riftbound}`. Seeding is
  strictly if-absent: a mesh-installed version is never clobbered by the
  bundle at next launch. Blobs that appear in the store while the node runs
  (a publish, a seed) are imported into the serving iroh store on the next
  mesh round, so hot-published modules are servable without restart.
- **Hot reload**: kai's modules panel lists `refs/modules` entries (name,
  kind, short hash, source), selects the active plugin for NEW tables and
  the active engine ref, and re-resolves from the store on demand — no app
  restart. Genesis pins both hashes at table create
  (`HostSession::with_engine` writes `config.plugin` from
  `PluginModule::module_hash`); existing tables keep their pinned modules,
  per the install-time-not-mid-game rule.
- **Join-time fetch**: a joiner holds the Welcome until every genesis-pinned
  module resolves — store lookup by hash first, bundled bytes if they match
  the pin, otherwise `Mesh::request_blob` queues the hash and the mesh round
  pulls it from a provider (`blob_providers`: hint, then complete
  `modules/*` advertisers, then known peers) via the existing iroh blob
  downloader; the web peer fetches `/gateway/blob/{hash}` instead. Bytes are
  blake3-verified against the pin before instantiation
  (`module_matches_pin`; a corrupt store blob refuses rather than loads),
  then `verify_engine_pin`/`verify_plugin_pin` gate the fold. A client that
  loaded a plugin the genesis does not pin is refused too — folding with an
  unpinned decider is a divergence, not a convenience.

## The Riftbound MVP as built (W6)

The MVP shipped: two seats on two clients host/join a Riftbound table, import
decks, deal them into the full zone anatomy, move cards between every zone in
both directions, and tap/untap — all through the hardened plugin arriving via
the store path, with byte-identical logs at both seats. Where reality diverged
from or sharpened the sketch above:

- **The plugin** (`plugins/riftbound`, package `agni-riftbound-plugin`,
  cdylib `riftbound_plugin.wasm`) is a manifest and nothing else, and its
  guest code is almost nothing too: a `build.rs` (host-side, with
  agni-riftbound + agni-sim as build-dependencies) bakes three CBOR constants
  into the module — the manifest, an accept-all `Verdict`, and a CBOR null
  for the `view` stub — and the guest exports the required set returning
  those static bytes. No serde, no ciborium, no decoding in the sandbox at
  all, which is also why the ~21 KB module passes the third-party pipeline's
  zero-float validation without ceremony. `decide` frees its request buffer
  unread: accept-all needs no inputs. Hardened with `HardenConfig::default()`
  (100M gas, 512 stack slots, 256-page cap).
- **The manifest schema** landed in `agni_sim::wire` as
  `PluginManifest { name, version, display, zones, hotkeys }` with
  `encode/decode_plugin_manifest`; zones ride the existing zone-table CBOR
  schema. Hotkeys (`e`/`d`/`t` → toggle-exhaust/draw/to-trash) are advisory
  display metadata — kai's client-side bindings from W3 are unchanged.
  `WasmPlugin` grew host-side `manifest_bytes()` (a `manifest` call through
  the same packed-u64 convention as every other export).
- **Zones source of truth**: hosting a Riftbound table reads the zone table
  FROM the loaded plugin's manifest and pins that into the genesis
  `TableConfig`, falling back to the compiled `agni_riftbound::zone_table()`
  when no plugin module resolves; the hosting status line names which one
  shipped ("zones from the plugin manifest" / "zones from the compiled
  table"). Today the two are identical by construction — the manifest is
  generated from the compiled table at build time — but the read path is the
  plugin's, so a newer mesh-installed plugin's zones win.
- **Minting and shipping**: the flake builds and hardens the module
  (`checks.agni-riftbound-plugin`, hash written beside the blob like the
  engine's), feeds the raw module to `agni-nextest` via
  `AGNI_RIFTBOUND_WASM`, and copies the hardened blob into kai's bundles at
  `assets/plugins/riftbound.wasm` (desktop `share/kai` tree and the web
  dist); devenv grew `plugin-build` in agni and kai mirroring
  `engine-build`. Nothing is checked in — kai's `.gitignore` covers
  `/assets/plugins/`. W5's seam does the rest unchanged: first-run seeding
  to `modules/riftbound`, the modules panel listing/selection, genesis
  pinning, join-time fetch by hash.
- **Table creation**: the multiplayer panel's riftbound toggle now
  pre-selects `modules/riftbound` as the active plugin when none is chosen,
  so a riftbound table pins the plugin hash without a trip through the
  modules panel; an explicit selection is respected (that plugin's manifest
  zones then govern the table).
- **Deal choreography** is three layers, each where it belongs. The
  vocabulary is neutral engine data in `agni_sim::wire`:
  `DealGroup { target, faces, shuffle }` with
  `DealTarget::Zone(name) | Spread(prefix)`. The riftbound arrangement is
  pure data in `agni-riftbound::deal_plan`: legend → `legend`, chosen
  champion → `champion`, runes → `rune-deck` (shuffled), main deck →
  `main-deck` (shuffled), battlefields → `Spread("battlefield-")`,
  sideboard → `sideboard`. The executor is
  `HostSession::deal_groups(seat, groups)`: zones resolve by name against
  the genesis-pinned table (all targets validated before anything folds, so
  a refusal mutates nothing), `All`-visibility groups deal then `Reveal`
  each card into the log (legend/champion/battlefields are public facts),
  `Owner` groups return private faces for the seat's `Faces` channel
  (sideboard), `None` groups deal ids only (both decks — faces stay in the
  dealer map until drawn, riding the W3 `private_faces` path). `Spread`
  places each card into the fewest-occupied matching zone (declaration
  order breaks ties), so two three-battlefield decks land 2-2-2 across the
  three shared zones, first-come.
- **The opening hand rides the same choreography.** A `DealGroup` carries a
  `draw` count: after the group's cards land, that many move off the top
  into the seat's hand — the zone-table's per-seat `Hand`-kind zone — as
  ordinary `Move` entries with private face delivery, exactly the `D`-draw
  mechanism. `deal_plan` sets `draw: 4` on the main deck (Riftbound's
  opening-hand size), so dealing your deck seats you with four cards at
  both the host and `DealDeck` joiner paths. This is convenience
  choreography, not rules enforcement: no rules weight attaches, players
  remain free to draw more or put cards back, and mulligans stay deferred
  with the rest of the no-rules posture.
- **Zone tables start clean.** `host_from_with` replays the pre-session
  solo table through the log only for free-form tables (empty zone table
  in the genesis config). A riftbound/zone-table table starts from a clean
  genesis: no solo import, no legacy hand dealt to joiners, and a redeal
  resets to an empty table until decks are dealt. The first dogfood round
  found the boot-time hob hand leaking onto riftbound tables; the gate and
  its regression tests (agni-net session tests plus kai's net tests) pin
  the fix.
- **Placeholder faces are playable.** A face with no art renders its card
  name painted onto the tinted card (an egui overlay using the same
  world-to-viewport projection as the zone labels, wrapped to the card
  width), so an un-ingested deck plays by name from the moment it deals;
  faces arriving later through private draw delivery label the same way,
  and the label yields to art the moment art lands.
- **The shuffle, stated honestly**: there is still no shuffle log action —
  the dealer's ordering IS the shuffle, exactly as at a paper table where
  the dealer shuffles before dealing. `deal_groups` seeds a Fisher-Yates
  from blake3(seat, next_seq, face names) and deals in that order; the seed
  is derived, not logged, because replay never needs it — the `Deal`
  entries fix the order for every replica by construction. What the seed
  buys is determinism and auditability of the dealer, not
  trustlessness: the host already holds every face in phase A's
  host-as-referee trust model, and a committed-randomness shuffle
  (commit-reveal between seats) is phase-D commitment work, deferred with
  the rest of hidden-with-rules-weight machinery.
- **Both seats deal on request**: the wire grew
  `ClientMsg::DealDeck { groups }` — a joiner stages an imported deck
  (W3's `SeatedDeck`) and clicks "deal my deck onto the table"; the host
  executes the same choreography for that seat and broadcasts. A seat whose
  `main-deck` zone is already occupied is refused (no double deals). The
  button lives in the deck-import panel and enables only at a joined table
  whose zone table has a `main-deck`.
- **Reloading a seat is a clear plus a deal.** Between-games sideboarding
  edits the *deck list*, not the live table — the main-deck zone is
  `ZoneVisibility::None`, so nobody (including its owner) can enumerate it
  from the table, and the honest source of the owner's own list is kai's
  `SeatedDeck` record. Putting the edited list on the table therefore means
  replacing the seat's cards, which needs a way to take cards off the table:
  `LogAction::Clear { seat }` sweeps every card whose `owner` is that seat
  (per-seat zones and the seat's contribution to the shared battlefields
  alike, since `owner` records who dealt a card and never changes), together
  with its reveals and annotations. It folds only from the host or from the
  seat itself, and is a `NoOp` when that seat holds nothing.
  `HostSession::reload_groups(seat, groups)` validates the groups first,
  then appends the clear and the deal as one sequenced run, so a refusal
  never leaves a seat swept-but-undealt. `ClientMsg::ReloadDeck { groups }`
  carries the joiner's path; both ends bump the render generation on the
  `Clear` entry so the table re-deals rather than accumulating.
- **Seats carry a colour.** `SeatInfo` grew `color: u8`, an index into
  kai's `table/colors.rs` palette (blue, red, green, gold, purple, teal,
  white, pink) — an opaque index on the wire, render data in the client. Only the first `SEAT_PICKABLE_COLORS` (5) can be claimed by a
  seat; the tail is reserved so contested zones can be named without ever
  colliding with a seat's name — `contested_color(slot)` hands battlefield
  1/2/3 teal/white/pink at every seat count, which is what makes "move it
  to blue" unambiguous at two seats (where both seats sit at x=0 and a
  nearest-seat mapping is a tie) and at four. `join` takes the first free
  colour starting from `seat % 5`, so play never blocks on a choice;
  `pick_color(seat, color)` is first-come-first-served and refuses a colour
  another seat holds. Either outcome broadcasts `HostMsg::Roster`, so the
  loser sees the winner in the roster and is prompted to pick again — the
  roster stays the single channel for seat colour.
- **The acceptance test** (`net/tests/riftbound_mvp.rs`) is the MVP proof
  and runs unignored in `agni-nextest`: both sessions on the hardened
  `engine.wasm` + hardened plugin (per-entry mediation through wasmi at
  both seats), plugin published to and fetched back from a real spirit
  store by its genesis pin (hash-verified via `module_matches_pin`, wrong/
  missing modules refused), every wire crossing round-tripped through
  `encode_/decode_` exactly as net_smoke does, synthetic 58-card decks
  dealt from both seats (host directly, joiner via `DealDeck`), draws out
  of hidden decks with private faces, hand↔battlefield and
  champion/legend↔battlefield moves in both directions from both seats,
  tap/untap replicating into both mirrors, a foreign-hand move refused,
  and at the end: logs byte-identical, engine snapshots byte-identical,
  and a byte-scan proving no undrawn main-deck, rune, or sideboard name
  ever entered the shared log. The in-memory two-session shape (rather
  than net_smoke's live iroh endpoints) is deliberate: it runs in the nix
  sandbox on every CI push, where the socket-bound variant stays
  `#[ignore]`d.
- **Deferred, named**: a real shuffle/reshuffle log action (needed the
  moment cards return to a deck mid-game); commit-reveal shuffle fairness
  (phase D); dealing the gas budget through genesis `options`; `view`
  output and prompt/affordance rendering (still no plugin emits any);
  browser-hosted tables (unchanged W3 posture — the browser joins, deals
  via `DealDeck`, but cannot host); mulligans (the opening four are dealt
  as convenience choreography per the deal bullet above — no rules weight
  attaches, and players still draw with `D`/drag per the no-rules
  posture).

## The second plugin, the game selector and on-the-fly art as built (W7)

W6 proved one guest plugin. W7 proved the *seam* by minting a second one
through the identical pipeline, replaced the boot-time sample deal with an
empty table on every platform, and made art arrive per card, on demand,
instead of per set. Where reality diverged from or sharpened the sketch:

- **No table populates itself any more.** The startup deal is gone from
  every platform: desktop/android no longer read a random hand out of the
  store's `hob` set at boot, and the browser no longer redeals when the
  gateway bridge lands (`bridge::faces` and `bridge_redeal` are deleted;
  the bridge now establishes itself from `/gateway/status` alone rather
  than requiring a complete card-set ref, so deck resolution and blob
  fetching survive a gateway that holds no sets). A joiner is seated and
  dealt nothing — the free-form auto-deal at join is gone too. Every shell
  boots to an empty table with one centred line: "create a table or import
  a deck to begin". The one surviving deal is explicit and native-only:
  the tuning window's "deal a sample hand" button still lays out the
  store's `hob` set for a free-form solo/host table, because free-form
  tables have nowhere else to get cards; on web the button does not exist.
- **MTG is a guest plugin, not a special case.** `plugins/mtg` (package
  `agni-mtg-plugin`, cdylib `mtg_plugin.wasm`) is a byte-for-byte copy of
  riftbound's shape — a `build.rs` baking manifest/accept/view CBOR
  constants, the same export set, no serde in the sandbox — and goes
  through the same flake path (`checks.agni-mtg-plugin`, hardened blob +
  `.blake3` beside it, bundled to `assets/plugins/mtg.wasm` in both the
  desktop tree and the web dist, raw module fed to `agni-nextest` as
  `AGNI_MTG_WASM`), the same devenv `plugin-build`, and W5's seam
  unchanged: first-run seeding to `modules/mtg`, modules-panel listing,
  genesis pinning, join-time fetch by hash. The zone table lives in
  `agni-mtg`: per-seat hand (fan/owner), library (pile/none), graveyard
  (pile/all), exile (row/all), battlefield (row/all), command (row/all).
  `agni_mtg::deal_plan` mirrors riftbound's: commander → `command`,
  library → `library` shuffled with `draw: 7`. Data and choreography,
  zero rules.
- **The table's game is a genesis fact, read back from its zones.**
  `kai::net::TableGame { FreeForm, Mtg, Riftbound }` replaces the old
  riftbound checkbox with a three-way radio in the hosting panel. The
  choice drives plugin pre-selection (`modules/mtg` | `modules/riftbound` |
  none — an explicit modules-panel selection still wins), the zone table
  (plugin manifest first, compiled table as the named fallback, empty for
  free-form), and which importer the deck panel offers. Joiners get no
  selector because genesis pins everything; their panel reads the game back
  out of the joined table with `game_of_zones`, which recognises a table by
  the deck zone its plugin declared (`main-deck` → Riftbound, `library` →
  MTG, neither → free-form). The same function drives the once-per-seat
  deal guard, so neither game can double-deal.
- **Scryfall moved into the MTG importer.** `agni-importers/mtg` is the
  riftbound importer's twin one game over: `text_list` parses `<qty> <name>`
  with optional Commander/Deck/Sideboard headers, `SB:` prefixes, `#`/`//`
  comments, Arena set+collector suffixes and `*F*` foil marks (a
  parenthetical that is not set-shaped stays part of the name);
  `catalog` holds the name-normalised lookup trait with `StaticCatalog`,
  `Cached` and a `Layered` local-first-then-remote combinator;
  `scryfall_named` is the resolver — `api.scryfall.com/cards/named?exact=`
  then `?fuzzy=`, 100 ms self-throttle, the importers' contactable
  User-Agent, `image_uris.normal` with a front-face fallback for
  double-faced cards; `ingest` owns the `refs/mtg` manifest and the
  `mtg-images` journal. Resolution order in kai is store catalog first,
  Scryfall for what it misses. The bulk `ingest-scryfall <set>` path is
  unchanged and still writes its own per-set ref.
- **Art fetching is now per visible card, and the deck-scoped fetch is
  just a prefetch batch through the same queue.** The core is
  `kai/src/art.rs`: an `ArtRequest { game, id, name }` keyed
  `game/id-or-name`, an `ArtQueue` with in-flight dedupe (a key that is
  pending, in flight, or settled is never enqueued twice), bounded retries
  (two attempts, then settled), and a `pump` that any fetcher can drive.
  The live fetcher resolves a request to an image URL — Riftcodex by id or
  name for riftbound, Scryfall by name for MTG, local store catalog
  consulted first in both — then rides the source-agnostic art core in
  `agni-importers::art` to fetch, content-address into the spirit store,
  journal it, and merge it into that game's manifest. Per-source throttles
  are 250 ms (riftbound) and 100 ms (MTG); the API lookups self-throttle
  on top. A single worker thread drains the queue and exits when it is
  empty. `queue_visible_art` scans the rendered table every frame for
  faces with a name and no art, which covers the case deck-scoped prefetch
  structurally cannot: an **opponent's reveal** arriving in the log with a
  name we have never resolved. Arrivals flow into `apply_store_art`, which
  patches both the seated deck's faces (by game id) and the table's faces
  (by name) and drops the placeholder tint. Failure is honest and quiet:
  after the retries the card keeps its named placeholder, no error banner
  fires, and the key is never asked for again in that session. The status
  line reads "fetching art… N left" only while the queue is non-empty.
  Native and android fetch directly; the browser stays gateway-honest and
  keeps the existing manifest+blob path — a per-card gateway art route is
  named as roadmap, not shipped.
- **The MTG acceptance test** (`net/tests/mtg_mvp.rs`) twins the riftbound
  one and runs unignored in `agni-nextest`: hardened engine + hardened MTG
  plugin at both seats, the plugin published to and fetched back from a
  real spirit store by its genesis pin, every wire crossing round-tripped,
  synthetic 60-card libraries plus a commander dealt from both seats (host
  directly, joiner via `DealDeck`), opening hands of seven, draws out of
  the hidden library with private faces, moves through
  hand→battlefield→graveyard→exile→library and command↔battlefield in both
  directions from both seats, a foreign-hand move refused, and at the end
  byte-identical logs and engine snapshots plus a byte-scan proving no
  never-revealed library name entered the shared log. Card names are
  invented; no real card data is checked in. The scan tracks names that
  were *ever* public rather than only currently revealed, because a card
  played to the battlefield and then returned to the library legitimately
  sheds its public face while its earlier `Reveal` stays in the log.
- **Deferred, named**: a gateway route for MTG deck resolution and
  per-card art on the web peer (the browser can join an MTG table and play
  by name, but cannot resolve or fetch art itself); MTG sideboards are
  staged by the importer but not dealt (there is no MTG sideboard zone
  yet); the bulk `ingest-scryfall` per-set refs and the on-the-fly
  `refs/mtg` manifest remain separate stores of the same kind of record;
  and art for free-form tables, which have no game and therefore no
  source to ask.

## Turns, showdowns and the first `view` output (W8)

The Riftbound plugin is no longer accept-all. It carries a turn structure, and
doing so forced two long-deferred surfaces into existence — both game-agnostic,
because the next plugin (MTG) needs the same ones.

- **The guest SDK** (`plugins/sdk`, `agni-plugin-sdk`) is the toolkit a guest
  links instead of serde: an integer-only CBOR reader/writer (the third-party
  hardening pipeline refuses float opcodes and float value types, and a serde
  data model drags `f64` visitors in), `decide::parse` for the engine's
  `{plugin_state, state, entry}` request (seat, seat count, action kind, the
  `Game` payload), `decide::Verdict` for the reply, `view::parse` for the view
  request plus `PluginView`/`Affordance` builders, `turns::{TurnOrder,
  PassWindow}` — turn order with `advance`, and a window that passes focus in
  turn order and closes once every seat has passed in sequence, which is the
  one mechanic MTG's priority passing and Riftbound's showdowns share — and the
  `export_plugin!` macro that owns the export set and the request/reply
  buffers. Both guests use the macro; MTG passes `decide::accept_all` and
  `view::nothing`.
- **Plugin state reaches the screen.** `TableView` gains `plugin_state`
  (skipped on the wire when empty, so goldens and old peers are untouched) and
  `ViewDelta::PluginState`. The turn machine's bytes therefore replicate like
  cards do and both seats fold the same turn.
- **`view` is called.** `PluginModule::view(request)` (default: an empty view)
  and `AbiPlugin::view` cross to the guest's `view` export with
  `abi::PluginViewRequest { plugin_state, state, seat }`, built by the session
  from its engine-truth `state_cache` — `HostSession::plugin_view(seat)` and
  `ClientSession::plugin_view(seat)`. The reply is `wire::PluginView { status:
  Vec<String>, affordances: Vec<Affordance { label, hotkey, enabled, data }> }`.
  A label may contain `{seat N}` or `{zone N}`; the client substitutes its own
  names (kai: the seat's colour, the battlefield's colour). An affordance's
  `data` is the `Game` payload the client sends back verbatim when it is
  pressed. kai renders any plugin's view as one strip (`table/plugin_ui.rs`)
  with no game knowledge — the same file will render MTG's phases.
- **The Riftbound turn machine** (`games/riftbound-turns`) follows the Core
  Rules v1.2 (`games/riftbound/rules`) and the shape Rift Atlas presents:
  a turn is a *beginning* step (awaken, hold scoring, channel, draw — the
  player's own business at the table) advanced with one action into the
  *main* step, and *end turn* hands the turn to the next seat. A showdown is
  opened by the turn player at a contested battlefield against a defender;
  the attacker gains focus; the focus holder may pass (focus walks the seats
  in turn order) or play — a card move by the focus holder resets the pass
  count and passes focus, per rule 343 — and the showdown closes when every
  seat has passed in sequence (344.3.a) or when the attacker ends it. The
  plugin refuses an out-of-turn advance, a pass without focus, an end-turn
  during a showdown, and a start when the game already started; everything
  else, including every card move, stays free-form. Scoring stays on the
  `points` counter for now; conquer and hold are the next rules to encode.
- **A fair dice roll, in the SDK.** `dice::Roll` is commit-and-reveal: each
  seat commits `dice::commitment(secret)` (an integer-only 64-bit mix over an
  8-byte secret), reveals only once every seat has committed, and the plugin
  refuses a reveal that does not match its commitment. Every replica then
  derives the same dice from the pooled secrets and the round number, so the
  host cannot steer the roll and no replica has to trust another's arithmetic.
  A tie re-rolls in a new round automatically. The client's half is generic:
  an affordance carries a `kind` — `Plain`, `Commit { roll }` or `Reveal {
  roll }` — and kai answers a `Commit` press by drawing a secret
  (`os/entropy.rs`: `/dev/urandom` natively, `Math.random` in the browser),
  sending `data ++ commitment` and keeping the secret in `RollSecrets`; a
  `Reveal` affordance is never drawn as a button, it is sent automatically the
  moment the plugin offers it. The Riftbound plugin starts every game in a
  lobby: one d6 per seat, the winner chooses who goes first (`StartGame` is
  refused from anyone else and before a decided roll). MTG's opening roll and
  coin flips are the same three affordances.
- **Verification**: `net/tests/riftbound_turns.rs` runs both seats on the
  hardened engine and hardened plugin through a whole round — the commit and
  reveal roll with a premature reveal and a mismatched reveal both refused,
  the winner's start, an out-of-turn refusal, beginning → main, a showdown opened, a pass refused
  without focus, two passes closing it, the turn handed over — asserting
  matching turn state, matching `plugin_view` affordances and byte-identical
  logs at both seats. The plugin's own tests decode the engine's real
  `DecideRequest`/`PluginViewRequest` bytes rather than hand-written ones.

## Effects, tokens and the first real rules (W9)

The fold now applies what a plugin asks for, not only what a player sent. A
`Verdict` carries `effects: Vec<Effect>` (`Move`, `Annotate`, `Counter`,
`Spawn`, `Despawn`) which `fold_finish` applies after the entry itself, on a
copy of the state that is committed only if every effect applies; one bad
effect refuses the whole entry as `FoldError::BadEffect`, so a plugin bug is a
deterministic refusal at every replica rather than a desync. Effects ride the
same engine invariants as actions: a move sheds a card's annotations and
card counters when it enters a hand, deck or discard zone (`wire::sheds_state`),
which is also why a recycled rune no longer arrives in the rune deck sideways;
a spawn mints a fresh id through `Table::add_face` and marks it revealed.

Three more surfaces landed with it:

- **Faces carry what is printed on the card.** `CardFace` gains optional
  `kind`, `energy`, `power` and `might` (serde-defaulted, skipped when absent,
  so every golden and every old log is byte-identical). The importers fill
  them from Riftcodex and the store catalogue, deck snapshots keep them, and
  kai seals them into the faces it deals. The decider — which only ever sees
  revealed faces — can therefore count a play's cost and tell a unit from
  terrain without a lookup it could not do inside wasm.
- **Tokens.** `LogAction::Spawn { face, to, seat }` is the client's way to put
  a token on the table (the token menu in kai, `spawn` in kai-cli); a plugin
  lists the tokens it knows in `PluginManifest.tokens` (`TokenDecl { name,
  kind, might, art }`) and can mint its own through `Effect::Spawn`.
- **The SDK reads the table.** `agni_plugin_sdk::table` parses the engine's
  `LogState` into a `Snapshot` (cards with zone, seat, owner, face stats and
  the `exhausted` mark; zones with kind and owner; counters), shared by
  `decide::parse` and `view::parse`; `decide::Action::Move` now names the card
  and destination; `decide::Effect` mirrors the engine's effects with helpers
  (`exhaust`, `ready`, `score`) and `Verdict::with_effects`. Still integer-only
  CBOR, still no dependencies.

On top of that the Riftbound turn machine (`games/riftbound-turns`, `rules.rs`)
encodes the first rules of the Core Rules v1.2 beyond turn order:

- **End beginning phase** does the start-of-turn choreography: awaken (ready
  everything the turn player owns), hold scoring (a point per contested
  battlefield the turn player holds — control is settled from who has units
  there, 184.4), channel two runes from the top of the rune deck (three on the
  last seat's first turn, 462.7), draw one.
- **Playing from hand** into the base or the chain pays the face's cost:
  energy exhausts ready runes in the pool, power recycles them to the bottom
  of the rune deck (160.2); too few ready runes refuses the move
  (`Refusal::NotEnoughRunes`). A unit is played to the base and enters
  exhausted; playing one straight onto a battlefield is refused. A march — a
  unit moving between the base and battlefields — needs a ready unit and
  exhausts it. Hidden plays carry no face and stay free. For the decider to
  see the cost at all, the host decides a hand play on a preview with the
  face revealed and then appends reveal before move (a card leaving a
  face-down deck still moves first, since a reveal inside a hidden zone is
  refused by the engine); a refusal therefore appends nothing and leaks
  nothing. After every entry the host reveals what effects surfaced into a
  public zone and sends each seat the faces it is owed in its own zones, so a
  channelled rune shows and an effect-drawn card is readable by its owner.
- **Contest and conquer.** A unit the turn player moves onto a battlefield they
  do not hold opens a showdown against the holder (or the next seat), and a
  showdown closing — all passed, or ended by the attacker — settles control:
  one side's units left means that side holds it, and a new holder scores a
  conquer, once per battlefield per turn (446, 447). `Resolve { zone }` is the
  affordance for settling a battlefield outside a showdown after combat has
  been resolved by hand. Ending the turn clears the once-per-turn marks.
- **The presenter** prints a points line for every seat, who holds which
  battlefield, and the winner at eight points; control lives in the plugin
  state (`Control` slots: zone, holder, scored mask), state version 3.

What is deliberately still by hand: combat damage, unit death, keyword
abilities, token minting by card text, and rune domains for power. The
`Resolve` affordance and the counters cover those at the table until each
gets its rule.

## The rules engine (W10)

W9 left combat, death, keywords, the chain's resolution and every card's text
to the players' hands. W10 is the rules engine: the Riftbound plugin
enforces and automates Core Rules v1.2 for the Lillia-versus-Irelia pool,
with the design written up in [rules-engine.md](rules-engine.md). The
shape, in one paragraph: the 45-byte turn state becomes a versioned CBOR
document written with the SDK writer (turn core, per-seat and per-card
facts, the chain, a trigger queue, delayed triggers, the one open prompt,
control slots, the lobby roll); every card is one file exporting a static
`Card` (keywords, abilities as `Trigger + TargetSpec + fn`, statics, an
optional replacement) found by face name in a registry, over a `Ctx`
projection that applies the entry and each effect to a local copy of the
`Snapshot` exactly as agni-sim will, so one entry can pay, resolve, trigger,
clean up and stage the next showdown and only stops where a player must
decide; free-form Moves are classified into plays, standard moves, hides and
plays-from-facedown or refused with a reason; every decision the rules leave
open is a prompt — one numbered affordance per option, `done`/`skip`/
`cancel` as allowed, options numbered by one function both `decide` and
`present` call so replicas agree — answered by `Pick { prompt, option }`
bytes or by a gesture (drag to the trash for a discard, to the deck bottom
for the mulligan); combat is automatic except "who takes lethal next".

The substrate changes are few and additive: a `reason` on the verdict the
host relays as a notice, an explicit `owner` on `Effect::Spawn` (tokens
minted inside the other seat's entry were mis-owned), a `card` on
affordances plus a prompt summary for the view, SDK `Action` arms for the
table edits the plugin must refuse, `Snapshot::apply` in the SDK
cross-tested against agni-sim, and later `Effect::Reveal`/`Effect::Peek` for
the two information cards. Enforced mode is chosen in the lobby beside the
free table, which stays byte-for-byte W9 and remains the panic button.

M0 landed the groundwork with two things a player notices. The retired
beginning-phase step means the first player's draw and channel now run
inside the `StartGame` entry against whatever is on the table at that
instant, so every deck must be dealt before the roll winner presses "go
first" (the lobby says so; a deck dealt later starts without its first
draw — later seats are unaffected, their beginning phase runs at the
previous seat's `EndTurn`). The lobby's mode switch is a toggle ("switch to
rules enforced" / "switch to free table", `SetMode`) beside the start
affordances, not a start button of its own. Two projection details the
parity test pins: the SDK's `Snapshot` reads the core `Board` zone as
`table::BOARD` (visibility all, aux kind, per seat — the engine's facts) and
`Hand` as `None`, and `Action::Reveal`/`Action::Spawn`/`Effect::Spawn` carry
a full `table::Face` (name, kind, energy, power, might, domain) so the AFTER
table inside those entries shows the same card the next request will. A
manifest may set `despawn_any` to let its plugin despawn dealt cards (the
engine copies it into `TableConfig` at genesis; default false, so
`Effect::Despawn` of a non-token stays a `BadEffect`). The engine ABI is
version 1 and the wire is version 4 since M0: a stale bundled engine or an
older kai is refused at load or at `Welcome` instead of faulting on its first
refusal.

## Hidden information: the decider sees what a human referee sees

The tension is real and worth stating baldly: the plugin computes rules, but
the fold deliberately contains no hidden faces —
deterministic-log.md's core commitment is a face-free, byte-identical log.
If the decider needed your hand's faces to validate your play, either faces
would enter the shared fold (leaking every hand to every peer's plugin) or
each peer would fold different inputs (destroying `state = fold(log)`).

The resolution mirrors the human game. **The authoritative decider is
blind to hidden faces, exactly like an opponent watching you play.** You
cannot play a card at a paper table without showing it; here, playing a
hand card is a sequenced `Reveal` immediately followed by the move —
precisely the pairing phase A already ships. At fold time the card is
public, the decider checks it, every peer folds identically. The decider's
inputs are: the plugin state blob, the public table (including
annotations and zone occupancy), hand and deck *counts*, and revealed
faces — nothing else exists in the fold for it to see.

For UX — legal-move highlighting, "which of these cards can I afford to
cast" — each client runs a second, **advisory** call into the same module:
`view(state, table, my_seat, my_faces)`. It receives only the faces its
own seat already sees on screen, and its output renders locally and never
enters the log. A malicious plugin therefore cannot exfiltrate a hand it
was never handed: the authoritative calls have no hidden inputs, the
advisory call has no outputs beyond pixels rendered to the player who
already holds those cards, and the module has not a single import to carry
bytes anywhere. The residual channel — encoding your own hand into *your
own* screen — leaks nothing to anyone else.

What this defers, named honestly: mechanics where hidden information has
rules weight *before* reveal — MTG morphs, face-down exile, hidden
placements — need the phase-D commitment machinery
(deterministic-log.md) so the log can prove a face-down card was what it
later claims. The decider API is already shaped for it (a commitment is
just more public bytes in the entry); the dogfood subsets simply exclude
those mechanics until D exists.

## One vocabulary: the mirrored view, its deltas, and plugin descriptors

Two directives meet here and turn out to be the same design. The
engine-as-wasm boundary needs a **delta vocabulary** (engine → shell, once
per action, applied to a render-side mirror). The plugin API needs a
**descriptor vocabulary** (plugin → client: zones, prompts, affordances) —
the place plugin APIs usually die, either handed a canvas (killing the
sandbox) or nothing (making every game look like the engine's one built-in
game). Unified: there is a single **TableView** document; the plugin's
manifest and `view` output populate it, the engine owns and revises it as
the fold advances, and the ABI carries *diffs of it* to the shell. The
renderer never sees fold state at all — it sees the view and nothing else.

The view document:

```toml
[[zones]]
id         = "rune-deck"
kind       = "deck"               hand | deck | discard | stack | battlefield | aux
owner      = 1                    or "shared"
visibility = "none"               all | owner | none
layout     = "pile"               fan | pile | row | grid | spread
label      = "Runes"

[[cards]]
card       = 17
zone       = "battlefield-2"
seat       = 1
index      = 0
face       = "<present only if visible to this seat>"
rotated    = true                 exhausted/tapped — one visual verb
badges     = [{ key = "damage", value = "2" }]

[[affordances]]
card       = 12
label      = "exhaust"
action     = "<CBOR template: a log action with a hole per required choice>"
hotkey     = "e"

[[prompts]]
kind       = "choose_cards"       choose_cards | choose_option | order_cards | confirm
seat       = 1
from       = [3, 9, 14]
min        = 1
max        = 1
why        = "choose a blocker"

[[stack]]
source     = 12
text       = "Deal 3 damage to any target"

[[narration]]
text       = "seat 1 exhausts Vanguard Sentinel"
```

The deltas are its diff, emitted per folded entry (and speculatively for
the local seat's optimistic overlay, same as moves today):

```
ViewDelta = ZoneUpsert(zone) | CardUpsert(card) | CardRemove(id)
          | PromptSet(seat, prompts) | AffordanceSet(seat, affordances)
          | StackSet(items) | Narrate(text)
```

Load-bearing details:

- **The view is per-seat.** Faces appear in a seat's view only when the
  zone's visibility grants them — the engine applies the zone table before
  anything crosses the ABI, so the mirror physically cannot contain what
  its seat may not see. This is today's `card_shown` masking, promoted
  from renderer courtesy to boundary invariant.
- **The affordance template is the input story.** The plugin hands the
  client a partially-filled log action with typed holes; kai's existing
  drag/tap machinery fills the holes and proposes the completed CBOR as an
  intent. kai never learns what "casting" or "exhausting" means — it
  completes forms. Hotkeys are the same templates bound to keys: the
  manifest suggests defaults (`e` = toggle exhaust on hovered card, `d` =
  draw), the client owns the actual binding and rebinding.
- **Prompts are how multi-step resolutions surface** (choose targets,
  order triggers, pay costs) without the plugin ever owning the event
  loop.
- **Zones answer architecture.md §3a** — the zone vocabulary stops being
  an engine enum and becomes plugin-declared data.
- Everything here is presentation; none of it enters the log. A plugin
  that renders garbage produces an ugly table, not a desynced one.

## "Hot" means install-time, not mid-game

Hot loading = a player downloads a plugin or card pack — or swaps between
installed games — **without restarting the app**: the module blob arrives
over the mesh (or a file picker), gets hashed, registered in the local
spirit store, and shows up in the table-create picker immediately. Module
*instantiation* happens at table create/join — the genesis pins the
hashes, the joiner instantiates what genesis names.

What it deliberately does not mean is swapping rules **mid-fold**. The
genesis hashes are part of every replica's fold; changing a module under a
live log is a determinism violation by definition — the same log would fold
differently before and after. A mid-game upgrade, if ever wanted, has
exactly one sound shape: a sequenced entry (`Migrate { new_plugin, at_seq }`)
every replica applies at the same point, with the old module folding
entries before it and the new one after. Designed slot, not built —
finish the game, start a new table with the new version.

## Distribution and trust

A plugin is a content-addressed blob plus records, riding machinery that
already exists end to end:

- **Identity**: a plugin CIR (`kind = "game-plugin"`, name, owner DGID for
  third-party plugins — the owned-CI mechanism from identity.md), with the
  module bytes linked by a content attestation
  `(plugin ci, td) → blob:<module>`. Versions are new attestations against
  the stable identity, newest-trusted-wins, same as errata. The engine
  module gets the same treatment (`kind = "engine"`).
- **Transport**: module blobs and card-pack records replicate under refs —
  gossip advert, pull, hash-verify. Genesis names the hashes, so what you
  run is what was pinned, regardless of who served the bytes. What enters
  the store is always the **hardened** module — the publish pipeline
  (gas injection, stack checks, memory maximum, float/SIMD validation)
  runs before minting, so the identity everyone pins already contains its
  own meter.
- **Bundled but never compiled in**: the Riftbound plugin ships *with* the
  client for now — as a blob asset the shell seeds into the local spirit
  store on first run, then loads back out by hash like any downloaded
  plugin. The client contains a copy of a plugin; it does not contain a
  plugin. The code path exercised on day one is the same one a
  third-party download uses later, and replacing the bundled blob with a
  newer mesh-served version is an install, not an update.
- **Signing**: attestation proofs ride identity phase 2's keys (the
  persisted node key as degenerate DGID, groups later). Verification
  precedes instantiation; unsigned or unknown-signer plugins surface as
  exactly that at install time.
- **Install UX**: show the plugin name, author identity, content hash and
  capability story ("this plugin can: compute game rules and describe the
  table. It cannot: access the network, your files, or your hidden
  cards"). The honest pitch is that the sandbox makes the *safe* claim
  true even when the *trusted* claim is not.

The engine posture is unchanged and worth restating: agni ships zero
rules. Bundling the Riftbound *zone* plugin as a runtime artifact keeps
the letter and the spirit — the neutral engine still enforces nothing, and
what the plugin defines (zone names and layouts, free movement, a tap
toggle) is table furniture, not game rules.

## The MVP: a Riftbound table, no rules, full furniture

Approved scope, restated: two players join a table, import Riftbound
decks (text, deck code, or link), and physically move cards between all
Riftbound zones — hand, main deck, rune deck, legend zone, chosen champion
zone, battlefields, trash — with tap/exhaust on cards in zones, hotkeys,
no rules enforcement. Served by a plugin that ships with the client as a
runtime, hot-reloadable, spirit-store-served artifact.

The MVP plugin is a **zone/layout plugin**: a manifest and nothing else.

- **Zone table** (from `agni-riftbound`'s pinned anatomy): per-seat
  `hand` (fan, owner-visible), `main-deck` (pile, visibility none),
  `rune-deck` (pile, visibility none), `legend` (row, all),
  `champion` (row, all), `trash` (pile, all), `sideboard` (grid, owner);
  shared `battlefield-1..3` (row, all). Exact kinds and layouts are the
  plugin author's call at execution time; the vocabulary above must
  express them all, and that is the point — the MVP is the first real
  test of the descriptor layer.
- **decide** answers accept-all (the free-form table posture, now
  formally a plugin's answer). Rules arrive later as decider logic in the
  same plugin, same module identity, new version — no re-architecture,
  because the decider slot was in the fold path from day one.
- **Tap/exhaust** is the new `Annotate` log action (`key = "exhausted"`),
  replicated through the fold like any move; the view renders it as
  `rotated`. Wire change noted in the log-growth section.
- **Hotkeys** ride manifest affordance templates: exhaust/ready toggle,
  draw from main deck, draw rune, shuffle, to-trash. Client-side binding.
- **Deck import** seats a deck as log entries: cards enter their starting
  zones (decks face-down — ids and counts only, faces stay in the owner's
  store per the visibility rules), legend and champion revealed.

## Deck import: text, deck codes, and links

Three intake forms, one output: an `agni-riftbound` `RiftboundDeck`
resolved against the user's imported Riftcodex catalog to card CIRs.

- **Text lists** — the plain card-name format every builder exports;
  parser lives beside the deck shapes.
- **Deck codes** — Piltover Archive's code format (open, Apache-2.0
  spec; prior research pinned it as the ecosystem's interchange format,
  and RiftMana's builder emits PA-compatible codes alongside its other
  exports). One codec covers multiple sites.
- **Links** — a resolver registry mapping URL patterns
  (piltoverarchive.com, riftmana.com, riftdecks and friends) to a fetch
  plus extraction that lands in one of the two forms above. Per-site
  page/API shapes get verified at execution time; the registry design
  assumes they churn.

W4 landed this in `agni-importers::riftbound` (parsers pure and
exhaustively round-tripped, PA codec v1–v5 against the published
Apache-2.0 spec) with per-site shapes verified live 2026-09-01:
piltoverarchive deck pages (`/decks/view/<uuid>`) embed their PA code in a
`/deckbuilder?code=` link — extracted and reused through the code parser;
riftdecks deck pages (`/riftbound-metagame/deck-<slug>-<id>`)
server-render `card-list-item` rows carrying `data-quantity` and
riftbound ids in image paths; the 403 the resolver met on 2026-09-01 was
Cloudflare's spoof check, not a bot wall — a browser User-Agent or a
browser Accept list arriving over HTTP/1.1 with a non-browser TLS
fingerprint is refused, while the honest `agni-importers` User-Agent with
`Accept: */*` is served (verified through ureq on 2026-09-11; only the
literal `curl/` UA is blocked outright) — so the fetch drops the browser
headers under the M9 pipeline (rules-engine.md, "Importer fixes") and the
resolver's structured error stays as the paste fallback; riftmana deck
views are
client-rendered behind nonce-gated admin-ajax with no stable
server-rendered deck shape, so its resolver scans fetched pages for an
embedded PA code and otherwise reports the same paste fallback. Zone
reconstruction resolves card TYPE against the user's imported Riftcodex
catalog (ref `riftbound`, `ingest-riftbound`), falling back to the live
Riftcodex API when no catalog is ingested; the gateway `/resolve/deck`
endpoint is generic spirit plumbing with the Riftbound resolver injected
per spirit's `wiki/design/gateway.md`.

Platform note, weighed: desktop and Android fetch links freely; the
**browser peer is CORS-bound** and most deck sites will not send
permissive headers. Rather than proxying through third parties, the
existing read-only HTTP gateway on the dev nodes can grow a
`/resolve/deck?url=` endpoint — server-side fetch against an allowlist of
known deck sites, returning the extracted text list. It stays read-only,
tailnet-scoped like the art bridge, and the browser falls back to
paste-the-text when no gateway is reachable. Import remains user-initiated
in every form, same as every importer in the posture: the project ships
resolver code, never deck content.

**Card art follows the same posture, in-client.** The primary UX is
kai's own importer flow, not a CLI: seating a deck auto-fetches the art
its cards are missing (deck-scoped
`agni_importers::riftbound::ingest_deck_art` — Riftcodex-resolved image
URLs, politely throttled, content-addressed into the user's own spirit
store and merged into the `riftbound` manifest ref), with faces
refreshing live as images land — cards play as name placeholders the
instant a deck seats and turn into art behind the game. The import panel
also offers the full-set download (~1,500 images, the same journal-backed
resumable ingest, progress named honestly throughout). The
`ingest-riftbound`/`ingest-scryfall` bins remain the bulk/headless path
for servers and pre-seeding. Desktop and Android fetch directly; the
CORS-bound browser peer keeps its existing posture — art arrives through
the gateways' blob bridge as they hold the set. User-initiated, the
user's own store, nothing distributed: shipped importer code pointing at
a public service, exactly like the deck resolvers above.

## Workstream breakdown for execution

Sized for independent execution agents; interfaces named so streams can
proceed against stubs. W1, W2, W4 and W5 are pure or boundary-independent
and can start immediately in parallel; W0 is the critical path; W3 needs
W0's delta schema fixed; W6 integrates. If W0 runs long, the acknowledged
stepping stone is **plugins-only-wasm** — the native engine hosting just
the plugin module through the same guest-side interface — which changes no
plugin substrate and defers only the genesis-pinned *engine* hash; the
target remains engine-in-wasm, because a table opened under engine hash X
must replay under X forever.

**W0 — engine.wasm boundary and the mirrored view** (agni + kai shells;
the critical path). Compile core+sim+net-session into `engine.wasm`;
wasmi hosting in the native shells, instantiating engine + plugin as
siblings and bridging the engine's plugin imports (wasmtime stays a
documented drop-in alternative); the JS sibling glue for the web shell
(browser-native instantiation, CBOR copied across memories); the
TableView diff engine and `ViewDelta` ABI; kai's render path moved onto
the mirror (drain-deltas replaces drain-net-into-agni-types); net pump
stays shell-side feeding frames in. Native build of the same crates
stays primary for tests. *Interface out*: the engine ABI (inputs:
frames, intents, local commands; outputs: deltas, outbound messages) and
the `ViewDelta` schema. *Verification bar*: determinism suite passes
natively AND driven through the wasm boundary with identical state
hashes; the spike's assertions (zero imports, identical replies,
deterministic trap) repeat under wasmi; desktop kai renders a two-seat
table entirely from the mirror; browser shell instantiates the same
engine bytes. **Estimate: 7–10 days.**

**W1 — engine log generalization** (agni-sim, agni-core; pure, native
tests, no wasm). `Zone::Plugin`/`WireZone::Plugin(u16)` against a
genesis-pinned zone table; per-zone visibility enforced in
validate/fold/deal/reveal paths; `LogAction::Annotate` with ordered-map
storage; `LogAction::Game { data }` and the plugin-state blob slot in
`LogState`; fold-equivalence and byte-stability tests extended.
*Interface out*: the new `LogAction`/`LogState` shapes and the zone-table
CBOR schema. *Verification bar*: existing determinism suite green plus
new cases — visibility per zone kind (a `none` zone's face never enters
any log or view encoding), annotate replicates byte-identically at three
replicas. **Estimate: 3–5 days.**

**W2 — the hardening/publish pipeline** (pure; a small tool crate).
Module in → deterministic gas injection + stack-height checks + declared
memory maximum (finite-wasm / wasm-instrument style) → float/SIMD opcode
validation for third-party modules → blake3 → spirit store blob + CIR +
content attestation. This is the minting path for every module identity;
W5 serves what it mints. *Interface out*: `harden(module: &[u8]) ->
HardenedModule { bytes, hash, report }`. *Verification bar*: hardening is
byte-deterministic (same input, same output, same hash, twice); a looping
module traps at the same gas count under wasmi, wasmtime and a browser
engine; a float-bearing module is refused with a named opcode.
**Estimate: 3–5 days.**

**W3 — descriptor renderer in kai.** Generic zone spawn/layout from the
zone table (retiring hardcoded hand/board), the five layout kinds,
rotation and badges, affordance-template completion through the existing
drag/intent path, hotkey binding with manifest defaults, prompt surfaces
(MVP needs none rendered, but the vocabulary lands). *Interface in*: W0's
ViewDelta schema (can develop against a fake mirror feed before W0
lands). *Verification bar*: a synthetic manifest with N zones of every
kind/visibility/layout renders correctly per seat; tap toggles via click
and hotkey against the fake feed. **Estimate: 5–8 days.**

**W4 — deck import parsers and resolvers** (pure; agni-importers +
agni-riftbound; independent of every other stream). Text-list parser; PA
deck-code codec against the published spec; link-resolver registry with
piltoverarchive/riftmana/riftdecks resolvers; catalog resolution to CIRs;
the gateway `/resolve/deck` endpoint behind its allowlist. *Interface
out*: `parse_deck(input: DeckSource) -> RiftboundDeck` plus resolution
errors worth showing users. *Verification bar*: golden-file round-trips
for codes and text against real exported decks from each site; a fetched
link and its exported code parse to the same deck. **Estimate: 4–6 days**
(assumes the Riftcodex importer from the existing roadmap; add 2–3 days
here if it has not landed).

**W5 — plugin store serving and hot reload** (spirit store + shell
glue). Refs for plugin distribution; first-run seeding of the bundled
(hardened) Riftbound plugin blob into the local store; install/swap
without restart; genesis pinning of both metered hashes and joiner
auto-fetch of missing modules. *Interface in*: W2's minted blobs, W0's
host loading path. *Verification bar*: delete the store, first run
reseeds and loads by hash; a second build with a modified plugin blob
hot-swaps for the next table without restart; a joiner lacking the blob
pulls it from the host's mesh and folds identically. **Estimate: 3–5
days.**

**W6 — the Riftbound MVP plugin and integration.** The manifest (zone
table above, hotkey templates), accept-all decide, deck-import-to-log
seating (respecting visibility: deck entries carry ids/counts, faces stay
home), two-player end-to-end. *Interface in*: everything above.
*Verification bar — the MVP acceptance test*: two players on two machines
join one table, each imports a deck by at least text and deck code (link
on native), every card can be moved between hand, main deck, rune deck,
legend, champion, battlefields and trash, exhaust/ready toggles by click
and hotkey, and both screens agree after every action; the plugin arrived
via the store path, not a compile-time link. **Estimate: 4–6 days.**

Roughly 29–45 agent-days; with W1/W2/W4/W5 parallel to W0 and W3 stacked
behind it, two to three weeks of wall-clock with a handful of agents.

## Beyond the MVP

- **Rules for Riftbound, in the same plugin**: turn structure, rune-based
  resources, contested battlefields scoring to the points win — decider
  logic replacing accept-all, new version of the same plugin identity.
  Rules IP caution, stated so it is not re-litigated per card: mechanics
  are not copyrightable but rules *text* is — implement paraphrased
  mechanics, never embed reproduced rulebook or card text; card text
  arrives, like art, only through the user's own importer run.
  **Estimate: 5–7 days.**
- **The MTG subset plugin** — the API's graduation exam. Vanilla and
  keyword creatures (flying, first strike, trample, haste, vigilance),
  lands and mana, a basic stack with instants and targeting, combat,
  lethal-damage and zero-toughness state-based actions, simplified
  priority. Chosen to stress prompts, LIFO presentation, affordance
  holes, fizzle-on-illegal-target, and plugin-global checks. The named
  API-breaking tests to attempt after it works: the layers system,
  replacement effects, full priority. Failing them cheaply is the point
  of dogfooding; rules-complete MTG is a non-goal. **Estimate: 5–8
  days.**
- **Commitments (phase D)** for hidden-with-rules-weight mechanics
  (morphs, face-down play) — the decider API needs no change when the log
  grows them.
- **Importer plugins as a capability class.** Today's in-client importers
  are first-party shipped code; the plugin sandbox stays zero-import. A
  future third-party "importer plugin" class would be network-capable by
  explicit user grant — capabilities declared in the manifest, granted at
  install, mediated by the host — unlike deciders, which never get one.
  The capability system is deliberately not built until a third-party
  importer earns it.

## Open questions

- **Schema evolution of the ABI.** `abi_version` gates hard breaks; the
  CBOR schemas want a compatibility rule (unknown-field tolerance)
  written down before third parties build against them.
- **Effect-DSL shape.** Layer 3's DSL is deliberately unspecified until
  the Riftbound and MTG rules plugins reveal what they actually share;
  premature generalization here is how effect languages bloat.
- **View-diff cost.** Per-entry diffing of the TableView is cheap at
  card-game rates; measure on the browser shell in W0 before optimizing.
- **wasmi 2.0.** The host pins 1.0.x; the 2.0 upgrade is a shell-side
  swap that must not change any metered module's behaviour — the
  hardening pipeline's cross-runtime trap test is the regression gate.
- **Engine-module hardening.** Plugins pass the full pipeline; whether
  the first-party engine module also takes gas injection (uniformity)
  or only the hash pin (it is trusted code on the critical path) is
  W2's call — either way the genesis pins whatever bytes actually run.
- **Per-site link formats.** riftdecks and riftmana page/API shapes are
  verified in W4, not assumed here; the resolver registry treats them as
  churn-prone by design.

# The Riftbound Rules Engine (W10)

This is the design for turning `games/riftbound-turns` from a turn
bookkeeper into a rules engine: enough of Core Rules v1.2 to play the Lillia
deck against the Irelia deck (`games/riftbound/rules/pool/lillia-house.md` and
`irelia-house.md`, 52 faces) with every rule enforced and every automatic step
automated. It
sits on the plugin substrate described in [plugins.md](plugins.md) and
changes none of its three decisions: the plugin is still a pure decider inside
the fold, still blind to hidden faces, still a hardened zero-import wasm
module on the dependency-free, float-free SDK.

The rules text and the card pool are not reproduced here or in code: scripts
carry paraphrased mechanics keyed by face name, and card text reaches the
client only through the user's importer run.

## Goals and non-goals

Goals:

- Legal plays and costs, movement and exhaustion, the chain with reactions
  and counterspells, showdowns and combat with automatic damage, kills and
  control, triggered abilities and keywords, temporary effects with
  durations, scoring with the final-point rule, and every player choice the
  rules leave open (targets, optional triggers, trigger order, combat damage
  assignment, the mulligan).
- One file per card behind a small static `Card` description and a registry,
  with shared keyword and effect primitives so most scripts are a few lines.
- The plugin state is a versioned CBOR document written with the SDK writer.
- Player choices flow through the numbered affordances the view already
  offers; the client keeps sending free-form Moves and the plugin decides
  which of them are actions.
- Every refusal carries a reason the player, kai-cli and the AI seat can read.
- Everything ships behind an enforced/free mode chosen in the lobby, so the
  free-form table of W9 never regresses and stays the panic button.
- Determinism as before: the decider is a pure function of
  `(plugin_state, LogState, entry)`; every replica folds the same effects.

Non-goals for this workstream:

- Cards outside the pool. The API is meant to grow to them, but no keyword or
  static is built without a card in the pool that needs it. Assault, Shield,
  Tank, Repeat, Quick-Draw, Weaponmaster and Vision exist as enum arms so the
  combat and cost code has the right shape; none is exercised.
- Multiplayer (3–4 seats, 2v2). The state carries seat counts and turn order
  for it, the third-player combat exclusions (440) are checked in legality,
  nothing beyond that is tested.
- Layer 1/2 effects ("Might becomes X", granted passives) beyond runtime
  keyword grants. Only arithmetic Might mods are needed by the pool.
- A banked rune pool: costs are paid at the moment they arise (416.3 lets a
  Reaction Add fire while paying), which is observationally identical because
  pools empty at the end of the Draw Phase and the turn.
- Hidden information with rules weight before reveal (phase D commitments).
  A hide is charged blind; the facedown card's Reaction status is public
  through the blob where 737.6 says it is private. Accepted deviation.
- A secret shuffle. Burn Out and the mulligan recycle use a fresh in-game
  commit-reveal roll; the resulting order is computable by both seats once
  revealed. The honest fix (a host re-deal of the recycled trash as fresh
  hidden ids) is listed under open questions.
- XP and Level. `xp` is counted (Scuttle Crab) and nothing reads it.
  Empowered is a toggle nothing in the pool sets.

## The player's contract

Everything below is in service of a contract the player can state in two
sentences: *drag a card or press a numbered button; if the table refuses it
tells you why.* Concretely:

- A drag from hand to the chain, the base or a battlefield is a play. A drag
  between base and battlefields is a standard move. A MoveHidden onto a
  battlefield is a hide. A drag of a facedown card onto the chain plays it
  from hidden. Every other Move is refused with a reason in enforced mode.
- Every decision the rules leave to a player arrives as a *prompt*: a status
  line and one affordance per legal option, `done`, `skip` and `cancel` when
  the rules allow them. Affordances name cards, and kai highlights those
  cards so clicking one is the same as pressing its button. A stale press
  (an old prompt id) is refused, never misapplied.
- The chain is played with the same two verbs: the priority holder drags a
  Reaction card to the chain or presses `pass`. Showdowns use focus the same
  way. Combat damage is automatic wherever there is no choice and asks "who
  takes lethal next" where there is one.
- Everything automatic (phases, costs, draws, kills, cleanup, staging the next
  showdown) runs inside the entry that caused it; the plugin only stops when
  a player must decide or hold priority.
- The presenter emits, for the viewing seat, exactly the affordances the
  decider would accept from that seat. The strip is never a lie, and the AI
  seat's `think` is keyed on the same strip.

## State: the CBOR document

The 45-byte `TurnState` (version 3) and the lobby blob (version 2) are
retired. The plugin state becomes one CBOR map written with
`agni_plugin_sdk::cbor::Writer` and read with `Reader`; unknown keys are
`skip()`ped so the layout can grow without a version bump, and a bump is
reserved for reinterpretation. The first byte stays the discriminator:
`GameState::decode` accepts a map whose first key is `"v"` with value 4 and
returns `None` for anything else, which — as today — means a fresh lobby.
`TableView.plugin_state` carries and diffs the blob per entry, so the encoder
omits every default field and every empty vector. Target: 200–600 bytes in
quiet play, low kilobytes with a full chain and an open prompt.

```rust
pub struct GameBlob {
    pub mode: Mode,
    pub lobby: Option<Roll>,
    pub roll: Option<InGameRoll>,
    pub turn: Option<TurnCore>,
    pub seats: Vec<SeatState>,
    pub cards: Vec<CardState>,
    pub control: Vec<Control>,
    pub chain: Vec<ChainItem>,
    pub queue: Vec<Pending>,
    pub delayed: Vec<Delayed>,
    pub prompt: Option<Prompt>,
    pub priority: Option<Priority>,
    pub showdown: Option<Showdown>,
    pub staged: Vec<Staged>,
    pub log: Vec<String>,
    pub next_prompt: u16,
    pub next_item: u16,
}
```

| Key | Field | Meaning |
|---|---|---|
| `v` | — | 4 |
| `m` | `mode` | 0 Free, 1 Enforced; chosen by the roll winner at start, immutable afterwards except through the panic button (below) |
| `l` | `lobby` | `dice::Roll::encode` bytes while the first-player roll is open |
| `r` | `roll` | an in-game commit-reveal roll `{id: u32, roll: bytes, why: u8}` (Burn Out, mulligan recycle); at most one open |
| `t` | `turn` | `TurnCore { players, first, turn: u16, player, phase }`; `phase` is one of Setup, Awaken, Beginning, Channel, Draw, Action, Ending, Cleanup, Expiration — only Setup and Action persist between entries, the rest exist so a prompt raised mid-choreography resumes at the right step |
| `s` | `seats` | one `SeatState` per seat, by index |
| `c` | `cards` | sparse `CardState` rows for cards with non-default runtime facts, sorted by id; a row is dropped when its card leaves the board (109) |
| `k` | `control` | one `Control { zone, holder: Option<u8>, scored: u8, contested: Option<u8> }` per battlefield in play; `contested` names the contester while 184.3 applies |
| `ch` | `chain` | bottom to top |
| `q` | `queue` | triggers and reflexives waiting to be ordered, chosen for, costed or finalized, in append order |
| `d` | `delayed` | "at the end of this turn" / "at the next Beginning Phase of seat N" items |
| `p` | `prompt` | the single open question; nothing but its answer (or a gesture that answers it) is accepted while it is `Some` |
| `pr` | `priority` | `Priority { active: u8, passes: u8 }`, present iff the state is Closed |
| `sd` | `showdown` | the open showdown, see the combat section |
| `st` | `staged` | showdowns and combats the last cleanup staged, waiting for the turn player's pick |
| `lg` | `log` | the last four narration lines, rendered as status by every replica |
| `np`, `ni` | counters | prompt and chain item ids |

```rust
pub struct SeatState {
    pub setup: SetupStage,
    pub draws: u8,
    pub played_main: bool,
    pub no_spells: bool,
    pub looks_facedown_of: u8,
}

pub struct CardState {
    pub id: u32,
    pub flags: u16,
    pub might: Vec<MightMod>,
    pub granted: Vec<(Keyword, Expiry)>,
    pub attached_to: Option<u32>,
    pub hidden_at: Option<u16>,
    pub hidden_since: u16,
    pub entered: u16,
}

pub struct MightMod { pub delta: i16, pub until: Expiry, pub src: u16 }
pub enum Expiry { Permanent, EndOfTurn(u16), CombatEnd, WhileAttached(u32) }
```

`SetupStage` is Waiting, Drawn or Done (the mulligan, 116–117). `draws` is
the per-turn draw count Frigid Jewel reads; `played_main` is the Legion flag;
`no_spells` is Lilting Lullaby's lockout; `looks_facedown_of` is the bitmask
of seats whose facedown cards this seat may look at this turn (Scuttle Crab).
All four reset at the Expiration Step.

`CardState.flags` bits: STUNNED, NO_MOVE_BY_OWNER (Vex), ATTACKER, DEFENDER,
ENTERED_THIS_TURN, NOT_PLAYED (countered), ONCE_USED (Pyke's per-turn
limiter), FROM_FACEDOWN (Back Off's provenance, Edge of Night). Current Might
is `printed + buffed + Σ might mods`, floored at zero at read time, plus
Assault/Shield while designated.

```rust
pub struct ChainItem {
    pub id: u16,
    pub kind: ItemKind,
    pub controller: u8,
    pub status: ItemStatus,
    pub origin: Origin,
    pub targets: Vec<TargetRef>,
    pub picks: Vec<u8>,
    pub stage: u8,
    pub noted: Option<Noted>,
}
pub enum ItemKind { Spell { card: u32 }, Ability { source: u32, index: u8 }, Trigger { source: u32, index: u8 } }
pub enum ItemStatus { Pending, Finalized, Resolving }
pub enum Origin { Hand, Champion, Facedown { zone: u16 }, Board }
pub enum TargetRef { Card(u32), Seat(u8), Zone(u16), Item(u16) }
pub struct Noted { pub zone: u16, pub might: i32, pub controller: u8 }

pub struct Pending { pub item: ChainItem, pub needs: Needs }
pub enum Needs { Order, Choices, OptionalCost, Legality }

pub struct Delayed { pub when: When, pub source: u32, pub seat: u8, pub ability: u8, pub args: Vec<u32> }
pub enum When { EndOfTurn(u16), BeginningOf(u8) }

pub struct Priority { pub active: u8, pub passes: u8 }
pub struct Showdown {
    pub zone: u16,
    pub attacker: u8,
    pub defender: u8,
    pub window: PassWindow,
    pub combat: bool,
    pub initial_chain: bool,
    pub stage: ShowdownStage,
}
pub enum ShowdownStage { Open, Damage { assigner: u8, remaining: u8, assigned: Vec<(u32, u8)> }, Resolution }
pub struct Staged { pub zone: u16, pub combat: bool, pub contester: u8 }

pub struct Prompt {
    pub id: u16,
    pub seat: u8,
    pub kind: PromptKind,
    pub why: PromptWhy,
    pub min: u8,
    pub max: u8,
    pub picked: Vec<u32>,
    pub cancel: bool,
}
pub enum PromptKind { Cards, Options, Order, Confirm, Assign, Roll }
pub enum PromptWhy {
    Mulligan,
    Target { item: u16, spec: u8 },
    PlayLocation { item: u16 },
    OptionalCost { item: u16, cost: u8 },
    OrderTriggers { seat: u8 },
    PickStaged,
    GroupMove { unit: u32, to: u16 },
    Assign,
    Resume { item: u16, stage: u8 },
    Replacement { dying: u32 },
    PayWith { item: u16 },
    Discard { item: u16, stage: u8 },
    Shuffle { why: u8 },
}
```

Units, Gear and Add abilities never become chain items (333.1.c, 356.2): a
permanent leaves the chain at finalize and Add abilities resolve while a cost
is paid. `Noted` is the Deathknell snapshot (734.1.d.3) taken before the card
leaves the board. `picks` records optional-cost answers (Accelerate paid,
"you may pay [1]" accepted). `stage` is the resume point of a script that
asked a question mid-resolution.

### Where a fact lives

The blob is the source of truth for every rules fact. The engine's card
counters and annotations are *display mirrors* the plugin maintains with
effects so kai shows them without game knowledge:

| Mirror | Source in the blob | Rendered as |
|---|---|---|
| counter 2 `might` | Σ active `MightMod` deltas plus buff (a delta is issued when a mod is added or expires) | the badge next to printed Might |
| counter 3 `damage` | marked damage | the damage badge |
| counter 4 `temporary` | set by `Ctx::spawn` for Sprites and, in enforced mode only, by `play::finalize` for any permanent whose script carries `Keyword::Temporary` (Sprite Fountain) | the Temporary glyph |
| counter 5 `buffed`, 6 `empowered` | Adaptatron's buff; nothing sets empowered | toggles |
| annotation `exhausted` | exhaustion (already the rotated verb) | rotation |
| annotations `stunned`, `attacker`, `defender` | the matching `flags` bits | badge glyphs |
| annotation `hidden` = zone id bytes | `hidden_at` | the card laid face down at that battlefield |
| annotation `attached` = unit id bytes | `attached_to` | a small link glyph |

The engine sheds card counters and annotations when a card enters a hand, a
deck or a discard zone (`wire::sheds_state`), which is rule 109 for free; the
plugin drops the `CardState` row at the same moment. Printed stats are read
from the face in the `Snapshot` (name, kind, energy, power, might, domain);
off-board Might for Mighty checks is the printed value. Control lives in
`control` as today; `scored` clears at Expiration. Rune pools are not
modelled (see non-goals).

### The projection

The decide request carries the table *before* the entry, and effects are
applied by the engine after it, in order, against that table. A rules engine
that pays a cost, resolves a spell, kills two units, collects Deathknells,
runs a cleanup and stages a combat inside one entry must see each of its own
consequences before computing the next. `Ctx` is that projection:

```rust
pub struct Ctx<'a> {
    pub table: Snapshot,
    pub blob: &'a mut GameBlob,
    pub scripts: &'a Resolved,
    pub effects: Vec<Effect>,
    pub events: Vec<Event>,
    pub seat: u8,
    pub actor: u8,
    pub spawned: u32,
}
```

`Ctx::new` clones the `Snapshot`, then **applies the incoming entry itself**
(`Snapshot::apply_entry`: the Move, the Reveal, the Spawn) so every rule
runs against the AFTER table — contest marking, staging, index arithmetic,
`units_at` all see the card where the player put it. Every primitive then
appends an engine `Effect` *and* applies it locally through
`Snapshot::apply(effect, actor)`, a line-for-line mirror of
`agni_sim::log::apply_effect`: ordinal indexes within (effective seat, zone)
with BOTTOM = 0 and TOP = `u32::MAX`, seat normalised to 0 in shared zones,
shed on Hand/Deck/Discard entry, token despawn on shed, face blanked on
entering a visibility-None zone, Spawn taking `next_id + spawned`, counters
clamped by the manifest bounds. `Snapshot::apply` lives in the SDK and is
cross-tested against agni-sim by folding random effect sequences both ways
(the parity test is the guard against the one desync class this design
introduces: a divergence makes a later effect in the same verdict a
`BadEffect`, which refuses the whole entry deterministically — no desync,
but a stuck game).

`Resolved` interns the script of every card in the request once
(`scripts.of(card_index) -> Option<&'static Card>`), so trigger collection
and target filtering never re-scan names. `units_at` excludes blank-faced
(hidden) cards, non-Unit kinds and the card of a not-yet-finalized play.
The contested battlefield set is derived from Battlefield-kind cards sitting
in shared zones, not from "the first `players` shared zones".

## Card scripts

Layout under `games/riftbound-turns/src/`: `engine/` (projection, legal,
cost, play, chain, priority, showdown, combat, cleanup, triggers, phases,
prompts, expiry, roll), `cards/mod.rs` (the registry) and one
`cards/<snake_name>.rs` per face — `cards/defy.rs`,
`cards/lillia_bashful_bloom.rs`, `cards/rockfall_path.rs`,
`cards/basic_rune.rs` (matched by the ` Rune` suffix), `cards/sprite.rs`,
`cards/gold.rs`. Each file exports `pub static CARD: Card`.

```rust
pub struct Card {
    pub name: &'static str,
    pub keywords: &'static [Keyword],
    pub abilities: &'static [Ability],
    pub statics: &'static [Static],
    pub replacement: Option<Replacement>,
}

pub enum Keyword {
    Accelerate, Action, Reaction, Assault(u8), Shield(u8), Tank, Backline,
    Deflect(u8), Ganking, Hidden, Legion, Temporary, Vision, Deathknell,
    Equip(Cost), QuickDraw, Repeat(Cost), Weaponmaster,
}

pub struct Cost { pub energy: u8, pub power: &'static [Power] }
pub enum Power { Domain(Domain), Rainbow, Own }

pub struct Ability {
    pub trigger: Trigger,
    pub optional: bool,
    pub cost: Option<Cost>,
    pub extra: Option<fn(&Ctx, Source) -> Cost>,
    pub condition: Option<fn(&Ctx, &Event, Source) -> bool>,
    pub targets: &'static [TargetSpec],
    pub run: fn(&mut Ctx, &Item, Stage) -> Flow,
}

pub enum Trigger {
    Play,
    PlayFromFacedown,
    Activated(Timing),
    Move { of: Who, to: Where },
    Conquer(Who),
    Hold(Who),
    Death,
    EnemyUnitDies,
    OpponentPlaysUnit,
    YouPlaySpell,
    AnyonePlaysSpell,
    Draw { nth: u8 },
    Chosen,
    Readied,
    ChosenFriendly,
    BeginningPhase,
    EndOfTurn,
    Reflexive,
}
pub enum Timing { Sorcery, Action, Reaction }
pub enum Who { Me, Friendly, You }
pub enum Where { Any, Battlefield, FromLocation }

pub struct TargetSpec { pub filter: Filter, pub min: u8, pub max: u8, pub kind: TargetKind, pub label: &'static str }
pub enum TargetKind { Card, Seat, Zone, Item }
pub enum Filter {
    Any, Unit, Gear, Rune, Legend, Spell, Ability, ItemOnChain,
    Friendly, Enemy, AtBattlefield, InBase, Here, HiddenBattlefield,
    SameLocationAs(u8), DifferentLocationFrom(u8), NotSame(u8),
    Domain(Domain), EnergyAtMost(u8), PowerAtMost(u8),
    Temporary, Exhausted, Ready,
    ItemTargetsFriendly, ItemTargets(&Filter), ItemControlledBy(Rel),
    And(&'static [Filter]), Or(&'static [Filter]), Not(&'static Filter),
}
pub enum Rel { Enemy, Any }

pub enum Static {
    Untargetable(fn(&Ctx, u32) -> bool),
    NoUnitsPlayedHere,
    IgnoresDeflect,
}
pub struct Replacement {
    pub applies: fn(&Ctx, &WouldDie, Source) -> bool,
    pub run: fn(&mut Ctx, &WouldDie, Source),
}

pub struct Stage(pub u8);
pub enum Flow { Done, Ask(Prompt) }
```

Keywords are data the engine consults; scripts never inspect them.
`Reaction`/`Action` feed `timing::may_play`; `Deflect` and `IgnoresDeflect`
feed `cost::total`; `Accelerate`/`Repeat` become optional-cost prompts at the
choice step; `Hidden` feeds `hide::legal` and `play::from_facedown`;
`Temporary` registers an implicit `BeginningPhase` ability that calls
`ctx.kill(self)`; `Deathknell` marks the `Death` ability and forces the
`Noted` snapshot; `Tank`/`Backline` order combat assignment; `Assault`/
`Shield` enter `current_might` while designated; `Legion` reads
`SeatState.played_main`; `Ganking` (granted through `CardState.granted` while
Boots of Swiftness is attached) widens `march::legal`; `Equip(cost)` is an
`Activated(Sorcery)` ability with `targets: [Friendly ∧ Unit]` whose `run` is
`ctx.attach`. `has_keyword(ctx, card, kw)` is the single query, merging the
static list and the runtime grants.

`Filter` is evaluated against the projection at choice time and re-checked
at resolution (356.3.e): a target that no longer matches is skipped, the
rest of the effect executes. `Static::Untargetable` is consulted by the
candidate builder for every target chosen by an opponent's item (Akali:
legal only while she holds a combat designation).

### The registry

`cards/mod.rs` holds `pub static CARDS: &[&Card]` sorted by name and
`pub fn script_of(name: &str) -> Option<&'static Card>` (binary search;
the ` Rune` suffix and the manifest token names are matched before the
lookup). A face with no script gets the *generic* script derived from its
kind — a Unit, Gear or Spell with no abilities — so unscripted cards still
play, pay, march, fight, die and score. Two tests pin coverage: every name in
the pool file has a script, and every script's name appears in the pool
file (or is a token or a rune).

### The primitive library

`Ctx` is the vocabulary scripts and the engine share. Every primitive
appends effects, applies them to the projection, and raises the events the
trigger collector drains afterwards.

| Primitive | Does |
|---|---|
| `current_might(card) -> i32`, `printed_might(card)`, `location(card) -> Option<Location>`, `controller(card) -> u8`, `units_at(loc)`, `is_token(card)`, `in_combat(card)`, `alone_at(card)`, `at_battlefield(card)`, `runes_of(seat)`, `holds(seat, zone)` | queries over the projection; `Location` is `Base(seat)` or `Battlefield(zone)` |
| `draw(seat, n)` | top of main-deck to hand (index TOP); Burn Out when short (see scoring); bumps `SeatState.draws`; raises `Drew { seat, nth }` per card |
| `might(card, delta, until, min)` | adds a `MightMod`, snapshotting a minimum-limited delta at its clamped value (454.3.b), mirrors to counter 2 |
| `damage(card, n, source)` | counter 3 delta; raises `DamageDealt` |
| `heal_all()` | counter 3 to zero on every unit |
| `kill(card, source) -> Killed` | the kill path (below); `Killed::{Yes, Replaced, NotOnBoard}` answers "if you do" |
| `stun(card)`, `lock_move(card)`, `buff(card)`, `disempower(card)` | flag bits and mirrors; stun and buff are idempotent |
| `ready(card) -> bool`, `exhaust(card) -> bool` | annotation; `Readied` is raised only when the state changed |
| `move_unit(card, to, cause) -> Moved` | the 425.2.b recall fallback, the 427.2 two-other-players cap, Contested marking (428), `Moved { from, to, cause }`; `MoveCause::{Standard, Effect, Swap}` |
| `recall(card, exhausted)` | to base without a move event (433–436) |
| `spawn(owner, token, at, ready) -> u32` | `Effect::Spawn` with the explicit owner; Sprite gets counter 4; Gold is kind Gear; raises `Played` for the token (Vex) |
| `bounce(card)` | to the owner's hand at TOP; mirrors shed by the engine; tokens vanish |
| `counter_item(item, dest)` | marks NOT_PLAYED, removes it from the chain, moves the card to trash or hand |
| `attach(gear, unit)`, `detach_all(unit)` | the attachment model |
| `reveal(card)`, `reveal_hand(seat)`, `peek(card, seat)`, `peek_top(seat)`, `look_at_facedown(viewer, of)` | `Effect::Reveal` / `Effect::Peek` (M7): a reveal is a debt the host pays with a `Reveal` entry from the card's owner, a peek is a face owed to one seat; `look_at_facedown` sets `looks_facedown_of` and peeks every facedown card of that seat, and `hide::hide` peeks a card hidden later in the turn for every looker. `await_face(card)` — the park-until-Reveal continuation — is unbuilt: no pool card reveals the top of a deck |
| `score(seat)`, `score_xp(seat)` | points through the final-point rule; xp |
| `note_played(seat)` | Legion |
| `delay(when, source, ability, args)` | a delayed trigger |
| `reflex(item, ability)` | queues a reflexive trigger of the resolving item |
| `narrate(line)` | a narration line into `blob.log` |
| `picks() -> &[u32]` | the answers to the prompt that resumed this script |
| `ask(prompt) -> Flow` | assigns a prompt id and returns `Flow::Ask` |

### Three worked scripts

**A unit with a play trigger — Tomb-Raider Barbara.** A 4-Might unit whose
play effect, if its controller controls seven or more runes, targets an enemy
gear and disempowers it if it is empowered, otherwise kills it.

```rust
pub static CARD: Card = Card {
    name: "Tomb-Raider Barbara",
    keywords: &[],
    statics: &[],
    replacement: None,
    abilities: &[Ability {
        trigger: Trigger::Play,
        optional: false,
        cost: None,
        extra: None,
        condition: Some(|ctx, _event, source| ctx.runes_of(ctx.controller(source.card)).count() >= 7),
        targets: &[TargetSpec { filter: Filter::And(&[Filter::Gear, Filter::Enemy]), min: 1, max: 1, kind: TargetKind::Card, label: "an enemy gear" }],
        run: |ctx, item, _stage| {
            let TargetRef::Card(gear) = item.targets[0] else { return Flow::Done };
            if ctx.table.counter(Target::Card(gear), COUNTER_EMPOWERED).unwrap_or(0) > 0 {
                ctx.disempower(gear);
            } else {
                ctx.kill(gear, Source::item(item));
            }
            Flow::Done
        },
    }],
};
```

What the engine does around it: the unit is dragged from hand to the base
(or a controlled battlefield), the host reveals then moves, `legal::classify`
yields `Play`, the cost is paid, the unit enters exhausted and raises
`Played`; `triggers::collect` finds the `Play` ability, evaluates `condition`
against the projection (runes on the board for that seat, ready or
exhausted), and only if it holds pushes a `Pending` whose `Needs::Choices`
raises a `Target` prompt listing every enemy gear (Gold tokens included);
with no candidate the trigger is dropped (390.2). When the item resolves,
`run` reads the chosen gear and either clears the toggle or calls `kill`,
which despawns a token or trashes a card. A `TargetSpec` with `min: 1` and a
single candidate still prompts unless the prompt is auto-answerable, and in
either case `Chosen` is raised for Irelia (352.10.d.2).

**A reaction spell with targeting — Defy.** Counters a spell whose printed
cost is at most 4 energy and at most 1 power.

```rust
pub static CARD: Card = Card {
    name: "Defy",
    keywords: &[Keyword::Reaction],
    statics: &[],
    replacement: None,
    abilities: &[Ability {
        trigger: Trigger::Play,
        optional: false,
        cost: None,
        extra: None,
        condition: None,
        targets: &[TargetSpec { filter: Filter::And(&[Filter::Spell, Filter::EnergyAtMost(4), Filter::PowerAtMost(1)]), min: 1, max: 1, kind: TargetKind::Item, label: "a spell to counter" }],
        run: |ctx, item, _stage| {
            if let TargetRef::Item(target) = item.targets[0] {
                ctx.counter_item(target, CounterDest::Trash);
            }
            Flow::Done
        },
    }],
};
```

The printed cost of the chain item's card comes from its face
(`energy`, `power.len()`), never from what was paid, so a spell played from
hidden for nothing is still judged by its printed cost. `Filter::Spell`
excludes the item being played (352.8) and abilities. Timing: Defy is a
drag to the chain that `timing::may_play` accepts in any Closed state for
its priority holder and in a Showdown Open state for the focus holder, on
any player's turn. Because the countered item is marked NOT_PLAYED before
it would resolve, `PlayedSpell` never fires for it (Ravenbloom Student and
Abandoned Hall stay quiet) and nothing is refunded (412). Discipline is the
same shape with `Filter::Unit` and a two-line body — `ctx.might(unit, 2,
Expiry::EndOfTurn(turn), None); ctx.draw(item.controller, 1)` — and
Stupefy passes `Some(1)` as the minimum.

**A legend ability — Lillia - Bashful Bloom.** An activated ability that
exhausts the legend and costs four energy, one less per friendly unit with
Temporary, and plays a ready 3-Might Sprite token with Temporary to a
location of the controller's choosing.

```rust
pub static CARD: Card = Card {
    name: "Lillia - Bashful Bloom",
    keywords: &[],
    statics: &[],
    replacement: None,
    abilities: &[Ability {
        trigger: Trigger::Activated(Timing::Sorcery),
        optional: false,
        cost: Some(Cost { energy: 4, power: &[] }),
        extra: Some(|ctx, source| {
            let seat = ctx.controller(source.card);
            let temporaries = ctx.table.cards.iter().filter(|c| ctx.controller(c.id) == seat && ctx.is_temporary(c.id)).count() as u8;
            Cost { energy: 4u8.saturating_sub(temporaries), power: &[] }
        }),
        condition: None,
        targets: &[TargetSpec { filter: Filter::Any, min: 1, max: 1, kind: TargetKind::Zone, label: "where the Sprite is played" }],
        run: |ctx, item, _stage| {
            if let TargetRef::Zone(zone) = item.targets[0] {
                ctx.spawn(item.controller, Token::Sprite, Location::of_zone(zone, item.controller), true);
            }
            Flow::Done
        },
    }],
};
```

Activated abilities on a legend also exhaust the source: `cost::total`
treats `Activated` on a legend or unit as `exhaust_self` unless the ability
says otherwise, and the affordance is offered only while the source is ready
(401.3). A `TargetKind::Zone` spec with `Filter::Any` is filled by the
engine's play-location rule (the controller's base plus the battlefields the
controller holds, minus any battlefield with `NoUnitsPlayedHere`), so the
prompt reads "where the Sprite is played" with one affordance per legal
location. The strip shows the computed cost ("Lillia: play a Sprite (2
energy)") and hides the affordance when the planner finds no payment. The
token enters ready, raises `Played` (Vex stuns it if it is an opponent's),
and its Temporary keyword kills it at the start of Lillia's next Beginning
Phase.

Two more, for size: Stellacorn Herder is `Trigger::Move { of: Who::Me, to:
Where::Any }` with `run: |ctx, item, _| { ctx.draw(item.controller, 1);
Flow::Done }`. Zhonya's Hourglass is `keywords: &[Hidden]` and
`replacement: Some(Replacement { applies: |ctx, would, source| friendly(ctx,
source.card, would.unit), run: |ctx, would, source| { ctx.kill(source.card,
Source::replacement()); ctx.recall(would.unit, true); } })`. Hwei is three
stages: draw and ask for a discard (a gesture prompt), resume on the revealed
discard's kind, and for Gear a `Cards` prompt over up to two exhausted runes.

## The action protocol

### Free-form Moves in enforced mode

`legal::classify(ctx, seat, mv) -> Result<Intent, Refusal>` maps every Move
(and MoveHidden, which the host sends as a Move whose card is blank) to
exactly one intent or a refusal with a reason.

| Drag | Intent | Legal when |
|---|---|---|
| hand → chain | `Play` (spells; a unit or gear dragged here is re-routed to a play with a `PlayLocation` prompt / the base) | the seat holds priority or focus with the right timing: Neutral Open on its own turn with an empty chain and no prompt for anything; a Showdown Open state for Action/Reaction; a Closed state for Reaction only; not `no_spells` for spells; the printed cost affordable by some payment plan |
| hand → own base | `Play` with location = base | Sorcery timing as above; Reaction/Action units also in the widened windows |
| hand → battlefield | `Play(unit)` with location = that battlefield | the seat holds it (`control.holder`) and it is not a `NoUnitsPlayedHere` battlefield; gear is refused |
| champion → base / battlefield | `Play(champion)` (107.2) | as above |
| base ↔ battlefield | `StandardMove(unit, to)` (143) | own Action Phase, Neutral Open, no prompt, the unit ready and not NO_MOVE_BY_OWNER, battlefield → battlefield only with Ganking, destination not holding two other seats' units |
| MoveHidden hand/champion → battlefield | `Hide(card, zone)` (737.1.b) | own turn, Neutral Open, the seat holds the battlefield, no facedown card of the seat there, one ready rune to recycle for [A] |
| facedown → chain | `PlayFromFacedown` | `turn > hidden_since`, Reaction timing, cost zero, targets restricted to that battlefield (lifted where the spec makes that impossible), a unit is played to that battlefield |
| hand → trash | answers an open `Discard` prompt for that seat (Hwei); otherwise refused: "kills and discards are automatic" |
| hand → own main-deck (any index; the plugin sinks it to the bottom) | answers the `Mulligan` prompt during Setup, at most two; otherwise refused: "draws and recycles are automatic" |
| anything else (to hand, to a deck, deck → anywhere, off the chain, rune moves, sideboard, another seat's card, a legend or battlefield) | refused | the reason names it: "the chain resolves itself", "runes are paid for you", "that is not your card" |

Group standard moves (143.3): after a legal `StandardMove` the plugin
exhausts the unit and, if the seat has other ready, unlocked units that
could legally move to the same destination, asks a `GroupMove` prompt
(`Cards`, min 0, `done`) before the single cleanup that stages the
showdown or combat. Without it a two-unit attack on a held battlefield is
impossible, because the first arrival opens the showdown and standard moves
are illegal inside it. kai may later batch a multi-select drag into one
intent; the prompt is the protocol either way.

Entry-level Annotate (kai's `e`), Counter nudges, Spawn (the token menu)
and Deal are refused in enforced mode with reasons. A `Reveal` entry from a
card's owner is accepted: the host's pre-play reveal and a voluntary show
(411.2) are indistinguishable at decide time, and both are legal. Seat-0
reveals appended by the host after a fold (`reveal_surfaced`) are accepted
and, when a chain item is parked on `await_face(card)`, resume it. `Reset`
returns `Verdict::advance(Vec::new())` — a fresh lobby. `Join` rebuilds the
lobby roll when the seat count changes. Free mode keeps every entry kind
accepted and runs exactly the W9 bookkeeping.

### Game payloads

`TurnEvent` stays a one-byte tag plus little-endian payload.

| Tag | Event | Payload | Notes |
|---|---|---|---|
| 0 | `StartGame { first }` | seat | kept; only in the lobby by the roll winner |
| 2 | `EndTurn` | — | kept; refused while a chain, prompt, showdown or open roll exists |
| 6, 7 | `CommitRoll`, `RevealRoll` | 8 bytes | kept; the client appends the secret bytes as today; an in-game roll reuses them with a fresh `roll` id in the affordance kind |
| 9 | `Pass` | — | passes priority in a Closed state or focus in an open showdown |
| 10 | `Pick { prompt: u16, option: u16 }` | 4 bytes | the answer to the open prompt |
| 11 | `Activate { source: u32, ability: u8 }` | 5 bytes | an activated ability |
| 12 | `SetMode { mode: u8 }` | 1 byte | lobby only, roll winner: "start (rules enforced)" and "start (free table)" carry it alongside StartGame |
| 13 | `FreeTable` | — | the panic button: the turn player proposes, the other seat's press confirms; the blob keeps its facts but enforcement stops for the rest of the game |

Tags 1, 3, 4, 5 and 8 (`EndBeginningPhase`, `OpenShowdown`, `PassFocus`,
`EndShowdown`, `Resolve`) are deleted; `net/tests/riftbound_turns.rs`, kai's
`plugin_ui` tests and the `--ai` brain move to the new vocabulary in the same
milestone as the blob change.

### Prompts

One function, `prompt::options(ctx, prompt) -> Vec<Opt>` with
`Opt { label: String, card: Option<u32>, answer: Answer }` and `Answer =
Card(u32) | Zone(u16) | Seat(u8) | Item(u16) | Yes | No | Done | Skip |
Cancel`, is called by both `decide` and `present` from the same public fold,
so option *n* means the same thing on every replica and `Pick.option` is the
whole wire contract. kai-cli's `do <n>` needs nothing new. `decide` refuses
a `Pick` whose `prompt` is not the open prompt's id, whose seat is not the
prompted seat, or whose `option` is out of range, each with a reason.

Multi-select (`max > 1`: the mulligan, Singularity's "up to two", Hwei's
runes, the group move) accumulates in `Prompt.picked` one entry at a time —
each pick is its own log entry — and `done` (offered once `picked.len() >=
min`) finishes. `skip` is offered when `min == 0` and nothing is picked;
`cancel` is offered on a play's prompts until its cost is paid (355): it
returns the card to hand (or the champion zone, or its battlefield) with its
face already public — the same leak as picking up a card at a paper table —
and clears the pending item.

A prompt with exactly one legal option, no `skip` and no `cancel` is
answered inside the same decide without a click; when the answer is a target
the `Chosen` event is still raised (352.10.d.2: being the only candidate does
not make it untargeted). `OrderTriggers` with a single item never prompts,
nor does one whose items are interchangeable (the Triggers ruling below).

Gesture-answered prompts: a `Discard` prompt accepts a hand → trash Move by
the prompted seat (the host reveals before a card enters the public trash,
so the decider sees its kind in the same fold and Hwei's branch resumes at
once); the `Mulligan` prompt accepts hand → main-deck BOTTOM Moves plus
`done`. Both also accept the numbered `Cards` affordances, whose labels for
hand cards read "a card in hand" in the public view — kai shows the real
faces because it holds them.

In-game rolls (`PromptKind::Roll`, M7, `engine/roll.rs`): `roll::open(ctx,
seat, cards, why)` opens a `Shuffle { why }` prompt for the recycling seat
whose `picked` holds the cards to order (so the blob needs no new field), and
`blob.roll` with id `ROLL_ID_BASE + prompt id` — a range the lobby's round
numbers never reach, so kai's `RollSecrets` and kai-cli's loop, already keyed
by roll id, cannot confuse the two. The presenter offers `Kind::Commit
{ roll }` to every seat, then `Kind::Reveal { roll }` as in the lobby; the
prompt has no pick options, so a `Pick` is refused and `next_auto` never
answers it, and `EndTurn` and every Move are refused with `PromptOpen` while
it is open. `CommitRoll`/`RevealRoll` during play route to `roll::commit` /
`roll::reveal` only while a roll is open (otherwise `AlreadyStarted` as
before). The last reveal seeds an in-plugin xorshift64 with the pooled
secret xor the roll id, Fisher-Yates permutes the cards, and each is sunk to
its deck bottom in that order — the final bottom-up order is the reverse of
the emission order, identical on every replica because the permutation is a
pure function of the log. A roll needs at least two cards; one card is
recycled without asking. A seat leaving abandons an open roll
(`roll::abandon`): the cards keep their order and play continues. The
mulligan is the only caller (`phases::finish_mulligan` → `roll::open` →
`phases::after_mulligan`); Burn Out is not in the pool and stays unbuilt.

### How the presenter phrases each prompt

Labels use `{seat N}`, `{zone N}` and the new `{card N}` placeholders; kai
expands the card to its face name (or "a hidden card") and highlights it.
The status line is the question; the affordances are the options.

| `PromptWhy` | Status line | Affordances |
|---|---|---|
| `Mulligan` | "set aside up to 2 cards to redraw" | one per hand card ("set aside {card N}"), `keep` (= done) |
| `Target { item, spec }` | "{card item}: choose <spec.label> (k of max)" | one per candidate `{card N}` / `{seat N}` / `{zone N}` / "{card item} on the chain"; `done`, `skip`, `cancel` as allowed |
| `PlayLocation` | "where does {card N} enter?" | "your base", one per held battlefield `{zone N}`; `cancel` |
| `OptionalCost` | "pay <cost> to <what>?" ("accelerate {card N} for 1 energy and 1 Mind power?", "pay 1 any power for the {card N} trigger · {card M}?") | `yes`, `no` |
| `OrderTriggers` | "order your triggers (last placed resolves first)" | one per unplaced trigger "{card N} trigger", suffixed " · {card M}" / " · {zone Z}" with the item's subject when it is not the source itself, so two triggers of one source (Irelia - Blade Dancer's ChosenFriendly for two chosen units) read apart |
| `PickStaged` | "which showdown opens first?" | one per staged battlefield "{zone N}" |
| `GroupMove` | "move others to {zone N} too?" | one per ready unit `{card N}`, `done` |
| `Assign` | "assign <remaining> damage: who takes lethal next?" | one per eligible unit "{card N} (lethal k)" |
| `Resume { item, stage }` | "{card item}: choose <the script's question> (k of max)" — Abandon's "the top card of your deck to recycle", Edge of Night's "a unit you control here to wear it", Hwei's "up to two runes to ready", Dusk Rose Lab's "a unit you control here to kill", and the parked kill's "which replacement applies" (one per Zhonya's Hourglass that would die instead; taken by itself when only one applies) | script-defined candidates; `done` when the pick is a multi (Hwei's runes), `skip` when it is optional (Abandon), `cancel` when the script allows it |
| `Discard` | "discard a card" | one per hand card, or drag one to the trash |
| `PayWith { item }` | "pay <n> power for {card item} with" | "kill {card N}" per ready Gold, then "recycle a rune"; `cancel` |
| `Shuffle` | "roll to shuffle <n> recycled cards" | none: `roll` and `reveal` are affordances of the in-game roll, and kai-cli sends the reveal by itself |

There is no `Replacement` prompt kind: the replacement choice is the parked
kill's `Resume` prompt (see the Kills ruling), and the tag the enum once
reserved for it stays retired. `PromptWhy::each()` decodes one sample of
every kind the writer knows and `prompts::answer_words(why)` names the words
the answers to each kind use, so a brain's tool text can be checked against
the presenter rather than against a hand-typed list.

Outside a prompt the strip carries, for the acting seat: `pass` (w) while it
holds priority or focus, `end turn` (space) in Neutral Open with an empty
chain, one affordance per activatable ability with a legal timing and an
affordable cost ("Lillia: play a Sprite (2 energy)", "equip Boots of
Swiftness to…"). Every other seat sees "waiting for {seat N}: <what>". The
turn/phase line, the scoreboard, "held by / contested by" per battlefield,
the chain ("chain: Discipline → Defy (top)"), the combat line and the last
four narration lines are status for everyone. The winner is set from the
points as today. `x` is the hotkey for `cancel`/`skip`/`no`.

## The chain, priority and focus

States (308–310): Neutral Open (no chain, no showdown; the turn player holds
priority in the Action Phase), Showdown Open (`showdown` present, chain
empty; the focus holder acts), Closed (`priority` present because a
finalized item sits on the chain; the priority holder may play Reaction
only). A Showdown Closed state is a Closed state inside a showdown.

Playing a card (351–356) as one drag:

1. The host reveals, previews the decide, then folds Reveal + Move. The
   decide creates a `Pending` item `{ Spell { card } | Ability | …, origin,
   controller }` and runs the choices step at once.
2. Choices (352): for each `TargetSpec` a `Target` prompt over the legal
   candidates, excluding untargetable cards, the item itself, and candidates
   that would deterministically lead to an illegal state (352.16); a unit
   gets a `PlayLocation` prompt (auto-answered when only the base is legal);
   optional costs (Accelerate, Repeat) are `OptionalCost` confirms.
   Candidates an opponent's Deflect makes unaffordable are not offered; in
   a multi-pick prompt each remaining candidate is priced together with the
   picks already recorded on the prompt, so a second Deflect unit vanishes
   from the list once the first one uses up the runes. If no candidate is
   affordable the prompt offers only `cancel`, and as a fallback a direct or
   stale answer whose full set the planner cannot pay is refused as
   `NotALegalTarget` with the prompt intact rather than failing at payment.
   A repeated pick is refused the same way: "each of up to two units" means
   two different units.
3. Cost (353–354): `cost::total(ctx, item)` = printed (zero from facedown)
   + Deflect per chosen enemy permanent (skipped by `IgnoresDeflect`) +
   additional (Accelerate) − discounts (`extra`), never below zero.
   `pay::plan(ctx, seat, cost) -> Option<Plan>`: energy by exhausting ready
   runes (160.2.a), power by recycling a rune of that domain (160.2.b —
   recycling carries no [E], so an exhausted rune pays power, and a rune
   exhausted for energy in the same plan may be the one recycled; rainbow
   takes any). The planner recycles already-exhausted runes first, then
   prefers the runes it is exhausting for energy, so a play needs
   `max(energy, power)` runes rather than their sum and leaves as many
   ready runes as it can; for energy it prefers runes whose domain no
   power need wants. A ready Gold token stands in as a rainbow source when
   runes fall short; if a Gold could stand in for a recycle, `PayWith`
   asks. The Move was pre-checked against the printed cost with the same
   planner, so most refusals come before the reveal.
4. Legality (355) is re-checked; `Chosen { card, by, item }` is raised per
   target (Irelia's triggers go on the chain above the item, 744.1.c.1);
   then the item **finalizes**: a Unit or Gear leaves the chain to its
   location (exhausted, ready with Accelerate; gear ready in the base) and
   raises `Played`; a Spell becomes `Finalized`, the state closes with
   `Priority { active: controller, passes: 0 }` (333.1.c.3 — the controller
   may react to their own spell).
5. Reactions (334–335): the priority holder sees "{seat N} may respond to
   {card N}" and `pass`; a Reaction play is a drag to the chain (a facedown
   card older than this turn counts). Each `Pass` walks the ring
   (`PassWindow` over `TurnOrder`); a play resets the passes; when every seat
   passes in sequence the top item resolves (336): `run` executes on the
   projection, suspending with `Resume` if it asks; `PlayedSpell` fires play
   triggers unless NOT_PLAYED; the card goes to the owner's trash; then
   `cleanup()` and `triggers::collect()`. If the chain is empty the state
   opens; inside a showdown focus then passes to the next seat (343) unless
   `initial_chain` is set (442.1.e).
6. All of this runs inside one decide until a prompt opens, priority or
   focus sits with a seat that can act, or the turn player has a Neutral
   Open table.

Auto-pass: a seat that provably cannot act in a Closed or Showdown window —
an empty hand, no facedown card older than this turn, no Action/Reaction
activation it can afford — is passed for it. One predicate
(`priority::can_act`) serves both windows so a showdown never skips a seat
the chain would wait for. The plugin cannot read hands, so this is the only
safe auto-pass; a client-side "always pass" toggle stays a client concern.

Activated abilities (`Activate { source, ability }`) walk the same pending
path: targets, cost (including exhausting the source), legality, then a
finalized item unless it Adds (416.2: Adds resolve at once and never sit on
the chain — basic runes and Gold are payment sources, not affordances).

Showdowns (338–345): a cleanup that finds a Contested battlefield stages a
showdown (only the contester's units) or a combat (two seats' units); with
several staged in Neutral Open the turn player gets `PickStaged`, else it
opens at once; a contest made inside an open showdown waits for that
showdown to close (322.13). Opening sets `Showdown { zone, attacker:
contester, defender, window: PassWindow::open(attacker), combat }`; the
focus holder may play an Action/Reaction spell (a chain as normal) or
activate an Action/Reaction ability or `pass`. A play resets the passes and,
once its chain empties, hands focus to the next seat; passes in sequence by
every seat close the showdown: non-combat → establish control (a Conquer if
the contester is alone and has not scored there this turn) or, with two
seats' units present, stay Contested and stage a combat with the contester
as attacker (345.2.b); combat → the damage step. Standard moves and Sorcery
activations are refused while a showdown is open.

## Combat, step by step

Staging happens at cleanup step 7 for every Contested battlefield holding
units of two seats; it un-stages if a side leaves before it opens.

**Step 1, the Showdown step (442.1).** `Showdown { combat: true, attacker:
the contester, defender: the other seat }`. Every unit at the battlefield
gains its controller's designation (ATTACKER/DEFENDER flags, display
annotations) and raises `Attacks`/`Defends`. Attack/Defend triggers form the
initial chain — the attacker's batch first, then non-defenders in turn
order, then the defender's — and if it is non-empty the state closes with
`initial_chain = true`, so focus stays with the attacker when it empties.
None of the pool's cards has such a trigger; the machinery is generic. Units
arriving mid-showdown (Charm, Tideturner) get their designation at the next
cleanup (322.5); units leaving lose it and take no part in damage.

**Step 2, combat damage (443.1), when the showdown closes.** Skipped unless
both sides still have units there. `might_sum(side)` is Σ current Might over
the side's units, stunned units contributing zero (410.1.b), negatives
floored at zero, Assault/Shield included. Each side then assigns its sum
among the opposing units, attacker first:

```rust
pub struct Assignment { pub assigner: u8, pub remaining: u8, pub done: Vec<(u32, u8)> }
pub fn candidates(ctx: &Ctx, a: &Assignment) -> Vec<u32>;
pub fn lethal(ctx: &Ctx, unit: u32) -> u8;
```

`candidates` lists the opposing units not yet assigned lethal, filtered by
ordering: Tank units while any remains, Backline units only when nothing else
remains, a unit carrying both handled by the assigner's choice (443.1.d.7),
units that cannot be dealt damage excluded (443.1.d.9). `lethal` is
`max(1, current Might − damage already marked)` — the reading that prior
damage counts, documented under rulings — so a 0-Might Scuttle Crab needs 1.
The loop: with `remaining == 0` or no candidates, done; with exactly one
candidate, or `remaining` below every candidate's lethal and one candidate,
automatic; otherwise an `Assign` prompt lists the candidates and the pick
receives `min(lethal, remaining)`. When `remaining` is below every remaining
lethal the last pick receives the remainder (443.1.d.4: excess only when no
units remain). Both assignments are collected first, then dealt
simultaneously with `Source::Combat`.

**Step 3, resolution (444.1.a).** A Combat Special Cleanup in one pass:
1 win check; 2a Deathknell triggers noted and queued for every unit with
nonzero damage ≥ Might (Zhonya's replacement consulted per unit); 2b kills
(trash or despawn); 2c heal all units; 2d recall surviving attackers to their
base if defenders survive (no move event, exhaustion untouched); 2e clear
designations and `CombatEnd` expiries; then steps 3–10 and
`establish(zone)`: the sole survivor's controller takes control — a Conquer
with the point (or the final-point draw) and `Conquered` for the units
present, the legend and the battlefield; a surviving defender re-establishes
without scoring; nobody left → Uncontrolled and Contested cleared. Queued
Deathknells then walk the normal chain (priority passes, reactions allowed)
so Sprite Fountain's Sprite, Unsung Hero's draw and Scuttle Crab's reveal land
after the cleanup, as 415.1.a.1.b orders it.

Stun (410): STUNNED zeroes a unit's contribution but `lethal` uses full
Might; every stun clears at the Ending step of every turn. Vex's
NO_MOVE_BY_OWNER refuses the owner's standard moves and removes the unit
from the candidates of the owner's move effects; Charm by the opponent still
moves it.

The player sees, after the last pass: "attackers 7 might vs defenders 5
might", the assignment prompts if any, then narration such as "Ravenbloom
Student dies · Irelia survives (3 damage healed) · {seat N} conquers
{zone N}".

## Triggers, events and durations

```rust
pub enum Event {
    Played { card: u32, controller: u8, kind: Kind, origin: Origin },
    PlayedSpell { item: u16, controller: u8 },
    Moved { card: u32, from: Location, to: Location, cause: MoveCause },
    Entered { card: u32, at: Location },
    Died { card: u32, controller: u8, noted: Noted },
    Drew { seat: u8, nth: u8 },
    Chosen { card: u32, by: u8, item: u16 },
    Readied { card: u32, by: u8 },
    Conquered { zone: u16, seat: u8, units: Vec<u32> },
    Held { zone: u16, seat: u8, units: Vec<u32> },
    BeginningPhase { seat: u8 },
    EndingStep { seat: u8 },
    Attacks { card: u32 },
    Defends { card: u32 },
    DamageDealt { card: u32, n: u8, source: Source },
}
```

`triggers::collect(ctx)` runs after every resolution, after every limited
action outside the chain (a standard move, a hide, a channel) and after each
cleanup step that kills — never inside a `run` (154.3, 351.3). It drains
`ctx.events` and asks every on-board permanent, both legends, every
battlefield card in play and every `delayed` entry whether an `Ability`'s
`trigger` matches (`Trigger::matches` encodes the relation words — `Who::Me`,
friendly/enemy, `here`, opponent-of-controller — and battlefield abilities
belong to the holder, or the turn player when uncontrolled, 184.6), then
evaluates `condition`. Everything one `collect` drains is one simultaneous
batch — a finalize that raises a `Chosen` per target, a cleanup that raises
a `Died` per unit, a draw-2 that raises two `Drew` — and only then are the
matches grouped by controller, the turn player's batch first then turn order
(376.3.b); a seat with one match is appended straight to `queue`, a seat
with two or more asks `OrderTriggers` — Dusk Rose Lab against a Temporary
Sprite is a real choice. Queued items then walk `Needs`: `Choices` (target
prompts), an optional-cost confirm when the trigger's total cost is not free
(392.2: "pay 1 any power for the {card N} trigger?" for a Deflect it incurred,
later "pay [A] and exhaust Irelia to ready {card N}?"; `no` and an
unaffordable cost both remove the trigger, narrated as declined), `Legality`
(an item whose targets vanished is dropped silently, 390.2), then finalize
onto the chain. Simultaneity (376.2.c)
falls out of running `collect` after the event against the projected table.
Reflexive "Do this:" abilities use `ctx.reflex` and `Trigger::Reflexive`.

Delayed triggers (383–385): `Delayed { when: EndOfTurn(turn) |
BeginningOf(seat), … }` appended by scripts (Targon's Peak) and consumed by
the Ending step or the Beginning phase whether or not the source is still on
the board.

Durations: `Expiry::EndOfTurn(turn)` mods are reversed at the Expiration
step by the opposite counter delta (317.3); `CombatEnd` at step 2e;
`WhileAttached(unit)` on detach; `Permanent` for the buff, mirrored by
counter 5 and shed by the engine when the unit leaves play. Might arithmetic
is `printed + buff + Σ mods`, increases before decreases, minimum-limited
deltas snapshotted at application (454.3.b), floored at zero at read time.
"Might becomes X" is a reserved `MightMod` tag no card in the pool needs.

The kill path, `ctx.kill(card, source) -> Killed`:

1. Not on the board → `Killed::NotOnBoard`.
2. Replacement check: every on-board card whose `Replacement::applies`
   returns true; one applies automatically. Several *should* ask the owner
   (`Replacement` prompt); M4 ships a deterministic owner-then-lowest-id
   choice instead, because `kill` is a synchronous fn whose `Killed` value
   scripts branch on for "if you do", so asking needs a resumable stage
   rather than a local prompt — see `kill::two_applicable_replacements_ask_
   the_dying_units_controller_which_one_runs`. A replacement runs *instead*
   and returns `Killed::Replaced`: no Deathknell, no `Died`, Pyke does not
   count it, and an "If you do" clause reads it as "you did not"
   (Pickpocket, Adaptatron, Dusk Rose Lab all agree).
3. Deathknell: snapshot `Noted { zone, might: current, controller }` and push
   a pending `Death` trigger.
4. Move to the owner's trash, or `Despawn` for a token (known from
   `Snapshot.tokens` plus this request's spawns).
5. Raise `Died` and return `Killed::Yes`.

Lethal-damage kills in cleanup step 2 call the same function with
`Source::Cleanup { last_item }` so 415.5 attribution is kept. A countered
item (`ctx.counter_item`) is marked NOT_PLAYED, its card moves to the trash
(or the hand for Abandon), it leaves the chain; an ability countered by Not
So Fast is removed with its source untouched. A card leaving the board drops
its `CardState` after the engine sheds the mirrors; attached gear detaches at
the unit's last location and is recalled at cleanup step 5.

Keyword summary — how each is represented and who reads it:

| Keyword | Representation | Reader |
|---|---|---|
| Accelerate | `OptionalCost` prompt at the choice step; paid → enters ready without `Readied` | `play::finalize` |
| Action / Reaction | timing; a facedown card older than this turn is granted Reaction | `timing::may_play` |
| Assault / Shield | `+X` while the matching flag is set | `current_might` |
| Tank / Backline | ordering in `combat::candidates` | combat |
| Deathknell | the `Death` ability plus the `Noted` snapshot | `kill` |
| Deflect(X) | `+X` rainbow per chosen permanent an opponent controls; skipped by `IgnoresDeflect` | `cost::total`, candidate filtering |
| Ganking | static or granted keyword | `march::legal` |
| Hidden | `Hide` and `PlayFromFacedown` intents; `hidden_at`/`hidden_since`; loss at cleanup step 5 | `legal`, `cleanup` |
| Legion | `SeatState.played_main`, tested before the current play sets it | `condition` helpers |
| Temporary | an implicit `BeginningPhase` ability calling `kill(self)`; tokens carry counter 4 | `phases::beginning` |
| Vision / Predict | `peek_top` (M7); the face reaches the seat through the host's owed-faces routing | scripts |
| Equip(cost) | an `Activated(Sorcery)` ability with `attach` | `activate` |
| Stun | STUNNED flag, idempotent, cleared at every Ending step | combat, `stun` |
| Buff / Empowered | counters 5 and 6 | `current_might`, Barbara |
| Mighty | `current_might >= 5` on board, printed off board, `Noted.might` at death | Sunken Temple, Unsung Hero |

## Scoring

`score::establish(ctx, zone)` is called when a showdown or combat ends and
by cleanup step 4. Control passes to the seat whose units stand alone;
a change of holder is a Conquer if that seat has not scored the battlefield
this turn (446.1, 447); a Hold scores at the Scoring step of the holder's
Beginning Phase for every battlefield still held (446.2), after the
Beginning step's Temporary kills and Dusk Rose Lab (315.2). The final-point
rule (448.1.b) stays as `rules::score` has it: at victory − 1 a Hold scores,
a Conquer scores only if the seat has scored every battlefield this turn and
otherwise draws a card. Points from Burn Out ignore it. Every score raises
`Conquered`/`Held` with the units present, so unit ("when I conquer"),
legend ("when you conquer") and battlefield triggers fire once per seat per
battlefield per turn (448.2). Reaching the victory score wins at cleanup
step 1 and the presenter's winner line follows. The victory score and the
battlefield count are table options (M8): `TableConfig.options` carries a
CBOR map of `victory_score` (default 8) and `battlefields` (default 2) that
`agni_riftbound::TableOptions` encodes and decodes (kai's decoder tolerates
unknown keys and drops them, the SDK's `Snapshot.options` and the plugin
keep them; missing keys take the defaults; kai never re-encodes a decoded
map — a re-host reuses the genesis bytes and a joiner only reads), the fold
copies it into `LogState.options` at genesis (kept across a Reset), the SDK
reads it as `Snapshot.options`/`Snapshot::option(key)`, and
`rules::Options::of(table)` is what `Ctx`, `score`, `winner` and
`contested` consult — an option the plugin cannot use (absent, zero,
negative, out of range) falls back to 8 points and one battlefield per seat
(never fewer than two). `TableOptions::in_play(bytes, players)` is the same
fallback on the kai side, field by field, truncated to the declared
battlefield zones the way `Zones::of` truncates, so what kai's lobby label,
deal and table layout say is what the plugin plays even for a foreign host
writing `battlefields: 0` or `200`; `plugins/riftbound` pins the two equal
through real request bytes. The final-point rule holds at the configured
score. kai writes options at genesis only when the host chose them in the
lobby (a mode preset or house rules): an untouched lobby opens a table with
no `options`, which the plugin plays by seat count exactly as every pre-M8
table — a 4-seat table hosted without a preset still stages three
battlefields. Joiners read the options back from the folded state, kai lays
out as many contested battlefields as are in play
(`SessionInfo::battlefields_in_play`, not one per seat, so a duel on three
battlefields has an anchor, felt and drop target for the third), and a table
with more battlefields than seats has the seats after the first player place
the extra ones (`agni_riftbound::contribution_of`, `Battlefields::Many`: the
chosen battlefield leads, the next in deck order follow).

Burn Out (418): a draw from an empty main deck asks both seats to roll
(`PromptKind::Roll`), recycles the trash into the deck in the seeded order,
gives an opponent (the only one, in a duel) a point, then completes the
draw; it repeats each time the deck is empty.

## Hidden cards

A hide is a MoveHidden the host sends without a reveal; it lands in the log
as `LogAction::Move { hidden: true }` (serde-defaulted, skipped when false,
so every earlier log and golden is byte-identical) and the fold strips the
card's face, drops it from `revealed`/`shown` and forgets its peeks — a hand
card the opponent was shown in place (Scuttle Crab's Deathknell) is secret
again once it is hidden, in every replica and in the plugin's snapshot
(`Snapshot::apply_entry` blanks it the same way, and `legal::from_hand`
reads the entry's `hidden` flag so a public face never turns the gesture
into a play). The fold refuses a hidden move of a card the actor neither
owns nor holds (`ForeignHand`); the host forgets the faces it sent other
seats for that card and the client drops a face another seat re-hid, so a
later `Effect::Peek` (a looker's grant) is paid again. The plugin charges
[A] blind (one ready rune recycled), records `hidden_at`/`hidden_since`,
mirrors the `hidden` annotation so kai lays the card face down at that
battlefield, and refuses a second facedown card of the same seat there.
Cleanup step 5 trashes it when its owner no longer holds the battlefield;
from the next turn it has Reaction and plays from facedown for zero with its
targets restricted to that battlefield (737.1.d) — lifted only when the spec
makes it impossible (Tideturner) — and a unit lands there. The host already
reveals a facedown card before its Move to the chain (the chain is a public
zone and the card is unrevealed), so no host change is needed for the play.
The plugin cannot verify the Hidden keyword at hide time, so the hide is
charged blind; the face is public by the time the card reaches the chain, so
`hide::play_legal` reads it there and refuses a card without Hidden with
`Reason::NoHiddenKeyword` — that refusal is the "stuck" the ruling promises,
and `hide::reacts` withholds the 737.6 grant from the same card. The keyword
has to survive generic resolution for the offer and the enforcement to agree,
so `cards::PRINTED_HIDDEN` names the three printed-Hidden pool cards with no
script of their own (Smoke and Mirrors, Switcheroo, Edge of Night) and
`resolve` gives them a generic script carrying `Keyword::Hidden`; a test walks
the pool file and asserts every card whose text prints [Hidden] resolves to a
script that has it and that no other card claims it. Back Off draws
only from hand and Edge of Night auto-attaches only from facedown, both via
`Origin::Facedown` on the item. Scuttle Crab's Deathknell sets
`looks_facedown_of` so the view can show that seat the opponent's facedown
faces; since M7 they arrive through `Effect::Peek` (`Ctx::look_at_facedown`),
`TableView.peeked` names them per viewer and kai's `face_down_for` lays a
peeked facedown card face up for the looker.

Six consequences of 737.1.d and 408.3 that M6 enforces in the engine rather
than per card. `triggers::played_origin` carries `Origin::Facedown` from the
`Played` event onto the `Play` trigger item of a hidden permanent, so a play
effect is restricted to the hiding battlefield the same way a hidden spell is
(737.1.d.2, the Blastcone Fae example); no other origin is carried, so a
trigger of a card played normally stays `Origin::Board`. `hide::play_legal`
refuses a hidden spell whose targets are all out of reach with
`Reason::NoLegalTargets` (737.1.d's closing sentence) instead of opening a
prompt whose only answer is cancel, which also drops the play out of
`legal::highlights`. `kill::applicable` skips a facedown source, so a card
whose face has become public while it still sits in a facedown zone stops
applying its printed replacement (408.3). And `Filter::Not(&Filter::Here)`
fails closed when the item's source has no location, so a speculative item
for a card still in hand offers nothing rather than everything.
`hide::play_legal` also refuses a hidden unit at a battlefield whose card
forbids units (Rockfall Path) with `Reason::NoUnitsPlayedHere` instead of
dropping it into the base: 739.3.a keeps the inherent placement restrictions,
so the forced placement of 737.1.d.1 is unconditional once the play is legal.
And `activate::offers` skips a facedown source the same way `kill::applicable`
does, so a card whose face has become public while it still lies face down
offers no activated ability either (408.3).

`hide::lifted_by` reads the target spec's filter tree and fails closed: only
`DifferentLocationFrom` and a bare `Not(Here)` lift the 737.1.d.2 restriction,
an `And` lifts when any conjunct does, an `Or` only when every branch does, and
a nested `Not` never recurses (`Not(Not(Here))` means Here). It is an allowlist,
not the rule's own semantic test ("a targeting restriction that can never be
fulfilled by a unit at its battlefield"): a spell whose targets could only ever
live off the board is refused with `NoLegalTargets` rather than freely targeted.
Nothing in the pool reaches that case — Tideturner is the intended lift — and
the semantic test wants a candidate-universe comparison per spec, which waits
for M7.

Secrecy is the property M6 exists to protect, so it is enforced at both ends.
`decide`'s `Action::Reveal` arm refuses a reveal of a facedown card the actor
does not control (`Reason::NotYourCard`) rather than waving it through, and it
still accepts every other reveal, including the automatic ones the host appends
as seat 0. `HostSession::intent` refuses a `WireIntent::Reveal` for a card
another seat hid, drops the `hidden_plays` mark only after the reveal has been
admitted, and restores the displaced mark when an intent is refused instead of
clearing it — otherwise a refused `MoveHidden` aimed at the other seat's
facedown card would publish its face on the next entry. `HostSession::table()`
renders through `table_for(seat)`, a face map holding the revealed faces plus
the viewer's own cards, so the host player's renderer no longer substitutes the
dealer's face for the other seat's facedown card; kai's `peeked_face` and
`face_down_for` take the viewer as well and refuse to peek at a card the viewer
does not own.

408.4's end-of-game clause — every facedown card is revealed when the game ends
— landed with M7: `engine::finish` calls `hide::game_over` when the deciding
entry set `ctx.won`, which drops every `hidden_at`, sheds the `hidden`
annotation and emits `Effect::Reveal` per card; the host's `reveal_surfaced`
honours `owed_reveals`, appends the `Reveal` from the seat that hid the card
and drops it from `hidden_plays`.

## What changes outside the plugin

Each item is additive and serde-defaulted or CBOR-skipped so old logs and
goldens stay byte-identical unless noted.

agni-sim:

1. `log::Verdict.reason: Option<String>` (skipped when `None`);
   `FoldError::Rejected { reason: Option<String> }` whose `Display` prints
   the reason; `fold_shadowed` compares accept, effects and state only.
2. `Effect::Spawn { …, owner: Option<u8> }` (None = the entry's actor, as
   today). `Table::add_face` mints owner = actor, so Sprites and Gold minted
   inside the other seat's EndTurn, Pass or spell entries would otherwise
   belong to the wrong seat and go to the wrong trash. Native and wasm folds
   both honour it; `Ctx::spawn` always sets it. Wire version bump.
3. `wire::Affordance.card: Option<u32>` and `PluginView.prompt:
   Option<PromptSummary { seat, why: String, min, max, picked, optional }>`
   so kai and kai-cli can show "choose up to 2 · 1 picked" without parsing
   labels (M0).
3a. `wire::PluginView.legal: Vec<Legal { card, kinds, zones }>`,
   `PluginView.arrows: Vec<Arrow>` and `PluginView.chain: Vec<ChainRow>`
   (M5), each serde-defaulted and skipped when empty, so an M4 plugin's view
   still decodes with none of them. `Legal.hidden: Vec<u16>` (M8, the same
   way): the battlefields a `MoveHidden` of that card is accepted at, kept
   apart from `zones`, which are the destinations of a face-up Move. An M7
   plugin writes no `hidden` key, so a host on this wire offers no hide
   drop target with it until the plugin is rebuilt; nothing else changes.
   `PLUGIN_ABI_VERSION` stays 0 through M5–M8: every field is additive on
   both sides of the `view` call and `decide`'s request is unchanged.
4. `abi::PluginViewRequest.faces: BTreeMap<u32, CardFace>` — the viewer's
   own faces, built by `HostSession::plugin_view` from `dealer` and
   `ClientSession::plugin_view` from `faces`; advisory only, the decide
   request is untouched (M5, serde-defaulted so an M4 plugin ignores it).
5. `Effect::Reveal { card }` → `LogState.owed_reveals: BTreeSet<u32>` and
   `Effect::Peek { card, seat }` → `LogState.peeks: BTreeSet<(u32, u8)>`,
   both skipped when empty (M7). Both refuse an unknown card (`BadEffect`),
   a peek refuses an unseated seat, and either is a no-op on a card already
   revealed. A `Reveal` entry pays the debt and clears the card's peeks; a
   peek survives moves and is forgotten when the card leaves the table. A
   `Reveal` entry on a card in an owner-visible zone also marks it
   `LogState.shown` — visible to every viewer in place — until the card next
   moves, so a hand reveal is seen at the table while the Dunebreaker rule
   (a card revealed on the board and bounced to hand is private again) still
   holds. `TableView.peeked` lists the viewer's peeks (`ViewDelta::Peeked`).
   No wire version bump: `Effect`, `LogState` and `TableView` travel inside
   the engine/plugin ABI and the fold, never inside `ClientMsg`/`HostMsg`,
   and the genesis pins already refuse a peer on a different engine or
   plugin, so the goldens were unchanged and `WIRE_VERSION` stayed 4 at M7.
6. `Effect::Despawn` refused for non-tokens unless the manifest declares
   `despawn_any` (default false); `FoldError::BadEffect { index }` for
   diagnosis.
6a. `LogAction::Move.hidden: bool` (M7, serde-defaulted, skipped when false)
   — the hide the wire always carried as `WireIntent::MoveHidden` is now a
   fact of the fold, not only of the host's `hidden_plays`; see Hidden
   cards. `ENGINE_ABI_VERSION` is 2: `Effect::Reveal`/`Effect::Peek`,
   `ViewDelta::Peeked` and the `hidden` flag all travel inside
   `FoldRequest`/`FoldOutcome`, kai pairs its store's engine and plugin
   modules independently, and an M6 engine.wasm would decode an M7 plugin's
   verdict as "fold request does not decode" mid-game; `AbiEngine::load`
   refuses it at load instead and `session_engine` falls back to the native
   engine. `WIRE_VERSION` stayed 4 at M7: the flag rides inside
   `HostMsg::Entry`, an M6 peer would fold it as a plain move, but the
   genesis engine and plugin pins already refuse that peer before any entry
   is exchanged.
7. `TableConfig.options` populated at genesis from kai's lobby options
   (`agni_riftbound::TableOptions`: victory score, battlefield count) and
   folded into `LogState.options` (M8, serde-defaulted and skipped when
   absent, so every earlier log, state and golden is byte-identical).
   `ENGINE_ABI_VERSION` is 3: an M7 engine.wasm would fold the genesis
   without keeping the options and hand the plugin a state that plays every
   table at 8 points and two battlefields, so `AbiEngine::load` refuses it
   and `session_engine` falls back to the native engine. `WIRE_VERSION` is
   5, even though the `options` bytes were always a field of the genesis
   `TableConfig` on the wire: the pins do not refuse an M7 kai joining an M8
   table. `join_modules` fetches the pinned plugin by hash and loads it
   (`PLUGIN_ABI_VERSION` is still 0), the engine pin is optional and the
   host pins none while `session_engine` falls back to native — the state
   kai is in until the store `engine.wasm` is rebuilt — and the M7 native
   engine's `LogState` has no `options` field, so the joiner's replica
   folds the genesis without them and its plugin plays 8 points on one
   battlefield per seat while the host plays the configured table. Because
   `ClientSession::from_welcome_with` re-decides every entry with its own
   plugin, the divergence would surface as a differing blob or a replica
   fault mid-game rather than a refusal at join; the wire version is the one
   gate an unpinned-engine table has, so it moves and the goldens with it.
   Web and desktop deploy together per kai's wire-version rule.

agni-plugin-sdk (still zero dependencies, still integer-only):

8. `decide::Action` grows `Annotate { card, key, value }`, `Counter { target,
   counter, delta }`, `Reveal { card }`, `Deal { zone, count }`, `Reset`,
   `Clear { seat }`, `Join`; `Other` remains for Genesis.
   `Verdict::refuse(reason)` and the `reason` key in `encode`.
9. `decide::Effect::Spawn.owner`, `Effect::Reveal`, `Effect::Peek`;
   `Action::Move.hidden` (read from the entry's `hidden` key, default false)
   and `Reader::boolean`.
10. `table::Snapshot.tokens`, `Snapshot.revealed`, `CardInfo.annotations:
    Vec<(String, Vec<u8>)>` (all keys), `Snapshot.options: Vec<(String,
    i64)>`; `Snapshot::apply_entry(&Action, seat)` and
    `Snapshot::apply(&Effect, actor)` — the projection primitives,
    cross-tested against agni-sim.
11. `view::Affordance.card`, `PluginView::offer_card(label, card, data)`,
    `PluginView::prompt(summary)`, `Request.faces`; `view::Legal { card,
    kinds, zones, hidden }`, `Arrow`, `ChainRow` and the `legal`/`arrows`/
    `chain` builders (M5, `hidden` M8) — the writer omits each empty list
    and the wire side defaults it back, which is what keeps the two
    `PluginView` types interchangeable across plugin versions.
12. A `prompt` module (`Prompt`, `Opt`, `Answer`, `Pick` encode/decode, the
    stale-id check) and a `blob` helper (`MapWriter`/`MapReader` with
    `field(key)` and `skip_unknown`) so the MTG plugin reuses the choice
    protocol and the versioned-document idiom.

agni-net:

13. `SessionError::Refused` carries the reason; `HostMsg::Notice` relays it
    to the refused seat; `decide_after`'s preview returns it.
14. `reveal_surfaced` honours `owed_reveals` — a card in a public zone is
    revealed by seat 0 (or by the seat that hid it), a card in an
    owner-visible zone by its owner, and a card in a face-down zone is left
    owed since the log refuses the entry (`HiddenZone`); `owed_faces`,
    `faces_owed_to`, `private_faces` and `face_known_to` honour `peeks`, so a
    peeked face is sent once per seat, re-sent on reconnect, and rendered by
    the host's own `table_for`. A seat's own cards in a face-down zone are no
    longer counted as known to it by `face_known_to` (M7).
15. The host drops a card from `hidden_plays` whenever a fold moves it out
    of its battlefield (a facedown card the plugin trashes at cleanup
    otherwise stays unrevealed in the public trash forever).
16. `net/tests/riftbound_turns.rs` rewritten for the new events in M0/M1.
16a. `WireIntent::MoveHidden` ↔ `LogAction::Move { hidden: true }` both
    ways; `HostSession::append` forgets `sent_faces` of other seats for a
    re-hidden card; `ClientSession::apply` drops the face of a card another
    seat hid.

kai:

17. `plugin_ui`: `{card N}` expansion, card highlighting for every card an
    enabled affordance names (a rim in the seat's colour), a click on such a
    card routed to `PluginActionRequested(data)`, the prompt summary line,
    the strip grouped as prompt options → `pass`/`end turn` → activations,
    the refusal reason in the existing toast; `x` bound to cancel/skip.
18. `sync`/`layout`: a card annotated `hidden=<zone>` laid face down at that
    battlefield; badge glyphs for `stunned`, `attacker`, `defender`,
    `attached`.
18a. `net`: every host-side `session.intent` (drags, `send_intent`, the
    plugin panel's actions, counter nudges, exhaust toggles) goes through
    `Conns::deliver`, i.e. broadcast plus `owed_faces` — a host pass that
    resolves Abandon's Predict for the joiner, or a host EndTurn that draws
    for them, sends the `HostMsg::Faces` in the same frame instead of after
    the joiner's next intent (`Conns::private_faces` is the testable half).
    `plugin_ui::card_label` takes the viewer: a facedown card the viewer
    owns or was granted (`TableView.peeked`) is named in narration, prompt
    and affordance labels, "a face-down card" for everyone else.
19. kai-cli: `refused: <reason>` output, the prompt line, `act <n>` as an
    alias of `do <n>`; the `--ai` brain's system prompt and tool set for
    enforced mode (no `trash`/`counter`/`draw`/`exhaust`; answer prompts
    with `do`; attack by moving; respond with `play` or `do pass`).
20. Manifest hotkeys: `x` added; `q` dropped; the free-mode verbs stay
    bound since enforced mode refuses them with a reason.

Everything the plugin's hardened hash, the genesis pin, the
`modules/riftbound` version and kai's bundled wasm depend on advances with
each milestone; web and desktop deploy together per the kai wire-version
rule. Where the versions stand after M8: `WIRE_VERSION` 5 (4 from W9
through M7 — every new fact rode inside the fold, the ABI or the genesis
config; 5 at M8 because an M7 peer on an unpinned-engine table is not
otherwise refused, see item 7), `ENGINE_ABI_VERSION` 3 (2 at M7 for the
`hidden` flag and the reveal/peek effects, 3 at M8 for `LogState.options`),
`PLUGIN_ABI_VERSION` 0, the riftbound plugin crate 0.3.0 (0.2.2 from M0
through M7; the store's `modules/riftbound` and kai's bundled
`assets/plugins/riftbound.wasm` are rebuilt from it, and the store's
`engine.wasm` must be rebuilt for ABI 3 — until it is, kai falls back to
the native engine with a note), kai 0.9.0: the first kai bump since before
M0, so the one release carries rules-enforced play, the legal list, hidden
cards, equipment, the options lobby, greying and the soak — a minor by
kai's own convention (a new flow a player learns, what the release notes
would lead with), not the per-push patch.

## Milestones

Each is shippable behind the mode switch with its tests green;
`unit-test` in agni/agni and `cargo test -p agni-net --test riftbound_turns`
are the gates, plus the kai `plugin_ui` tests. M0–M8 have landed; the
commit under each heading is where its scope, tests and cards arrived, and
the section text below it is kept as the contract that commit met. M9 is
the next contract, written ahead of its batches.

### M0 — protocol groundwork and the CBOR blob

Landed: `b0ef3c1a` (2026-09-08).

Scope: `Verdict.reason` end to end; `Effect::Spawn.owner`; SDK `Action`
arms; `Snapshot.tokens/revealed/annotations`; `Snapshot::apply_entry`/
`apply` with the parity test; `Affordance.card`, `PromptSummary`, `{card N}`
in kai and kai-cli; the SDK `prompt` and `blob` helpers; `GameBlob` v4
replacing `TurnState` with identical free-mode behaviour (lobby roll, turn
and step, showdown, control slots); `SetMode` with the two start
affordances; `Reset` to a fresh lobby; the host `hidden_plays` fix; the
retired events deleted and every test that named them moved.
Files: `sim/src/{log,wire,abi}.rs`, `plugins/sdk/src/{decide,table,view,
prompt,blob}.rs`, `net/src/{host,proto}.rs`, `net/tests/riftbound_turns.rs`,
`games/riftbound-turns/src/{lib,state,present}.rs`, kai `src/table/
plugin_ui.rs`, `src/bin/kai_cli.rs`.
Tests: SDK round trips for every new `Action` arm, the reason, `card`/prompt
summary, blob encode/decode with unknown keys; agni-sim goldens unchanged
except the versioned regeneration for Spawn.owner; a log test that a Spawn
with `owner: Some(1)` inside a seat-0 entry yields owner 1; the projection
parity test over random move/annotate/counter/spawn/despawn sequences
including an entry-level Spawn before an effect Spawn; net: a refused move
carries its reason at both seats, byte-identical blobs; kai: `{card N}` with
a hidden card.
Cards unlocked: none (free mode identical to W9).

### M1 — the enforced skeleton

Landed: `b686206f` (2026-09-08).

Scope: `legal::classify` with the full refusal table; `Ctx` with draw,
channel, exhaust, ready, `move_unit`, `recall`, `kill` (no replacement yet),
`spawn`; automated Setup with the mulligan prompt (gesture and buttons);
Awaken → Beginning (Hold) → Channel → Draw → Action → EndTurn → Ending /
heal / Expiration inside one entry; plays from hand and champion with
location prompts; the payment planner over runes; units enter exhausted;
Accelerate as an optional cost; standard moves with the group-move prompt
and the two-other-seats cap; contest → staged showdown → `PickStaged` →
focus; pass-in-sequence closes; establish/Conquer/Hold/final point/winner;
the generic script; `present` rewritten (prompts, `pass`, `end turn`,
waiting lines, narration); the `FreeTable` panic button; the auto-pass;
kai-cli and AI prompt update.
Files: `games/riftbound-turns/src/engine/{ctx,legal,cost,pay,play,march,
phases,showdown,cleanup,prompts,present}.rs`, `cards/{mod,generic}.rs`.
Tests: one rules test per legality row with its reason; the phase walk
producing the exact effect list; mulligan by gesture and by buttons on both
seats; payment plans including `NoPowerOf`; contest/showdown/close/conquer/
final point; the group move stages one combat; net: two seats play three
turns on the hardened engine and plugin with a mulligan, a unit play, a
two-unit march, a showdown closed by passes, a conquer, a hold and a
refused trash; a kai-cli `--ai` smoke run reaching turn 3 without a refusal
loop.
Cards unlocked: every vanilla unit as a body (Ravenbloom Student, Plundering
Poro, Pickpocket, Unsung Hero, Scuttle Crab, Stellacorn Herder, Treasure
Hunter, Adaptatron, Tideturner as plain units), the runes, Rockfall Path.

### M2 — the chain

Landed: `026fc2f7` (2026-09-09).

Scope: pending/finalized items; `TargetSpec`/`Filter` evaluation with
`Untargetable` and 352.16; Deflect/`IgnoresDeflect` in cost with candidate
filtering and the refuse fallback; `Chosen` events; the Closed-state
priority ring with Reaction timing in Neutral and Showdown states; LIFO
resolution with cleanup and `triggers::collect`; `OrderTriggers`; this-turn
expiry with counter mirrors; cancel; damage and lethal-damage kills at
cleanup (no Deathknell yet); Legion.
Files: `engine/{chain,priority,triggers,expiry,targets}.rs`, `cards/`.
Tests: per-script unit tests on a fixture table (effects and prompts);
react to one's own spell; the pass ring; a countered spell keeps its cost
and fires no play trigger; a Reaction refused in Neutral Open from the
non-turn seat; a stale `Pick` refused; cancel returns the card; expiry
reverses the might mirror at Expiration; Frigid Jewel on the second draw of
either seat's turn once; net: Discipline countered by Defy across two seats
with prompts on both, identical blobs.
Cards unlocked: Discipline, Stupefy, Smoke Screen, En Garde, Consult the
Past, Decree of Insight, Singularity, Unchecked Power, Defy, Lilting
Lullaby, Not So Fast, Abandon (counter to hand; Predict in M7), Ravenbloom
Student, Abandoned Hall, Frigid Jewel, Defiant Dance, Star-Crossed, Rebuke.

### M3 — combat, stun and effect moves

Landed: `09b06f7b` (2026-09-09).

Scope: combat staging and the combat showdown with designations and the
initial-chain hook; might sums with stun and Assault/Shield; `Assign`
prompts with Tank/Backline ordering and the automatic path; simultaneous
damage; the Combat Special Cleanup 2a–2e; control after combat; recall of
attackers; `move_unit` for effects with contest marking and `MoveCause`.
Files: `engine/combat.rs`, `engine/cleanup.rs`, `cards/`.
Tests: lethal ordering (Tank first, Backline last, both); 5 damage against
four 3-Might units; a stunned unit contributes zero and needs full lethal;
attackers recalled when defenders survive; a sole survivor conquers; a
surviving defender re-establishes without scoring; Deathknell-less deaths
trash cards and despawn tokens; net: a full attack with damage prompts on
the attacker's seat and a conquer scored.
Cards unlocked: Back Off (from hand), Vex - Apathetic, Akali, Silent,
Charm, Ride The Wind, Stellacorn Herder, Treasure Hunter (Gold as a body),
Pyke's Backline, Scuttle Crab as a 0-Might unit with its play draw.

### M4 — tokens, Temporary, Deathknell, replacement, activations

Landed: `907ea76c` (2026-09-10).

Scope: Sprite and Gold spawns with mirrors and owners; Gold as a payment
source with `PayWith`; Temporary as a Beginning-phase kill ordered with Dusk
Rose Lab; the full kill path with `Noted` and Zhonya's replacement;
Deathknell queued before the card leaves; `Activate` affordances with
dynamic costs and `exhaust_self`.
Files: `engine/{kill,activate}.rs`, `cards/`.
Tests: Temporary kills before Hold scoring and can lose the hold; Sprite
Fountain's chain (Temporary kill → Deathknell → a Sprite that survives that
scoring); Zhonya's replaces a combat death and a Singularity death and
suppresses Deathknell and Pyke; Lillia's discount; Gold pays rainbow power;
Adaptatron's buff mirrored and shed on bounce; Pickpocket's "if you do";
net: a Sprite conquers and dies next turn with the owner right.
Cards unlocked: Lillia - Bashful Bloom, Lillia - Fae Fawn, Sprite Fountain,
Plundering Poro, Pickpocket, Adaptatron, Tomb-Raider Barbara, Pyke's Gold,
Unsung Hero, Dusk Rose Lab, Seat of Power, Sunken Temple, Targon's Peak.

### M5 — legal-play highlighting and target arrows

Landed: `bfd38162` (2026-09-10); its design was written ahead in `9ef5183b`.

Scope: the plugin's view tells every seat what is legal and what is aimed at
what, and kai draws it the way MTG Arena does.

- `PluginViewRequest.faces: BTreeMap<u32, CardFace>`: the advisory `view`
  call is the one place plugins.md hands the module the viewer's *own*
  hidden faces (`view(state, table, my_seat, my_faces)`), and M5 is where
  that argument arrives. `abi::own_faces` fills it from the replica's
  private knowledge — the host's dealer map, the client's own deals —
  filtered to cards of the asking seat that sit in a `ZoneVisibility::Owner`
  zone and carry no public face. `view::parse` overlays them onto the
  snapshot before the presenter runs. The fold is untouched, `decide` still
  has no hidden input, and the legal list is therefore replica-dependent by
  design: the host holds every seat's faces, a client holds only its own,
  and a replica that does not hold a seat's faces returns a subset of that
  seat's list. Without this the headline highlight could never fire, because
  a hand card carries no face in the public fold until the host's `Reveal`
  on the way out.
- `PluginView.legal: Vec<Legal { card, kinds }>` (the SDK view builder and
  `agni_sim::wire`, serde-defaulted and skipped when empty): for the viewing
  seat, every card it may act on right now and how — `Play` (a hand card it
  can afford and has a legal location for, with Accelerate noted), `March`
  (a ready unit with a legal destination; the destination list rides as
  `zones`), `Activate` (an ability whose cost it can pay), `React` (a hand
  card with Reaction timing while a chain item or showdown is open), and
  `Answer` (a card that is a candidate of the open prompt). The presenter
  derives it from `legal::classify`, `pay::affordable`, `march::
  legal_destination` and the prompt candidates, so the list is exactly what
  the engine would accept — never a second implementation of legality.
  `highlights` returns nothing once `ctx.winner()` is `Some`, because
  `decide` refuses every event but `FreeTable` after a win, and
  `present::enforced` drops its activation affordances there for the same
  reason. It builds one scratch `Ctx` for the whole call and probes only the
  destinations a card's own zone can reach (`legal::reachable`), so the view
  costs one snapshot clone rather than one per candidate pair; a busy board
  used to exhaust the plugin's gas budget and blank the whole HUD, since
  `plugin_view` decodes a fault as an empty view. A fault now renders as a
  status line instead of silence.
- `PluginView.arrows: Vec<Arrow { from: Origin, to: TargetRef, kind }>`:
  one arrow per target of every pending or chain item (spell → unit, ability
  → seat, counter → chain item), plus attacker → battlefield for a staged or
  open showdown and each combat designation (attacker ↔ defender) once M3's
  combat is staged. `Origin` is a card id or a chain item id; `kind` colours
  the arrow (`Spell`, `Ability`, `Attack`, `Counter`, `Combat`). Both seats
  receive the same arrows, so the defender sees what the attacker chose
  before priority passes.
- `PluginView.chain: Vec<ChainRow { item, card, seat }>` — the pending queue
  first, then the chain bottom to top, so the last rows are the panel's rows
  counted down from the top: the card each item stands for (a spell
  itself, an ability or trigger's source) and its controller. Nothing else
  on the wire maps an `Origin::Item` to anything a client can point at, so
  without it kai has to guess a panel row from the item id, and an arrow at
  a guessed row is worse than no arrow. `card` names the source even after
  it has left the table, which is what lets kai keep one fade key across the
  fold where the core switches `Origin::Card` to `Origin::Item`.
- `PluginView.turn`, `seats`, `waiting`, `narration`, `primary`, `hidden`
  (kai's UX milestone U11, serde-defaulted and skipped when empty so
  `WIRE_VERSION` stays at 5): the presenter says in fields what it says in
  status lines — `TurnInfo { number, seat, phase, phases, mode }` with the
  nine phase labels for a phase bar, `SeatInfo { seat, points, victory, xp,
  hand, deck, runes_ready, runes_total }` per seat counted from the
  snapshot, `Waiting { seat, what }`, the blob's log as `narration`, the
  index of the affordance a primary button fires (`end turn`, `pass`,
  `roll`/`reveal`, a prompt's `Answer::Done` option), and `hidden` — the
  indices of affordances that belong to a table menu rather than a strip:
  `concede` (`TurnEvent::Concede`, byte 14, recorded in `GameBlob.conceded`;
  the last seat standing wins by concession), `confirm free table` and
  `withdraw free table`. The proposer may withdraw by sending `FreeTable`
  again and a proposal expires when the turn advances
  (`engine::expire_free_table`); `Refusal::ConfirmPending` is gone,
  `AlreadyConceded` is new. `PluginView::shown()` lists the affordances
  that are not hidden; kai's strip, primary derivation, auto-pass and the
  kai-cli listing all go through it, which is how "confirm free table" left
  the AI's list. The old status lines keep being emitted for one release.
- kai (`table/highlight.rs`, `table/arrows.rs`): a card in `legal` gets a
  rim in the kind's colour (green play, blue march, amber activate, violet
  react, white answer) and the prompt's candidates pulse; hovering a
  legal-march unit tints its legal destination zones — only the viewer's own
  copy of a per-seat zone, since `highlights` probes with `to_seat = seat`
  and the wire's bare zone id therefore always means the acting seat's copy;
  a hand card that is legal to play lifts slightly. The rim palette is held
  a measured distance from `colors::SEAT_COLORS` and from the arrow tints,
  and the M0 affordance rim drawn just inside it is dashed, so the two rings
  are told apart by form as well as hue. Arrowheads that share a target fan
  out along the target's perpendicular instead of stacking on one pixel. Arrows are drawn as bezier curves from the
  source card's centre to the target card's centre (or the seat's plate, or
  the chain panel row) in screen space with an arrowhead, above the felt
  and below the HUD, keyed by `(from, to)` so they fade in and out rather
  than blink between folds. A card that is targeted by an enemy item gets a
  red rim. The existing M0 affordance-card rims stay for prompt options.
- kai-cli prints `legal:` lines (card, kinds, destinations) and `arrow:`
  lines; the AI brain's prompt lists them so a model no longer guesses at
  affordability.

Files: `sim/src/{abi,wire}.rs`, `net/src/{host,client}.rs`,
`plugins/sdk/src/{table,view}.rs`,
`games/riftbound-turns/src/present.rs` (+ `engine/legal.rs` helpers),
kai `src/table/{plugin_ui,highlight,arrows}.rs`, `src/bin/kai_cli.rs`,
`src/ai/brain.rs`.
Tests: SDK/wire round trips with unknown keys; the presenter's `legal` equals
the set of Moves `legal::classify` accepts on the same fixture, and its
`Activate` and `Answer` rows equal the `TurnEvent::Activate` and
`TurnEvent::Pick` events `decide` accepts (property tests over the fixture
tables, including a won game, a half-answered mulligan and a staged
showdown); arrows for a pending spell with two targets, a counterspell aimed
at a chain item, a staged showdown; net: the host holds a seat's hand faces
and highlights its plays while the other replica does not, and a forty-unit
board still answers `view` inside the plugin's gas budget; kai pure helpers
for arrow geometry (curve control point, arrowhead, head fan-out), rim
colour by kind against the seat and arrow palettes, the owner map over a
shared battlefield, and the chain item's anchor; kai-cli prints them.
Cards unlocked: none new; every scripted card becomes visibly playable.

### M6 — Hidden

Landed: `0239e128` (2026-09-10).

Scope: `Hide` with the [A] cost and the one-facedown rule; the `hidden`
annotation and kai's face-down placement; loss to trash at cleanup; granted
Reaction from the next turn; play from facedown for zero with restricted
targets and forced placement; `Origin::Facedown` provenance; voluntary
reveal; forced reveal on leaving to a private zone; the Hidden keyword read at
play time so a card hidden without it is stuck; a reveal refused for every seat
but the card's controller, at the plugin and at the host; a viewer-scoped
`HostSession::table_for` so neither seat's renderer reads the other's facedown
card; the AI's prompt and hide tool.
Files: `engine/hide.rs`, `engine/legal.rs`, `cards/{mod,generic}.rs`,
`net/src/host.rs`, kai `sync`/`layout`/`highlight`/`ai/brain.rs`.
Tests: hide refused without a held battlefield or with a facedown card
present; hide costs one rune; the facedown card trashed after losing
control; play from facedown refused on the hiding turn and accepted as a
Reaction for zero the next; targets outside the battlefield not offered;
net: hide, lose control, card in trash and revealed there; hide, react from
facedown during the opponent's chain; the other seat's reveal and its
`MoveHidden` both refused with the face still blank in both replicas afterwards;
neither seat's rendered table names the other's facedown card.
Cards unlocked: Pyke - Returned, Tideturner's Hidden half, Back Off from
facedown, Consult the Past hidden, Smoke and Mirrors and Switcheroo as
hidden-capable spells (swap semantics in M7), Zhonya's from hidden.

### M7 — equipment, swaps, Irelia, Hwei, information effects

Landed: `01ad26da` (2026-09-10).

Scope: attach/detach with `WhileAttached` grants (Boots → Ganking, Edge of
Night's attached text once confirmed); Equip activations; gear recall at
cleanup; the simultaneous swaps (Smoke and Mirrors conditioned on Temporary,
Tideturner, Switcheroo as two this-turn deltas from layered Might); Irelia's
`ChosenFriendly` optional trigger and Fervent's `Chosen`/`Readied`; Hwei's
gesture discard and typed branch; `Effect::Reveal`/`Effect::Peek` in agni-sim
and the host; Scuttle Crab's Deathknell (hand reveal, look bit, xp);
Abandon's Predict; the in-game roll; Burn Out; the mulligan recycle through
the roll; Ganking in march legality.
Files: `engine/{attach,roll}.rs`, `sim/src/log.rs`, `net/src/host.rs`,
`cards/`.
Tests: a swap contesting two battlefields stages two showdowns and the turn
player picks; Fae Fawn leaves Sprites at both origins; Switcheroo then
Stupefy composes; Irelia's legend readies a Discipline target and Fervent
gains +1 twice on a self-choose; Hwei's branch per discarded kind; agni-sim
tests for `owed_reveals`/`peeks`; the roll's permutation replica-identical;
net: Scuttle Crab dies and the opponent's hand is revealed to both seats,
Abandon's peek reaches only its seat and the other replica never holds the
face, a swap inside a showdown folds the same blob on both replicas.
Cards unlocked: Boots of Swiftness, Edge of Night, Smoke and Mirrors,
Switcheroo, Tideturner's trigger, Irelia - Blade Dancer, Irelia - Fervent,
Hwei - Brooding Painter, Scuttle Crab's Deathknell, Abandon's Predict.

### M8 — polish for play

Landed: `dce394d4` (2026-09-10), with riftbound plugin 0.3.0 and kai 0.9.0.

Scope: greyed unaffordable hand cards (`PluginViewRequest.faces` landed with
M5; the greying is derived from the legal list for the acting seat only, see
the rulings); `TableConfig.options` for the victory score and battlefield
count, folded into `LogState.options`, read by the plugin as
`rules::Options` and edited in kai's lobby with the sanctioned modes as
presets; the AI prompt and tool set naming every prompt answer, and the
self-play soak `kai-cli soak` — one `HostSession` and a `ClientSession`
replica per seat in one process, a seeded random brain or the NanoGPT brain
per deck, decks swapping seats every game, a JSONL record per game and a
summary that exits 1 on an `EngineFault` or a stuck game; the gas benchmark
of decide and view on a 150-card table under wasmi (`net/tests/gas_bench.rs`)
with the O(n²) board scans it found removed; blob size per entry; this
document finalised with the rulings; the plugin and kai version bumps.
What the soak surfaced and M8 fixed: `Legal.zones` was the union of the
face-up and the hide destinations, so both kai's hide drop target and the
random brain guessed which battlefield a hide could go to and were refused
where a facedown card already lay — the row now carries `hidden` separately;
and a `hidden` flag on a Move of a board unit or of the champion to base was
accepted as a face-up move while the fold stripped the face — `classify`
refuses both.
Files: kai `src/table/{highlight,zones,layout,sync,ui,arrows,mod}.rs`,
`src/ai/{brain,random,soak}.rs`, `src/bin/kai_cli/soak.rs`, `src/menu.rs`,
`src/net/mod.rs`, `Cargo.toml` (the plugin crate as a dev-dependency); agni
`sim/src/{log,abi,wire}.rs`, `net/src/proto.rs` (`WIRE_VERSION` 5 and the
`_v5` goldens), `plugins/sdk/src/{table,view}.rs`,
`games/riftbound/src/lib.rs`, `games/riftbound-turns/src/{rules,present,state}.rs`,
`engine/{ctx,cleanup,legal,triggers,kill,prompts}.rs`, `net/tests/gas_bench.rs`.
Tests: decide and view under 25% of the gas budget on the worst fixture
(Unchecked Power with twelve units and four Deathknells) — measured at 6.2%
and 6.6%; twenty self-play games without an `EngineFault` or a stuck prompt
(the bin's smoke test runs two; the integration ran 20 saved-deck games, 90
pool-deck games and 6 on the wasm engine with zero refusals of a legal-list
move); the legal-list parity test probes the `hidden` flag both ways on
every fixture and on a hidden card with two held battlefields; kai tests for
the affordability greying, the split hide destinations, the options lobby and
the 2-seat, 3-battlefield layout; the options round trip at three layers,
kai's `in_play` fallback pinned equal to the plugin's `Options::of` through
real request bytes, and a two-point, three-battlefield table played on the
hardened engine and plugin; the brain's prompt-kind coverage derived from
`PromptWhy::each()` and `prompts::answer_words` (kai takes the plugin crate
as a dev-dependency for it) instead of a hand-typed phrase list, and the
hide tool, victory-score and automatic-reveal wording pinned.

### M9 — the Vendetta and Origins mechanics

Not landed. This section is the contract for the four tournament decks
whose pool files sit unwired under `games/riftbound/rules/pool/` (see the
M9 pipeline below): Master Yi - Wuju Bladesman (Calm/Body), Nasus - Curator
of the Sands (Calm/Mind), Kha'Zix - Voidreaver (Body/Chaos) and the
competitive Lillia - Bashful Bloom list — 112 faces across the six pool
files, 60 of them new to the pool, sideboards included. It is written
against Core Rules 2026-07-16 (the Vendetta update, extracted beside the
v1.2 text in `games/riftbound/rules/`); every rule number in this section
and its rulings is from that edition, where the numbers M0–M8 cite have
moved (Deathknell 734 → 808, Combat 437–444 → 459–466, the Process of Play
350–356 → 353–359, Play 406 → 419, Recycle 403 → 416, Banish 414 → 427,
Kill 415 → 428). The landed vocabulary it extends is `cards/mod.rs`,
`cards/prelude.rs` and `engine/{ctx,state,kill,activate,hide,attach,
targets,triggers,cost,pay,legal,prompts,play,chain,phases,cleanup,combat}.rs`
as they stand after M8 (`BLOB_VERSION` 5, `ChainItem.spec_counts`/`subject`,
`FLAG_PAYING`/`FLAG_DISCARDED`, `Trigger::Attacks/Defends/Damaged`,
`Static::WhileAttached`, `Grant`, `SelfCost`, `Ability.candidates/question/
label/once`). The design was reviewed adversarially against that code
before it was written here; where the review moved a decision the paragraph
says so, and the section is landable batch by batch as the plan at the end
lays out.

One correction to the Phase 1 inventory before anything else: XP is not
missing from the engine. It is the seat counter `COUNTER_XP` (1) of the
manifest, kept by the fold like points, bumped by `Ctx::score_xp` (Scuttle
Crab), shown on kai's score window per seat already, public by construction
(729.2) and never a game object (731). M9 adds no `xp` field to
`SeatState`; it adds the readers, the spend path and the Level projection.

Scope: XP spent as a cost and read by `Level N`; `Hunt N`; `Empower`/
`Empowered`/disempower; `Ambush`; `Flow` with a Banishment zone; `Repeat`;
`Burn N`; extra turns; damage prevention; "alone" in its four readings;
"win a combat"; `Weaponmaster`; each-player sequencing; look at the top N;
reveal a hand and pick; pay-or-let-resolve; optional additional costs with
"if you do", on plays and on triggers; conditional and floating discounts;
the attach turn; healing a subset; move-restricting battlefields;
conditional continuous Might from legends, gear and battlefields;
multi-pick same-location targets; limited plays from the trash and from
Banishment; a control change; three tokens; and the Body rune as a fourth
pool domain. Everything is behind the same mode switch and free mode is
untouched.

**The versions M9 moves.**

| What | After M8 | After M9 | Why |
|---|---|---|---|
| `BLOB_VERSION` | 5 | 6 | `SeatState` (5 → 8 fields), `CardState` (8 → 11), `ChainItem` (+ `execution`, `awaiting`, slot-indexed `picks`), `Noted` (+ `alone`), `Origin` (tags 4–6), `PromptWhy` (tag 13), `When` (tag 2, `AfterKillsBy`), two new top-level keys `xt` and `pv`; every one is a fixed-arity array the reader rejects at the old length, so the bump is a reinterpretation, not a courtesy |
| riftbound plugin crate | 0.3.0 | 0.4.0 | the manifest grows a zone (`banishment`, id 13) and two tokens (Shadow Clone, Tentacle); the genesis pins the zone table, so the hash changes anyway |
| `WIRE_VERSION` | 5 | 5 | nothing in `ClientMsg`/`HostMsg` changes: the Flow play rides as a `TurnEvent::Activate` with an implicit ability index, the new prompt kind is inside the entry payload like tags 9–13 at M0, and the genesis carries the zone table it always carried; an M8 kai joining an M9 table lays the extra pile out from the decl and is refused by the plugin pin before any entry if it lacks the module — decide with rae before the push, since kai.rae.blue deploys only from main |
| `ENGINE_ABI_VERSION` / `PLUGIN_ABI_VERSION` | 3 / 0 | 3 / 0 | the fold, `Effect`, `PluginView` and the decide request are unchanged; `Legal`, `Arrow`, `ChainRow` gain nothing |
| kai | 0.9.0 | 0.10.0 | a minor by kai's own convention: six pinned decks, XP on the table, a banish pile, tokens a player has not seen, and Flow as a button on the trash |

**The vocabulary as it grows.** `Keyword` gains `Hunt(u8)` (code 18),
`Empower(Cost)` (19, not wire-carriable, like Equip and Repeat), `Flow(Cost)`
(20, not wire-carriable) and `Ambush` (21); `Repeat(Cost)` and
`Weaponmaster` already exist. `Static` gains `Level(u8, &'static [Grant])`,
`While(Applies, &'static [Grant])`, `Aura { scope: Scope, when: AuraWhen,
grants: &'static [Grant] }` with `Scope::{FriendlyUnits, UnitsHere}`,
`NoMoveToBase`, `AmbushIntoEnemies`, `SelfDiscount(Discount)`,
`SpellDiscount(ItemDiscount)` and `NoCombatDamageFrom(Suppresses)`, where
`Applies = fn(&Ctx, u32) -> bool` (the card), `AuraWhen = fn(&Ctx, u32, u32)
-> bool` (source, unit), `Discount = fn(&Ctx, u32, u8) -> Cost` (card,
seat), `ItemDiscount = fn(&Ctx, &ChainItem, u32) -> Cost` (item, source)
and `Suppresses = fn(&Ctx, u32, u32) -> bool` (source, unit). `Grant` gains
`Static(Static)` and `MightIf(fn(&Ctx, u32, u32) -> bool, i16)` (unit,
source). `Filter` gains `InTrash`, `InBanishment`, `InHand`, `MightAtMost(u8)`,
`Equipment`, `SameLocationAsPicks`, `MovableToBase`, `InCombatWith(&'static
Filter)`, `ChosenByEnemyItem(&'static Filter)` and `Kind(&'static str)`.
`Trigger` gains `Empowered`, `CombatWon(Who)`, `CombatLost(Who)`,
`Activated { of: Who }`, `YouPlayCard` and `UnitPlayedHere`. `Ability` gains
`xp: u8`, `burn: u8`, `usable: Option<fn(&Ctx, Source) -> bool>` and its
`once: bool` becomes `once: Once` with `Once::{Never, PerTurn,
PerSeatPerTurn}`; `prelude::once_each_turn` writes `Once::PerTurn` and the
two readers (`triggers::once_each_turn`, `activate::playable`) match on the
enum, so no M0–M8 script changes. `SelfCost` gains `BanishTarget` (the
trigger's first chosen target is banished as its cost). `Card` gains
`additional: Option<Cost>`. `cards::Cost` keeps its two fields so no script
literal changes; XP and Burn are ability fields (`prelude::spending_xp(
ability, n)`, `prelude::burning(ability, n)`) because only activations and
triggers spend them; engine-side `cost::Cost` gains `xp: u8` and `burn: u8`
and `Cost::label` prints "1 XP" and "burn 1" beside energy and power. The
implicit ability indices grow `IMPLICIT_HUNT` (`u8::MAX - 2`) and
`IMPLICIT_FLOW` (`u8::MAX - 3`) beside `IMPLICIT_TEMPORARY`.

`Token` gains `SandSoldier`, `ShadowClone` and `Tentacle`. `Origin` gains
`Trash { leave: Leave }` (tag 4, `Leave::{Banish, Recycle}` in the second
slot) and `Banishment` (tag 6). `Event` gains `Empowered { card }`,
`Disempowered { card }`, `CombatWon { zone, seat }`, `CombatLost { zone,
seat }`, `Activated { item, source, index, controller }`, `Burned { seat,
card }`, `Banished { card, owner }` and `TurnQueued { seat }`. `PromptWhy`
gains `PayOrLet { item, stage }` (tag 13, `PromptKind::Confirm`, addressed
to the payer). `When` gains `AfterKillsBy(u16)` (tag 2). `SeatState` gains
`cards_played: u8`, `spells_played: u8` and `next_discount: (u8, u8)`;
`CardState` gains `attached_turn: u16`, `controlled_by: Option<u8>` and
`control_source: Option<u32>`, and `CardState::is_default` learns all three
so a row whose only fact is a control change is never pruned by
`attach::sync` or `expiry`; `Noted` gains `alone: bool`; `ChainItem` gains
`execution: u8` and `awaiting: Vec<u32>`, and its `picks` become
slot-indexed (below). `GameBlob` gains `extra_turns: Vec<u8>` (key `xt`)
and `preventions: Vec<Prevention { source: DamageSource, value: Amount,
until: Expiry }>` (key `pv`) with `DamageSource::{SpellOrAbility, Combat,
Any}` and `Amount::{All, N(u8)}`. New flag bits: `FLAG_SHROUDED` (1 << 10,
"can't be chosen by enemy spells and abilities this turn", cleared with the
stuns at the Ending step) and `FLAG_ONCE_BY_SEAT` (bits 11–14, one per seat,
for `Once::PerSeatPerTurn`, cleared at Expiration). `Expiry` is unchanged.

`Ctx` gains `xp(seat)`, `spend_xp(seat, n)`, `hunt_value(card)`,
`banish(card)`, `burn(seat, n)`, `heal(card)`, `shroud(card)`,
`reveal_top(seat)`, `channel_exhausted(seat, n)`, `set_controller(card,
seat)`, `attached_turn(gear)`, `deals_combat_damage(unit)`,
`ambush_locations(seat, card)`, `queue_turn(seat)`, `prevent(source, value,
until)`, `ask_seat_resume(item, seat, stage, min, max)`, `ask_pay_or_let(
item, payer, cost, stage)`, `await_faces(item, cards)`, `score_effect(seat)`
and `contest(zone, controller)`; `controller(card)` reads
`CardState.controlled_by` before the face's owner. `current_might`,
`has_keyword`, `deflect_of` and `targets::untargetable` each add one line
that consults the statics projection. `cleanup::run` takes a second
argument, `last_item: Option<u16>`, that every one of its callers passes as
`None` until the chain and cleanup batches fill it in.

Zones: the manifest gains `ZONE_BANISHMENT` (id 13, name `banishment`,
`ZoneKind::Discard`, per seat, visibility All, `ZonePlace::Outer`, pile,
label "Banished"). `Zones::of` learns `banishment`. `ZoneKind::Discard`
means the sim sheds counters and annotations on entry and despawns tokens,
which is 124.1 and 186.1 for free.

**The play stages, re-cut for optional costs.** Today a spell begins at
`STAGE_TARGET` (16) and walks targets → `STAGE_PAY_WITH` → `STAGE_PAY`;
only permanents visit `STAGE_LOCATION` (0) and `STAGE_ACCELERATE` (1). The
review caught that Repeat and the additional-cost confirm cannot sit
"between Accelerate and pay-with" — a spell never passes there, and
`specs_of` doubles the specs for a paid Repeat, so the answer must precede
the targets (355.1.a puts the optional-cost choice first anyway). Two
stages are added: `STAGE_REPEAT` (4) and `STAGE_ADDITIONAL` (5). `begin`
starts a spell at `STAGE_REPEAT`; `STAGE_REPEAT` asks the Repeat confirm
when the card prints it and the printed cost plus the Repeat cost is
affordable, then moves to `STAGE_ADDITIONAL`; `STAGE_ADDITIONAL` asks the
additional-cost confirm when `Card.additional` is set and affordable, then
moves a spell to `STAGE_TARGET` and a permanent to `STAGE_PAY_WITH`.
`STAGE_ACCELERATE`'s fall-through goes to `STAGE_ADDITIONAL` instead of
`STAGE_PAY_WITH`, so a unit walks location → accelerate → additional →
pay-with → pay and Akshan's `[Body][Body]` is asked after Accelerate. The
trigger path's guard (`stage < STAGE_TARGET` and not pay or pay-with →
jump to `STAGE_TARGET`) already skips both new stages, and activations
begin at `STAGE_TARGET` as today; no ability in the pool prints Repeat.
`ChainItem.picks` becomes slot-indexed — `SLOT_ACCELERATE` 0,
`SLOT_TRIGGER_COST` 1, `SLOT_REPEAT` 2, `SLOT_ADDITIONAL` 3 — with
`ChainItem::new` filling four slots with `UNANSWERED` (`u8::MAX`), and
`choose_cost` writing by slot. The three landed readers move with it:
`base_of_item`'s accelerate check and `finalize`'s enter-ready check read
`picks[SLOT_ACCELERATE] == 1`, the `STAGE_ACCELERATE` gate reads
`UNANSWERED` where it read `picks.is_empty()`, and the trigger-cost reader
at `STAGE_PAY` reads `SLOT_TRIGGER_COST` — `UNANSWERED` is "not yet asked",
0 is "declined" — which the review flagged as the reader that would
otherwise have declined every trigger cost unasked. `targets::specs_of`
returns `Vec<TargetSpec>` (the struct is `Copy`) instead of a `&'static`
slice because a repeated item's specs are the script's doubled, and
`spec_of_index` takes a `&[TargetSpec]` and returns a `TargetSpec` by
value; the callers in `play.rs` (205, 257), `prompts.rs` (141, 293) and
`hide.rs` (164) are B0 hook lines that compile against the new signature
unchanged in meaning.

**`engine/statics.rs` — the projection of conditional grants.** Every
"while" in the new pool is a fact read at query time, never an event with
an expiry: Level while the controller has N XP, Brutalizer's +2 while it
was attached this turn, Master Yi's +2 while a friendly unit defends alone,
Forbidding Waste's −2 while a unit here defends alone. The engine has no
cached projection to rebuild (the rulings' "rebuilt after every XP change"
is automatic here), so the one place that answers "what does this card
have right now" is `statics::grants_on(ctx: &Ctx, card: u32) ->
Vec<Grant>`, which walks: the card's own `Static::Level(n, g)` (active while
`ctx.xp(ctx.controller(card)) >= n`, 824.1.c; a control change re-reads it
through the new controller, 824.1.c.1); its `Static::While(f, g)` (active
while `f(ctx, card)`); the *conditional* grants of every gear attached to it
— `Grant::MightIf(f, n)` while `f(ctx, card, gear)` and `Grant::Static`
through `attach::granted_statics` as today — and nothing else from
attachments, because `attach::attach` materializes a gear's plain
`Grant::Might` into `CardState.might` and `Grant::Keyword` into `granted`
with `Expiry::WhileAttached` (attach.rs 115–143) and the review showed the
draft's "walk every WhileAttached grant" would have counted Brutalizer's
+1, Boots' +2 and Edge of Night's +2 twice; and every in-play source
(legends, battlefields, permanents) with `Static::Aura { scope, when,
grants }` whose scope admits the card (`FriendlyUnits`: same controller as
the source; `UnitsHere`: same location as the battlefield) and whose
`when(ctx, source, card)` holds. `ctx.has_keyword` ORs `Grant::Keyword`s
from it (Level 6's Deflect and Ganking reach `deflect_of` and
`march::legal` on the next check after the sixth XP lands);
`current_might` adds Σ `Grant::Might`/`MightIf` from it in the same
increases-then-decreases pass as the stored mods, floored at zero;
`targets::untargetable` sees a granted `Untargetable`. Nothing in the
projection reads `current_might`, so there is no cycle: `alone_at` counts
units, the designation flags are bits, XP is a counter, `attached_turn` is
a state field. Vilemaw's "less Might than me" is deliberately not a grant:
it is `Static::NoCombatDamageFrom` read by `combat::might_sum` through
`ctx.deals_combat_damage(unit)`, which also folds the stun flag, so the one
Might comparison happens outside the projection. Dependent *triggered*
abilities need no static: Nasus, Ascended's "[Empowered] > When I conquer,
you score 1 point" is `when(on_conquer_me, |ctx, _, src| ctx.is_empowered(
src.card))`, evaluated against the projection after the event, which is
727.1.c.1 and, because the condition is read after the same event that
might have empowered it, 727.1.c.1.a. Pending items already finalized are
never re-examined when XP or the Empowered counter later drops
(727.1.c.3.a): their costs are paid. The M4 attach tests stay in
`attach.rs`; `statics.rs` tests only the conditional half. Tests: a fixture
unit with `Level(6, [Keyword(Deflect(1)), Keyword(Ganking)])` is neither
Deflect nor Ganking at 5 XP, both at 6, and neither again after a 2-XP
spend; a control-changed card reads the new controller's XP; a `While`
grant flips with its predicate inside one decide; an `Aura { FriendlyUnits
}` on a legend reaches only its controller's units and an `Aura { UnitsHere
}` on a battlefield reaches both seats' units at that battlefield and
nothing at base; `MightIf` on attached gear counts and stops counting when
`attached_turn` lags the turn while the stored +1 stays; a Boots wearer
reads Ganking once; `current_might` of a 2-Might unit under a −4 aura reads
0; the legal list and the presenter agree with `decide` on every fixture
(the M5 parity test runs unchanged over the new fixtures).

**XP as a resource: gain, spend, Level.** `Ctx::xp(seat)` reads
`COUNTER_XP`; `Ctx::score_xp` stays the gain (an immediate add during
resolution — Voidreaver's "gain 1 XP", Mutating Horror's 2, Scuttle Crab's
Deathknell, Alpha Strike's per-kill, Hunt); `Ctx::spend_xp(seat, n)` emits
`Effect::score(seat, COUNTER_XP, -n)`. `Ability.xp` is the spend written
into an ability's cost (`spending_xp(exhausting_self(activated(Sorcery,
Cost::FREE, targets, run)), 1)` for Voidreaver's Buff, 2 for his recall).
`cost::base_of_item` copies it into `cost::Cost.xp`; `pay::plan` refuses
with `Reason::NotEnoughXp` when `ctx.xp(seat) < cost.xp` (202, 205: an XP
spend with a linked effect is a cost, legal only while affordable);
`pay::pay` emits the spend beside the rune exhausts, so it lands at
finalization with the rest (357.2) and there is nothing to refund if the
item is cancelled before payment (358.5): the spend is emitted after the
plan is accepted. Where: `engine/ctx.rs`, `engine/cost.rs`, `engine/pay.rs`,
`engine/statics.rs`, `cards/prelude.rs`. Blob: nothing — XP is a table
counter. Wire and kai: the strip's scoreboard line grows a tail, `points ·
{seat 0} 2 · {seat 1} 0 · xp {seat 0} 1 · {seat 1} 3`, emitted only while
any seat's XP is nonzero so the M0–M8 status goldens stay byte-identical
(`present::scoreboard` and the enforced branch at present.rs 349). kai's
score window already shows the XP row; under rules enforced its nudges are
refused today, and M9 makes the row read-only there
(`counters_editable_on` knows the mode). `activate::label` prints the XP
in the affordance: `{card N}: buff a unit (1 XP, exhaust)`; the offer is
greyed, not hidden, while XP is short. kai-cli prints the same line.
Tests: `pay.rs` — a plan for `xp: 2` at 1 XP is `NotEnoughXp`, at 2 XP it
emits the −2 score after the rune exhausts; an activation greyed at 0 XP
becomes enabled after a Scuttle Crab Deathknell resolves; a cancelled
activation leaves XP untouched; `present.rs` — the xp tail appears exactly
when a seat has XP and the status slice without it equals M8's.

**Hunt N.** `Keyword::Hunt(n)` is sugar for one implicit triggered ability
with two triggers: `triggers::find_among` synthesizes a `Match { index:
IMPLICIT_HUNT }` for `Event::Conquered { units }` and `Event::Held { units
}` whenever `units` contains a source with a Hunt value (printed or
granted, summed across sources per 823.2 by `ctx.hunt_value(card)`), the
way it synthesizes `IMPLICIT_TEMPORARY` today; `targets::ability_of`
returns a static `HUNT_ABILITY` whose `run` is `ctx.score_xp(item.
controller, ctx.hunt_value(source))`, read at resolution so a Hunt granted
or lost while the trigger waits is honoured. The item walks the chain like
Seat of Power's conquer effect (383.4.c.2.a), ordered by its controller
among simultaneous triggers, once per battlefield per scoring (470): a
unit that conquers with Hunt 2 and holds next Beginning gains 2 then 2.
The XP goes to the unit's controller at resolution. Where:
`engine/triggers.rs`, `engine/targets.rs`, `engine/ctx.rs`. The presenter
names the item "{card N} hunt" in `OrderTriggers` and the chain rows.
Tests: `triggers.rs` — a Hunt 2 unit's conquer queues one item that scores
2 XP after priority passes, the Hold at the next Beginning scores 2 more,
an opponent's Not So Fast on the item leaves XP unchanged; two Hunt sources
on one unit sum; a unit at a battlefield it did not conquer this scoring
gets nothing.

**Empower, Empowered, disempower.** `Keyword::Empower(cost)` prints on the
card for the presenter; the behaviour is `prelude::empower(cost)`: an
`Activated(Sorcery)` ability with `SelfCost::Free`, no targets, `usable:
Some(|ctx, src| !ctx.is_empowered(src.card))` and `run = ctx.empower(
source)` — the Equip precedent (827.1, 827.1.c.1). `activate::playable`
evaluates `usable` after the existing checks and refuses with
`Reason::AlreadyEmpowered` (377.2.b), so the offer disappears from the
strip while counter 6 is set rather than greying. `Ctx::empower` sets
counter 6 and raises `Event::Empowered { card }` only when the counter
actually flipped (441.1.c); `disempower` clears it and raises
`Disempowered` only if it was set (442.1.a.1; Tomb-Raider Barbara's branch
is unchanged). `Trigger::Empowered` matches `Event::Empowered` for
`Who::Me` (Tail-Cloaked Matriarch, 828.1.d). The status is a board status:
the engine sheds counter 6 when the card leaves the board (124.1), and an
attached gear's counter never reaches its Top-Most card (719.4.a) because
`is_empowered` reads the card's own counter. Where: `cards/prelude.rs`,
`engine/activate.rs`, `engine/ctx.rs`, `engine/triggers.rs`. Blob:
nothing (counter 6 exists). kai: the Empowered badge already renders from
counter 6; the offer reads "{card N}: empower (8 energy)". Tests:
`activate.rs` — the offer exists while not empowered, is gone once
empowered, and a direct `Activate` is refused `AlreadyEmpowered`;
`ctx.rs` — `empower` twice raises one event; `disempower` on a
never-empowered card raises none; a Matriarch fixture's `Empowered`
trigger fires once per real flip.

**Ambush.** `Keyword::Ambush` adds locations, not a cost (822.1.c).
`Ctx::ambush_locations(seat, card)` returns every battlefield with at
least one unit the seat controls, plus — when the card's script has
`Static::AmbushIntoEnemies` (Rengar - Trophy Hunter, 822.1.d) — every
battlefield with enemy units, in both cases minus `NoUnitsPlayedHere`
battlefields (054.1: Rockfall Path forbids). `play_locations` is
unchanged for everything else. `legal::timing` becomes `timing_at(ctx,
seat, card, location: Option<Location>)`: with a location it treats a play
to an ambush battlefield as Reaction timing — Closed states, showdowns,
the opponent's turn (822.1.b) — and a play to base or to a held
battlefield without the seat's units under the card's own timing; with
`None` (the drag to the chain or the tap that `classify` checks before any
location is known, legal.rs 355–364) it passes when *any* location is
legal now, so a plain-Ambush unit dragged to the chain on the opponent's
turn is not refused `ClosedTiming` before it can name its battlefield. The
`PlayLocation` prompt's options (`prompts.rs`) and `choose_location`
filter the union of `play_locations` and `ambush_locations` by
`timing_at` for each location, so in a Closed state the prompt lists only
"ambush {zone N}" entries, and the one-option auto-pick applies as today.
`legal::play_to` accepts a unit dragged to an ambush battlefield it does
not hold. `STAGE_PAY` re-checks the chosen location with
`ambush_locations` before paying and cancels the play (card back to hand,
822.3 with 358.5) if the seat's units left it in the meantime.
`play::finalize` calls `ctx.contest(zone, seat)` for a unit that enters a
battlefield its seat does not hold — the review split this out of
`arrived`, which raises `Event::Moved` and would have fired every "when I
move" on a play — so Contested is marked with the ambusher as contester if
nobody had marked it (461) and the cleanup stages a showdown or a combat
with the ambusher as the attacker only when its units applied Contested
(464.2.c.1). An ambush into an open *combat* showdown gets its designation
at the next cleanup through `combat::refresh_open` (464.2.c.3.a), where
its attack or defend triggers are evaluated; an ambush into an open
non-combat showdown keeps the landed path — `showdown::stage` skips the
showdown's own zone, the showdown closes, `establish` returns `Combat`
and a combat is staged and opened afterwards — so the ambusher is
designated when that restaged combat opens, which is where its triggers
fire (the outcome 464.2.c.1 and 464.2.d describe; the 464.1 conversion of
a showdown into a combat is not built). Vex - Apathetic sees the `Played`
event as usual. Where: `engine/ctx.rs` (`ambush_locations`, `contest`),
`engine/legal.rs` (`play_to`, `timing_at`, and `highlights`: a `Legal {
kinds: [React], zones }` row for an Ambush unit in hand whose ambush list
is non-empty in a Closed or showdown state), `engine/play.rs`,
`engine/prompts.rs`. Blob: nothing. kai: the `Legal.zones` of an
ambushable unit include the enemy-held battlefields, so the drop target
lights them; the AI sees the location prompt's "ambush" labels. Tests:
`play.rs` — Rengar from hand onto a battlefield held by the other seat
with only enemy units while the opponent's spell is on the chain is
accepted, enters exhausted, marks Contested with Rengar's seat as
contester and stages a combat with him as the attacker after the chain
empties; Rengar ambushed into an open non-combat showdown is designated
when the restaged combat opens and his `Attacks` trigger fires then; the
same drag with a plain-Ambush unit is refused `NotHeld`; Mutating Horror
dragged to the chain on the opponent's turn is offered only its ambush
battlefields and enters there, to base on the opponent's turn refused
`ClosedTiming`; a replayed Stellacorn Herder raises no `Moved`; Charm
removing the friendly unit while the ambush is pending makes the pay
stage cancel and return the card; Rockfall Path is never listed; net — an
ambush during the other seat's chain folds the same blob on both replicas.

**Flow, limited plays from the trash and from Banishment, and the
Banishment zone.** Three origins share one mechanism: the play begins from
a zone other than hand, champion or a facedown battlefield, with a cost
and a leaving rule fixed by the origin. Every landed play of a real card
arrives as a host `Move` entry that already put the card in the chain
zone; these three do not, so `play::begin` emits the `Effect::Move` to the
chain zone itself for `Origin::Trash` and `Origin::Banishment` (and for
`Origin::Hand` when the card is not already there), which is what kai's
chain, `Filter::ItemOnChain` and `chain::finish`'s `held.zone == chain`
guard read. `play::cancel` gains arms that return a trash-origin card to
the trash and a banishment-origin card to Banishment, and the limited
plays' own prompts are opened non-cancellable — the "may" was answered at
the `Resume` that picked the card.

- `Keyword::Flow(cost)` (829.1). `activate::offers` lists, for each spell
  in the seat's trash whose script prints Flow, an implicit offer `{card N}:
  play from your trash (4 energy)` with `index: IMPLICIT_FLOW`;
  `activate::activate` with that index routes to `play::begin(ctx, seat,
  card, Origin::Trash { leave: Leave::Banish }, None)` instead of an
  ability item, so kai's affordance click and `TurnEvent::Activate` carry
  it without a new event tag, and a drag of the card from the trash to the
  chain (`legal::classify` sends a trash-origin Move of a Flow spell to
  `from_trash` instead of `TrashIsFinal`) is the same intent. Timing is
  the spell's own (`timing_at` reads the card's keywords as from hand:
  Onslaught, Twilight Shroud and Up from the Deep are Sorcery, 829.1.b.2).
  `cost::base_of_item` returns `of_script(flow cost)` for `Origin::Trash {
  Banish }` in place of the printed cost, before Deflect and discounts
  (829.1.c.1, 356.1.a); `Filter::EnergyAtMost` keeps reading the printed
  face (206), so Defy still counters an Onslaught played for Flow. The
  trash is public, so the offer is on the strip for its owner only and
  `Legal { card, kinds: [Play] }` names the trash card for the rim.
- Fizz - Trickster's "play a spell from your trash with Energy cost no
  more than [3], ignoring its Energy cost. Recycle that spell after you
  play it" is `Origin::Trash { leave: Leave::Recycle }`: `base_of_item`
  returns the printed power needs with energy zero (356.1.b.2). The
  candidate is picked at resolution through a `Resume` over `And[Spell,
  InTrash, Friendly, EnergyAtMost(3)]` (419.3.c: no candidate, nothing
  happens); the pick calls `play::begin` and the pending item's own
  prompts follow inside the same proceed loop, which is how a limited play
  "follows all the steps" (419.3.b). A Flow spell played through Fizz is
  recycled, not banished: it was not played for its Flow cost.
- `Origin::Banishment` (Temporal Breach): base cost free, Accelerate still
  offered (356.1.b.3), played by the *owner* (the item's controller is the
  owner, not Temporal Breach's controller), entering exhausted, `Played`
  raised so play triggers fire again as for a new object (124). A play
  "ignoring any and all costs" (356.5.a, Bone Skewer) goes through
  `play::begin_ignoring_any_and_all_costs`: the same Banishment origin with
  the Accelerate, Repeat and additional-cost slots pre-declined, so no
  optional-cost prompt opens for the playing seat and the play lands inside
  the caller's resolution — which is what lets Bone Skewer stun the unit on
  entry instead of losing the stun to a paused pay stage. The
  location is fixed by the effect — "plays it to the same location" — and
  355.2.b makes a location an effect names valid whether or not the owner
  holds it, so the replay happens at the unit's last location whenever
  that is not a `NoUnitsPlayedHere` battlefield (Rockfall Path, 054.1:
  "can't" beats "may"); `finalize` calls `contest` if the owner does not
  hold it and the next cleanup stages the showdown or combat. The draft's
  "a battlefield the owner neither holds nor has units at" branch is
  dropped: the review showed it would have made the card's ordinary use —
  a Hidden reaction that banishes a lone attacker mid-showdown — a
  permanent removal for two energy, which neither the text nor 355.2
  supports. The three-player redirect of 462.2.a is out of scope.
- Tail-Cloaked Matriarch's "Play it to your base, ignoring its cost" is
  `Origin::Trash { leave: Leave::Recycle }` with a unit: it leaves the
  chain to the base at finalize like every permanent, so `leave` is moot.

`chain::finish` reads the origin when the spell leaves: `Leave::Banish` →
`ctx.banish(card)`, `Leave::Recycle` → `ctx.recycle_to_bottom(card)`, else
the trash; `ctx.counter_item` does the same for the Defy, Hard Bargain and
Crumbling Sands cases (829.1.b.1, 390.3.a). A spell whose own execution
already moved it off the chain (Time Warp's "Banish this") is left where
its execution put it, as the `held.zone == chain` guard already arranges.
`Ctx::banish(card)`: a card on the board first detaches every gear
attached to it (719.5, recalled by the next `attach::sync`), is detached
itself if it is attached gear, has its designation cleared and its
`CardState` dropped, and is not killed — no Deathknell, no `Died`, no
Zhonya's (427.2.a); a token is despawned instead (186.1); any other card
moves to its owner's `banishment` zone at TOP with a `hide::reveal_before`
so a facedown card becomes public on the way (427.2). It raises
`Event::Banished` and narrates; nothing in the pool triggers on it.
Sources: Flow spells, Time Warp, the Shadow Clone's cost, Temporal Breach.
Nothing returns cards from Banishment except Temporal Breach's own replay.
Where: `engine/ctx.rs`, `engine/activate.rs`, `engine/legal.rs`,
`engine/cost.rs`, `engine/play.rs`, `engine/chain.rs`,
`games/riftbound/src/lib.rs`. Blob: `Origin` tags 4 and 6. kai lays the
pile beside the trash from its decl (`ZonePlace::Outer`, pile, label
"Banished · n") and `zones::offstage_decl` must stop assuming one
per-seat offstage zone (it finds the sideboard by place; Banishment is
Outer, so it is not caught, but the test that pins "one offstage" should
say so). The trash pile needs no browse for Flow: the affordance button is
the play. Tests: `activate.rs` — Onslaught in the trash offers at 4 energy
on its owner's turn in Neutral Open, not on the opponent's turn, not for
the other seat, enabled only when affordable; `play.rs` — the Flow play
moves the card onto the chain, pays [4] not the printed cost, Sandswept
Tomb still discounts it, the resolved spell lands in Banishment and the
trash count is unchanged; a cancel at its target prompt returns it to the
trash; Defy counters it into Banishment; Fizz's pick plays Stacked Deck
for its power only and recycles it after; Temporal Breach on a unit at a
held battlefield banishes it, its owner replays it there exhausted with
its play trigger firing again and Vex stunning it if the owner is Vex's
opponent; the same on a lone attacker mid-showdown replays it at the
contested battlefield and it is designated when the combat is restaged;
the same with Rockfall Path as the location leaves the unit banished;
`ctx.rs` — banish of a unit wearing Brutalizer detaches the gear, fires no
Deathknell and sheds its counters; banish of a Sprite despawns; net — a
Flow play from the trash folds identically on both replicas and the
banish pile shows the card face up.

**Repeat.** `Keyword::Repeat(cost)` is the second optional cost beside
Accelerate (820.1), asked at `STAGE_REPEAT` above ("repeat {card N} for 1
energy and 1 Mind power?") with the answer in `picks[SLOT_REPEAT]`. When
Repeat is paid the choices step runs every `TargetSpec` twice:
`targets::specs_of` returns the script's specs doubled for a repeated
item, each with its own `spec_counts` entry, so the second group is chosen
at the usual time with its own full choice (820.2, 820.2.a: the same spell
twice is legal for Hard Bargain, two different sets for Bellows Breath).
`cost::of_item` adds Deflect for every chosen enemy permanent across both
groups. At resolution `chain::run` calls the script once per execution:
`ChainItem.execution` (0, then 1) selects the target group that
`card_target`/`item_target`/`of_spec` read, so scripts stay unaware of
Repeat; a script that asks mid-resolution parks with its `execution`
intact and resumes into the same group. A group whose targets are all
illegal at resolution executes nothing while the other still runs
(359.3.e.1). `PlayedSpell` fires once (820.3.a); `Filter::EnergyAtMost`
reads the printed cost (206), so Defy is unaffected by a paid Repeat.
Where: `engine/play.rs`, `engine/targets.rs`, `engine/chain.rs`,
`engine/cost.rs`, `engine/prompts.rs`. Blob: `ChainItem.execution`. kai:
one more `OptionalCost` phrasing; the AI's `answer_words` stay "yes or
no". Tests: `play.rs` — Bellows Breath with Repeat paid asks the Repeat
confirm before any target, then two `Target` prompts of up to three units,
and deals 1 twice to the second group's units only when they differ, once
each to units in both; Repeat declined asks one; an unaffordable Repeat is
not offered; `chain.rs` — a countered repeated spell resolves nothing and
fires no `PlayedSpell`; Hard Bargain repeated counters two spells or the
same spell twice (the second execution finds no target and does nothing).

**Optional additional costs and "if you do", on plays and on triggers.**
"As you play this, you may pay X as an additional cost" (Rampage: [Body];
Akshan: [Body][Body]) is not a keyword the presenter prints, so it is not
a `Keyword` arm; it is `Card.additional: Option<Cost>` set by
`prelude::with_additional(card, cost)` and asked at `STAGE_ADDITIONAL`
("pay 1 Body power as an additional cost for {card N}?") when the printed
cost plus it is affordable. The answer is `picks[SLOT_ADDITIONAL]`, read by
the script through `prelude::paid_additional(item)` for "If you paid the
additional cost" (Rampage's +2, Akshan's steal condition) — the 355.1.a
choice-step reading, and 356.4.f.1's: paid means chosen, whatever a
discount then made of it. Deflect and discounts apply over the summed
cost. The trigger shape "you may [self cost] to [effect]" had no
vocabulary in the draft, which the review caught on Nasus - Curator of the
Sands ("you may exhaust me to ready up to 2 runes": a trigger whose only
cost is the exhaust, which `pay_self` would have paid silently) and on the
Shadow Clone ("you may banish a unit from your trash. If you do, give me
[Assault 4]"). `Ability.optional` is now read at `STAGE_PAY`: a trigger
with `optional: true` and a cost that is not `SelfCost::Auto` — an exhaust,
a `KillSelf`, a `BanishTarget`, an XP or Burn need, energy or power — asks
`OptionalCost { item, cost: SLOT_TRIGGER_COST }` before paying ("exhaust
{card N} to ready up to 2 runes?", "banish {card M} for the {card N}
trigger?"); declining removes the trigger as "its cost is declined",
accepting pays at finalization before anyone can respond (383.3.b.1). A
trigger with `optional: true` and an `Auto` cost keeps the landed 0..1
target reading (Pickpocket). `SelfCost::BanishTarget` is paid by
`activate::pay_self` banishing the item's first chosen target, checked
payable while that card is still in the trash; the Shadow Clone's trigger
targets `And[Unit, InTrash, Friendly]` with `min: 1`, so with an empty
trash it is dropped as "no legal target" (390.2) and never asks. Where:
`cards/mod.rs`, `engine/play.rs`, `engine/activate.rs`, `engine/cost.rs`,
`engine/prompts.rs`. Blob: nothing beyond the slot layout. Tests: Rampage
with the Body power paid gives +2 before the mutual damage, unpaid does
not; the confirm is skipped when no Body rune can be recycled; Akshan
without the payment steals nothing; Nasus - Curator asks after Astral
Heron's play and after Nasus, Ascended's Empower, yes exhausts him and
readies the two chosen runes, no leaves him ready, a 6-cost card asks
nothing; the Shadow Clone's attack trigger asks with a unit in the trash,
yes banishes it at finalization and the Clone reads Assault 4 in
`combat_might` this combat and not next turn, an empty trash asks nothing.

**Burn N.** `Ctx::burn(seat, n)` moves the top `n` cards of the seat's
main deck to its trash one at a time, raising `Event::Burned { seat, card
}` per card and narrating "{seat N} burns k". Burned cards are neither
discarded nor killed (440): no discard or death watcher fires; a Flow
spell burned into the trash is playable there next. `Ability.burn` is a
cost: Shadow Order Disciple's "When I move, you may [Burn 1] to give me +1
Might this turn" is `burning(optional(on_move(...)), 1)`, asked by the
trigger-cost confirm above ("burn 1 for the {card N} trigger?"), burned at
finalization before anyone can respond (383.3.b.1), with the Might
arriving on resolution. Ruling: a Burn cost is payable only while the main
deck holds at least `n` cards; with fewer the trigger is removed as "its
cost can't be paid" rather than burning out mid-cost — 440.4 governs Burn
as an effect, and 431's Burn Out stays with the M7 roll flow (no card in
the four decks burns as an effect). Where: `engine/ctx.rs`,
`engine/cost.rs`/`pay.rs`, `engine/play.rs`. Blob: nothing. Tests:
`ctx.rs` — burn 1 moves the top card face up into the trash and raises one
`Burned`; `play.rs` — Disciple's move trigger asks, yes burns then grants
+1 at resolution, no removes it, an empty deck removes it without asking.

**Extra turns (Time Warp).** `GameBlob.extra_turns` is a queue of seats
(`xt`, omitted when empty). `Ctx::queue_turn(seat)` pushes and raises
`TurnQueued`. `phases::finish_turn`, after `heal_all`, `at_expiration` and
the rune-pool emptying it already does, pops the front instead of calling
`TurnCore::advance` when the queue is non-empty: `turn += 1`, `player =
seat`, and the rotation is untouched because `TurnOrder::next_seat` is a
function of the current player (737: after A's extra turn the next
advance yields B). An extra turn is a whole turn through `start_turn`:
Awaken, Beginning with Hold scoring (a held battlefield scores again),
Channel, Draw, Action, Ending; "this turn" effects expired at the real
turn's Expiration do not survive into it, `SeatState.reset_turn` runs,
once-per-turn flags clear. Two Time Warps queue two turns in resolution
order (738). `rules::runes_this_turn` keeps reading the turn index (the
second player's extra rune is `turn == players`); an extra turn cannot
precede turn `players` with a 10-cost spell, recorded rather than guarded.
Time Warp's "Banish this" is its own execution: `ctx.banish(self)` at
resolution, so a countered Time Warp goes to the trash. Where:
`engine/phases.rs`, `engine/ctx.rs`, `state.rs`. Blob: `xt`. kai: the
turn line already names the turn player; the presenter narrates "{seat N}
takes an extra turn" when the queue is consumed. Tests: `phases.rs` — with
`[0]` queued, `EndTurn` by seat 0 starts turn n+1 for seat 0 and the
following `EndTurn` starts n+2 for seat 1; a held battlefield scores on
both; a this-turn Might mod from the real turn is gone in the extra one;
two queued turns run in order; net — Time Warp resolves and both replicas
agree on the turn player.

**Damage prevention as a replacement.** `GameBlob.preventions` (`pv`)
holds `Prevention { source, value, until }`. `Ctx::prevent(source, value,
until)` appends. `Ctx::damage(card, n, cause)` consults `prevent::amount(
ctx, card, n, cause)` before marking anything: a `Prevention` whose
`source` matches the cause (`SpellOrAbility` matches `Cause::Item`,
`Cause::Ability` and a cleanup attributed to an item — Alpha Strike's and
Rampage's unit-dealt damage are attributed to the spell, 411.5; `Combat`
matches `Cause::Combat`) reduces `n` by its value, `All` to zero; a zero
result marks nothing and raises no `DamageDealt` (437.4).
`expiry::at_expiration` drops entries whose `until` is this turn.
`combat::lethal` and `combat::candidates` read `prevent::amount(ctx, unit,
lethal, Cause::Combat)`: a unit whose combat damage is wholly prevented is
exempt from mandatory assignment and never lethal (437.5.b, 465.2.c.10).
Unyielding Spirit is the only source in the four decks, with
`SpellOrAbility`, `All` and `EndOfTurn(turn)`; the numeric countdown of
437.3 is carried on the entry (`Amount::N` decremented per prevented
point) but exercised only by `All`. Where: `engine/prevent.rs` (new),
`engine/ctx.rs`, `engine/expiry.rs`, `engine/combat.rs`. Blob: `pv`. kai:
narration "{card N}: 6 damage prevented". Tests: `prevent.rs` —
Singularity after Unyielding Spirit marks nothing and raises no
`DamageDealt`, so a `Damaged` trigger and Zhonya's are silent; combat
damage in the same turn is dealt; the entry is gone next turn; Alpha
Strike under prevention kills nothing and grants no XP; a numeric `N(3)`
entry reduces 5 to 2 and is spent.

**"Alone", the four readings.** `Ctx::alone_at(unit)` is unchanged: no
other unit with the same controller at the unit's location, counting
tokens, stunned and exhausted units, not counting gear or the legend
(740.2.a). "While a friendly unit defends alone, it gets +2" (Master Yi -
Wuju Bladesman) and "While a unit here is defending alone, it has −2"
(Forbidding Waste) are `Static::Aura { scope: FriendlyUnits | UnitsHere,
when: |ctx, _, unit| ctx.is_defender(unit) && ctx.alone_at(unit), grants:
[Might(±2)] }`: on at the cleanup that assigns the designation, off when a
second friendly unit arrives or the designation clears at combat end
(466.7.a), on again if a friendly death leaves the unit alone mid-combat,
and read by combat damage because `might_sum` uses `current_might`
(465.2). "When a friendly unit attacks or defends alone" (Mask of
Foresight) and "When I attack or defend, if an enemy unit is alone here"
(Kha'Zix - Mutating Horror) are `Trigger::Attacks(Who::Friendly | Me)` /
`Defends(...)` with a `condition` reading the subject at the cleanup that
raised the event: `alone_at(subject)`, or "exactly one enemy unit at the
subject's location" (`prelude::enemy_alone_at`); evaluated once, when the
unit first gains its designation this combat, never again (383.4.e.2.b,
f.2.b). Fiora - Peerless's "one on one" is `alone_at(me) &&
enemy_alone_at(me)` at that moment (740.2.b), and "double my Might this
combat" is `ctx.might(me, current_might(me) as i16, Expiry::CombatEnd,
None, item)`, the existing combat-end expiry (doubling is additive, 432).
"[Deathknell] — If I died alone" (Lonely Poro) reads `Noted.alone`, taken
by `kill::plan` while the whole dying batch is still on the board
(808.1.d.3, 323.4): a Poro dying in the same cleanup as another friendly
unit at that battlefield did not die alone; one that was the last friendly
unit standing when it took lethal damage did — the snapshot reading the
rulings record as a choice. Where: `engine/statics.rs`, `engine/kill.rs`,
`state.rs`, `cards/prelude.rs` (`enemy_alone_at`, `died_alone(item)`).
Blob: `Noted.alone`. Tests: `statics.rs` (the aura cases); `combat.rs` — a
lone defender under Master Yi deals +2 in the damage step and a second
friendly arrival mid-showdown removes it before damage; Forbidding Waste
drops a lone defender to zero and it dies to 1 damage; `kill.rs` — two
Poros dying in one batch draw nothing, a Poro dying beside a Sprite that
survives draws nothing, a Poro alone draws one; Mutating Horror ambushing
against one enemy gains +2 and 2 XP once and nothing when a second enemy
arrives later or when it faced two.

**"Win a combat" (Kha'Zix - Voidreaver).** 466.3 defines the result as
the seat that received a designation and is the only one with units at
the battlefield "during this step", and 466.2 asks for the combat
cleanup's chain items to resolve first. The draft deferred the
determination until the cleanup's Deathknells were off the chain; the
review showed that `showdown::owed` is a same-decide gate (pending queue,
unraised deaths, uncollected events), false as soon as the Deathknell
items reach the chain and not when they resolve, so waiting would need
blob state carrying the zone and both designated seats across decides
(the loser is not derivable from designations once its units are dead)
and would move `establish` — and with it the conquer, Hunt and Seat of
Power triggers — out of the batch they share with the Deathknells today,
re-baselining M4's kill tests and goldens. M9 takes the other branch:
`cleanup::after_combat` determines the result in-frame, right after the
lethal kills and the step-3d recall and before `establish`, and raises
`CombatWon { zone, seat }` and `CombatLost` when exactly one of the two
designated seats has units at the battlefield; both present, both gone,
or attackers recalled (466.3.d) raise nothing. So a defender whose
attackers all died or were Charmed away wins without scoring a point, an
attacker whose defenders died wins and then conquers, and Voidreaver's
trigger is collected in the same batch as the Deathknells and the conquer
triggers of that combat, ordered by its controller among them (383.3.d)
rather than strictly before them. Deviation from 466.2, recorded in the
rulings: a side whose last unit dies to a Deathknell or a reaction in the
cleanup window has already lost or won by the time it resolves; no card in
the four decks reads the difference. `Trigger::CombatWon(Who::You)`
matches for the seat's legend and permanents once per combat. Where:
`engine/cleanup.rs` (`after_combat`), `engine/combat.rs` (`result`),
`engine/triggers.rs`. Blob: nothing. kai: narration "{seat N} wins the
combat at {zone N}". Tests: `combat.rs` — attackers die → defender's
Voidreaver gains 1 XP and no point; defenders die → attacker gains 1 XP
and conquers; both survive → no event, attackers recalled; both die → no
event; Charm during the showdown moving the only attacker away → the
defender wins with no damage step; M4's "a showdown waits until the
deaths" test and the kill goldens are unchanged.

**Weaponmaster.** `prelude::weaponmaster()` is an optional `Trigger::Play`
ability with one `TargetSpec { filter: And[Gear, Friendly, Equipment],
min: 0, max: 1, label: "an Equipment you control to attach" }`, chosen at
finalization like every play trigger (821.1.c). `Filter::Equipment` is
"the script prints `Equip(cost)`" — `CardInfo` carries no tags, so
Equipment-ness lives in the script, where `Card::equip_cost` already reads
it. At resolution the script builds the gear's Equip cost with one
`Power::Rainbow` need struck if the cost contains one (821.1.c.3: "If the
chosen card's Equip cost does not contain [A], it can still be paid, but
will not be reduced" — Brutalizer's `[Calm]` is paid in full; the
Weaponmaster reduction is the one place M9 keeps that reading, see the
discounts below), plans and pays it with `pay::plan`/`pay::pay` from the
controller's runes at that moment, and calls `attach_gear`, which detaches
the gear from its previous wearer first (434.1.f). If the plan fails the
gear stays where it is (821.1.c.5). It is not an Equip activation: no
`Chosen` for the wearer, no Deflect on the wearer, nothing that watches
"equip" fires (821.1.c.6). Akshan - Mischievous is the only carrier; his
steal trigger and this one are placed simultaneously and both choose at
finalization, so Weaponmaster cannot pick a gear the steal has not taken
yet, and the steal attaches an Equipment itself. `Keyword::Weaponmaster`
stays printed on the card for the presenter. Where: `cards/prelude.rs`,
`engine/targets.rs`, `engine/attach.rs` (nothing new). Tests: a fixture
Equipment with `Equip [1][A]` is attached for [1]; one with `Equip [Calm]`
is attached only when a Calm rune can be recycled and stays put otherwise;
a gear already on another unit moves; no Equipment on the board → the
trigger is dropped silently (390.2).

**"Each player chooses", in turn order.** `Ctx::ask_seat_resume(item,
seat, stage, min, max)` opens a `Resume` prompt addressed to `seat` rather
than the item's controller; `prompts::answer` already checks
`prompt.seat`, and `chain::resume` already runs the script with the picks.
Acceptable Losses ("Each player kills one of their gear") targets nothing
(355.10.e); its script walks the seats from the turn player in turn order
(303.2.a), one `Resume` at a time over that seat's gear with the earlier
kills already applied and visible, skipping a seat with no gear silently
(055), and each kill carries `Cause::Item` with the choosing seat as the
killer for "when you kill" purposes (411.1 example). The Deathknells
collect after the spell finishes and are ordered by their controllers
(383.3.d) as today. Sabotage, Decree of Strength and Scuttle Crab's
Deathknell are the other shape: "Choose an opponent" is a
`TargetKind::Seat` target at finalization (355.9.a); the chooser, not the
opponent, picks from the revealed hand. Where: `engine/ctx.rs`,
`engine/prompts.rs` (status "{seat N}: choose <question>" when the
prompted seat is not the controller). Blob: nothing. Tests: Acceptable
Losses with gear on both sides asks the turn player first, then the other
seat, and both gears die with their Deathknells queued after; a seat
without gear is not asked; an opponent's stale `Pick` on the first prompt
is refused `NotYourPrompt`.

**Alpha Strike's split.** "It deals damage equal to its Might split among
enemy units at battlefields" needs amounts per target, and the SDK
`Answer` has no number (Card, Zone, Seat, Item, Yes, No, Done, Skip,
Cancel). The split is therefore one point at a time: a `Resume` "deal 1
to" over enemy units at battlefields, repeated `current_might` times with
no `done` until every point is placed (055 stops it early when no
candidate remains), the status line reading "{card N}: 3 of 5 to place",
each point dealt as `Cause::Item` from the chosen unit. Kills come at the
cleanup after the item, attributed to it: `chain::finish` passes the
resolved item's id into `cleanup::run(ctx, Some(item))`, the batch's
`Cause::Cleanup { last_item }` carries it, and the delayed effect the
script registered at resolution — `Delayed { when: When::AfterKillsBy(
item), .. }` — fires once after that cleanup with the count of `Died`
events it attributed, scoring that much XP ("Then for each unit this
kills, do this: Gain 1 XP"). Tests: 5 Might split 3 + 2 through five
prompts kills two units and the seat gains 2 XP after the cleanup; a
10-Might Yi asks ten times; under Unyielding Spirit nothing is marked and
no XP is gained.

**Look at the top N and pick (Stacked Deck).** `Ctx::peek(card, seat)`
for each of `top_of(main_deck, seat, 3)` gives the controller the faces
(128.4; the cards stay in the deck, 431.1.c); the script asks a `Resume`
whose candidates are those card ids, and the presenter labels them `{card
N}` — kai expands a peeked face for the peeker and "a hidden card" for the
other seat, which sees the prompt exists and its count and nothing else.
The pick is a put, not a draw (`Effect::Move` to hand at TOP with no
`Drew`, no draw count); the rest go to the bottom by `recycle_to_bottom`
in the order the controller listed them — the deviation from 416.5's
random order the rulings record. Fewer than three cards: the prompt shows
what there is; an empty deck: the spell does nothing. Where:
`cards/stacked_deck.rs` on existing primitives. Tests: three peeks reach
only the controller, the pick lands in hand without `Drew`, the two others
are at the deck's bottom in the listed order, and Frigid Jewel's draw
count is unmoved; with two cards the prompt offers two.

**Reveal an opponent's hand and pick (Sabotage, Decree of Strength).** The
plugin cannot read a hand until the host pays the reveal, and it needs the
kinds and domains to filter "a non-unit card" and "a Mind card". So the
parked-item continuation the spec reserved as `await_face` lands here:
`Ctx::await_faces(item, cards)` records the ids on `ChainItem.awaiting`
and leaves the item `Resolving`; the host's `reveal_surfaced` pays each
debt with a `Reveal` entry from the owner (a hand is owner-visible, so it
is paid at once, one entry per card, and the cards are `shown` in place);
`decide`'s `Action::Reveal` arm calls `chain::face_arrived(ctx, card)`,
which removes the id from `awaiting` and, when the list is empty, resumes
the item at its recorded stage — the generalisation of
`discard::revealed`. Stage 1 then opens a `Resume` for the chooser over the
revealed cards that match (`And[InHand, Not(Kind("Unit"))]` for Sabotage,
`And[InHand, Domain(Mind)]` for Decree of Strength), and the pick is
`recycle_to_bottom` (416.1; the opponent "recycles" it but the choice is
the chooser's, 424.3.a). No match: nothing (055). An empty hand: no
reveal, nothing. Scuttle Crab's Deathknell keeps its M7 script. Where:
`state.rs`, `engine/chain.rs`, `engine/mod.rs` (the `Reveal` arm),
`engine/targets.rs` (`InHand`, `Kind`, the Card universe widened to the
revealed set for a `Resume`). Blob: `awaiting`. kai: the opponent's fan
already draws shown cards face up in place (M7); the chooser's prompt rims
them. Tests: `chain.rs` — a script awaiting two faces stays `Resolving`
across two `Reveal` entries and resumes on the second; a `Pass` while it
waits is refused (the chain top is `Resolving` and `proceed` returns);
Sabotage over a hand of a unit and two spells offers the two spells and
recycles the pick; net — the host pays the reveals in the same frame and
both replicas fold the same resume.

**Pay or let it resolve (Hard Bargain).** `PromptWhy::PayOrLet { item,
stage }` is a `Confirm` addressed to the countered spell's controller:
"pay 2 energy to keep {card N}?" with `yes` and `no`. The script asks it
through `Ctx::ask_pay_or_let(item, payer, cost, stage)`, and the presenter
offers `yes` only when `pay::affordable(payer, cost)` (444.2 makes the
payment optional; an unaffordable one is answered `no` by `next_auto`
without a click). `yes` pays through `pay::plan`/`pay::pay` from the
payer's runes and the spell stays; `no` counters it (`counter_spell`).
Under Repeat each execution asks its own spell's controller separately
(444.2.b). Ravenbloom Student sees one `PlayedSpell` for Hard Bargain
itself. Where: `state.rs`, `engine/ctx.rs`, `engine/prompts.rs` (options,
status, `answer_words` → `["pay", "let it resolve"]`). Blob: the tag. kai:
a new `PromptSummary.why`; the strip shows the opponent's confirm as
"{seat N}: pay 2 energy to keep Discipline?" to the other seat; the AI
brain's phrase table gains the two words (its coverage test over
`PromptWhy::each()` fails until it does). Tests: with two ready runes the
payer is asked and `yes` exhausts two and leaves the spell; with one rune
the spell is countered without a prompt; repeated against two spells asks
each controller in order.

**Conditional and floating cost reductions.** Three shapes, one reader.
`cost::of_item` becomes printed (or the origin's substitute) + Deflect +
optional costs − discounts, never below zero (356.6), where discounts are:
the card's own `Static::SelfDiscount(f)` (Find Your Center: `[2]` less
while an opponent's points are within 3 of the victory score, read from
`ctx.points` and `ctx.options.victory_score`); every in-play battlefield's
`Static::SpellDiscount(f)` (Sandswept Tomb: `[A]` less for a spell whose
chosen cards include a unit at the Tomb friendly to the spell's controller
— computed after targets are chosen, so it applies at `STAGE_PAY` and in
`choose_targets`' affordability check, and the strip's pre-target `Play`
rim stays a promise about timing only, as the M8 ruling says); and the
seat's floating `SeatState.next_discount` (Astral Heron: `(2, 2)` set by
its trigger, consumed by the seat's next card — a play, never an
activated ability, since the card prints "your next card" and 206.1 keeps
an ability's cost at its base for every reader — cleared when applied). "[N] less" removes energy; "[A] less"
removes one power need of any kind — a `Rainbow` need first, then an
`AnyOf`, then a printed domain need — which is what 356.4.f.1's example
shows ("Units you play cost [A] less" reduces Clockwork Keeper's optional
[C] to nothing). The draft had a discount never touching a printed domain
need; the review showed that reading would have made Sandswept Tomb and
Astral Heron do nothing to any single-domain card (Nasus, Ascended's
[8][Calm], Rengar's [5][Body]) and the 821.1.c.3 "does not contain [A],
not reduced" clause is Weaponmaster's alone. Astral Heron's trigger is
`Trigger::YouPlayCard` with `condition: first card && at_battlefield(me)`.
Two clocks are kept apart here. `SeatState.cards_played` and
`spells_played` count at finalization (359.1, 359.3.a, 419.4.b: a
finalized card was played whether or not it later resolves, which is what
Crumbling Sands and Legion read) and reset at Expiration. Triggered
abilities on playing a card fire when the card resolves (419.4.a) and
never for a countered card (419.4.a.1): `YouPlayCard` matches
`Event::Played` for permanents, whose finalization is their resolution,
and the resolution-time `Event::PlayedSpell` for spells, which carries
`nth` — the seat's `cards_played` at that spell's finalization, stored in
the item's `SLOT_ORDINAL` pick — so "first card" is the first card
finalized, not whatever count the seat has reached by the time the spell
resolves. The same rule for activated abilities (377.2.a) raises
`Event::Activated` from `chain::finish`, so Nasus - Curator of the Sands
asks once Empower has resolved and reads the ability's base cost
(`cost::base_of_activation`, 206.1). Ravenbloom Student's `PlayedSpell`
from M2 is the same event. The Heron discount persists until used because
the card prints no duration — recorded as a reading. Where:
`engine/cost.rs`, `engine/statics.rs` (the discount walk), `state.rs`,
`engine/play.rs` (the counters at finalize), `engine/expiry.rs`. Blob: the
`SeatState` fields. kai: the affordance and `Legal` rows already carry
computed costs; greying follows. Tests: `cost.rs` — Find Your Center reads
1 energy at 5–7 opponent points on an 8-point table and 3 below; Sandswept
Tomb takes one of Punch First's two Body needs off when it is aimed at a
friendly unit there and nothing off one aimed at an enemy there; Astral Heron's `[2][A][A]`
turns Nasus, Ascended into `[6]` with no power need and is gone after;
Weaponmaster on a `[Calm]` Equip pays the Calm; Crumbling Sands counters
while the opponent's second spell is still on the chain.

**The attach turn (Brutalizer).** `attach::attach` records
`CardState.attached_turn = ctx.turn()` on the gear whenever an attach
completes (Equip, Weaponmaster, Akshan's steal); detach leaves it (the
next attach overwrites). Brutalizer's `WhileAttached` grants are
`[Might(1), MightIf(|ctx, _, gear| ctx.attached_turn(gear) == ctx.turn(),
2)]`: the +1 is materialized by `attach` as today, the +2 is a projection
fact that survives the unit moving, being stunned or exhausted and combat
ending, lapses when the turn counter advances with no expiry to clean,
and resets on any re-attach (434.1.f then a fresh attach). It compares the
global turn number regardless of whose turn it is. Where:
`engine/attach.rs`, `engine/statics.rs`, `state.rs`. Blob:
`attached_turn`. Tests: equip on turn n reads +3, turn n+1 reads +1,
re-equip to the same unit on n+1 reads +3 again; a detach and attach to
another unit moves both parts.

**Heal a subset (Janna - Savior).** `Ctx::heal(card)` zeroes counter 3 on
one unit (a delta of −damage) with no event; `heal_all` is rewritten over
it. Janna's play trigger heals the friendly units at her location, then
targets an enemy unit there to move to its base — a move by effect, so
Vilemaw's Lair refuses it and Vex's lock does not (the lock binds the
owner's moves). Where: `engine/ctx.rs`. Tests: Janna played as a Reaction
to a held battlefield mid-chain heals two friendly units there, not one at
base, and moves the enemy unit home; at Vilemaw's Lair the heal happens
and the move does not.

**Move-restricting battlefields (Vilemaw's Lair).** `Static::NoMoveToBase`
on a battlefield card in play: `march::route` refuses a standard move from
that battlefield to base with `Reason::NoMoveToBase`;
`march::effect_destinations` and `Ctx::move_unit` drop or refuse a base
destination for a unit standing there (Charm's destination prompt does not
offer it; Kha'Zix - Voidreaver's 2-XP recall, Star Spring's move-home,
Janna's, Void Assault's and Irresistible Faefolk's moves resolve as
nothing when the destination closes, 358.3.a); `Filter::MovableToBase`
keeps such units out of Voidreaver's candidate list so the prompt never
offers a no-op (352.16). The combat recall in step 3d is not a move (456)
and still happens; `bounce` and `recall` are untouched. Where:
`engine/march.rs`, `engine/ctx.rs`, `engine/targets.rs`. Tests: a standard
move Lair → base refused with the reason and absent from `Legal.zones`;
Lair → another battlefield with Ganking accepted; Charm on a unit at the
Lair offers only battlefields; Voidreaver's recall does not list a unit at
the Lair; a combat there still recalls surviving attackers.

**Multi-pick targets at one location (Bellows Breath).**
`Filter::SameLocationAsPicks` is evaluated by `targets::candidates_with`
against the picks already on the prompt: true for the first pick, and for
later picks true only at the location of the first. Bellows Breath's spec
is `target(And[Unit, SameLocationAsPicks], 0, 3, Card, "up to three units
at one location")`; under Repeat each execution's group has its own first
pick. Switcheroo's two-spec `ANOTHER_UNIT_WITH_FIRST` shape is unchanged.
Where: `engine/targets.rs`. Tests: after picking a unit at battlefield 1
the candidates are the other units there only; `done` after one pick
deals 1 to one unit; three picks at one location are accepted and a
fourth is refused; a repeated Bellows Breath's second group may be at a
different location.

**A control change (Akshan - Mischievous).** `CardState.controlled_by:
Option<u8>` overrides the face's owner in `Ctx::controller`;
`Ctx::set_controller(card, seat)` sets it with `control_source` (the
thief's card id), moves the card to the new controller's base (an
`Effect::Move` to that seat's base zone at TOP) and narrates. Akshan's
play trigger, if the additional cost was paid, targets an enemy gear
(unattached or attached — an attached gear is detached first, 434.1.f),
takes control, and if the script prints Equip attaches it to Akshan.
`control::sync`, run inside every cleanup after `attach::sync`, reverts
every row whose `control_source` is no longer on the board: the gear is
detached if attached, recalled to its owner's base as exhausted or ready
as it stands, and both fields are cleared. Level and Hunt read the current
controller, which matters for no gear in the pool. The review found the
draft's "activate.rs already filters by controller" false twice over:
`activate::offers` filters by `card.owner` (activate.rs 216) and
`phases::awaken` readies by owner (phases.rs 118), so a stolen gear's
activations would be listed for nobody and it would ready on the victim's
turn; both switch to `ctx.controller`, and `is_default` learns the fields
so the row survives `attach::sync` and expiration. No pool outcome turns
on it today (Brutalizer is attached by Akshan and attached gear cannot
activate) but the invariant is the one the section claims. Where:
`engine/control.rs` (new), `engine/cleanup.rs`, `engine/ctx.rs`,
`engine/activate.rs`, `engine/phases.rs`, `state.rs`. Blob:
`controlled_by`, `control_source` (the state row is 11 wide). kai: the
card sits in the thief's base with its owner's colour on the rim, which
the owner map already draws per card. Tests: Akshan paid steals Brutalizer
off an enemy unit, it attaches to him and its grants follow; Akshan dying
returns the gear to the opponent's base; Akshan bounced does the same; an
unpaid Akshan has no target prompt; a stolen unattached fixture gear
offers its activation to the thief only and readies on the thief's Awaken;
its state row survives a cleanup with nothing else on it.

**Tokens: Sand Soldier, Shadow Clone, Tentacle.** `Token` gains
`SandSoldier` (Unit, 2 Might), `ShadowClone` (Unit, 0 Might) and
`Tentacle` (Unit, 1 Might); `Token::face` builds each; the manifest's
`token_table` gains Shadow Clone and Tentacle (Sand Soldier is already
declared) with `art: None`; `script_of` maps the three names to
`cards/{sand_soldier,shadow_clone,tentacle}.rs` (Sand Soldier and Tentacle
are bare units; the registry test's token allowance covers them). Tokens
have no domains (185.3.b), so Decree of Insight never sees a Body token.
`Ctx::spawn` raises `Played` for each as today (Vex stuns an opponent's
Tentacle), and Up from the Deep's two Tentacles and Sprite Burst's two
Sprites each take a `TargetKind::Zone` play-location target per token,
auto-answered at base when the seat holds nothing. The Shadow Clone's
script is the optional `on_attack` trigger with the `BanishTarget` cost
above, granting `Keyword::Assault(4)` `EndOfTurn` through
`grant_this_turn` (Assault carries its arg on the wire, so the grant
round-trips). Zed's `[Action] > [1][Chaos]: Move me and a Shadow Clone you
control to each other's locations` is `activated(Action, cost,
&[a_card(And[Unit, Friendly, Kind("Shadow Clone")])], run)` with
`swap_units`, the existing one-batch swap; a Clone at Zed's own location
is filtered by `DifferentLocationFrom` on the source
(`prelude::ELSEWHERE_THAN_ME`). Zed's conquer trigger spawns the Clone at
his controller's base exhausted. Where: `engine/ctx.rs`,
`games/riftbound/src/lib.rs`, `cards/`. kai: the tokens window lists the
two new decls from the manifest with no change; token art stays
name-painted until an art id is chosen. Tests: Zed swaps with a Clone at
another battlefield and both contests are marked; a Clone bounced ceases
to exist.

**The Body rune, end to end.** `Domain::Body` exists with code 4;
`Need::Domain(Body)` exists in `cost` and `pay`; `is_rune_face`/
`rune_domain` match the `Body Rune` row by the ` Rune` suffix and its
domain. What M9 adds is data: the `Body Rune` pool row, the coverage
test's rune assertion, `soak::pool_deck`'s successor finding it by domain,
and kai's orange glyph for `Domain::Body` wherever the Calm/Mind/Chaos
glyphs are drawn (the rune-pool badge and the AI text's `plain_icons`
mapping `:rb_rune_body:` → "body rune"). A Body power cost is paid by Body
or Gold; a `[C]` cost on a two-domain card (Void Assault Body/Chaos, Alpha
Strike Calm/Body) is `Need::AnyOf`, which `power_needs` already produces;
Rampage's optional `[Body]` is a `Need::Domain(Body)` on top. Deck
identity (103.1.b.4) is the legend's domains, so Kha'Zix admits Body,
Chaos and Body/Chaos cards. Tests: `cost.rs` — Rengar's `[5][1]` Body face
plans a Body rune recycle and refuses with `NoPowerOf(Body)` on a
Calm/Mind seat; Alpha Strike's `AnyOf([Calm, Body])` accepts either; kai
`ai/cards.rs` — the Body icon renders.

**Two triggers with no card of their own.** `Trigger::Activated { of:
Who::You }` matches `Event::Activated { item, source, index, controller
}`, raised by `play::finalize` when an ability item finalizes (Nasus -
Curator of the Sands: "an activated ability with Energy cost [7] or more"
— its condition reads the ability's script cost through
`cost::of_activation`, printed, 206; Nasus, Ascended's Empower [8]
qualifies). `Trigger::UnitPlayedHere` matches `Event::Played { kind: Unit
}` for a battlefield source at the unit's entry location, handing the item
to the player who played it (Star Spring's "they may", the 184.6 reading
Abandoned Hall already uses); its `Once::PerSeatPerTurn` uses the
`FLAG_ONCE_BY_SEAT` bits on the battlefield card, cleared at Expiration,
and its `condition` reads `!ctx.is_token(subject)` so a Sprite never
counts ("non-token unit") and never consumes the seat's once.
`Trigger::YouPlayCard` is Astral Heron's, above.

**Ravenbloom Conservatory's revealed top card.** "When you defend here,
reveal the top card of your Main Deck. If it's a spell, put it in your
hand. Otherwise, recycle it" needs a face the plugin can branch on while
the card is in a face-down zone, which the host cannot pay (the Reveals
ruling). `Ctx::reveal_top(seat)` moves the top card to the chain zone at
TOP — a public zone, so `reveal_surfaced` pays the debt in the same frame
— marks it `FLAG_REVEALING`, emits `Effect::Reveal`, and the script parks
on `await_faces`; on resume `kind_of` branches: a Spell goes to hand as a
put, anything else to the deck's bottom. Deviation from 424.1.a.2 (the
card is in the chain zone for one entry rather than "in the zone of
origin"); recorded, and the only reading that keeps the fold honest about
faces. A `Defends(Who::You)` on a battlefield card fires for the holder
when its units gain the Defender designation.

**Pool files and the coverage tests.** The four decks do not fit
`cardpool-lillia-irelia.md`'s positional sections, and the pipeline
section below replaces that file with one Markdown file per deck under
`games/riftbound/rules/pool/`, six files whose `## Cards` lines the engine
reads as a union through `POOL_FILES` and whose `Scripted:` line gates the
coverage test. The four tournament files carry their sideboard faces as
card lines too (the pipeline draft kept them out to avoid scripting debt;
M9 scripts them — Alpha Strike, Unyielding Spirit, Acceptable Losses,
Vilemaw, Crumbling Sands and Fiora carry mechanics of their own — so the
registry test's "every script is a pool card" holds), listed on a
`Sideboard:` line and absent from the deck block so the deal is the
tournament forty. The three coverage tests iterate the union, the rune
assertion lists the four runes, `PRINTED_HIDDEN` carries Temporal Breach
and Twilight Shroud until they are scripted and is empty after, and the
`.json` twin is deleted. The files are copied in place now, unwired, and
the pipeline batch P0 wires them.

**What changes outside the engine.** kai: `deck/pinned.rs` becomes the
data-driven pool-deck list of the pipeline section; `table/zones.rs` lays
the `banishment` pile out from its decl beside the trash, and the layout
test for two seats pins the fourteenth zone; `table/counters.rs` makes XP
read-only under rules enforced; `table/plugin_ui.rs` renders
`PromptSummary.why` for `PayOrLet` through the same path; `ai/brain.rs`'s
phrase table gains `pay`/`let it resolve`, the Flow offer wording ("play
from your trash"), the ambush location label and the XP cost wording, and
its card-specific coaching moves to the pool files; `ai/cards.rs::
plain_icons` maps `:rb_rune_body:` → "body rune" and passes the `[Hunt
2]`, `[Level 6]`, `[Empower]`, `[Empowered]`, `[Flow]`, `[Repeat]`,
`[Ambush]`, `[Burn 1]` and `[Weaponmaster]` brackets through as words;
the importer's `SetCode::Opp` and the print-name fold are the pipeline's;
kai-cli prints the xp tail and the new prompt line, and `do <n>` answers
everything. agni outside the plugin: nothing in `sim`, `net` or the SDK.
The riftbound plugin crate goes to 0.4.0 and kai's bundled
`riftbound.wasm` and the store's `modules/riftbound` are rebuilt.

Files: `cards/mod.rs`, `cards/prelude.rs`, sixty new `cards/<snake>.rs`
scripts (the gap table names them), `state.rs` (v6), `present.rs`,
`engine/{ctx,triggers,targets,expiry,cost,pay,activate,play,legal,chain,
hide,kill,cleanup,combat,showdown,phases,march,attach,prompts,discard,
mod}.rs`, new `engine/{statics,prevent,control}.rs`, `rules.rs`,
`games/riftbound/src/lib.rs` (zone 13, two tokens, 0.4.0),
`games/riftbound/rules/pool/*.md`; kai `src/deck/pinned.rs`,
`src/table/{zones,counters,plugin_ui}.rs`, `src/ai/{brain,cards,soak}.rs`,
`src/bin/kai_cli.rs`; the flake's source filter.

Tests: the per-mechanic lists above, all under `unit-test` in agni/agni;
the blob v6 round trip with every new field and an unknown key;
`PromptWhy::each()` including tag 13; the token faces; the coverage tests
green over six pool files; the M5 parity test over the new fixtures; the
M8 soak over all six pool decks (each pair, both seats first) at the
zero-refusal bar; the gas benchmark on a Kha'Zix mirror with two Hunts, an
aura and a repeated Bellows Breath on the chain under the 25% bar; net —
an ambush during the other seat's chain, a Flow play from the trash, a
Sabotage reveal, Time Warp and a Hard Bargain payment each folding the
same blob on both replicas.

Cards unlocked (the 60 faces new to the pool, sideboards included): Master
Yi - Wuju Bladesman, Master Yi - Tempered, First Mate, Lonely Poro,
Onslaught, Pit Rookie, Punch First, Rampage, Rengar - Trophy Hunter, Ruin
Runner, Sabotage, Twilight Shroud, Emperor's Dais, Star Spring, Alpha
Strike, Decree of Focus, Decree of Strength, Disarming Rake, Fiora -
Peerless and the Body Rune; Nasus - Curator of the Sands, Nasus, Ascended,
Lecturing Yordle, Astral Heron, Thousand-Tailed Watcher, Bellows Breath,
Retreat, Premonition, Temporal Breach, Find Your Center, Time Warp, Sigil
of the Storm, Vilemaw's Lair, Crumbling Sands, Vilemaw; Kha'Zix -
Voidreaver, Kha'Zix - Mutating Horror, Irresistible Faefolk, Shadow Order
Disciple, Traveling Merchant, Fizz - Trickster, Akshan - Mischievous,
Tail-Cloaked Matriarch, Zed, Without a Sound, Stacked Deck, Hard Bargain,
Void Assault, Up from the Deep, Forbidding Waste, Sandswept Tomb, Zaun
Warrens, Acceptable Losses, Gust, Unyielding Spirit, Angler Beast; Janna -
Savior, Brutalizer, Mask of Foresight, Sprite Burst, Ravenbloom
Conservatory; plus the Sand Soldier, Shadow Clone and Tentacle tokens. The
32 faces the four decks share with the house decks resolve to their landed
scripts.

**Rulings M9 adds.** Rule numbers are Core Rules 2026-07-16. Each entry
names the reading the engine enforces; the ones that deviate from a plain
reading of the text say so.

- XP (729–733, 202, 205). XP is the seat counter `COUNTER_XP`: public,
  never a target, never capped, never negative. Gain is an immediate add
  during the resolving instruction; spend is a cost paid at finalization
  beside energy (357.2), legal only while the seat has that much
  (`NotEnoughXp`), and emitted only after the payment plan is accepted, so
  a refused plan spends nothing and there is nothing to refund (358.5).
- Level N (824.1, 727.1). A projection fact, active while the
  controller's XP is at least N, re-read through a new controller
  (824.1.c.1), no event, no expiry, no hysteresis. A pending item already
  finalized is not undone when XP later drops (727.1.c.3.a); a dependent
  trigger fires when the same event crosses the threshold and meets the
  condition (727.1.c.1.a).
- Hunt N (823.1, 823.2, 383.4.c.2.a, 470). One implicit triggered ability
  on Conquer and Hold of the unit itself, on the chain so the opponent can
  respond, once per battlefield per scoring, value summed across sources
  and read at resolution, XP to the unit's controller.
- Empower and Empowered (827.1, 441, 442, 828.1, 124.1, 719.4.a). An
  activated ability with the printed cost and no exhaust, Sorcery timing,
  refused `AlreadyEmpowered` while counter 6 is set; `Empowered` is
  raised only on a real flip, `Disempowered` only when it was set; the
  status is shed on leaving the board and never crosses an attachment.
- Ambush (822.1, 822.3, 355.2, 359.2.c, 464.2.c). Adds locations, not a
  cost; Reaction timing only for a play to an ambush battlefield; the
  card's own timing to base or a held battlefield; Rengar adds
  enemy-held battlefields; Rockfall Path never (054.1); re-checked at
  payment and undone to hand if the units left (822.3, 358.5). The
  ambusher's seat is the attacker only if its units applied Contested. An
  ambush into an open non-combat showdown is designated when the combat
  is restaged and opened after that showdown closes — the engine keeps its
  close-then-restage path rather than 464.1's conversion; the outcome is
  464.2.c.1's.
- Flow (829.1, 356.1.a, 366.1, 390.3.a, 206). A passive that applies only
  in the owner's trash; the Flow cost replaces the base cost before
  Deflect and discounts; the spell's own timing; banished when it leaves
  the chain after finalization unless its own execution moved it —
  resolved or countered alike; Defy reads the printed cost. Fizz's play
  zeroes energy, keeps power and recycles (356.1.b.2); a Flow spell played
  through Fizz is recycled, not banished.
- Banishment (108.6, 427, 186.1, 124). A per-seat public zone reached
  only through `banish`: a board card is not killed (no Deathknell, no
  Zhonya's), attached gear detaches (719.5), a token ceases to exist,
  counters and statuses are shed. Temporal Breach's replay is a limited
  play by the owner (419.3) from Banishment: base cost zero, Accelerate
  offered (356.1.b.3), enters exhausted, play triggers fire again, Deflect
  on the target taxes Temporal Breach's choice. The location is the one
  the effect names and is valid by 355.2.b whether or not the owner holds
  it; only Rockfall Path's "can't" (054.1) makes the replay impossible,
  and then the card stays banished (055). This replaces the phase-1
  reading "a battlefield the owner has no claim to" — that gloss is not
  in the rules and would have turned the card's normal use against a lone
  attacker into a permanent removal.
- Repeat (820.1–820.3, 355.1.a, 444.2, 359.3.e.1). An optional additional
  cost chosen before targets, then every target group chosen twice at the
  usual time with its own full choice; the script runs once per
  execution; the spell is played once; Defy reads the printed cost; a
  group that is wholly illegal at resolution executes nothing while the
  other runs; Hard Bargain asks each countered spell's controller
  separately.
- Optional costs on triggers (383.3.a, 383.3.b, 383.3.b.1, 392.2). A "you
  may [cost]" at the head of a trigger's effect is the trigger's base
  cost, decided and paid at finalization: declined, the trigger is
  removed; paid, nobody can respond in between. This covers Burn 1
  (Disciple), the self-exhaust (Nasus - Curator), the banish from trash
  (Shadow Clone) and energy or rune costs (Emperor's Dais).
- Burn (440, 431). Burned cards are neither discarded nor killed; a Burn
  cost is payable only from a deck holding at least that many cards and
  an unpayable one removes the trigger without burning out; Burn as an
  effect and Burn Out stay with M7's roll.
- Extra turns (735–738, 317.2.c, 317.3). A queue of seats consumed at the
  end of the turn ahead of the rotation, which is untouched; a whole
  turn with Hold scoring, Channel and Draw, a fresh turn counter and
  nothing surviving from the real turn's Expiration; queued in resolution
  order; the first-turn rune adjustment never applies to an extra turn.
- Prevention (437, 465.2.c.5, 465.2.c.10). A delayed replacement on the
  table consulted before damage is marked; wholly prevented damage was
  never dealt (437.4) — no `Damaged`, no Deathknell, no Zhonya's; never
  lethal (437.5.b); assignment applies it, so an All-prevented unit is
  exempt from mandatory assignment. Damage a unit deals under Alpha
  Strike or Rampage is spell damage (411.5).
- "Alone" (740.2, 383.4.e.2.b, 383.4.f.2.b, 808.1.d.3, 323.4). `alone_at`
  counts tokens, stunned and exhausted units and not gear or the legend.
  "Defends alone" is a projection read (designation and alone); "attacks
  or defends alone" and "if an enemy unit is alone here" are read once,
  at the designation; "one on one" is both sides alone then; "died alone"
  reads the batch snapshot — units dying together are still present for
  each other. The snapshot reading is a choice (808.1.d.3 does not say),
  recorded so it is not re-litigated.
- "Win a combat" (466.3, 466.2, 466.5). The result is the seat that
  received a designation and is the only one with units at the
  battlefield, determined in `after_combat` right after the lethal kills
  and the step-3d recall and before `establish`; both present, both gone
  or a recall is No Result; a defender wins without a point; the
  `CombatWon` trigger is collected with the Deathknells and conquer
  triggers of the same combat and ordered among them by its controller.
  Deviation from 466.2: the Deathknells and reactions of the combat
  cleanup have not resolved when the result is read, so a side whose last
  unit dies to one of them has already won or lost; chosen over a
  cross-decide pending result because it needs no blob state and keeps
  M4's conquer ordering.
- Weaponmaster (821.1, 821.1.c.3, 821.1.c.5, 821.1.c.6, 434.1.f). A play
  trigger choosing a friendly Equipment anywhere on the board at
  finalization; on resolution the Equip cost is paid with one `[A]`
  struck only when the cost prints one — Brutalizer's `[Calm]` is paid
  in full, departing from the phase-1 "becomes free" — the gear detaches
  from its old wearer and attaches; an unpayable cost leaves it where it
  is; not an Equip activation, so no `Chosen`, no Deflect on the wearer,
  no equip watcher.
- "[A] less" (356.4, 356.4.f.1, 356.6). A discount of `[A]` removes one
  power need of any kind, Rainbow first, then AnyOf, then a printed
  domain need; `[N]` less removes energy; never below zero. The
  Weaponmaster clause above is the one place a printed domain need is
  not reduced. Sandswept Tomb is computed after targets are chosen, so
  the hand rim promises timing and affordability at the printed cost
  only. Astral Heron's discount has no printed duration and persists
  until the seat's next card; an activated ability neither takes nor
  consumes it (206.1, and the card says "card").
- Cards played (359.1, 359.3.a, 419.4). `cards_played` and
  `spells_played` count at finalization: a countered spell was played for
  Legion and Crumbling Sands. Abilities that trigger on playing a card
  (Astral Heron, Ravenbloom Student, Curator's activation clause) fire
  when the card or ability resolves and not at all when it is countered
  (419.4.a, 419.4.a.1, 377.2.a).
- Counters and where the card goes (425, 829.1.b.1, 390.3.a). Every
  scripted counter goes through `chain::counter`: a spell played from the
  trash for Flow is banished instead of trashed, one Fizz replayed is
  recycled, and "return it to its owner's hand" counters (Abandon) yield to
  those delayed replacements too.
- Additional turns (738). A newly queued turn is taken before turns queued
  earlier: `extra_turns` is a stack, not a queue.
- Prevention (437.3). A numeric Prevent value is spent by the damage it
  stops and the entry lapses at zero; `Amount::All` never counts down.
- Control (203, 353.1). Standard moves, group-move companions and
  "friendly" reads key on the controller, so a stolen unit marches for the
  seat that controls it and no longer for its owner.
- "Each player chooses" (303.2.a, 355.10.e, 411.1). Not a target; the
  turn player first, then turn order, one prompt at a time with earlier
  choices visible, a seat without a candidate skipped (055), each kill
  the choosing seat's own. "Choose an opponent" is a seat target at
  finalization (355.9.a) and the chooser picks from the revealed hand.
- Look at the top N (128.4, 431.1.c, 413.1, 416.1, 416.5). A private pick
  with the cards still in the deck; the pick is a put, not a draw; fewer
  cards show fewer, an empty deck does nothing. Deviation from 416.5: the
  rest are recycled in the order the controller listed them, not at
  random, because the information is that seat's alone and a
  commit-reveal roll per Stacked Deck is not worth two round trips —
  revisit if a card ever reads the bottom of a deck.
- Ravenbloom Conservatory (424.1.a.2). The revealed top card passes
  through the chain zone for one entry so the host can pay the reveal;
  the only reading that keeps the fold honest about faces.
- "Attached to me this turn" (434.1.f, 434.4, 718.4). A fact about the
  attach event compared with the global turn number: survives moves, stun
  and combat end, lapses when the turn advances, resets on any
  re-attach.
- Body (134.2.d, 164.1, 135.2.e, 103.1.b.4, 185.3.b). Nothing new in
  `cost` or `pay`; a `[C]` on a two-domain card accepts either domain; a
  unit is Body if Body is among its printed domains; tokens have no
  domain; deck identity is the legend's domains.
- Emperor's Dais. "Pay [1] and return a unit you control here to its
  owner's hand" is priced as a `[1]` trigger cost confirmed at
  finalization with the unit chosen as a target at the choices step and
  bounced at resolution just before the Sand Soldier is played; the
  bounce therefore happens at resolution rather than as part of the cost
  (383.3.b would put it at finalization). If the unit is gone at
  resolution no Soldier is played (359.3.e). No outcome in the pool
  differs.
- Nasus, Ascended's point (471.1.a.1). "You score 1 point" from an
  ability is not a Conquer point and is not beholden to the final-point
  restriction; `score_effect` scores it unrestricted, so it can be the
  winning point.
- Rockfall Path versus "play it there" (054.1, 055, 358.3.a, 184.2). An
  effect that names a location fixes it and does not beat a prohibition:
  Fae Fawn's Sprite, Emperor's Dais's Soldier (never a Rockfall Path),
  Temporal Breach's replay and Ambush all skip Rockfall Path; the
  instruction resolves as impossible and the trigger still resolves.
  Vilemaw's Lair forbids the standard move and Charm to base the same
  way; the combat recall is not a move (456).
- `WIRE_VERSION` stays 5; the versions table says why, and the decision to
  bump anyway for the deploy rule is rae's.

### M9 appendix — the per-card gap table

Mechanics name the M9 paragraphs above; "existing" means the M0–M8
primitive carries the card as written. Every card is scripted in
`cards/<snake>.rs` under its Riftcodex name; runes resolve to the generic
rune script. Sideboard cards are scripted like main-deck cards because the
coverage test demands it and the kai sideboard window can swap them in.
Decks: Yi = Master Yi (Akame), Nasus = Nasus (ThunderTrees), Kha'Zix =
Kha'Zix (Hotkee), Lillia = Lillia (Jonnynick); "side" is that deck's
sideboard, "bf" its battlefields.

| Card (decks) | Mechanics | M9 adds | Done when |
|---|---|---|---|
| Master Yi - Wuju Bladesman (Yi legend) | aura, defends alone | `Static::Aura { FriendlyUnits, defender && alone, [Might(2)] }` | a lone friendly defender reads +2 in the damage step and loses it when a second friendly unit arrives; nothing at base |
| Master Yi - Tempered (Yi champion) | Hunt 2, Level 6, Deflect, Ganking | `Keyword::Hunt(2)`, `Static::Level(6, [Deflect(1), Ganking])` | conquer gains 2 XP after the chain; at 6 XP an enemy Charm on him costs one more rainbow and a battlefield-to-battlefield move is legal; at 5 neither |
| Akali, Silent (Yi) | existing | — | script unchanged; the Yi pool row resolves to it |
| Charm (Yi, Lillia) | existing, NoMoveToBase | destination filter honours Vilemaw's Lair | Charm on a unit at the Lair offers no base |
| Defy (Yi, Nasus, Lillia) | existing, printed cost under Flow/Repeat | — | Onslaught for Flow [4] is still counterable |
| Discipline (Yi, Nasus, Lillia) | existing | — | — |
| En Garde (Yi, Lillia) | existing ("only unit you control there" = alone) | — | second +1 iff `alone_at` |
| First Mate (Yi) | play trigger, ready another unit | — | play readies a chosen exhausted friendly or enemy unit and raises `Readied` |
| Lonely Poro (Yi, Lillia) | Deathknell, died alone | `Noted.alone` | the three kill.rs cases above |
| Onslaught (Yi, Kha'Zix) | +6 this turn, Flow [4] | `Keyword::Flow`, `IMPLICIT_FLOW`, `begin`'s move onto the chain, banish on leave | from hand it is trashed; from trash for [4] it is banished; countered from trash it is banished; cancelled at its target prompt it returns to the trash |
| Pit Rookie (Yi) | play, buff another friendly unit | — | buff counter set on a chosen other friendly unit, none when alone |
| Punch First (Yi, Kha'Zix) | Action, +5 | — | playable in a showdown by the focus holder |
| Rampage (Yi, Kha'Zix) | optional additional [Body] at `STAGE_ADDITIONAL`, "if you paid", mutual damage attributed to the spell | `Card.additional`, `SLOT_ADDITIONAL`, `paid_additional` | paid → +2 then both deal current Might to each other as `Cause::Item`; Unyielding Spirit prevents both |
| Rengar - Trophy Hunter (Yi, Kha'Zix) | Ambush into enemies | `Keyword::Ambush`, `Static::AmbushIntoEnemies`, `timing_at` with and without a location, `contest` at finalize | the play.rs Rengar tests, including the drag to the chain on the opponent's turn and the ambush into an open non-combat showdown |
| Ruin Runner (Yi; Kha'Zix side) | untargetable by enemies | — (`Static::Untargetable(\|_, _\| true)`, Akali's shape) | an enemy Charm never lists him; a friendly Discipline does |
| Sabotage (Yi, Kha'Zix; Kha'Zix side) | choose opponent, reveal hand, pick non-unit, recycle | `await_faces`, `InHand`, `Kind` | the chain.rs Sabotage test |
| Scuttle Crab (Yi, Nasus) | existing | — | — |
| Twilight Shroud (Yi) | +1, shrouded this turn, Flow [2] | `FLAG_SHROUDED`, `shroud`, Flow | an enemy Stupefy after Shroud finds no candidate; a friendly one does; the flag is gone next turn; from the trash for [2] it is banished |
| Zhonya's Hourglass (Yi, Nasus) | existing | — | banish of the wearer does not consult it |
| Body Rune (Yi ×7, Kha'Zix ×6) | Body domain | the pool row, the rune-list assertion, kai glyph | Rengar's power is paid by a Body rune |
| Calm Rune / Mind Rune / Chaos Rune | existing | pool rows | — |
| Emperor's Dais (Yi bf) | conquer trigger, optional [1] cost, bounce a unit here, Sand Soldier | `Token::SandSoldier`, the Dais ruling | pay → a chosen unit here is bounced and a 2-Might Soldier enters exhausted here; decline → nothing |
| Seat of Power (Yi, Lillia bf) | existing | — | — |
| Star Spring (Yi bf) | UnitPlayedHere, once per player per turn, non-token, may move another unit here to base | `Trigger::UnitPlayedHere`, `Once::PerSeatPerTurn`, `FLAG_ONCE_BY_SEAT`, NoMoveToBase respect | the first non-token unit each player plays here asks once; a Sprite played here asks nothing; the second unit asks nothing |
| Alpha Strike (Yi side) | Action, split damage one point at a time, attributed, per-kill XP | the point-by-point `Resume` loop, `cleanup::run`'s `last_item`, `When::AfterKillsBy(item)` | 5 Might split 3 + 2 over five prompts kills two units and the seat gains 2 XP after the cleanup; a 10-Might Yi asks ten times; under Unyielding Spirit nothing |
| Decree of Focus (Yi, Nasus, Lillia side) | Reaction, friendly unit in combat with an enemy Fury unit or chosen by an enemy Fury spell | `Filter::InCombatWith`, `ChosenByEnemyItem`, `Domain(Fury)` | no Fury card in any of the four decks → the candidate list is empty against these decks and the cancel-only prompt opens; a fixture Fury attacker makes the defender a candidate |
| Decree of Strength (Yi side) | reveal hand, pick a Mind card, they recycle | as Sabotage with `Domain(Mind)` | pick lands at the opponent's deck bottom |
| Disarming Rake (Yi side, Lillia) | play, may kill a gear | — (Pickpocket's shape) | optional target, kill |
| Fiora - Peerless (Yi side) | attack/defend one on one, double Might this combat | `enemy_alone_at`, combat-end mod | 3 Might vs one enemy reads 6 until the combat ends; vs two reads 3 |
| Not So Fast (Yi side, Nasus side) | existing | — | — |
| Nasus - Curator of the Sands (Nasus legend) | Played/Activated with printed energy ≥ 7, optional self-exhaust cost, ready up to 2 runes | `Trigger::Activated`, `Event::Activated` raised as the ability resolves, the optional trigger-cost confirm at `STAGE_PAY`, `cost::base_of_activation` | Astral Heron's play asks; yes exhausts Nasus and readies two chosen runes, no leaves him ready; Empower [8] asks once it resolves and under a Heron discount still costs eight; a 6-cost card asks nothing |
| Nasus, Ascended (Nasus champion) | Deflect 2, Empower [8], Empowered conquer trigger | `prelude::empower`, `usable`, `when(is_empowered)`, `score_effect` | empower offered once, refused after; conquer while empowered scores an unrestricted extra point that can be the final one (471.1.a.1); conquer before it does not |
| Ravenbloom Student (Nasus, Lillia) | existing | — | — |
| Lecturing Yordle (Nasus) | Tank, play draw | — | assignment orders it first |
| Tomb-Raider Barbara (Nasus; side) | existing, disempower event | `Disempowered` raised | disempowering an Empowered Nasus clears counter 6 |
| Astral Heron (Nasus) | YouPlayCard first each turn, if at a battlefield, floating discount | `Trigger::YouPlayCard`, `PlayedSpell.nth` at resolution, `next_discount` | the cost.rs Heron test (`[2][A][A]` off Nasus, Ascended); the discount lands when the first spell resolves and not when it is countered; an Empower activation neither takes nor spends it; at base it fires nothing |
| Thousand-Tailed Watcher (Nasus; Lillia side) | Accelerate, play, enemy units −3 min 1 | — | existing `might_this_turn` with `Some(1)` |
| Bellows Breath (Nasus) | Action, Repeat [1][Mind] asked before targets, up to three units at one location, deal 1 | `Keyword::Repeat`, `STAGE_REPEAT`, `execution`, `SameLocationAsPicks` | the play.rs Bellows test |
| Retreat (Nasus) | Reaction, bounce friendly unit, owner channels 1 rune exhausted | `channel_exhausted` | the rune enters the pool exhausted |
| Stupefy (Nasus, Lillia) | existing | — | — |
| Premonition (Nasus) | Reaction, draw 3 | — | three `Drew` |
| Smoke Screen (Nasus, Lillia) | existing | — | — |
| Temporal Breach (Nasus) | Hidden, banish a unit, owner replays it at the same location ignoring cost | `banish`, `Origin::Banishment`, owner-controlled limited play at the effect-fixed location, `contest` | the play.rs Temporal Breach tests: a held battlefield, a lone attacker mid-showdown, Rockfall Path; from facedown its target is restricted to the hiding battlefield |
| Back Off (Nasus) | existing | — | — |
| Find Your Center (Nasus) | Action, conditional self discount, draw, channel exhausted | `Static::SelfDiscount`, `channel_exhausted` | the cost.rs test |
| Time Warp (Nasus) | extra turn, banish self | `queue_turn`, `extra_turns`, `banish` | the phases.rs tests; the card is in Banishment after |
| Rockfall Path (Nasus bf) | existing; never an ambush or replay location | `ambush_locations` and the Banishment replay exclude it | Rengar cannot ambush there; Temporal Breach there leaves the unit banished |
| Sigil of the Storm (Nasus bf) | conquer, recycle one of your runes | `Resume` over the seat's runes | a chosen rune goes to the rune deck's bottom |
| Vilemaw's Lair (Nasus bf) | NoMoveToBase | `Static::NoMoveToBase` | the march.rs tests |
| Crumbling Sands (Nasus side) | Reaction, counter a spell if an opponent played another spell this turn | `spells_played` per seat, counted at finalization | counters while the opponent's second spell is still on the chain, cancel-only before it was finalized |
| Decree of Insight (Nasus, Lillia side) | existing; "enemy Body unit" | `Domain(Body)` filter on printed domains | Rengar is a candidate, a Tentacle is not |
| Pickpocket (Nasus, Lillia side) | existing | — | — |
| Singularity (Nasus side, Lillia) | existing; prevented by Unyielding Spirit | — | — |
| Vilemaw (Nasus side) | Ambush, enemy units here with less Might deal no combat damage, hold draw | `Static::NoCombatDamageFrom`, `deals_combat_damage` | a 7-Might enemy beside an 8-Might Vilemaw contributes 0 to its side's sum; an 8-Might one contributes 8 |
| Kha'Zix - Voidreaver (Kha'Zix legend) | CombatWon, spend 1 XP buff, spend 2 XP recall exhausted friendly unit | `Trigger::CombatWon` raised in-frame by `after_combat`, `spending_xp`, `MovableToBase` | the combat.rs result tests; buff offered at 1 XP; recall lists exhausted friendly units at battlefields not at the Lair |
| Kha'Zix - Mutating Horror (Kha'Zix champion) | Ambush, attack/defend if an enemy unit is alone here, +2 and 2 XP | `Keyword::Ambush`, `enemy_alone_at` | the kill.rs/combat.rs Horror tests |
| Irresistible Faefolk (Kha'Zix) | move to battlefield, may move an enemy unit there | existing move; Lair respected | the enemy unit arrives, contest marked, combat staged |
| Shadow Order Disciple (Kha'Zix) | move, optional Burn 1 trigger cost, +1 | `burn`, `Ability.burn`, the trigger-cost confirm at `SLOT_TRIGGER_COST` | the play.rs Disciple tests |
| Tideturner (Kha'Zix) | existing | — | — |
| Traveling Merchant (Kha'Zix) | move, discard 1 then draw 1 | — (`ask_discard`) | a gesture or pick discards, then one `Drew` |
| Fizz - Trickster (Kha'Zix) | play, may play a spell from trash ≤ [3] ignoring energy, recycle after | `Origin::Trash { Recycle }`, `InTrash` | the play.rs Fizz test; a countered Fizz-played spell is recycled |
| Akshan - Mischievous (Kha'Zix) | Weaponmaster, additional [Body][Body] at `STAGE_ADDITIONAL`, steal an enemy gear, control until he leaves, attach if Equipment | `weaponmaster()`, `Card.additional`, `set_controller`, `control::sync`, `is_default`, controller-keyed offers and awaken | the control tests |
| Tail-Cloaked Matriarch (Kha'Zix) | Empower [2][Chaos], on Empowered play a trash unit ≤ [3] ≤ 1 power to base ignoring cost | `empower`, `Trigger::Empowered`, `Origin::Trash` limited play | empower once; the trigger asks for a unit in the trash and it enters the base exhausted with its play trigger firing |
| Vex - Apathetic (Kha'Zix) | existing; sees ambushed and token plays | — | an ambushed Rengar is stunned |
| Zed, Without a Sound (Kha'Zix) | conquer → Shadow Clone at base; Action [1][Chaos] swap with a Clone; the Clone's optional banish-from-trash trigger cost | `Token::ShadowClone`, `SelfCost::BanishTarget`, existing swap | the token tests; the Clone asks only with a unit in the trash and banishes it at finalization |
| Stacked Deck (Kha'Zix) | Action, look at top 3, put 1 in hand, recycle the rest | peeks + `Resume` | the look tests |
| Hard Bargain (Kha'Zix) | Reaction, Repeat [2], counter unless controller pays [2] | `PayOrLet`, Repeat | the pay-or-let tests |
| Void Assault (Kha'Zix) | move a friendly unit then an enemy unit, contest semantics | existing move + destination prompts | both destinations chosen at play; an enemy moved to an uncontrolled battlefield makes the caster the attacker only if its unit marked it first |
| Star-Crossed (Kha'Zix) | existing; can hand a defender the win | — | the combat.rs Charm-away test shape |
| Up from the Deep (Kha'Zix; side) | two Tentacles, Flow [3] | `Token::Tentacle`, Flow | two 1-Might Tentacles enter exhausted at chosen locations; from trash for [3] banished |
| Forbidding Waste (Kha'Zix bf) | aura, defending alone −2 | `Static::Aura { UnitsHere, ... [Might(-2)] }` | the statics tests |
| Sandswept Tomb (Kha'Zix bf) | spell discount `[A]` less for friendly units here, any power need | `Static::SpellDiscount` | Punch First aimed at a friendly unit there loses one of its two Body needs; aimed at an enemy there pays both |
| Zaun Warrens (Kha'Zix bf) | conquer, discard 1 then draw 1 | — | as Merchant on conquer |
| Acceptable Losses (Kha'Zix side) | Action, each player kills a gear | `ask_seat_resume` | the each-player tests |
| Gust (Kha'Zix side) | Reaction, bounce a unit at a battlefield with ≤ 3 Might | `Filter::MightAtMost(3)` (current Might) | a 4-Might unit under Stupefy is a candidate |
| Unyielding Spirit (Kha'Zix side) | Reaction, prevent all spell and ability damage this turn | `prevent`, `pv` | the prevent.rs tests |
| Rebuke (Kha'Zix side) | existing | — | — |
| Switcheroo (Kha'Zix side) | existing | — | — |
| Angler Beast (Kha'Zix side) | play, bounce all units with ≤ 2 Might | `MightAtMost(2)` over every unit, no target | tokens vanish, others go to hand, Angler Beast itself (5) stays |
| Lillia - Bashful Bloom (Lillia legend) | existing | — (pool row in the new file) | — |
| Lillia - Fae Fawn (Lillia champion) | existing | — | Rockfall ruling stands |
| Janna - Savior (Lillia) | Reaction unit to a held battlefield, heal friendly units here, move an enemy unit here home | `heal`, Reaction-unit timing in `timing_at` (813.3.a) | the heal tests |
| Brutalizer (Lillia) | Equip [Calm], +1, +2 if attached this turn | `attached_turn`, `Grant::MightIf` | the attach tests |
| Mask of Foresight (Lillia) | friendly unit attacks or defends alone, +1 this turn | `Attacks/Defends(Who::Friendly)` with `alone_at(subject)` | fires at designation, once per unit per combat |
| Sprite Fountain (Lillia) | existing | — | — |
| Smoke and Mirrors (Lillia; side) | existing | — | — |
| Sprite Burst (Lillia) | two ready Sprites with Temporary, chosen locations | two zone targets | both enter ready at the chosen held locations and die next Beginning |
| Unchecked Power (Lillia; side) | existing; prevented by Unyielding Spirit | — | — |
| Dusk Rose Lab (Lillia bf) | existing | — | — |
| Ravenbloom Conservatory (Lillia bf) | defend here, reveal top, spell → hand else recycle | `reveal_top`, `await_faces` | a spell on top lands in hand after the host's Reveal; a unit goes to the bottom |

### M9 batches and file ownership

B0 lands first and alone; it owns every shared enum, every blob field,
the manifest, the pool wiring and every *hook line* in an existing module
that a later batch fills, so that B1–B8 touch disjoint files. New modules
are created by B0 as stubs whose functions return the empty or identity
answer (`statics::grants_on → vec![]`, `prevent::amount → n`,
`control::sync → ()`, `combat::result → None`) and are filled by their
owners. Each batch owns its card scripts outright; a card whose primitives
span batches is listed under the batch that owns the last primitive it
needs and named in the other batch's "unblocks". Engine tests that need a
card from another batch use a fixture `static Card` in the engine module,
the way `combat.rs` defines `TANK` and `BOTH` today. The review's batch
collisions are resolved here: `reveal_top`, `ask_pay_or_let`,
`score_effect`, `contest` and the `When::AfterKillsBy` tag are B0's; the
`Event::Activated` raise is a B0 hook line in `play::finalize`;
`cleanup::run`'s `last_item` parameter is added by B0 with every caller
passing `None`, so B3 (the chain) and B4 (the cleanup) only fill it;
Astral Heron, Nasus - Curator of the Sands and Zed with his Shadow Clone
move to B3, which owns the last primitive each needs.

**B0 — vocabulary, blob, manifest, pool wiring (serial).** Files:
`cards/mod.rs` (all new enum arms, `Ability.{xp,burn,usable,once}`,
`Card.additional`, `SelfCost::BanishTarget`, `IMPLICIT_HUNT/FLOW`, the
pool-file table and the coverage tests as the pipeline's P0 lays them
out), `cards/prelude.rs` (every new const constructor: `spending_xp`,
`burning`, `usable_if`, `with_additional`, `empower`, `weaponmaster`,
`on_empowered`, `on_combat_won`, `on_activated`, `on_unit_played_here`,
`enemy_alone_at`, `died_alone`, `paid_additional`, filters `IN_TRASH`,
`ELSEWHERE_THAN_ME`), `state.rs` (v6: `SeatState`, `CardState` with
`is_default`, `Noted`, `ChainItem` with the slot layout and `UNANSWERED`,
`Origin`, `PromptWhy::PayOrLet`, `When::AfterKillsBy`, `xt`, `pv`, flags),
`engine/ctx.rs` (`Event` arms, `Token` arms and faces, `Zones.banishment`,
`xp`, `spend_xp`, `hunt_value`, `banish`, `burn`, `heal`, `shroud`,
`reveal_top`, `channel_exhausted`, `set_controller`, `attached_turn`,
`ambush_locations`, `queue_turn`, `prevent`, `ask_seat_resume`,
`ask_pay_or_let`, `await_faces`, `score_effect`, `contest` split out of
`arrived`, the `controller` override, and the four hook lines into
`statics`/`prevent`), `engine/triggers.rs` (every new `matches` row,
`IMPLICIT_HUNT` synthesis, `Once::PerSeatPerTurn`), `engine/targets.rs`
(every new `Filter` arm, `Equipment`, `SameLocationAsPicks` in
`candidates_with`, the universe widened to trash, banishment and revealed
hand cards, `FLAG_SHROUDED` in `untargetable`, `specs_of` returning
`Vec<TargetSpec>` and `spec_of_index` by value, the `HUNT_ABILITY` arm),
`engine/play.rs` hook lines only (`STAGE_REPEAT`, `STAGE_ADDITIONAL`, the
`SLOT_*` constants, `begin` starting spells at `STAGE_REPEAT`, the
`STAGE_ACCELERATE` fall-through to `STAGE_ADDITIONAL`, the three slot
readers, the `Event::Activated` raise in `finalize`, the `specs_of`
callers), `engine/prompts.rs` and `engine/hide.rs` (the `specs_of`
callers only), `engine/cleanup.rs` (the `run(ctx, last_item)` signature and
its callers across the engine, all `None`), `engine/combat.rs` (the
`result` stub), `engine/expiry.rs` (shroud, once-by-seat, preventions,
the seat counters), stubs `engine/{statics,prevent,control}.rs`,
`present.rs` (xp tail), `games/riftbound/src/lib.rs` (zone 13, two
tokens), `flake.nix`'s source filter, and the vanilla scripts: Punch
First, Premonition, Lecturing Yordle, Pit Rookie, First Mate, Disarming
Rake, Ruin Runner, Gust, Angler Beast, Thousand-Tailed Watcher, Sprite
Burst, Traveling Merchant, Zaun Warrens, Sand Soldier, Tentacle, plus the
registry rows of the 32 existing scripts the new files share. Tests: blob
v6 round trip with every new field and an unknown key; `PromptWhy::each()`
includes tag 13; token faces; the coverage tests green over six files;
the vanilla scripts' own tests; every M0–M8 test green with `picks` in
slots. Unlocks: everything below; on its own, 15 vanilla cards and the
Body rune.

**B1 — the statics projection and attachment.** Files: `engine/statics.rs`,
`engine/attach.rs`. Mechanics: Level, `While`, `Aura`, `MightIf` (the
conditional half only; `attach` keeps materializing `Might` and `Keyword`),
`attached_turn`, granted statics, Weaponmaster's attach path. Cards:
Master Yi - Wuju Bladesman, Master Yi - Tempered, Forbidding Waste,
Brutalizer, Mask of Foresight, Fiora - Peerless. Unblocks: B2's
Deflect-by-Level reading, B5's aura-fed damage.

**B2 — costs, payment, activations.** Files: `engine/cost.rs`,
`engine/pay.rs`, `engine/activate.rs`. Mechanics: XP spend, Burn cost,
`usable`/Empower, `IMPLICIT_FLOW` offers and routing (the `play::begin`
call only; the origin handling is B3's), `offers` keyed on
`ctx.controller`, `pay_self` for `SelfCost::BanishTarget`, discounts
(`SelfDiscount`, `SpellDiscount`, `next_discount`, the "[A] less" order),
`NotEnoughXp`, `AlreadyEmpowered`, `Cost::label`. Cards: Kha'Zix -
Voidreaver (its combat test lives in B5), Nasus, Ascended, Tail-Cloaked
Matriarch (its limited play needs B3; the script lands here with the
empower half tested), Shadow Order Disciple, Find Your Center, Sandswept
Tomb, Rampage, Akshan - Mischievous (waits on B1's `weaponmaster` and B4's
`control::sync`).

**B3 — plays, legality, the chain.** Files: `engine/play.rs`,
`engine/legal.rs`, `engine/chain.rs`, `engine/hide.rs` (`timing_at`'s
callers). Mechanics: the `STAGE_REPEAT` and `STAGE_ADDITIONAL` bodies, the
optional trigger-cost confirm at `STAGE_PAY`, the `execution` loop,
`Origin::Trash`/`Banishment` in `begin` (with its move onto the chain),
`finalize`, `cancel`, `finish` and `counter_item`, `from_trash`, Ambush
locations, `timing_at` with and without a location, the 822.3 re-check
and the `contest` call, the Temporal Breach location, Reaction units,
`face_arrived`, `cards_played`/`spells_played` at finalize, `PayOrLet`
plumbing in `resume`, `finish` passing `Some(item)` into `cleanup::run`.
Cards: Onslaught, Twilight Shroud, Up from the Deep, Fizz - Trickster,
Temporal Breach, Bellows Breath, Rengar - Trophy Hunter, Kha'Zix -
Mutating Horror, Janna - Savior, Crumbling Sands, Astral Heron, Nasus -
Curator of the Sands, Zed, Without a Sound, Shadow Clone. Unblocks: B2's
Matriarch play, B7's Hard Bargain and Sabotage resumes, B7's ambush
location filter.

**B4 — kills, cleanup, prevention, control.** Files: `engine/kill.rs`,
`engine/cleanup.rs`, `engine/prevent.rs`, `engine/control.rs`. Mechanics:
`Noted.alone`, `prevent::amount`, `after_combat` calling `combat::result`
after the kills and recall and raising `CombatWon`/`CombatLost` before
`establish`, `Cause::Cleanup { last_item }` from the `run` parameter and
`When::AfterKillsBy` firing after that cleanup, `control::sync`, the
banish-from-board detach path's cleanup recall. Cards: Lonely Poro,
Unyielding Spirit, Alpha Strike, Acceptable Losses (uses B0's
`ask_seat_resume`).

**B5 — combat and showdowns.** Files: `engine/combat.rs`,
`engine/showdown.rs`. Mechanics: `combat::result` (466.3),
`deals_combat_damage` in `might_sum`, prevention in `lethal`/`candidates`,
ambushers' designations in `refresh_open`. Cards: Vilemaw; the Voidreaver,
Horror, Fiora, Yi-legend and Forbidding Waste combat tests with fixture
cards.

**B6 — turns, moves, battlefields.** Files: `engine/phases.rs`,
`engine/march.rs`, `rules.rs`. Mechanics: extra turns, Awaken keyed on
`ctx.controller`, `NoMoveToBase` in routes and destinations,
`channel_exhausted`'s phase use, the seat counters' reset. Cards: Time
Warp, Vilemaw's Lair, Star Spring, Sigil of the Storm, Emperor's Dais,
Irresistible Faefolk, Void Assault, Retreat.

**B7 — prompts and information.** Files: `engine/prompts.rs`,
`engine/discard.rs`, `engine/mod.rs` (the `Reveal` arm), `engine/roll.rs`
(untouched unless Burn Out is pulled in). Mechanics: `PayOrLet` options,
status and `answer_words`; the `OptionalCost` phrasings for the four
slots; "{seat N}: choose …" for another seat's `Resume`; the
`PlayLocation` options filtered by `timing_at` (waits on B3); the Alpha
Strike status line; `reveal_top`'s host hand-off. Cards: Stacked Deck,
Sabotage, Decree of Strength, Decree of Focus, Hard Bargain (waits on B3's
Repeat), Ravenbloom Conservatory, Scuttle Crab's pool row check.

**B8 — kai and kai-cli.** Files: kai `src/table/{zones,counters,
plugin_ui}.rs`, `src/bin/kai_cli.rs`, the `plain_icons` table in
`src/ai/cards.rs` and the keyword glossary lines in `src/ai/brain.rs`.
Mechanics: the banish pile, XP read-only, the `PayOrLet` summary, the AI
phrases and icons, the xp tail in kai-cli. Depends on B0's manifest and
prompt tags and lands after the pipeline's P1, P2 and P4, which own
`deck/pinned.rs`, `ai/soak.rs`, the rest of `ai/cards.rs` and
`ai/brain.rs`; the importer changes are the pipeline's P3. Unblocks:
playing the decks at all in kai's enforced lobby.

After B1–B8: one integration pass reruns the M5 parity test, the M8 soak
over all six pool decks (each pair, both seats first) and the gas
benchmark on a Kha'Zix mirror with two Hunts, an aura and a repeated
Bellows Breath on the chain; the soak's zero-refusal bar and the 25% gas
bar are the gates, then the plugin 0.4.0 and kai 0.10.0 bumps, the
`Scripted:` lines flip to `complete`, and this section gains its commit
under "Landed".

### M9 pipeline — many decks through one pool

Not landed. How cards and decks flow from a tournament list to a pinned,
scripted, art-bearing deck at an enforced table, generalised from the two
house decks to six. Everything below was checked against the code on
2026-09-11 and, where it says "verified", against the live services. The
six pool files in the format below are already in
`games/riftbound/rules/pool/` — copied in, read by nothing yet; P0 wires
them.

**Decisions.** One Markdown file per deck under
`games/riftbound/rules/pool/`; `cardpool-lillia-irelia.md` splits into
`lillia-house.md` and `irelia-house.md` (its copy tables leave kai's
`soak.rs` and become deck blocks) and `cardpool-lillia-irelia.json` is
deleted (read by nothing). A file is an H1 label, a provenance line, a
`Scripted:` line, a `Sideboard:` line, a fenced deck block in the
text-list format the importer already parses, and the card lines in the
unchanged `- **Name** (id; Kind; Domain; cost): text` grammar; consumers
read the union of card lines, identical duplicates are fine, and a test
fails on any card whose line differs between files. kai's `pinned::Side`
becomes a data-driven `PoolDeck { slug, label, legend, champion, scripted
}` list read from the same files; the lobby offers every `Scripted:
complete` deck to both seats, a saved deck is preferred first by identity
(the pool deck's snapshot CiHash), then by exact label, and `--deck-a
pool:<slug-prefix>` accepts any pool deck. The importer gains
`SetCode::Opp` for parsing and printing, folds an OPP print to its base
print for deck codes (OPP has no Piltover Archive wire id, verified against
the published `mappings.ts` 1.4.0), and folds print names to base names
with a two-entry id alias table; riftdecks fetches work once the fetch
sends the honest `agni-importers` User-Agent with `Accept: */*`, verified
through ureq. The AI's `CardTexts` reads the pool union through the same
module and card-specific coaching moves out of `brain.rs` into a `##
Coaching` section per pool file. No art re-ingest is needed: both gateways
hold the 1451-card `riftbound` manifest and serve a blob for all 112 pool
ids (verified, HTTP 200 on every one), and the desktop fetches art on
demand by riftbound id. A pool-built or link-imported deck's history label
carries provenance. The batches P0–P6 keep off the files the M9 engine
batches and the UX milestones edit, except one first-landing change to the
test module of `cards/mod.rs`.

**Layout and format.**

```
games/riftbound/rules/pool/
  lillia-house.md          Lillia (house)        Scripted: complete
  irelia-house.md          Irelia (house)        Scripted: complete
  lillia-jonnynick.md      Lillia (Jonnynick)    Scripted: partial
  master-yi-akame.md       Master Yi (Akame)     Scripted: partial
  nasus-thundertrees.md    Nasus (ThunderTrees)  Scripted: partial
  kha-zix-hotkee.md        Kha'Zix (Hotkee)      Scripted: partial
```

The file name is the deck's slug: the legend's first word plus the pilot
or `house`, lower-case, ASCII, hyphenated; it is what `pool:<prefix>` and
every log line use. In the order a parser meets them: the H1 is the deck
label, used verbatim as the lobby button, the history label of a
pool-built deck, the soak record's deck name and the `Source::Pool` note,
unique across files (test). `Provenance:` is one prose line (event, pilot,
placing, field, date, URL, text source), shown on the docs page and not
parsed. `Scripted: complete | partial` gates enforcement: `complete` means
the coverage test demands a non-stub script for every non-rune card line;
`partial` means the file is staging for the M9 batches and the coverage
test checks only text drift, hidden-keyword consistency through
`PRINTED_HIDDEN` and registry membership; the commit that scripts a file's
last card flips the line; the lobby offers only `complete` decks in
enforced mode, the soak accepts any and prints how many faces will play as
generics. `Sideboard:` lists the sideboard with counts; the sideboard
faces are card lines in `## Cards` too (M9 scripts them — the pipeline
draft kept them out as scripting debt, and the M9 section says why they
are in) but are absent from the deck block, so the deal is the tournament
forty. `## Deck` holds exactly one fenced block in the text-list format
`agni_importers::riftbound::text_list::parse_text` accepts (header counts
like `Main Deck (39)` are stripped); the champion line holds the copy that
starts in the champion zone, further copies go in `Main Deck`, main plus
champion is 40; the block pasted into kai's import panel imports the same
deck. `## Cards` holds the card lines; a file's Cards section must contain
every name its deck block uses (test) and may contain extras
(`irelia-house.md` keeps Unsung Hero, scripted but filtered out of the
Irelia deck by domain identity, so "every script is a pool card" holds).
Ids are base prints from a set the Piltover Archive codec knows: Consult
the Past is `ogn-083-298` (same text, verified in the catalog dump);
Master Yi's legend stays `ogs-019-024` because Riftcodex has no unsuffixed
print, with the folded base name on the line. 34 of the 112 faces appear
in more than one file (Defy in five); the union is keyed by name, the
first file in table order supplies the row, and a differing line fails
`every_shared_card_reads_the_same_in_every_pool_file` — re-fetching a
card's text means editing every file that carries it, which is the
intended pressure.

**One parser, three consumers.** Today three hand parsers read the file
(`cards/mod.rs` 591–614, kai `ai/cards.rs` 116–149, kai `ai/soak.rs`
272–305). After this: kai `src/deck/pool.rs` (new, platform-independent,
compiled on wasm too) is the only parser in kai — it `include_str!`s every
pool file in a `FILES: &[(&str, &str)]` table, parses the H1, the
`Scripted:` line, the deck block and the card lines, and exposes `decks()`,
`cards()` (union, first occurrence wins, drift rejected in a test),
`deck(slug_prefix) -> Result<ResolvedDeck, PoolError>` and `catalog() ->
StaticCatalog`; `deck()` runs the fenced block through `text_list::
parse_text` and `agni_importers::deck::resolve` against a `StaticCatalog`
made from the union's card lines, so pool decks and imported decks go
through the same resolver and the same zone placement, and because
`catalog`, `text_list` and `resolve` are available on wasm this closes
"web builds get no pool" for free. `ai/soak.rs` drops `POOL`, `PoolRow`,
`pool_rows`, `pool_copies`, the copy tables, `in_domain_identity` (moves
to `pool.rs` as a legality check) and `pool_deck`; `pool_deck_names()`
becomes a thin call into `deck::pool::decks()`; kai-cli's soak keeps its
`pool:` prefix. `ai/cards.rs::from_pool(markdown)` becomes `from_pool()`
built from `deck::pool::cards()`. The engine tests keep their own tiny
parser (the crate depends only on the SDK) but read the union: `POOL_FILES:
&[(&str, &str)]` of `include_str!("../../../riftbound/rules/pool/<file>.md")`,
with `pool_rows()` iterating all of them and a `scripted_files()` filter.
The nix source filter replaces the single-file line with the directory
`./agni/agni/games/riftbound/rules/pool`, so adding a deck never touches
nix again. The two `include_str!` tables are the only per-deck registration
points; a test in each crate reads the directory at test time and fails
when a file on disk is missing from the table.

**The lobby pin, generalised.** `deck::pool::decks()` yields `PoolDeck`s
in file-table order; `Side::ALL` becomes `pool::pinnable()` (the
`scripted` ones); `matches(label)` compares the whole label because two
Lillia decks share a legend; `other()` becomes `pool::another_than(legend)`
— the first pinnable deck whose legend differs, the second entry when none
does (mirror legends are legal); `default_for(role)`: the host takes the
first pinnable deck, a joiner `another_than` the host's legend when one is
on the table and the second entry otherwise, which keeps every `side_after`
test (an off-list host legend now yields `another_than`, no longer `None`).
`PinnedDeck.side` and `Seating.side` become the slug; `seating_holds` keys
on it. `resolve(slug, rows)` prefers, in order: a held row whose `ci`
equals `history::identity_of(&snapshot(pool deck))` (the user's own copy
of the pool deck, with its art URLs and provenance); a held row whose
label equals the pool label exactly; the pool deck built by
`deck::pool::deck(slug)` as `Source::Pool(label)`; `Source::Missing` only
when the pool file itself failed to resolve, which a test makes impossible
for every shipped file. Copy: "Lillia and Irelia are the scripted pool"
becomes "rules enforced pins the decks to the scripted pool ({n} decks)";
`menu.rs` 515 becomes "the table opens with the rules enforced; the decks
are pinned to the scripted pool" and `ENFORCED_HINT` "the plugin refuses
illegal plays and runs the turn; only the pool decks are scripted"; the
README's pool paragraph is rewritten around the directory. The soak:
`pool:<prefix>` resolves through `deck::pool::deck`, a unique prefix wins,
an ambiguous one (`pool:lillia`) errors with the candidates, so the
existing `pool:lillia`/`pool:irelia` invocations become `pool:lillia-house`
and `pool:irelia-house`; records keep the label as the deck name; a
`partial` deck is accepted and the run's first line says how many faces
resolve to generics.

**Importer fixes.** `SetCode` gains `Opp` in `parse` and `as_str`;
`to_wire` becomes `Option<u8>` with `None` for `Opp` (the published map is
OGN 0, OGS 1, ARC 2, SFD 3, UNL 4, VEN 5, RAD 6 and nothing else; inventing
a number would produce codes no other tool decodes); `deck_to_code` folds
an OPP card to its base print before encoding through
`CardCode::base_print(total)` — OPP prints are organized-play copies
numbered like their base set and the id's total names it (298 OGN, 221
SFD, 219 UNL, 024 OGS; 125 of 133 OPP prints have a same-number twin, the
eight that do not are `b`-variant runes and two promos) — applied where
the riftbound id is known; a bare `OPP-083` from a code list stays `Opp`
and `deck_to_code` returns `None` with the reason. Text and code lists
with `OPP-083` or `opp-083-298` now parse. Print names: Riftcodex names
prints, not cards, and the two endpoints disagree (`ven-192-166` is "Nasus
- Curator of the Sands (Overnumbered)" on the list and "Curator of the
Sands" on the single-card endpoint; `ogs-019-024` is "(Starter)",
`opp-019-024` "(Metal)"), and `script_of` is an exact binary search, so
every such face plays as a vanilla generic today. Two layers:
`agni/importers/src/naming.rs::base_name` strips one trailing
parenthesised suffix when it is one of Starter, Alternate Art,
Overnumbered, Signature, Metal, Promo, Prerelease, Foil (a legitimate
parenthesis such as "Teemo - Scout (GG EZ)" is kept), applied in
`riftcodex.rs::catalog_card`, `ingest.rs::catalog_card` and
`ingest::record`; and `riftbound/prints.rs::ALIASES` of riftbound id to
base name for the prints Riftcodex renamed outright (`ven-192-166` →
"Nasus - Curator of the Sands", `ven-179-166` → "Rengar - Trophy Hunter"),
with a test that every alias target is a pool name. Defence in depth in
the engine: `cards::resolve(face)` tries `script_of(face.name)` then
`script_of(base_name(face.name))` with a local copy of the suffix rule,
so a deck imported on an older kai or a snapshot recalled from history
with a print name still scripts; the alias table is not duplicated there
and old snapshots are fixed by the history backfill when the store
catalog knows the id. Riftdecks: tested against three deck pages through
ureq, curl and urllib — Cloudflare refuses a request that claims to be a
browser (Mozilla UA or a browser Accept list) but arrives over HTTP/1.1
with a non-browser TLS fingerprint, serves an honest non-browser UA with a
plain Accept, and blocks only the literal `curl/` UA; the phase-1 note
that "a plain curl with a browser UA got 200" holds only because curl
negotiates HTTP/2. So `query.rs` drops the Firefox UA and the browser
Accept list, `UreqFetch::new()` builds the transport with `art::USER_AGENT`
and sends `Accept: */*`; the paste hint stays as the fallback; three
fetches inside a minute earned a 429 and `TransportError::transient`
already retries it. The web build resolves through the gateway, whose
`deck-gateway` binary uses the same `UreqFetch`, so the gateway on omashu
benefits once redeployed. `plugins.md`'s deck-import paragraph is
corrected accordingly.

**The AI seat.** `CardTexts::from_pool()` reads every face of every pool
file with `plain_icons` unchanged, so every pool card has a reference line
without a store ingest; `CardTexts::load(store_dir)` still takes precedence
when the store is ingested. `brain.rs::system_prompt` names a dozen house
cards today; it keeps a card-agnostic paragraph per mechanic (chain and
priority, hidden cards, equip, swaps, reveals and peeks, discards,
replacements, rolls, refused commands) with no card names, and the M9
keywords each get one glossary sentence when their `PromptWhy` variant
lands, since the answer vocabulary in the `act` tool text and
`PromptWhy::each()` are what the AI needs; that is the AI owner's task,
sequenced after each engine batch. Each pool file gains an optional `##
Coaching` section of prose bullets, one per card that needs it, read by
`deck::pool::coaching(slug)`; the brain appends its own deck's coaching at
game start and the opponent's once `legends_on_table` names their legend;
the card author writes the coaching line in the commit that scripts the
card. For a `partial` deck the brain's first state line says how many
cards play as vanilla, from the `Scripted:` line until the engine exports
`is_generic` per card.

**Art.** Verified: `kai/src/net/gateway.rs` loads the `riftbound`
manifest from the first reachable default peer and `riftbound_art` fetches
`/gateway/blob/{hash}` by riftbound id; both `dev1` and `dev2` report the
set complete, the manifest lists 1451 cards with an image hash each, all
112 pool ids are present and every blob answered 200. Desktop:
`stage_store_art` serves from the local manifest when it exists and
`prefetch_deck_art` → `SourceFetcher::riftbound_want` falls back to a live
Riftcodex `by_id` per missing face at one request per second, journaling
the bytes into the store, so a pool deck's art lands on first use; fifty
new faces is under a minute, and the Settings full-set download remains
the way to do it once. The three token faces are M9 engine work
(`token_table`). Headless, the `ingest-riftbound` bin takes the same set
scoped to ids: `ingest-riftbound <store> --audit --pool <file.md>...`
lists the pool ids whose art the store lacks and writes nothing;
`--only <id>...` and `--ids <file|->` name ids directly; without `--audit`
the missing faces are fetched through `ingest::ingest_ids` (Riftcodex
`by_partial_id`, one request per second, journaled, merged into the
manifest as full records). `kai/scripts/ingest-pool-art.sh <store>` wraps
it with every pool file. Ops: redeploy `deck-gateway` after P3; and when
kai's wire or blob version bumps, the "kai wire version deploys" rule —
main first, then the gateway.

**History labels and provenance.** Pool-built decks are not saved; their
label for notes and soak records is the file's H1. When the user saves one,
`history::label` returns the H1 because `ResolvedDeck` gains `label:
Option<String>` that the pool builder sets, round-tripped through a new
optional `label` on `agni_deck::Snapshot` (serde default, skipped when
`None`, excluded from `identity()` so the CiHash keeps meaning "these
cards"). Link imports: `extract` returns a `title` read from `og:title`
(riftdecks: "8/14/2026 nasus by ThunderTrees"), carried on `ParsedDeck`
and `QueryReply.body["title"]`, and kai's `parse_reply` labels the deck
`"{legend} — {title}"`; pasted lists and codes keep the legend-name label;
`pinned::saved_row` matches whole labels or identities, never prefixes.
`import_td(source)` becomes `(source_kind, source_detail, version)` with
the URL or `pool:<slug>` as the detail; showing it in the history window
is a UX nicety. The docs page lists the six decks from the files' H1 and
Provenance lines.

**Tests.** Engine crate, `cards/mod.rs` test module: every pool file on
disk is in the table; every shared card reads the same in every file;
every deck block names only cards of its own file; labels unique and
`Scripted:` lines well formed; the hidden test reworked over the union
(`{names printing [Hidden]}` equals `{scripts claiming Keyword::Hidden} ∪
PRINTED_HIDDEN`, the literal eight-name list gone); the registry test
unchanged in meaning; the coverage test split by `Scripted:` (for
`complete` files every non-rune card resolves to its own non-stub script
and every Rune row to `generic::RUNE`; the `generic_ones ==` literal and
the `CARDS.len()` arithmetic replaced by set equality between `CARDS` and
the union of scripted pool names plus Gold); a print name resolves to the
base script. kai `deck/pool.rs`: every pool file builds a legal
constructed deck (the `legal_constructed` body moves here from `soak.rs`:
main plus champion 40, runes 12, battlefields 3, at most 3 copies counting
the champion, every card inside the legend's identity, no duplicate rows);
no text drift and first file wins; slug prefixes resolve uniquely or name
the candidates; decks carry label and flag; the table matches the
directory (native only). kai `deck/pinned.rs`: the `Side` tests rewritten
against `PoolDeck`; a saved copy wins by identity; a saved deck wins by
whole label, not prefix; partial decks are not offered. kai `ai`:
`from_pool` covers every pool face; coaching is appended only for decks at
the table. kai-cli soak: the invocations renamed, and `--deck-a pool:nasus`
runs a random game to a result printing the generic count. Importer:
`OPP-083`, `opp-083-298`, `OPP-SP1` parse, `to_wire` is `None` for Opp,
`base_print` folds `opp-083-298` → `ogn-083-298`, `opp-019-024` →
`ogs-019-024`, `opp-118a-221` → `sfd-118a-221`; a deck with an OPP print
encodes to the base print's code and a bare code-list `OPP-083` gives
`None` with the reason; `base_name` on the eight suffixes and on "Teemo -
Scout (GG EZ)"; every alias target is a pool name; `catalog_card` returns
base names for the fixture prints and for `ven-192-166`; the fetch sends
`art::USER_AGENT` and `Accept: */*`; the riftdecks fixture gains an
`og:title`; and an ignored live test `riftdecks_answers_the_honest_user_
agent` for the day Cloudflare changes its mind.

**Batches and ownership.** The M9 engine batches own `cards/<card>.rs`,
`cards/mod.rs` outside its test module, `engine/*.rs`, `state.rs`,
`present.rs` and the plugin manifest; the UX milestones own `menu.rs`
beyond the two copy strings, `plugin_ui.rs`, `scene/`, `render/`, `app.rs`,
`web/`, `android/`.

| Batch | Files | Owner | Must land before |
|---|---|---|---|
| P0 pool files | the six files under `games/riftbound/rules/pool/` (in place); delete `rules/cardpool-lillia-irelia.{md,json}`; `flake.nix` line 38 (the single-file entry becomes the directory); `games/riftbound-turns/src/cards/mod.rs` line 589 (`include_str!` of the old file); kai `src/ai/soak.rs` line 9 (`include_str!` of the old file); kai `README.md` 197–215 and `AGENTS.md` line 62 (the `soak` entry names the old file); `rules-engine.md` lines 5 and 2117 (the old file names); `rules/README.md` (pointer to the directory) | pool author | everything |
| P0 engine test module | `cards/mod.rs` test module only (`POOL_FILES`, the union, tests above), `PRINTED_HIDDEN` gets the hidden cards of the four partial files, `cards::resolve` gains the suffix fold | pool author, one commit, reviewed with the M9 lead | M9 B0 (which then only adds registry lines and removes `PRINTED_HIDDEN` entries) |
| P1 kai pool module | new `kai/src/deck/pool.rs`, `kai/src/deck/mod.rs` (one `pub mod`), `kai/src/ai/soak.rs` (delete the parser and copy tables), `kai/src/ai/cards.rs` (`from_pool`), `kai/src/bin/kai_cli/soak.rs` (`resolve_deck`, tests), `kai/AGENTS.md` 62, `kai/README.md` 197–215 | kai pipeline implementer | P2, P4, M9 B8 |
| P2 lobby pin | `kai/src/deck/pinned.rs`, the two strings at `kai/src/menu.rs` 515 and 596 | kai pipeline implementer | the UX milestone that restyles the lobby, M9 B8 |
| P3 importer | `agni/importers/src/riftbound/card_code.rs`, `deck_code.rs`, `mod.rs` (`deck_to_code`), `naming.rs`, new `prints.rs`, `riftcodex.rs`, `ingest.rs` (`catalog_card`, `record`), `query.rs`, `link.rs`, `text_list.rs` tests, `wiki/design/deck-import.md` | importer implementer; independent of P0–P2 | gateway redeploy |
| P4 AI | `kai/src/ai/brain.rs` (`system_prompt` split, coaching hook), `## Coaching` sections in the pool files (append-only, per card, by whoever scripts the card) | AI owner; glossary lines land after each M9 batch | M9 B8 |
| P5 history and labels | `agni/games/deck/src/snapshot.rs` (optional `label`), `agni/games/riftbound/src/lib.rs` (`ResolvedDeck.label`), `kai/src/deck/history.rs` (`label`, `import_td`), `kai/src/deck/import.rs` (`parse_reply` title) | kai pipeline implementer, after P3's `title` | UX history window work |
| P6 ops | redeploy `deck-gateway` on omashu after P3; optional full desktop ingest; docs page listing the six decks | ops | none |

Sequencing: P0 is one small PR that lands first because both the engine
batches and the kai batches read the new files. P1–P2 and P3 run in
parallel with M9 B1–B7 and with the UX milestones; P4's coaching text is
appended to the pool files by the card authors as they script, the only
recurring touch on the pool files and append-only per file so concurrent
card PRs rebase cleanly. P5 waits for P3's `title`. No pipeline batch edits
`engine/*.rs`, `state.rs`, `present.rs`, `plugin_ui.rs` or the scene code.

**Risks and open points for rae.** Naming the two current decks "(house)"
is a placeholder; the mechanism keys on the H1, so renaming them later is
a one-line edit per file plus the soak invocations. The house decks change
shape slightly: the champion's second main-deck copy becomes the
champion-zone copy, so they deal 40 cards instead of today's 41, matching
imported decks and the rules — call it out in the commit. The
identity-first preference means a saved riftdecks import of the same list
with alternate-art ids is preferred by label, not identity; acceptable and
documented. Cloudflare can change its mind; the ignored live test exists
for that day and the paste fallback stays. `Scripted: partial` decks are
hidden from the enforced lobby by design; to play them early the flag is
one word in the file and the soak already accepts them. `Snapshot.label`
is a blob-format addition, optional and serde-defaulted so old blobs load,
but the wire-version deploy rule applies to anything that changes what the
gateway serializes.

### M10 — the Origins set: triage, stubs and gaps

Landed as a triage, not as scripts. `games/riftbound/rules/pool/origins.md`
is the first *set file*: every base print of Origins, ogn-001 to ogn-298
in collector order, in the unchanged card-line grammar, with `Scripted:
partial`, no deck block and no coaching. The 37 rows it shares with the six
deck files read byte for byte the same, which the coverage test enforces.
The consumers learned the one distinction a set file needs: kai's
`deck::pool::decks()` lists only files with a deck block
(`is_deck_file`), so a set file is never a deck to pin, coach or soak,
while `cards()` and `CardTexts::pool()` take its rows into the catalog and
the card reference like any other file; the engine's coverage tests skip
the deck-block checks for a file without one and, under the partial gate,
accept a script that is still a stub. The rune list the tests pin grew from
the four pool domains to all six.

**Stubs.** Every non-rune, non-token Origins card without a script got
`cards/<snake_name>.rs` exporting `pub static CARD: Card` as the generic
constructor of its kind (`unit`, `spell`, `gear`, `battlefield`, `legend`)
with the pool name and the keywords the card prints as its own — the
bracketed keyword lines of the rich text, Assault/Shield/Deflect with
their numbers — and nothing else. A keyword the text grants or mentions
(Cleave's `[Assault 3]`, Noxus Saboteur's `[Hidden]`) is not the card's,
and the Hidden coverage test now reads a printed Hidden as a text that
*starts* with `[Hidden]`. All 255 are registered in `cards/mod.rs` in
sorted order, so the scripters own only their card files. A stub counts as
written once it carries an ability, a static, a replacement or an
additional cost — or once the printed text is nothing but keyword lines,
which is what `keyword_only` in the coverage test accepts, so the fifteen
vanillas need only their tests. The three Recruit prints and the Sprite
print are token faces: they are rows of the set file, resolve to the
generic unit, and carry no file.

**Groups.** Each is one pattern to reuse, sized for one scripter; the
theme names the shape to copy and the seam to name where the vocabulary
ends.

| Group | Theme | Cards |
|---|---|---|
| `units-vanilla` (15) | plain units: the printed keywords are the whole script (Accelerate, Deflect, Shield, Tank, Assault, Hidden); the stub is complete once its unit test pins the keywords; Vision is an engine gap | Blazing Scorcher, Legion Rearguard, Pouty Poro, Playful Phantom, Stalwart Poro, Sunlit Guardian, Mega-Mech, Pakaa Cub, Mountain Drake, Shipyard Skulker, Daring Poro, Petty Officer, Vanguard Sergeant, Mystic Poro, Jeweled Colossus |
| `units-play-effects` (14) | units with a play trigger that draws, deals, stuns, bounces, kills or spawns (Lecturing Yordle / Riptide Rex shape: play(targets, run)) | Riptide Rex, Solari Shieldbearer, Blastcone Fae, Whiteflame Protector, Zaunite Bouncer, Maddened Marauder, Harnessed Dragon, Solari Chief, Carnivorous Snapvine, Kadregrin the Infernal, Mindsplitter, Cemetery Attendant, Teemo - Scout, Sprite Mother |
| `units-play-buffs-legion` (13) | units whose play trigger buffs, discards, channels or is gated by Legion (Pit Rookie shape, legion(ctx, seat) as the condition, Card.additional for Clockwork Keeper) | Kinkou Monk, Peak Guardian, Poro Herder, Cithria of Cloudfield, Trifarian Gloryseeker, Dangerous Duo, Scrapyard Champion, Noxus Hopeful, Darius - Executioner, Chemtech Enforcer, Jinx - Demolitionist, Stormclaw Ursine, Clockwork Keeper |
| `units-attack-defend` (11) | units with attack or defend triggers that deal, stun, reveal or play (on_attack / on_defend + deal / stun; Twisted Fate and Teemo - Strategist need the reveal seams) | Crackshot Corsair, Dune Drake, Yasuo - Remorseful, Anivia - Primal, Volibear - Furious, Warwick - Hunter, Leona - Determined, Ahri - Inquisitive, Ava Achiever, Twisted Fate - Gambler, Teemo - Strategist |
| `units-conquer-hold-move` (12) | units with conquer, hold and move triggers (on_conquer_me / on_hold_me / on_move, once_each_turn; Yasuo - Windrider and Kayn need a per-turn move counter, Volibear - Imposing an enemy-move trigger) | Ahri - Alluring, Blitzcrank - Impassive, Qiyana - Victorious, Kai'Sa - Evolutionary, Miss Fortune - Captain, Stealthy Pursuer, Yasuo - Windrider, Kayn - Unleashed, Kai'Sa - Survivor, Tryndamere - Barbarian, Vayne - Hunter, Volibear - Imposing |
| `units-death-discard` (14) | Deathknells, death watchers and discard triggers (deathknell(...) with Noted; friendly-death, discard and recycle triggers are engine gaps, scripted with a named seam) | Watchful Sentry, Tasty Faefolk, Undercover Agent, Soaring Scout, Kog'Maw - Caustic, Ekko - Recurrent, Karthus - Eternal, Wraith of Echoes, Immortal Phoenix, Flame Chompers, Jinx - Rebel, Raging Soul, Brazen Buccaneer, Karma - Channeler |
| `units-statics-auras` (14) | units with continuous effects: Static::While for conditional self Might and keywords, Static::Aura for other units here or all friendly units (Ruin Runner / Master Yi - Wuju Bladesman shape); Draven, Dr. Mundo and Sett - Kingpin need a dynamic Might grant | Wielder of Water, Wizened Elder, Bilgewater Bully, Fiora - Victorious, Captain Farron, Taric - Protector, Lee Sin - Centered, Gemcraft Seer, Draven - Showboat, Dr. Mundo - Expert, Sett - Kingpin, Leona - Zealot, Magma Wurm, Noxus Saboteur |
| `units-activated-costs` (11) | units with activated abilities, cost reductions or unusual play costs (activated(...) + exhausting_self, Static::SelfDiscount, Card.additional; kill/recycle/discard costs are engine gaps) | Caitlyn - Patrolling, Vi - Destructive, Lee Sin - Ascetic, Malzahar - Fanatic, Heimerdinger - Inventor, Cruel Patron, Commander Ledros, Rhasa the Sunderer, Herald of Scales, Eager Apprentice, Raging Firebrand |
| `units-timing-location` (13) | units with play-location, entry and timing rules: open or enemy battlefields, Reaction units, enters ready, opponent locks, end-of-turn and play-watch triggers (Rengar / Janna - Savior shape) | Deadbloom Predator, Sneaky Deckhand, Sai Scout, Miss Fortune - Buccaneer, Shen - Kinkou, Nocturne - Horrifying, Mageseeker Warden, Brynhir Thundersong, Sona - Harmonious, Pit Crew, Eclipse Herald, Darius - Trifarian, Ember Monk |
| `spells-damage` (14) | damage spells, Action and Reaction (Punch First / Hextech shape with deal(); Shakedown is a pay-or-let, Bullet Time an X cost, Super Mega Death Rocket! a trash trigger) | Hextech Ray, Disintegrate, Void Seeker, Falling Comet, Falling Star, Icathian Rain, Cannon Barrage, Flurry of Blades, Sky Splitter, Get Excited!, Challenge, Shakedown, Bullet Time, Super Mega Death Rocket! |
| `spells-might-stun` (12) | this-turn Might, keyword grants and stuns (Discipline / Cleave shape: might_this_turn, grant_this_turn, stun; Stand United needs a turn-scoped aura) | Cleave, Block, Primal Strength, Grand Strategem, Back to Back, Siphon Power, Convergent Mutation, Last Stand, Stand United, Rune Prison, Facebreaker, Zenith Blade |
| `spells-kill-move-control` (14) | kill, bounce, move and control spells (Rebuke / Charm / Rampage shape; Possession uses set_controller, Imperial Decree and Noxian Guillotine need a turn-scoped damage watcher) | Vengeance, Hidden Blade, Thermo Beam, Salvage, Cull the Weak, Fading Memories, Fight or Flight, Possession, Dragon's Rage, Last Breath, Showstopper, Stormbringer, Noxian Guillotine, Imperial Decree |
| `spells-counter-draw-channel` (11) | counters, draw, channel and ready spells (Defy / Retreat / Find Your Center shape; Meditation needs an exhaust-a-unit additional cost, Party Favors a modal per seat, Mystic Reversal chain-item control) | Wind Wall, Mystic Reversal, Meditation, Progress Day, Spoils of War, Mobilize, Catalyst of Aeons, Confront, Party Favors, Morbid Return, Sprite Call |
| `spells-library-each-player` (13) | look-at-top-N, trash and Banishment plays, each-player sequencing (Stacked Deck / Temporal Breach / Fizz / Acceptable Losses shape) | Reinforce, Blind Fury, Promising Future, Portal Rescue, The Harrowing, Guerilla Warfare, Whirlwind, King's Edict, Divine Judgment, Invert Timelines, Fox-Fire, Spectral Matron, Soulgorger |
| `gear-seals-card-flow` (11) | Seals and other [Add] sources plus draw/channel gear (the [Add] payment seam generalises Gold; Garbage Grabber's recycle cost is a gap) | Seal of Rage, Seal of Focus, Seal of Insight, Seal of Strength, Seal of Discord, Seal of Unity, Energy Conduit, Garbage Grabber, Mushroom Pouch, Treasure Trove, Scrapheap |
| `gear-activated-watchers` (12) | exhaust-activated gear and gear that watches the turn (activated(Timing, cost, targets, run) on gear; Ravenborn Tome, Unlicensed Armory, Symbol of the Solari and Dazzling Aurora need seams) | Iron Ballista, Orb of Regret, The Syren, Pack of Wonders, Sun Disc, Ravenborn Tome, Unlicensed Armory, Baited Hook, Symbol of the Solari, Dazzling Aurora, Solari Shrine, Pirate's Haven |
| `buff-economy` (14) | spend-a-buff and buff-watch cards across kinds (buff()/is_buffed exist; spending a buff as a cost or effect is one engine gap shared by the whole group) | Wallop, Call to Glory, Overt Operation, Kraken Hunter, Albus Ferros, Udyr - Wildman, Sett - Brawler, Wildclaw Shaman, Monastery of Hirana, Sett - The Boss, Arena Bar, Mistfall, Vanguard Helm, Spirit's Refuge |
| `recruit-tokens` (9) | Recruit token makers across kinds (Token::Recruit is one engine gap; spawn() and the Sprite shape are the pattern) | Faithful Manufactor, Vanguard Captain, Noxian Drummer, Machine Evangel, Viktor - Innovator, Viktor - Leader, Forge of the Future, Viktor - Herald of the Arcane, Altar to Unity |
| `battlefields-triggers` (10) | battlefields with hold, conquer, defend, move and choose triggers (Zaun Warrens / Sunken Temple / Star Spring shape) | Grove of the God-Willow, Navori Fighting Pit, Startipped Peak, Hallowed Tomb, Reckoner's Arena, The Candlelit Sanctum, Fortified Position, Reaver's Row, Back-Alley Bar, The Dreaming Tree |
| `battlefields-statics` (8) | battlefields with continuous effects and game-rule changes (Forbidding Waste / Vilemaw's Lair shape; victory score, hide capacity, bonus damage and first-Beginning-Phase are gaps) | Aspirant's Climb, Bandle Tree, Obelisk of Power, The Arena's Greatest, Trifarian War Camp, Void Gate, Windswept Hillock, The Grand Plaza |
| `legends` (10) | legends: exhaust activations and legend triggers (Kha'Zix - Voidreaver / Lillia - Bashful Bloom / Irelia - Blade Dancer shape; the [Add] legends share the Seals seam) | Kai'Sa - Daughter of the Void, Volibear - Relentless Storm, Jinx - Loose Cannon, Darius - Hand of Noxus, Ahri - Nine-Tailed Fox, Lee Sin - Blind Monk, Yasuo - Unforgiven, Leona - Radiant Dawn, Teemo - Swift Scout, Miss Fortune - Bounty Hunter |

**Engine gaps.** The triage's estimate of where the vocabulary would end
is superseded by the section below, written after the scripting pass from
the seams the card files actually name.

### M10 — what Origins still needs

The scripting pass landed 255 card files beside the 33 the six deck
files already scripted, one for each of the 288 non-rune, non-token rows
of the set file: 235 cards run end to end on the M0–M9 vocabulary, 16 are
vanillas whose printed keywords are the whole script (the fifteen the
triage listed plus Shen - Kinkou), and 37 stay stubs — `CARD` is the
generic constructor of its kind with the printed keywords — because their
entire text is one mechanic the engine does not have yet. `origins.md` therefore
stays `Scripted: partial`; the coverage test pins the 37 by name
(`SEAM_STUBS`) and the vanillas by count, so a stub that gains a script
or a script that regresses to a stub fails the build.

Every gap is a *seam*: a `pub fn` in the card file whose name says what
the engine still owes, already tested for what it reads, and an
`#[ignore = "engine gap · …"]` test that states the behaviour the card
wants end to end and fails today for the stated reason — all 106 of them
fail when run with `--ignored`, none is a placeholder. When a primitive
lands, the engine consults the seam and the ignored test is un-ignored;
the card file itself does not change unless the row says so. The table is
ordered by how many cards each primitive unblocks; "mechanic → cards →
primitive" is the reading order, and the rules numbers are the Core
Rules paragraphs the primitive implements.

| Mechanic | Cards | Primitive the engine owes |
|---|---|---|
| Vision (817) | Mystic Poro, Jeweled Colossus, Sai Scout, Karma - Channeler, Gemcraft Seer (grants it through an aura) | `IMPLICIT_VISION` beside `IMPLICIT_TEMPORARY`: an implicit play trigger on every unit carrying `Keyword::Vision` at the moment it enters (aura grants included) that peeks the top card of the controller's main deck to the controller, asks `[{card top}, skip]`, and recycles the pick to the bottom; once per instance, one chain item. The cards keep only the keyword so nothing double-fires when it lands; Karma's own Vision recycle then feeds her Recycled trigger. |
| [Add] payment sources | Seal of Rage, Seal of Focus, Seal of Insight, Seal of Strength, Seal of Discord, Seal of Unity, Energy Conduit, Kai'Sa - Daughter of the Void, Darius - Hand of Noxus, Malzahar - Fanatic | `pay::plan` / `pay::choose_payment` generalised from the Gold token: a ready card with an [Add] ability stands in for the resource it adds (a power of its domain, one energy, one rainbow for spells only, one energy under Legion, two rainbow for a kill) and pays by exhausting at the pay stage, uncounterable (`adds_while_paying(ctx, seat, card, item) -> Option<Cost>` is the seam each card exposes). Malzahar's is also a kill-a-friendly-unit-or-gear activation cost. |
| Token::Recruit | Faithful Manufactor, Vanguard Captain, Noxian Drummer, Machine Evangel, Viktor - Innovator, Viktor - Leader, Forge of the Future, Viktor - Herald of the Arcane, Altar to Unity; Cithria of Cloudfield | a `Token::Recruit` face in `engine/ctx.rs` (1 Might, no domain, the Recruit tag), the manifest token in `games/riftbound/src/lib.rs`, `TOKEN_RECRUIT` and `is_token_name` in `cards/mod.rs`; `faithful_manufactor::spawn_recruit` builds the face by hand and replays `Ctx::spawn` until then, and every Recruit maker calls it. 350.2 makes a token a played card: since M12 `Trigger::YouPlayCard` matches the `Origin::Board` play a spawn raises, so a spawned Recruit or Sprite buffs Cithria; the "when you play a card" readers (Viktor - Innovator, Astral Heron, Darius - Trifarian) filter with `prelude::a_card_not_a_token` because a token is not a card (185). |
| missing trigger subjects | Wraith of Echoes, Viktor - Leader, Vanguard Helm (`UnitDies(Who::Friendly)`, `Noted.buffed`); Jinx - Rebel, Flame Chompers, Scrapheap, Raging Soul (a `Discarded` event, one per batch); Eclipse Herald, Leona - Radiant Dawn, Solari Shrine (an `Event::Stunned { units, by }` raised once per stun instruction, `Noted.stunned`); Karma - Channeler (`Recycled(Who::You)`, runes are not cards); Mistfall (`Buffed(Who)`); Pirate's Haven (`Readied(Who::Friendly)` handing the readied unit over as the subject); Volibear - Imposing, Ahri - Nine-Tailed Fox, Back-Alley Bar (`Who::Enemy` / `Who::Any` for `Move` and `Attacks`); The Dreaming Tree (`Who::Any` on `ChosenFriendly`); Immortal Phoenix, Solari Shrine (a killer on `Died`); Treasure Trove (`Trigger::Death` covers only kills — a bounce or banish off the board must fire it); Obelisk of Power, The Arena's Greatest (a battlefield's `BeginningPhase` reads its holder only, "each player's" needs the turn player) | `Trigger` variants and `triggers::matches` arms for each subject above; the events `Ctx::stun`, `Ctx::buff`, `discard` and the recycle paths do not raise today. Each card carries the condition (`another_non_recruit_unit_of_yours_died`, `you_stunned_one_or_more_enemy_units`, `a_friendly_unit_died`, …) and the run; the row's ignore reason names the exact wiring, e.g. `once_each_turn(triggered(UnitDies(Friendly), &[], echo))`. |
| non-resource costs at the pay stage | Cruel Patron (kill a friendly unit, mandatory), Commander Ledros (kill any number, one Order off per kill), Malzahar - Fanatic (kill one as an activation cost), Meditation (exhaust a friendly unit), Brazen Buccaneer (discard 1, two energy off), Unlicensed Armory (discard as the base cost), Garbage Grabber (recycle three from your trash), Vi - Destructive (`SelfCost::RecycleTarget`), Ekko - Recurrent (`SelfCost::RecycleSelf`, 383.3.b paid at finalization) | additional-cost and activation-cost kinds beyond energy, power, XP, Burn, exhaust-self and kill-self: each is a pick prompt at the pay stage (355.10.c, 357) that raises no `Chosen`, cannot be undone by a response, and is recorded on the item (`paid_additional` grows a payload) so a `SelfDiscount` can read it. `Card.additional` today is one optional rune cost. The kill/discard/exhaust/recycle candidates and the discount arithmetic are live in each file (`kill_candidates`, `discount_for`, `pay_kills`, `exhausted_as_additional_cost`, …), gated off by `usable_if` or run at resolution until then. |
| spend a buff | Wallop, Call to Glory (a buff spent instead of the printed cost), Kraken Hunter (any number, one Body off each), Sett - Brawler (`SelfCost::SpendBuff` at activation, 204.1.b), Lee Sin - Ascetic (any number of buffs), Udyr - Wildman (modes used this turn) | `Ctx::spend_buff(card)` as an effect and as a cost kind (the same pay-stage machinery as the row above, with "ignore the printed cost" and "per-buff power discount" flavours); `COUNTER_BUFFED` uncapped in the manifest and `Ctx::buff` incrementing rather than setting so a second buff reads as 2 and +2 Might. |
| enters ready (369.3) | Warwick - Hunter, Vayne - Hunter, Leona - Zealot, Magma Wurm (other friendly units here), Confront (every unit this turn), Sun Disc (the next unit this turn under Legion), Darius - Executioner | `Static::EntersReady(Applies)` consulted by `play::finalize`, which today exhausts every non-Accelerate unit unconditionally, plus two per-seat turn flags (all units, the next unit) that expire with the turn. Every card's `enters_ready(&Ctx, u32) -> bool` is live and tested. |
| play locations | Sneaky Deckhand, Sai Scout (to an open battlefield), Miss Fortune - Buccaneer (grants it to friendly units), Deadbloom Predator (to an occupied enemy battlefield at sorcery speed), Mageseeker Warden (opponents only to their base; spells and abilities cannot ready enemy units and gear), Brynhir Thundersong (a can't-play-cards lock: `no_spells` covers spells only, Reaction units still play) | `Ctx::play_locations` and `legal::play_to` consulting in-play statics: `open_play_locations`, `enemy_play_locations`, `units_only_to_base`; `Static::AmbushIntoEnemies` is Ambush-timed and needs the keyword. `Ctx::ready` gains a per-card veto (`ready_suppressed`) that Awaken bypasses; `SeatState.no_spells` becomes a card lock read by `legal::classify` for units and gear too (`lock_cards`). |
| floating turn effects | Imperial Decree (kill any unit on damage this turn), Noxian Guillotine (kill the target the next time it takes damage this turn), Stand United (an aura for the turn), Ravenborn Tome (next spell this turn deals one more), Raging Firebrand (next spell this turn is cheaper: `SeatState.next_discount` is next-card and survives the turn), Sett - The Boss and Unlicensed Armory (a replacement with an optional cost on the kill path, which has no prompt) | effects a resolving spell or ability registers on the blob for the turn: watcher triggers (`Trigger::Damaged` over a card or any unit, sourced from a resolved item), a turn aura `statics::grants_on` folds in beside in-play cards, per-seat next-spell promises (discount, bonus damage) that expire at Expiration, and a prompt on the kill replacement path. `mark_for_the_guillotine`, `decree_this_turn`, `snapshot_of_the_turn_aura`, `kindle_next_spell` are the seams. |
| triggers off the board | Flame Chompers (from the hand when discarded), Immortal Phoenix (from the trash when your spell kills), Scrapheap (from the hand when discarded), Super Mega Death Rocket! (from the trash on conquer), Nocturne - Horrifying (from the top of the deck when looked at) | `triggers::sources` lists in-play cards only; a `Card` declares the zones its abilities listen from, `sources` walks hands, trashes and the deck top for them, and `play::begin` accepts a play as an effect mid-resolution (the Chompers and the Phoenix pay a cost to play themselves). Nocturne also needs a `Looked` event carrying the cards a player looked at without drawing (`seen_among`, `SEEN_COST`). The Rocket's conquer trigger, `fires_from_the_trash` and the reload are fully written and pinned by a test of today's silence. |
| per-turn counters | Yasuo - Windrider, Kayn - Unleashed (moves per card this turn), Raging Soul (discards per seat this turn), Spoils of War (enemy units killed this turn), Obelisk of Power (a seat's first Beginning Phase, which an extra turn in the opening round shifts off the turn number), Udyr - Wildman (modes a card used this turn) | counters on `CardState` and `SeatState` reset at Expiration; `moves_this_turn(ctx, unit) -> Option<u8>`, `you_discarded_this_turn`, `enemy_unit_died_this_turn`, `is_first_beginning_phase` are the readers (the last two read `ctx.events` today, so only a death or discard in the same request counts). |
| damage modifiers (712–715) | Void Gate, Ravenborn Tome (Bonus Damage keyed on the cause), Kayn - Unleashed (per-card immunity, 465.2.c.10), Tryndamere - Barbarian (excess combat damage per attack) | `Ctx::damage` summing `bonus_damage(ctx, unit, cause)` from in-play cards before prevention; `Static::NoDamage(Applies)` consulted by `Ctx::damage` and exempting the unit from lethal assignment; `combat.rs` recording the excess of each attack so `excess_damage_assigned_in_my_attack` can read it when the conquer trigger resolves. |
| Might layers (476) | Fiora - Victorious (Mighty reads no aura Might: `own_grants → While.applies → current_might → grants_on` would recurse), Leona - Zealot (an aura minimum of 1), Draven - Showboat, Dr. Mundo - Expert, Sett - Kingpin (a dynamic Might, wired today through `MightIf` ladders of 16 / 48 / 16 rungs that play correctly) | a re-entrant Might reading that excludes the querying static, a per-aura floor, and `Grant::MightBy(fn(&Ctx, u32) -> i16)` to replace the ladders with the `might_bonus` fns the cards already expose. |
| Hidden rules | Bandle Tree (a second hide slot), Teemo - Swift Scout (hide for one energy instead of the rainbow rune), Guerilla Warfare (hide free this turn), Noxus Saboteur (opponents' facedown cards cannot be revealed here), Ava Achiever (built: every hand card is offered because decide sees no hand faces, the pick is revealed and awaited, and a face with [Hidden] is played at her battlefield through `Origin::Banishment`; a non-Hidden pick is revealed and stays in hand, and a cancelled play would land in Banishment, the owner's-zones row), Pack of Wonders (a facedown friendly card in a target universe), Ember Monk (`PlayedSpell` carries no origin and the spell's item is dropped before triggers collect) | `hide::legal` reading a per-battlefield capacity (`hide_capacity`), `hide::cost` consulting `alternative_hide_cost` and a per-seat free-hide grant, `hide::play_legal` consulting `blocks_reveal` on opponents' units here, a face-aware candidate list for hand picks (decide is blind to hand faces) and an `Origin` for a free hand play whose cancel returns the card to hand, `targets::in_universe` admitting facedown friendly cards to a `Card` spec, and an origin on `PlayedSpell`. |
| discounts from other cards | Herald of Scales (Dragons cost two energy less, minimum one), Eager Apprentice (`legal::classify` prices a spell with `cost::total`, which omits `SpellDiscount`, so a spell affordable only through her is refused before the item exists) | `Static::PlayDiscount` for units from another card with a floor (`dragon_discount` has the `ItemDiscount` signature ready), and `legal::classify` pricing with the same discounts `cost::discounts_of` applies. |
| game-rule statics | Aspirant's Climb (`Static::VictoryScore(+1)` summed into `Ctx::points_winner`), The Grand Plaza (a direct `Ctx::win(seat)` that leaves the counters alone), Hallowed Tomb (the Chosen Champion's name in `SeatState`, 103.2.a.3), Karthus - Eternal (`triggers::deathknells` queues each Death match once more per Karthus), Heimerdinger - Inventor (`activate::ability_at` reads the source's own script only; a borrowed activation has him as source and an index into another card), Symbol of the Solari (`cleanup::after_combat` step 3d recalls the attackers alone and consults no static: a combat-tie replacement hook reading `tie_recalls_all`) | one hook each, named in the card; every predicate is live and tested. |
| owner's zones (056.2, 157, 359.3.d) | Possession (a possessed unit that dies lands in the possessor's trash: `Ctx::trash` files under the controller), Mystic Reversal (`chain::leave` sends a resolved stolen spell to its controller's trash), Blind Fury (`play::cancel` sends an `Origin::Banishment` card to the item controller's Banishment) | the three zone writers reading the card's owner where the rules say owner. |
| prompts, origins and limited plays | Party Favors (a modal with named modes: `Resume` options are `TargetRef`s, so Cards and Runes show as the hand and the rune pool), The Harrowing, Soulgorger, Spectral Matron (`play::begin(None)` mid-resolution is refused by `legal::locations_for` under the closed chain, so the scripts ask their own location; a Power-only play from the trash has no origin), Reinforce (the plugin folds on public faces only, so a unit cannot be told from a spell among the peeked five; the residual cost is paid by the script), Promising Future and Dazzling Aurora (no `Origin` for a card revealed off the deck, so the free play rides `Origin::Banishment` and `Event::Played` misreports it), Fox-Fire (355.11 group targeting: no `Filter` reads the total Might of the picks so far) | named modes on `Resume`, a location prompt the engine serves to a limited play inside a resolution, `Origin::Revealed`, a face-aware fold for peeks, and a group-total filter. `twisted_fate_gambler::reveal_top_rune` and `teemo_strategist::reveal_top_cards` work in-engine today and are candidates to lift into `ctx.rs` beside `reveal_top`. |
| tags on the face | Poro Herder (Poro), Herald of Scales (Dragon), Viktor - Leader (Recruit) | `CardInfo` carries no tags, so the scripts match the printed name (a unit whose last word contains "Poro", a Dragon base-name list, the token name); a `tags` row on the face closes all three. |
| legend replacements | Sett - The Boss | `kill::applicable` lists faces on the board only, so a legend's replacement is never consulted, and the may (pay a rune and exhaust him) needs the kill-path prompt from the floating-effects row. |

Of the 37 stubs, nine wait on [Add] payment alone (the six Seals, Energy
Conduit, Kai'Sa - Daughter of the Void, Darius - Hand of Noxus), seven on
a missing trigger subject and five on play locations, so those three rows
turn 21 stubs into scripts without touching a card file. The rows that
touch the pay stage (non-resource costs, spend a buff) share one design —
a pick prompt before the item exists, recorded on the item — and should
land together; the Recruit token is the smallest change and unblocks nine
cards that today spawn a hand-built face.

### M10 review — the second pass over the Origins scripts

The review of the 255 files found two blockers (Shakedown's pay-or-let
prompt mixing `Card` and `Seat` refs in one candidate list, Ava Achiever
reading Hidden off hand faces the fold never carries) and a dozen
fidelity findings; all landed, and each un-ignored its review probe.
What changed in the vocabulary while fixing them:

- `prelude::enemy_units` reads the controller like `Filter::Enemy` and
  `friendly_units` do, so a unit taken by Possession changes sides for
  Bullet Time, Cannon Barrage and Twisted Fate's Order alike.
  `FRIENDLY_UNIT_IN_TRASH` and `FRIENDLY_SPELL_IN_TRASH` read
  `Filter::Kind` — `Filter::Unit` is on-board only and `Filter::Spell`
  is the chain, so the old definitions could never match.
- `Filter::ToOrFromBaseOf(anchor)` is a zone filter for "move to or
  from its base": a unit at a battlefield is offered its base alone, a
  unit in its base the open battlefields. Yasuo - Unforgiven's
  destination is a play-time zone target now (355.4), the same shape as
  `CHARM_DESTINATION`.
- The Champion Zone joined the card universe (355.10.a: it is public):
  `Ctx::in_champion_zone`, `Filter::InChampionZone`, `Filter::Owned`
  (the owner, not the controller) and `Filter::Champion(name)` (the
  name or `name - title`). Teemo - Swift Scout's Teemo is a play-time
  target: `And[Owned, Champion("Teemo"), Or[Unit, And[InChampionZone,
  Kind(Unit)]]]`. Every card spec already restricts itself to a zone or
  the board, so nothing else sees the new zone.
- `Static::EntersExhausted` is consulted by `play::finalize` (369.3),
  the inverse of the planned `EntersReady`: Iron Ballista lands
  exhausted with nothing on the chain to respond to.
- `Ctx::remember(target)` carries a resolution-time pick on the item:
  `chain::run` appends what a stage remembered to `item.targets` when
  the item parks on a prompt, and `prelude::remembered_cards(item)`
  reads them back past the spec-counted targets (`valid_cards` ignores
  them, kai draws them as arrows). King's Edict collects every chooser's
  unit and skips units already chosen for the spell, then
  `kill::batch`es them (simultaneous deaths, one Deathknell batch); Cull
  the Weak likewise; Volibear - Furious tallies the split and deals each
  unit its share once (one `DamageDealt` per unit) and admits zero
  targets (355.13). Alpha Strike, older than this pass, still deals its
  split one point at a time and is the next candidate for the seam.
- Mystic Reversal remakes group specs (max > 1) as a whole: the new
  controller is offered the spec's candidates up to its max, skip keeps
  the old group, and the spec's slice of `targets` and its
  `spec_counts` entry are rebuilt. Named modes are still not remade (no
  modal spell is scripted with modes on the item yet — Party Favors's
  row above); play locations do not apply, the spell only ever takes a
  spell.
- Dazzling Aurora's search is bounded by the deck: the stage counts the
  cards recycled so far and the search stops once every card has been
  revealed, so a deck without a unit no longer parks the Ending phase
  forever. Baited Hook follows 359.3.e.12 when its bait left the board:
  the top five are still looked at, nothing can be chosen (null Might)
  and all five are recycled.

### M11 — what Spiritforged still needs

`games/riftbound/rules/pool/spiritforged.md` is the second set file:
every base print sfd-001 to sfd-221 in collector order plus the set's
Gold token face, `Scripted: partial`, no deck block and no coaching. The
29 rows the deck files already scripted read byte for byte the same; the
scripting pass landed the other 192 as `cards/<snake_name>.rs`, one per
non-token row, registered in sorted order in `cards/mod.rs`. Of the 221
cards, 191 run end to end on the M0–M10 vocabulary, 8 are vanillas whose
printed keywords are the whole script (Armed Assailant, Combat Chef,
Laurent Bladekeeper, Laurent Duelist, Master Bingwen, Navori Scout,
Sentinel Adept, Veteran Poro) and 22 stay stubs — `CARD` is the generic
constructor of its kind with the printed keywords — because their entire
text is one mechanic the engine does not have yet. The coverage test pins
the 22 by name (`SEAM_STUBS` in
`the_spiritforged_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams`)
and the vanillas by name, so a stub that gains a script or a script that
regresses to a stub fails the build. Equipment is the set's new kind: 31
`[Equip]` gear, each carrying the attached line the catalog omits
(read from the card image), `equip(cost)` as its first ability and
`while_attached(EFFECT_TEXT)` as its static.

The rules are the M10 rules. Every gap is a *seam*: a `pub fn` in the
card file whose name says what the engine still owes, tested for what it
reads, and an `#[ignore = "engine gap · …"]` test that states the
behaviour the card wants end to end and fails today for the stated
reason — all 95 fail when run with `--ignored`, none is a placeholder. A
Spiritforged card that needs a primitive an Origins sibling already
named gets the sibling's seam, never a second private workaround:
`rumble_mechanized_menace::{MECHS, MECH, is_mech, your_mech,
friendly_mechs}` is the one Mech list (printed names plus a worn
Experimental Hexplate), `herald_of_scales::{DRAGONS, is_dragon}` spans
both sets, `ferrous_forerunner::{mech_face_until_token_mech_lands,
spawn_mech, play_mechs}` is the one Mech token stand-in beside
`faithful_manufactor::spawn_recruit`, the enters-ready seam is
`enters_ready(&Ctx, u32) -> bool` on every card that has one, and
`prelude::{equipment_of, became_mighty}` hold the two predicates three
files each had written for themselves. Minefield, which the engine
fixture had used as the name of its blank first battlefield, is a real
card now: the fixture's battlefield is `Proving Grounds`
(`fixtures::GROUNDS`), a name no set prints.

The table merges the M10 rows where the primitive is the same — the
Spiritforged cards are added to the row and the primitive column says
what, if anything, the new cards ask of it beyond the M10 wording — and
adds the rows Spiritforged opened. "Mechanic → cards → primitive" is the
reading order; a card in **bold** is one of the 22 stubs.

| Mechanic | Cards | Primitive the engine owes |
|---|---|---|
| equipment abilities on the wearer (818) | Boneshiver, Cull, Doran's Ring, Warmog's Armor (when the wearer conquers), Trinity Force, World Atlas (holds), Forgefire Cape, Recurve Bow (attacks or defends), Eye of the Herald (moves), Sacred Shears, The Zero Drive (the wearer's Deathknell), Last Rites (`WEARER_TEXT`: a conquer or hold plays a unit from the trash at full cost), Skyfall of Areion (`matches_through_the_skyfall`: the wearer's hold effects are conquer effects and vice versa), Svellsongur (`copied_text`: the wearer's own text once more, 136.2.c), Shurelya's Requiem (`Grant::Static(AURA)`: Ganking for friendly units at the wearer's battlefield) | `triggers::sources` skips attached gear, so a listener on the gear never hears the wearer, and `Grant` has no `Ability` arm. The primitive is `Grant::Ability(&'static [Ability])` among a `WhileAttached` grant list, resolved by `triggers::sources` as the wearer's own abilities (the gear stays the item source so the effect narrates as the gear; `cull::wearer_is_subject` is the condition every card carries today), `statics::aura_grants` folding `Grant::Static(Aura)` from `attach::granted_statics`, and `triggers::matches` consulting a per-wearer trigger mirror. Every run and condition is live and tested by queueing the trigger by hand. |
| enters ready (369.3) | M10: Warwick - Hunter, Vayne - Hunter, Leona - Zealot, Magma Wurm, Confront, Sun Disc, Darius - Executioner; Spiritforged: **Eager Drakehound** (always), Dunebreaker (two or fewer cards in hand), **Direwing** (another friendly Dragon), **Xin Zhao - Vigilant** (two other friendly units in your base), Breakneck Mech (another friendly Mech), **Renata Glasc - Industrialist** (your tokens), Bushwhack (friendly units this turn, the Confront flag) | `Static::EntersReady(Applies)` consulted by `play::finalize` — and, for Renata, by `Ctx::spawn`, so every token maker stops passing a constant `ready` — plus the per-seat turn flag Confront named (`confront::units_enter_ready_this_turn`). Every card's `enters_ready` is live and tested. |
| [Add] payment sources (429) | M10: the six Seals, Energy Conduit, Kai'Sa - Daughter of the Void, Darius - Hand of Noxus, Malzahar - Fanatic; Spiritforged: **Hextech Anomaly** (recycle X runes for X rainbow, add X energy), **Ancient Henge** (exhaust X ready runes for X energy, add X rainbow), **Ornn - Fire Below the Mountain** (one rainbow for a gear play or a gear ability only), Renata Glasc - Chem-Baroness (a Gold also adds one energy within three points of the Victory Score, `gold_adds_while_paying`), Undertitan (two energy when revealed from the deck, `adds_as_revealed_from_deck`, 370.1.b.1) | the M10 `adds_while_paying(ctx, seat, card, item) -> Option<Cost>` generalisation of the Gold path, with a variable amount (`most_it_can_add`, `pays`, `adds` on the Anomaly and the Henge), a kind filter on the item paid for (Ornn), a Gold that stands in for energy, and a `Revealed` hook on `Ctx::reveal_top` for a replacement that adds as the card is revealed. |
| tokens | M10 Token::Recruit: Faithful Manufactor and the eight other Recruit makers; Spiritforged: Vanguard Armory, Eye of the Herald (Recruit, through `faithful_manufactor::play_recruits`); Ferrous Forerunner, Rumble - Scrapper, Assembly Rig, Production Surge (Mech, through `ferrous_forerunner::spawn_mech`) | `Token::Mech` beside `Token::Recruit` in `engine/ctx.rs` (3 Might, no domain, the Mech tag), the manifest token in `games/riftbound/src/lib.rs`, `TOKEN_MECH` and `is_token_name` in `cards/mod.rs`; `mech_face_until_token_mech_lands` builds the face by hand and replays `Ctx::spawn` until then. |
| tags on the face | M10: Poro Herder, Herald of Scales, Viktor - Leader; Spiritforged: Bubble Bot, Danger Zone, Forecaster, Breakneck Mech, Rumble - Hotheaded, Rumble - Scrapper, Production Surge (Mech), Direwing (Dragon), Experimental Hexplate (`GRANTED_TAG`: the wearer *is* a Mech) | a `tags` row on `CardInfo` and `Filter::Tag(name)`, plus `Grant::Tag` on a `WhileAttached` grant list read by `Ctx::has_tag`; until then `rumble_mechanized_menace::MECHS` and `herald_of_scales::DRAGONS` are the sorted printed-name lists of every tagged unit in the pool, `bubble_bot`'s play-time target is `Filter::Or` of `Filter::Named` over the list, and a unit tagged under a name the list does not know is invisible. |
| missing trigger subjects | M10: Wraith of Echoes, Viktor - Leader, Vanguard Helm, Jinx - Rebel, Flame Chompers, Scrapheap, Raging Soul, Eclipse Herald, Leona - Radiant Dawn, Solari Shrine, Karma - Channeler, Mistfall, Pirate's Haven, Volibear - Imposing, Ahri - Nine-Tailed Fox, Back-Alley Bar, The Dreaming Tree, Immortal Phoenix, Treasure Trove, Obelisk of Power, The Arena's Greatest; Spiritforged: **Altar of Memories**, Sacred Shears, The Zero Drive (`UnitDies(Who::Friendly)`), **Aphelios - Exalted**, Jax - Unrelenting (`Attached(Who::Me)`, `attach::attach` raises nothing), **Fiora - Worthy**, Fiora - Grand Duelist (`BecameMighty(Who)`, 709: no Might writer raises an event when `current_might` crosses to 5), **Simian Ancestor** (`Buffed(Who::Me)`), Sivir - Battle Mistress (`Recycled(Who::You)` for runes on the pay path), Fae Dragon (`BuffSpent(Who::You)`, the spend-a-buff row), Yone - Blademaster (`Event::Conquered` carries the taker and the units, not whether the battlefield was open before `cleanup::establish`, 170.11.c) | `Trigger` variants and `triggers::matches` arms for each subject; `Ctx::buff`, `Ctx::might`, `attach::attach`, `pay::pay` and `Ctx::spend_buff` raising the events. Each card carries the condition (`the_wearer_died`, `an_equipment_attached_to_me`, `became_mighty`, `you_recycled_a_rune`, `open_before_the_conquer`, …) and the run; the ignore reason names the exact wiring. The Altar's whole effect (draw one, pick a hand card, top or bottom) is live and tested. |
| non-resource costs at the pay stage (355.10.c, 357, 383.3.b) | M10: Cruel Patron, Commander Ledros, Malzahar - Fanatic, Meditation, Brazen Buccaneer, Unlicensed Armory, Garbage Grabber, Vi - Destructive, Ekko - Recurrent; Spiritforged: Zaun Punk (kill a friendly gear, optional), **Legion Quartermaster** (return a friendly gear to hand, mandatory), Bard - Mercurial (exhaust your legend, optional), Blade of the Ruined King (kill a friendly unit as part of the Equip, 818.1.c.3), Last Rites (recycle two from your trash as part of the Equip), Assembly Rig (recycle a unit from your trash as an activation cost), Rumble - Hotheaded (recycle another friendly unit as the trigger's base cost, `SelfCost::RecycleTarget` paid at finalization), The Zero Drive (`SelfCost::BanishSelf`) | the M10 design: a pick prompt at the pay stage before the item exists, recorded on the item as `paid_additional` grows a payload, uncounterable. The candidates and the pays (`kill_candidates`, `return_candidates`, `exhaust_legend_cost_payable`, `recycle_cost_payable`, `pay_recycle_cost`, …) are live in each file; the gated triggers are tested by raising `Event::Played { paid_additional: true }`, and the Equip and activation costs are gated by `usable_if` and paid at resolution through a `Resume` pick until then. |
| play locations | M10: Sneaky Deckhand, Sai Scout, Miss Fortune - Buccaneer, Deadbloom Predator, Mageseeker Warden, Brynhir Thundersong; Spiritforged: **Dauntless Vanguard** (`enemy_play_locations`, the Deadbloom seam), **Perched Grimwyrm** (`only_play_locations`: a restriction that *replaces* the list and refuses the base), **Rengar - Pouncing** (`attacking_play_locations`: the battlefield of the combat you are attacking, where he lands as an attacker), Rek'Sai - Swarm Queen (`play_locations_with_here`: the engine's own `PlayLocation` prompt with her battlefield added) | `Ctx::play_locations` and `legal::play_to` consulting in-play statics for grants and restrictions alike, and serving the location prompt to a play the engine begins inside a resolution (today the scripts ask their own `Resume` location). |
| floating turn effects | M10: Imperial Decree, Noxian Guillotine, Stand United, Ravenborn Tome, Raging Firebrand, Sett - The Boss, Unlicensed Armory; Spiritforged: Jayce - Man of Progress (`promise_a_gear_for_free_this_turn`: one gear of energy cost 7 or less from hand for its Power alone), Rally the Troops (`rally_this_turn`: a turn-scoped `Trigger::YouPlayCard` watcher sourced from a resolved item), Relentless Pursuit (`retreats_on_conquer_this_turn`: a trigger granted to one unit for the turn), Temporal Portal (`next_spell_repeats_for_its_cost`: a per-seat promise `play::advance` reads at `STAGE_REPEAT`) | the M10 primitive — effects a resolving item registers on the blob for the turn — with three new flavours: a kind-aware per-seat play promise (`SeatState.next_discount` is next-card and kind-blind), watcher triggers sourced from a resolved spell, and a granted ability with a turn scope. The four seams narrate today. |
| per-turn counters | M10: Yasuo - Windrider, Kayn - Unleashed, Raging Soul, Spoils of War, Obelisk of Power, Udyr - Wildman; Spiritforged: Needlessly Large Yordle (`points_scored_from_holding_this_turn`: holds score in the Beginning Phase request), Sivir - Mercenary (`power_spent_this_turn`: runes and Golds alike), Azir - Emperor of the Sands (an Equipment played this turn), Ezreal - Prodigal Explorer (enemy choices this turn), **Perched Grimwyrm** (`conquered_this_turn`: a hold and a conquer both set `Control.scored`), **Ornn's Forge** (gear played this turn), Aphelios - Exalted (modes chosen this turn, the Udyr row) | counters on `SeatState` and `CardState` reset at Expiration: `holds_this_turn`, `power_spent`, `equipment_played`, `enemy_choices`, `gear_played`, a per-turn conquer record, and the modes a card used. Every reader works within one request today by scanning `ctx.events` / `ctx.effects`. |
| damage modifiers (712–715) | M10: Void Gate, Ravenborn Tome, Kayn - Unleashed, Tryndamere - Barbarian; Spiritforged: Rabadon's Deathcrown (`bonus_damage`: spells of the wearer's controller deal three more), Sivir - Ambitious (`excess_to_deal` through `tryndamere_barbarian::excess_damage_assigned_in_my_attack`), Ezreal - Dashing (`deals_no_combat_damage`: `Ctx::deals_combat_damage` consults every *other* card's `NoCombatDamageFrom` and skips the unit's own) | the M10 `Ctx::damage` summation and combat excess record, plus `deals_combat_damage` reading a unit's own static. |
| discounts and surcharges from other cards | M10: Herald of Scales, Eager Apprentice; Spiritforged: Vex - Cheerless (`enemy_surcharge`: `Static::SpellSurcharge(ItemDiscount)` summed into `cost::base_of_item`, and the Eager Apprentice pricing gap in `legal::classify`), Irelia - Graceful (`REDUCTIONS`: one energy *or* one rainbow less is a player's pick at the pay stage; the seam picks for them today), Ezreal - Prodigy (`cost::discounts_of` consults `SpellDiscount` for spells only, so a unit's additional cost — Clockwork Keeper's shape — and an Accelerate payment are never discounted at the pay stage though `discount_for` prices both, and `play::advance` prices the additional-cost confirm before the slot is set; the Repeat component is live end to end), **Ornn's Forge** (`Static::PlayDiscount` for permanents, the Herald row, floored), Marai Spire (`play::begin` prices the Repeat ask with `cost::of_item` before `SLOT_REPEAT` is set), **Rek'Sai - Breacher** (`grants_accelerate`: an in-play keyword grant `cost::can_accelerate` reads with the pending item's origin — `statics::grants_on` cannot reach a unit still on the chain) | `Static::PlayDiscount` and `Static::SpellSurcharge` folded by `cost::discounts_of` and priced by `legal::classify` with the same arithmetic, a pay-stage pick between alternative reductions, the additional-cost and Repeat asks priced after their slots are set, and an origin-aware keyword grant on chain items. |
| keywords the engine does not read | M10 Vision (817): Mystic Poro, Jeweled Colossus, Sai Scout, Karma - Channeler, Gemcraft Seer; Spiritforged: Forecaster (Vision through an aura over your Mechs), Long Sword and **Jax - Unmatched** (Quick-Draw: `hide::reacts` and `legal::timing` read `Keyword::Reaction` only and nothing attaches on play; Jax grants it to Equipment in hand, a hand-zone static `statics::grants_on` never reads), Azir - Emperor of the Sands (a *granted* Weaponmaster does nothing: only a script's own `weaponmaster()` ability attaches), **Rek'Sai - Breacher** (Accelerate granted by origin, the row above) | `IMPLICIT_VISION` beside `IMPLICIT_TEMPORARY`; `Keyword::QuickDraw` read as Reaction timing by `hide::reacts` / `legal::timing` with `play::finalize` attaching the gear as it resolves (Long Sword carries an explicit Reaction and an on-play attach until then); an `IMPLICIT_WEAPONMASTER` play trigger for every unit entering with the keyword. |
| prompts, origins and limited plays | M10: Party Favors, The Harrowing, Soulgorger, Spectral Matron, Reinforce, Promising Future, Dazzling Aurora, Fox-Fire; Spiritforged: Rocket Barrage (`mode_of`: named modes on `Resume`, the Party Favors row — today the one target list implies the mode), Rek'Sai - Swarm Queen and Void Rush (`Origin::Revealed`: a play off a deck reveal rides `Origin::Banishment`, so `Event::Played` misreports it and a cancel would land in Banishment; both play through `rek_sai_void_burrower::{playable_for, play_revealed_for}` — kind known, affordable at the script's price, `targets::first_spec_fillable` on a Banishment-origin probe, then `pay` and `play::begin` — so a revealed spell with no legal target is never offered), Buhru Captain (`Ctx::picked` as `Vec<TargetRef>`: one ref kind per prompt until then), Rell - Magnetic and Here to Help (an `Origin` for a free or discounted hand play inside a resolution, the Ava Achiever row: `legal::locations_for` refuses `Origin::Hand` under the closed chain and decide is blind to hand faces), Rumble - Hotheaded (`play::begin(None)` mid-resolution, The Harrowing shape), Black Market Broker and Yordle Explorer (`PlayedSpell` carries no origin and no card: the Ember Monk seam, `spell_played_from_hidden` / `spell_with_two_or_more_power_played`) | the M10 list plus `Origin::Revealed`, a hand origin for mid-resolution plays reported on `Event::Played`, a face-aware hand candidate list, and a `PlayedSpell` payload carrying the spell's card and origin. |
| target filters (355) | Angle Shot (`same_controller`: `Filter::SameControllerAs(anchor)` for "a unit and an Equipment with the same controller"), Strike Down (`is_equipped`: `Filter::Equipped`), Temptation (`destinations_with_a_unit_of_the_same_controller`: a zone filter for "a location where there's a unit with the same controller"), Piercing Light and Temptation under Repeat (`targets::matches` reads `NotSame(0)` / `DifferentLocationFrom(0)` at the item's absolute index, so the second group's anchor is the first group's pick at prompt time; `chain::execution_view` slices the view so resolution is right), M10 Fox-Fire (a group-total filter) | three `Filter` arms, and `targets::matches` resolving an `earlier(index)` anchor relative to the group being prompted. Each spell judges the pair again at resolution and leaves a mismatch alone. |
| per-unit prevention (437) | Counter Strike (`prevent_the_next_damage_to`) | `Prevention` carries no unit and `prevent::reduce` ignores the card it is asked about; the primitive is a unit plus `Amount::Next` on `Prevention`, spent by the first `Ctx::damage` to that unit and cleared at Expiration. The seam narrates the promise; the draw runs. |
| control in place | M10 owner's zones: Possession, Mystic Reversal, Blind Fury; Spiritforged: Hostile Takeover (`takes_control_where_it_stands`: `Ctx::set_controller` relocates the stolen unit to its new controller's base, the Possession reading, so it cannot stay and contest, start the combat or conquer the reminder describes) | an in-place `set_controller` that marks the battlefield contested by the new controller, beside the M10 owner-not-controller zone writers. Control, readying and the end-of-turn release run today. |
| game-rule statics | M10: Aspirant's Climb, The Grand Plaza, Hallowed Tomb, Karthus - Eternal, Heimerdinger - Inventor, Symbol of the Solari; Spiritforged: **Tianna Crownguard** (`opponents_cannot_score_points`: `Ctx::score` consults no static; the point is simply not scored), **Forgotten Monument** (`scoring_vetoed_at`: `Static::NoScoreHere` read by `cleanup::establish` and `cleanup::score_holds` — control still changes hands, nothing is marked scored), Minotaur Reckoner (`units_cannot_move_to_base`: `Ctx::moves_to_base_from` reads the static off the battlefield card in the zone only), **Forge of the Fluft** (`lent_ability`: `Grant::Ability` with a legend scope, read by `activate::ability_at` and `activate::offers` as the legend's own — the Heimerdinger row), **Void Hatchling** (`look_at_the_top_first` / `recycle_the_look`: `Ctx::reveal_top` consults no in-play static before revealing) | one hook each, named in the card; every predicate is live and tested. |
| legend replacements | M10: Sett - The Boss | unchanged. |
| triggers off the board | M10: Flame Chompers, Immortal Phoenix, Scrapheap, Super Mega Death Rocket!, Nocturne - Horrifying | unchanged; no Spiritforged card listens from a hand, trash or deck. |
| Might layers (476) | M10: Fiora - Victorious, Leona - Zealot, Draven - Showboat, Dr. Mundo - Expert, Sett - Kingpin | unchanged. |
| Hidden rules | M10: Bandle Tree, Teemo - Swift Scout, Guerilla Warfare, Noxus Saboteur, Ava Achiever, Pack of Wonders, Ember Monk | the Ember Monk and Ava Achiever halves moved to the origins row above; the rest unchanged. |

Of the 22 stubs, four wait on enters-ready alone (Eager Drakehound,
Direwing, Xin Zhao - Vigilant, Renata Glasc - Industrialist), four on a
missing trigger subject (Altar of Memories, Aphelios - Exalted, Fiora -
Worthy, Simian Ancestor), three on [Add] payment (Hextech Anomaly,
Ancient Henge, Ornn - Fire Below the Mountain) and three on play
locations (Dauntless Vanguard, Perched Grimwyrm, Rengar - Pouncing), so
the four rows M10 already ranked first turn 14 stubs into scripts. The
new row that matters most is equipment abilities on the wearer: none of
its fifteen cards is a stub, but the printed trigger of every one of them
is silent in play until `Grant::Ability` lands, and it is the row the
next set's equipment will grow. `Static::EntersReady` is the smallest
change on the list and, with the `Ctx::spawn` consult and the Confront
flag, closes four stubs, Dunebreaker, Breakneck Mech and Bushwhack.

### M12 — what Unleashed still needs

`games/riftbound/rules/pool/unleashed.md` is the third set file: every
base print unl-001 to unl-219 in collector order, `Scripted: partial`, no
deck block and no coaching; the set prints no runes and no token faces of
its own (the Bird, Reflection, Brush and Baron Pit tokens are spawned by
name). The 27 rows the deck files already scripted read byte for byte the
same; the scripting pass landed the other 192 as `cards/<snake_name>.rs`,
one per row, registered in sorted order in `cards/mod.rs`. Of the 219
cards, 193 carry a written script on the M0–M11 vocabulary, 6 are
vanillas whose printed keywords are the whole script (Inferna, Mutated
Mouser, Rengar - Unseen, Sharkling, Towering Combatant, Voracious Gromp)
and 20 stay stubs — `CARD` is the generic constructor of its kind with the
printed keywords — because their entire text is one mechanic the engine
does not have yet. The coverage test pins the 20 by name (`SEAM_STUBS` in
`the_unleashed_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams`)
and the vanillas by name. The five `[Equip]` gear carry the attached line
the catalog omits, read from the card image, and Shepherd's Heirloom's
Equip paid in XP alone reads as `Cost::FREE`. The two later print names
the set introduces, `Ultimate` and `Launch Exclusive`, joined
`PRINT_SUFFIXES`, so the overnumbered Baron Nashor resolves to his base
name like any alternate art.

The rules are the M10 rules. Every gap is a *seam*: a `pub fn` in the card
file whose name says what the engine still owes, tested for what it reads,
and an `#[ignore = "engine gap · …"]` test that states the behaviour the
card wants end to end and fails today for the stated reason — all 104
fail when run with `--ignored`, none is a placeholder. An Unleashed card
that needs a primitive an Origins or Spiritforged sibling already named
gets the sibling's seam, never a second private workaround, and the
integration pass folded the few duplicates the parallel scripting had
grown into one holder each: `daisy::{BIRDS, CATS, DOGS, is_bird, is_cat,
is_dog}` is the one set of Bird, Cat and Dog printed-name lists (every
tagged unit of every set, plus the Bird token) that Friendship, Ivern -
Nurturer, Ivern - Friend to All, Ivern - Green Father and Stalking Wolf
read; `starhound::{COMPANIONS, COMPANION, is_companion}` is the one
Bird-Cat-Dog-Poro union as a `Filter::Or` of `Filter::Named` for
play-time targets, tested to equal the daisy lists plus every name
`poro_herder::is_poro` reads, and Undying Loyalty prices through it;
`herald_of_scales::DRAGONS` names the three Unleashed Dragons (Elder
Dragon, Gentle Gemdragon, Inviolus Vox) beside the twelve it had;
`frisky_hunter::{BIRD, bird_face_until_token_bird_lands, is_bird,
spawn_bird, play_birds, play_birds_here}` is the one Bird token stand-in
beside `faithful_manufactor::spawn_recruit` and
`ferrous_forerunner::spawn_mech`; `keeper_of_masks::{REFLECTION,
reflection_face_until_token_reflection_lands, is_reflection,
spawn_reflection(ctx, owner, at, ready), play_reflections,
become_copy_of}` is the one Reflection stand-in and copy seam that LeBlanc
- Deceiver and Mirror Image spawn their ready Reflections through;
`tryndamere_barbarian::excess_damage_assigned_in_my_attack` is read by the
four cards keyed on excess; `enters_ready(&Ctx, u32) -> bool` is the
enters-ready seam on all ten; `jhin_murderous_artist::add_to_rune_pool` is
the banked [Add] Blue Sentinel shares; `ivern_nurturer::{look_at_the_top_three,
on_top, reveal_the_pick, revealing, draw_the_revealed_unit}` is the
look-at-three / reveal-a-unit / draw / recycle-the-rest flow Rift Herald
reuses (Double Trouble and Fate Weaver carry their own copies of the Ornn -
Blacksmith shape; all three offer a top card only while its kind may still
be the wanted one, so a card the plugin already knows to be something else
is never revealed); `rell_magnetic::origin_of_a_free_hand_play`
is the origin Rift Herald's Deathknell rides; and `the_harrowing::{play_from_trash,
for_nothing}` and `cruel_patron::pay_kill_cost` carry Heedless
Resurrection, Sacrifice and Undying Loyalty.

The table merges the M10 and M11 rows where the primitive is the same —
the Unleashed cards are added to the row and the primitive column says
what, if anything, the new cards ask of it beyond the earlier wording —
and adds the rows Unleashed opened. "Mechanic → cards → primitive" is the
reading order; a card in **bold** is one of the 20 stubs.

| Mechanic | Cards | Primitive the engine owes |
|---|---|---|
| enters ready (369.3) | M10: Warwick - Hunter, Vayne - Hunter, Leona - Zealot, Magma Wurm, Confront, Sun Disc, Darius - Executioner; M11: Eager Drakehound, Dunebreaker, Direwing, Xin Zhao - Vigilant, Breakneck Mech, Renata Glasc - Industrialist, Bushwhack; Unleashed: **Arena Kingpin** (always), Daisy (always), **Bandle Soldier** and Scorchclaw (Level 3), **Monch** (an opponent controls a stunned unit; the discount half is live), Shadow (played to a battlefield, read off the pending item's zone target), Crescent Guardian (paid the additional cost after a spell this turn), **Towering Pairofant** (a unit died this turn) and **Shadow Watcher** (a friendly unit died during your Beginning Phase this turn — both read this request's `Event::Died` and the blob's phase, the per-turn-counters row), Master Yi - Wuju Master (Level 11: every unit you play from hand) | `Static::EntersReady(Applies)` consulted by `play::finalize`, unchanged; the two death-keyed readers also want the per-turn death record below. Every card's `enters_ready` is live and tested. |
| [Add] payment sources (429) and the rune pool (167) | M10: the six Seals, Energy Conduit, Kai'Sa - Daughter of the Void, Darius - Hand of Noxus, Malzahar - Fanatic; M11: Hextech Anomaly, Ancient Henge, Ornn - Fire Below the Mountain, Renata Glasc - Chem-Baroness, Undertitan; Unleashed: **Dragonsoul Sage** (one energy), **Diana - Scorn of the Moon** (one energy, only during a showdown and only while ready), Honeyfruit (one rainbow, and one energy more at six XP, as a Reaction), Jhin - Murderous Artist (`add_to_rune_pool`: a *triggered* [Add] on his move, 1 energy and a rainbow), Blue Sentinel (a delayed [Add] rainbow at the start of the next Main Phase, through Jhin's seam) | the `adds_while_paying(ctx, seat, card, item) -> Option<Cost>` generalisation, plus a banked pool: `SeatState` keeps no floating energy or power, so a triggered [Add] is banked as `next_discount` (next card only, spells and permanents only, survives the turn) where 167 wants a per-seat pool `pay::plan` spends first across any number of plays and abilities, emptied at the start of the next Main Phase and at the end of the turn. |
| tokens | M10 Token::Recruit; M11 Token::Mech; Unleashed Token::Bird (1 Might, no domain, the Bird tag, printed Deflect): Frisky Hunter, Flurry of Feathers, Walking Roost, Carrion Dredger, Ultrasoft Poro, Gutter Palace, Trapping Grounds; Token::Reflection (0 Might, no domain) and a copy: Keeper of Masks (two exhausted copies of himself), LeBlanc - Deceiver and Mirror Image (a ready copy of another unit, then Temporary); battlefield tokens: Baron Nashor (the Baron Pit, "units can move here from anywhere", and his 369.3 entry replaced by an entry there), Ivern - Green Father (a hold replaces the battlefield with Brush, a later score there may swap it back); token plays: **Zilean - Time Mage** (a replacement doubling one token play a turn); Lillia - Protector of Dreams, Blood Rose, Fresh Beans, Rengar - Pridestalker and the unit Nami - Headstrong's promise waits for all see a token play since `Trigger::YouPlayCard` matches `Origin::Board` (350.2, the Cithria of Cloudfield row — landed, each carries the token test), while Viktor - Innovator, Astral Heron and Darius - Trifarian read "a card" and exclude tokens through `prelude::a_card_not_a_token` (185; Viktor's Recruit never wakes him again) | `Token::Bird` and `Token::Reflection` in `engine/ctx.rs` with the manifest tokens, `TOKEN_BIRD` / `TOKEN_REFLECTION` and `is_token_name` in `cards/mod.rs` (the Bird's Deflect on its face instead of the permanent grant `spawn_bird` makes); `Ctx::become_copy(token, of)` giving a token the copied unit's name, script and Might (477.1.b) so `wears_a_copied_face` reads true; `Ctx::add_battlefield(token)` and `Ctx::replace_battlefield` / `swap_back` on a zone list that is fixed at table setup today; `Ctx::spawn` (and the by-hand spawn seams) asking `would_double_a_token_play(ctx, owner)`, an optional prompt (371.2), a second spawn of the same face, location and ready state (375), then `spend_the_doubling`. |
| tags on the face | M10: Poro Herder, Herald of Scales, Viktor - Leader; M11: Bubble Bot, Danger Zone, Forecaster, Breakneck Mech, Rumble - Hotheaded, Rumble - Scrapper, Production Surge, Direwing, Experimental Hexplate; Unleashed: Daisy, Friendship, Ivern - Nurturer, Ivern - Green Father, Starhound, Undying Loyalty, **Stalking Wolf** (Bird, Cat, Dog, Poro), Gentle Gemdragon, Elder Dragon, Inviolus Vox (Dragon), Ivern - Friend to All (`chosen_tag` reads nothing: a tag chosen as he is played needs named modes on `Resume` and a stored tag on `CardState`), The List (`named_tag` reads nothing: naming a tag needs a name prompt kind and a stored name, then `Filter::Tag` over it) | a `tags` row on `CardInfo`, `Filter::Tag(name)`, `Grant::Tag`, and now a name-prompt kind plus a `CardState` slot for a chosen tag; until then `daisy::{BIRDS, CATS, DOGS}`, `starhound::COMPANIONS`, `herald_of_scales::DRAGONS` and `poro_herder::is_poro` read printed names, and a unit tagged under a name the lists do not know (a future print, a granted tag) is invisible. |
| missing trigger subjects | M10 and M11 as listed; Unleashed: **Vicious Snapjaws**, **Spectral Centaur**, **Shard of Undoing** (`UnitDies(Who::Friendly)`, the Wraith of Echoes row; the Shard also wants the once-per-turn and the Beginning-Phase condition it carries), Katarina - Reckless (`hide::hide` raises no `Hidden` event, no you-hide-a-card trigger; the from-face-down line is live), Vex - Mocking (`Event::Stunned { units, by }`, the Eclipse Herald row; the optional move is live), Blast Cone (`Who::Enemy` on `Trigger::Move` plus the seat whose effect moved the unit on `Event::Moved`, the Volibear - Imposing row), **Ripper's Bay** (`Event::ReturnedToHand { card, from, owner }` raised by `Ctx::bounce`, with the owner as the trigger's controller), **Sumpworks Map** (a `Scored` event carrying the scoring seat, and `Who::Enemy` on Hold/Conquer), Frozen Fortress (a battlefield's `BeginningPhase` for each player, The Arena's Greatest row), **Diana - Lunari** (`Trigger::ShowdownBegins(Where::Here)` raised as a showdown opens, 464.2.b; the pay-1 / Predict / reveal / draw-if-spell run is live through a test-only wiring) | `Trigger` variants and `triggers::matches` arms for each subject; `Ctx::stun`, `Ctx::bounce`, `hide::hide`, `Ctx::score` and `showdown::open` raising the events. Each card carries the condition and the run; the ignore reason names the exact wiring. |
| delayed triggers (316.4, 359.3.f) | Unleashed: **Iascylla** (her hold lures an enemy unit at the start of your next Main Phase, the held battlefield as the argument per 359.3.f.3.b), Blue Sentinel (her hold adds a rainbow then), Ashe - Focused (the banished card returns when its owner holds, even with Ashe gone), Nami - Headstrong (live: the next-unit promise rides `ctx.delay(When::EndOfTurn)` as its own record and the `YouPlayCard` trigger reads and retires it) | `When::MainPhaseOf(seat)` queued by `phases::continue_beginning` as the Action phase opens after the pool empties (316.3), and `When::HeldBy(seat)` queued from `cleanup::score_holds` — a delayed trigger that needs no source on the board, so a card that left play can still fire it. `iascylla::at_the_start_of_your_next_main_phase` and `ashe_focused::return_when_they_hold` narrate; the abilities they would queue (`LURE`, `ADD`, `GIVE_BACK`) are fully written and tested through `triggers::queue_delayed`. |
| non-resource costs at the pay stage (355.10.c, 357, 383.3.b) | M10 and M11 as listed; Unleashed: **Stalking Wolf** (kill a friendly Bird, Cat, Dog or Poro, mandatory, and its battlefield offered as his play location), Sacrifice (kill a friendly Mighty unit) and Heedless Resurrection (kill a friendly unit) — both through `cruel_patron::pay_kill_cost`, paid at resolution today where a response can void the cost, Atakhan (an optional kill with the victim's energy and power struck off his, the Commander Ledros row), Conscription (spend 5 XP, the Bard - Mercurial row; a paid play widens the target to any enemy unit at a battlefield), Safety Inspector and Poppy - Defender of the Meek (3 XP as `Card.additional`: `cards::Cost` carries energy and power only, so the confirm is never asked; Poppy's `SelfDiscount` already reads `SLOT_ADDITIONAL`), Crescent Guardian (the ask gated on a spell played this turn), Forgotten Signpost (exhaust a friendly unit as the activation cost, the Meditation row), Square Up (discard 1 as the Repeat cost, the Brazen Buccaneer row; an empty hand withholds the ask), Gutter Palace (discard as the base cost, the Unlicensed Armory row), Altar of Blood (the may on the kill path, the Sett - The Boss row), **Undying Legion** (his own play from the trash for 3 energy and a Fury under Legion: `activate::flow_playable` lists Flow spells only and `cost::origin_cost` prices a Banishment-origin unit at its printed cost), **Jhin - Meticulous Killer** (an alternative cost, one Mind power instead of the printed four: `SelfDiscount` strips energy but cannot add a power need), Cursed Sarcophagus (a play from Banishment for the printed cost, plus the banished-with link of the Zero Drive row), Hextech Gauntlets (a target-dependent Equip cost: `ExtraCost` sees the source only, so the energy cannot read the chosen unit's Might) | the M10 design plus: an `xp` field on `cards::Cost` so `Card.additional` can ask for XP, an alternative-cost kind, a gate on the additional-cost ask, a from-trash play offer for units under Legion priced at a script cost, and an `ExtraCost` evaluated on the item at `choose_targets` and `STAGE_PAY`. Every candidate list and pay is live in the card file. |
| per-turn counters | M10 and M11 as listed; Unleashed: Wily Newtfish (`xp_gained_this_turn` reads this request's XP scores), Prepared Neophyte and Jhin - Meticulous Killer (`energy_spent_on_spells_this_turn` prices spells still on the chain), Blighted Battleaxe (`conquered_this_turn` per unit, the Perched Grimwyrm row), Towering Pairofant and Shadow Watcher (a unit death this turn and the phase it fell in), Dancing Grenade (the times a spell dealt damage this turn) | counters on `SeatState` and `CardState` reset at Expiration: `xp_gained`, `spent_on_spells`, a per-unit conquer record set by `cleanup::conquer`, a per-turn death record with its phase, and a per-spell deal count. Every reader works within one request today. |
| the spell's card and cost on `PlayedSpell` | M11: Black Market Broker, Yordle Explorer, Ember Monk; Unleashed: Forgotten Library (`forgotten_library::energy_spent_on_the_spell` is the one reader; Revna the Lorekeeper's `spent_four_or_more_on` is defined over it and Jhin - Virtuoso imports Revna's), Jhin - Virtuoso (a four-energy spell offered to banish, the fourth trashing the set — with the banished-with link), Prepared Neophyte (a this-turn static, `energy_spent_on_spells_this_turn`, the per-turn counters row) | `Event::PlayedSpell` carrying the card, the origin and the paid cost, raised while the item can still be read; today it carries none and is raised after the spell left the chain. |
| damage modifiers (712–715) and lethal (142.3) | M10: Void Gate, Ravenborn Tome, Kayn - Unleashed, Tryndamere - Barbarian; M11: Rabadon's Deathcrown, Sivir - Ambitious, Ezreal - Dashing; Unleashed: Elder Dragon (any of your damage is lethal to enemy units: damage is one counter per unit with no record of who marked it, and `cleanup::dying` reads `current_might` as the lethal amount), Lotus Trap (double all damage to one unit this turn), Galio - Indefatigable (his own `NoCombatDamageFrom`, the Ezreal row), Yeti Brawler, Vi - Piltover Enforcer, Trapping Grounds, Hextech Gauntlets (three or more excess in an attack, through `tryndamere_barbarian::excess_damage_assigned_in_my_attack`), Dancing Grenade (Bonus Damage per earlier deal) | the M10 summation and combat excess record, `deals_combat_damage` reading a unit's own static, plus per-player damage marks with a lethal consult of `any_of_your_damage_is_lethal_to` before the cleanup kills, and a turn-scoped per-unit damage multiplier on `CardState` consulted after prevention. |
| floating turn effects | M10 and M11 as listed; Unleashed: Lotus Trap (`double_damage_to_this_turn`), Tactical Retreat (`retreats_instead_of_dying_this_turn`: a next-death replacement on the kill path from a resolved spell; `retreat` = heal, exhaust, recall is live), Smite (`banished_instead_of_dying_this_turn`: `kill::applicable` lists faces on the board only), Deadly Flourish (`gold_when_it_dies_this_turn`: a death watcher sourced from a resolved item), Grim Resolve (`gains_xp_when_it_wins_a_combat_this_turn`: a trigger granted for the turn, the Relentless Pursuit row), The Academy (`temporal_portal::next_spell_repeats_for_its_cost`, the Temporal Portal row), Dancing Grenade and Death from Below (`may_play_again_from_the_trash`: a play as an effect while the spell is still resolving, the Flame Chompers row) | the M10 primitive — effects a resolving item registers on the blob for the turn — with replacements on the kill and damage paths, watcher triggers and granted abilities sourced from a resolved spell, and `play::begin` accepting a replay of a spell that has just left the chain. The seams narrate today. |
| play locations and moves | M10 and M11 as listed; Unleashed: **Arachnoid Horror** (`enemy_play_locations` for himself and, once on the board, granted to friendly units — the Deadbloom Predator and Miss Fortune - Buccaneer shapes; Hunt 2 is live), Stalking Wolf (the killed unit's battlefield), Thrill of the Hunt (the engine's `PlayLocation` prompt serving any battlefield to a limited play mid-resolution; the script asks its own `Resume` location and rides `Origin::Banishment`), Determined Sentry (`cannot_move_to_base(ctx, unit)`: `Static::NoMoveToBase` per unit, where `Ctx::moves_to_base_from` reads it off the battlefield card alone — the Minotaur Reckoner row), **Mageseeker Investigator** (`group_move_surcharge`: a rune per extra unit moved to him, priced into the companion offers and paid by `march::group_move` before the units move; `PromptWhy::GroupMove` carries no cost today) | `Ctx::play_locations` and `legal::play_to` consulting in-play statics, a location list a limited play hands to `play::begin(None)`, `march::route` / `march::effect_move` / `Ctx::movable_to_base` consulting the moving unit's static, and a cost on the group-move prompt. |
| prompts, origins and limited plays | M10 and M11 as listed; Unleashed: Disposal Order (`mode_of`, named modes — the Rocket Barrage row), Curtain Call (escalating Repeat steps 820.1.c.2 / 820.3: `play::advance` prices one Repeat off the printed keyword, so the rainbow and 1-plus-rainbow steps are never offered and the modes are walked at resolution instead of chosen at play time), Rift Herald (his Deathknell offers every hand card, reveals the pick and plays a unit to the base for its Power alone through `Origin::Banishment` — the Ava Achiever / Rell - Magnetic row, plus a face-aware affordable list), Bone Skewer (a play by another seat inside a resolution: `Event::Played` should report `Origin::Hand`; the play itself goes through `play::begin_ignoring_any_and_all_costs`, so a unit with Accelerate is not asked its Accelerate cost and the stun lands on entry), Undying Loyalty (discounts at `legal::classify`, the Eager Apprentice row), Skyward Strike (a Level-gated `TargetSpec`: below six XP the stun target must not be offered, at six it is required; the spec is static data), **Syndra - Transcendent** (`grants_repeat`: a Repeat of 2 energy and a Chaos granted to spells played while she is in a showdown; `play::repeat_cost` reads printed keywords only), Divining Shells (Vision, the keywords-the-engine-does-not-read row) | the M10/M11 list plus escalating Repeat steps, a Level-aware target spec, a granted Repeat consulted at `STAGE_REPEAT` and in `chain::run`'s repeat pass, and a hand origin reported for another seat's play. |
| target filters (355) | M11: Angle Shot, Strike Down, Temptation, Piercing Light, Fox-Fire; Unleashed: Elder Dragon (`one_per_location`: `Filter::DifferentLocationFromPicks`, the spec is `target(ENEMY_UNIT, 0, 7)` and a second pick at a picked location is dropped at resolution), Repulse (`chooses_it_and_no_other_friendly_unit`: `Filter::ItemTargetsOnly(anchor)`, the Angle Shot shape), Moonfall (a zone filter for a battlefield where you have units), Tricksy Tentacles (355.11: a group-total Might filter, the Fox-Fire row) | four `Filter` arms read by `targets::candidates_with`; each spell judges the picks again at resolution and leaves a mismatch alone. |
| equipment abilities on the wearer (818) | M11's fifteen; Unleashed: Blighted Battleaxe (at the end of your turn, a wearer that did not conquer drops the axe and takes four), Hextech Gauntlets (the wearer's conquer after three excess draws one), Soul Sword (a granted Level: `statics::own_grants` walks `Static::Level` on the wearer's own script only) | `Grant::Ability` resolved by `triggers::sources` as the wearer's own, unchanged, plus `Grant::Static(Level)` read from `attach::granted_statics`. |
| discounts and surcharges from other cards | M10: Herald of Scales, Eager Apprentice; M11 as listed; Unleashed: Vaults of Helia (`unit_surcharge`: the holder's non-token units cost one more this turn after a hold), Undying Loyalty (`legal::classify` prices with `cost::total` before the item exists, so a seat that can only afford the discounted play is refused), Monch and Poppy - Defender of the Meek (`SelfDiscount` live) | `Static::PlayDiscount` / `SpellSurcharge` folded by `cost::discounts_of` and priced by `legal::classify` with the same arithmetic, unchanged. |
| game-rule statics | M10 and M11 as listed; Unleashed: Red Brambleback (`extra_conquer_triggers_here…`) and Blue Sentinel (`extra_hold_triggers_here…`): `triggers::collect` queues each Conquer / Hold match once, the Karthus - Eternal row; Maduli the Gatekeeper (`ready_suppressed`, binding on Awaken too, the Mageseeker Warden row); Galio - Indefatigable (own-static consult); **LeBlanc - Everywhere At Once** (`temporary_is_vetoed`: the implicit Temporary trigger in `triggers::find_among` consults no card); **Gardens of Becoming** (`lent_ability`: `Grant::Ability` with a `UnitsHere` scope read by `activate::ability_at` and `activate::offers` as the unit's own — the Forge of the Fluft / Heimerdinger row); Gutter Palace (a direct `Ctx::win(seat)`, the Grand Plaza row); Frozen Fortress (each player's Beginning Phase); Determined Sentry and Mageseeker Investigator (the moves row) | one hook each, named in the card; every predicate is live and tested. |
| keywords the engine does not read | M10 Vision; M11 Quick-Draw, granted Weaponmaster, granted Accelerate; Unleashed: Divining Shells (Vision), Undying Legion (Legion as a from-trash play), Syndra - Transcendent (a granted Repeat) | `IMPLICIT_VISION` and the rows above. |
| legend replacements, triggers off the board, Might layers, Hidden rules, control in place, owner's zones | M10 and M11 as listed | unchanged; no Unleashed card opened one of these rows — Vi - Hotheaded's doubled Might, the Hidden units and spells of the set and the two Pyke champions run on the vocabulary as it is. |

Of the 20 stubs, four wait on enters-ready alone (Arena Kingpin, Bandle
Soldier, Monch, Towering Pairofant — Shadow Watcher on that plus the
death record), four on a missing trigger subject (Vicious Snapjaws,
Spectral Centaur, Shard of Undoing, Ripper's Bay, Sumpworks Map and
Diana - Lunari make six), two on [Add] payment (Dragonsoul Sage, Diana -
Scorn of the Moon) and two on a delayed-trigger `When` (Iascylla, and
Blue Sentinel's second line), so the rows M10 ranked first still turn
the most stubs into scripts. The rows Unleashed opened are the small
ones: `When::MainPhaseOf` / `When::HeldBy` are two arms on an existing
enum and a queue call each in `phases` and `cleanup`; `Token::Bird` is
the Recruit change once more and closes seven hand-built spawns;
`Filter::DifferentLocationFromPicks` and the group-total filter are the
two target arms 355 still lacks; and the banked rune pool is the first
Unleashed card (Jhin - Murderous Artist) that reads 167 as written. The
one row that grew a design question is the copy primitive: a Reflection
that becomes a copy needs `Ctx::become_copy` to rewrite the face's name,
script binding and Might while keeping the token flag and Temporary, and
`ScriptRegistry::of_card` would resolve the token by its copied name
from then on — the same shape Svellsongur's `copied_text` asked of
equipment in M11.

### M13 — the Vendetta set: triage, stubs and gaps

Landed as a triage, not as scripts. `games/riftbound/rules/pool/vendetta.md`
is the fourth set file: every base print ven-001 to ven-166 in collector
order, in the unchanged card-line grammar, `Scripted: partial`, no deck
block and no coaching. The 18 rows the six deck files already scripted
read byte for byte the same. Three things the set file leaves out on
purpose: the six basic runes ven-r01 to ven-r06 are the six runes Origins
already lists under the ogn ids (the shared-line test keys a row by name,
and the engine keys runes by the name suffix); the six ven-sp prints —
Kai'Sa, Survivor; Sona, Harmonious; Ahri, Inquisitive; Sett, Brawler;
Ezreal, Prodigy; Lux, Crownguard — are reprints the importer's `ALIASES`
already map to their Origins, Spiritforged and Proving Grounds names, so a
row would duplicate a script under a second name; and the catalog dump
carries two records of most Vendetta prints (a draft from 2026-07-10 and a
later one with a `clean_name`), of which the later is the row, which is
how the legends read as the importer's canonical names, `Yordle, Kennen -
Heart of the Tempest` included (kai's `champion_name` already reads the
species before the comma). The four `[Equip]` gear carry the attached line
read from the card image, and Jagged Cutlass's `Equip` is bracketed as the
card prints it (the catalog drops the brackets). The consumers grew one row
each: `POOL_FILES` and a set-file test in `cards/mod.rs`, kai's
`deck::pool::FILES` with its set list, and the importer's set-file
allowance in `text_list`.

**Stubs.** Every one of the 148 Vendetta cards without a script got
`cards/<snake_name>.rs` exporting `pub static CARD: Card` as the generic
constructor of its kind with the pool name and the keywords the card
prints as its own — the bracketed keyword lines of the rich text,
Assault/Shield/Deflect with their numbers (a bare `[Shield]` or `[Deflect]`
is 1), `Keyword::Flow(cost)` wherever a Flow cost is printed, and
`Keyword::Empower(cost)` with the printed resource cost, `Cost::FREE` when
the printed cost is not a resource (a discard, a kill, an exhaust — the
stub keeps the keyword and the scripter names the cost as the pay-stage
seam) and the energy half of Legion Marauder's "one energy or a Body rune".
A keyword the text grants, talks about or gates behind `[Empowered]` is not
the card's own (Repair Specialist's Assault, Risen Altar's Empower, Baccai
Sandspinner's Deflect). All 148 are registered in `cards/mod.rs` in sorted
order, so the scripters own only their card files. The set-file test pins
the two vanillas (Horns of the Dragon, Soulspinner), the Equip costs, and
the Hidden, Empower and Flow keywords against the printed text.

**Groups.** Each is one pattern to reuse, sized for one scripter; the
theme names the shape to copy and the seam to name where the vocabulary
ends. A Vendetta card that needs a primitive an Origins, Spiritforged or
Unleashed sibling already named gets the sibling's seam, never a second
private workaround.

| Group | Theme | Cards |
|---|---|---|
| `units-vanilla-statics` (12) | plain units and continuous effects: the two vanillas (Horns of the Dragon, Soulspinner) are complete once their tests pin the printed keywords; the rest are one Static each — Static::While over alone_there-style counts for Disciple of Shen's Shield and Spiderling's per-namesake Might (the Draven - Showboat MightIf ladder until Grant::MightBy lands), Repair Specialist's Assault equal to your gear count (the same ladder over Grant::Keyword), Static::NoCombatDamageFrom for Sacred Protector (the Galio - Indefatigable own-static seam), Static::SelfDiscount for Plaza Guardian, Shadowblade Lurker and Keeper of Law (the Monch / Herald of Scales shape, a same-name trash count for the Lurker), Static::SpellDiscount reading the item's trash origin for Stargazer (the Marai Spire shape), Dune Surfer's ignore-Tank assignment and Esteemed Hierophant's prevent-all from enemy items are game-rule statics named as seams (the Kayn - Unleashed NoDamage row) | Horns of the Dragon, Soulspinner, Disciple of Shen, Spiderling, Repair Specialist, Sacred Protector, Plaza Guardian, Shadowblade Lurker, Keeper of Law, Stargazer, Dune Surfer, Esteemed Hierophant |
| `units-rule-changers` (10) | units whose text changes a game rule: a play veto keyed on the seat's turn count (Ol' Poro: the Obelisk of Power per-turn counter row plus a legal::classify consult), the Channel Phase count (Sandstone Chimera), a scoring replacement that draws instead on a player's first two turns (Otterpus: the Tianna Crownguard Ctx::score consult), a named spell lock (Fallen Feline: The List name-prompt row plus the Brynhir Thundersong lock), Empowered rule statics for Mel, Newly Awakened (uncounterable items, an extra -1 on chosen -Might), Gangplank, Naval (a replacement on the stun / -Might / bounce paths), Ambessa, The Wolf (no damage unless in combat, the Kayn row) and Kayle, Justified (an Empower counter up to three, Ctx::empower is a flag today), Applied Researchers (Empowered: Static::While over Grant::Static(SpellDiscount) with the floor, the Eager Apprentice legal::classify pricing gap) and Aurok General (Empowered: Static::Aura over friendly Empowered units, which runs today); each carries its live predicate and an ignored end-to-end test, the Empower and the plain halves (Mel's play draw, Kayle's Empower) run on the vocabulary | Ol' Poro, Sandstone Chimera, Otterpus, Fallen Feline, Mel, Newly Awakened, Gangplank, Naval, Ambessa, The Wolf, Kayle, Justified, Applied Researchers, Aurok General |
| `units-empower-statics` (12) | Empower units whose Empowered line is a continuous effect (Nasus, Ascended / Tail-Cloaked Matriarch shape: Keyword::Empower(COST) plus prelude::empower(COST) as the first ability, then Static::While(empowered, &[Grant::Might(n), Grant::Keyword(...)]) with `fn empowered(ctx, unit) -> bool { ctx.is_empowered(unit) }`): Shadow Fiend, Serene Ascetic, Steel Paws, Brutal Hunter, Kinkou Lifeblade, Solari Sunhawk; the three with a non-resource Empower cost — Punching Poro (discard 1), Escaped Grayback (kill a friendly unit), Legion Marauder (one energy or a Body rune, the Irelia - Graceful pick) — carry Keyword::Empower(Cost::FREE) or the energy half in the stub and name the cost as a seam beside cruel_patron::pay_kill_cost and the Brazen Buccaneer discard (the non-resource-costs-at-the-pay-stage row); the three with a printed cost rider (Baccai Sandspinner's 3 less under 4 or fewer runes, Frostcoat Mother's and Grumpy Rockbear's 1 less per rune) price the Empower through costing(empower(COST), dynamic_cost) since Ability.extra replaces the printed cost | Shadow Fiend, Serene Ascetic, Steel Paws, Brutal Hunter, Kinkou Lifeblade, Solari Sunhawk, Baccai Sandspinner, Frostcoat Mother, Grumpy Rockbear, Punching Poro, Legion Marauder, Escaped Grayback |
| `units-attack-defend-combat` (10) | attack, defend and combat-end triggers (Yasuo - Remorseful / Caitlyn - Patrolling shape: on_attack / on_defend with deal, ready, grant_this_turn, an optional pay through Ability.cost): Baccai Reaper, Renekton, Rage Fueled (runes_of count, read once as the trigger Condition — 383.2.a.1, a fifth rune gained in response does not stop the damage), Twilight Reveler, Riven, Shattered (equipment_of count), Corrupted Dragon (any number of enemy units of 5 or less to base, plus the enters-ready seam keyed on the Victory Score), Kennen, Keeper of Balance (Hidden, play-or-attack pay-2 stun and a While over a stunned enemy here); Empowered-gated attack triggers use when(on_attack(..), empowered) as Nasus, Ascended does (Dame the Despoiler's Might-to-theirs-then-plus-one through might_this_turn, Ambessa, Respected and Feared's kill of a lesser unit); Affectionate Poro and Mournful Witness wait on a combat-ended trigger with the unit as subject (Trigger::CombatWon / CombatLost cover only the outcome, a named seam beside them, the Poro also reading damage_on this turn) | Baccai Reaper, Renekton, Rage Fueled, Twilight Reveler, Riven, Shattered, Corrupted Dragon, Kennen, Keeper of Balance, Dame the Despoiler, Ambessa, Respected and Feared, Affectionate Poro, Mournful Witness |
| `units-move-conquer-hold` (11) | move, conquer, hold and score triggers (Yasuo - Windrider / Shen, Scourge shape: on_move, on_move_to_battlefield, on_conquer_me, on_hold_me, once_each_turn): Blade Twirler (first move each turn, a chosen player burns), Eclipse Dragon (draw under 4 or fewer runes), Akali, Deadly Weapon (Empowered doubles the deal, Empowered +1 Might static), Covert Informant (Empowered move draw), Pakaa Protector (reveal_top then draw_revealed or trash and Might), Minah Swiftfoot (a modal each-player discard or draw, the Rocket Barrage mode_of seam), Shen, Scourge of Shadows and Shen, Leader of the Kinkou Order (exactly one other unit of yours here), Noxian Demolitionist (a gear with energy at most my Might: a candidates fn), Swain, Visionary (Vision is the keyword the engine does not read; the conquer score reads a per-turn kinds-played counter, the per-turn-counters row), Illaoi (play-or-score Tentacle from Bilgewater through spawn(Token::Tentacle) as Up from the Deep does, a Score trigger is the Sumpworks Map row, +1 per token unit through a MightIf ladder) | Blade Twirler, Eclipse Dragon, Akali, Deadly Weapon, Covert Informant, Pakaa Protector, Minah Swiftfoot, Shen, Scourge of Shadows, Shen, Leader of the Kinkou Order, Noxian Demolitionist, Swain, Visionary, Illaoi, Prophet of the Great Kraken |
| `units-play-triggers` (11) | play triggers and additional play costs (Lecturing Yordle / Akshan - Mischievous shape: play(targets, run), with_additional(card, cost) and paid_additional(item)): Field Musicians, Cloud Drake, Patched Porobot (friendly_gear count), Morgana, Vindictive (Ambush, deal damage_on), Jayce, Brilliant Inventor (play or first non-token gear each turn, ready something else), Ocean Drake (an open battlefield is the Sneaky Deckhand play-locations seam; the optional non-Dragon bounce runs through herald_of_scales::is_dragon), Reluctant Leader (YouPlayCard filtered to another unit), Kennen, Storm of Shuriken (Burn 2 on play; the conquer grant of Flow-for-its-cost this turn is a granted-keyword floating effect, the Syndra - Transcendent grants_repeat row), Masa, Crashing Thunder (an optional Order rune, exactly Akshan), Gust Monk (an optional energy additional cost, then a banish from any trash as the trigger's base cost — 383.3.b, the Shadow Clone SelfCost::BanishTarget shape, paid at finalization — to grant Assault 2), Zed, From the Shadows (an optional discard as the additional cost is the Brazen Buccaneer pay-stage row; the text prints no location, so the Shadow Clone spawns through Token::ShadowClone at the a_play_location target as Death Mark does) | Field Musicians, Cloud Drake, Patched Porobot, Morgana, Vindictive, Jayce, Brilliant Inventor, Ocean Drake, Reluctant Leader, Kennen, Storm of Shuriken, Masa, Crashing Thunder, Gust Monk, Zed, From the Shadows |
| `units-turn-death-activations` (12) | turn-phase triggers, Deathknells, watchers and activations (Ekko - Recurrent / Vi - Peacekeeper shape: Trigger::BeginningPhase, deathknell(...), on_readied, on_enemy_unit_dies with once_each_turn, activated(Timing, cost, targets, run) with exhausting_self and usable_if): Forsaken Baccai and Oasis Raider (fewer runes than an opponent at the start of your Beginning Phase), Fretful Feline and Jayce, Hammer in Hand (Readied; Jayce's choose-one is the mode_of seam), Baccai Witherclaw and Noxian Emissary (Empowered Deathknells: when(deathknell(..), empowered); the two Recruits through faithful_manufactor::spawn_recruit), Nasus, Guardian of Knowledge (channel_exhausted once each turn), Hungry Wolf (a per-turn enemy-choices counter, the Ezreal - Prodigal Explorer row, and once each turn), Sky Cruiser (discard a gear as an activation cost, the Unlicensed Armory pay-stage row), Shadow Assassin (enters ready with a namesake in the trash, the enters_ready seam), Mask Mother (a when-you-discard-me trigger from the hand, the Flame Chompers off-the-board row), Ravenbloom Prefect (an opponent-plays-gear trigger subject and SelfCost::BanishSelf, the Zero Drive row) | Forsaken Baccai, Oasis Raider, Fretful Feline, Jayce, Hammer in Hand, Baccai Witherclaw, Noxian Emissary, Nasus, Guardian of Knowledge, Hungry Wolf, Sky Cruiser, Shadow Assassin, Mask Mother, Ravenbloom Prefect |
| `spells-flow-combat` (12) | Flow spells and sorcery-speed kill, damage and move spells (Onslaught / Twilight Shroud shape: spell(name, &[Keyword::Flow(FLOW)], &[play(targets, run)]) with pub const FLOW; kill(), deal(), move_unit, ready, grant_this_turn, might_this_turn): Brittle Steel, Perfect Execution, Twilight Step (Filter::MightAtMost), Lacerate (disempower then a conditional kill), Shuriken Flip (up to one enemy then a friendly move), Shadow Dash (a battlefield where you have units is the Moonfall zone-filter seam; the exactly-two bonus reads at resolution), Public Execution (less Might than the chosen friendly unit: judged at resolution, an anchor-relative Filter is the Angle Shot target-filter row), Dragon Form (base Might becomes 5 this turn: the Might-layers row, a MightIf ladder or a named seam), Decree of Unity (Filter::Domain over unit or gear), Decree of Discord (any number of enemy Order units with total Might 5 or less: the Fox-Fire group-total seam), Siphoning Strike (7 under seven runes; the dies-this-turn channel is the Deadly Flourish death-watcher seam), Cataclysmic Duel (each player picks one of theirs, a per-seat prompt as Party Favors sequences, then kill the rest) | Brittle Steel, Perfect Execution, Twilight Step, Lacerate, Shuriken Flip, Shadow Dash, Public Execution, Dragon Form, Decree of Unity, Decree of Discord, Siphoning Strike, Cataclysmic Duel |
| `spells-reaction-action` (12) | Reaction and Action spells (Defy / Back Off / Hextech Ray shape: Keyword::Reaction or Keyword::Action first, play(targets, run), counter_spell, might_this_turn, bounce, stun): Resonating Strike (Hidden Reaction: your battlefield and a friendly unit elsewhere, move then +2), Mesmerize and Sanction (modal: the Rocket Barrage mode_of seam, Sanction's end-of-turn revert through at_end_of_turn), Ki Barrier (the Counter Strike per-unit prevention seam), Rebuttal (a spell of energy cost 4 or less; pay a rainbow to steal it and re-choose — the Mystic Reversal chain-item control — else counter), Ruthless Strike (an optional discard as the additional cost, the Brazen Buccaneer pay-stage row, 3 or 5), Consuming Curse (2 plus one per namesake in your trash, computed at resolution), Decree of Rage (can't be countered: a per-item uncounterable static named as a seam beside Mel, Newly Awakened's; the deal to an enemy Calm unit runs), Shock Blast (Static::SelfDiscount while you control something Empowered), Guttural Roar (+2, or +4 if the target is Empowered), Wind and Ghosts (banish at 3 or less, else bounce), Dominus (double Might this turn is the Vi - Hotheaded Might-layers shape; the granted ready ability this turn is the Relentless Pursuit granted-ability seam) | Resonating Strike, Mesmerize, Sanction, Ki Barrier, Rebuttal, Ruthless Strike, Consuming Curse, Decree of Rage, Shock Blast, Guttural Roar, Wind and Ghosts, Dominus |
| `spells-card-flow` (8) | draw, look, token and ready spells (Stacked Deck / Dredge Up / Up from the Deep shape: draw, ivern_nurturer::look_at_the_top_three, spawn, ready): Dredge Up (Flow), Lightning Rush (look at three, draw one, trash the rest; Flow), Clairvoyance (Reaction, Predict 5 through scryers_bloom::{look, recycle, predicted, predicted_cards, put_on_top} — the bloom's look and recycle take the count, the order width and the finish — with its own full-reorder ORDER stage, then draw 2), Death Mark (Burn 3 then a Shadow Clone through Token::ShadowClone; Flow), Iterative Design (a Mech token through ferrous_forerunner::spawn_mech until Token::Mech lands; Flow), Wild Claw (look at five, banish and play a unit or gear five energy cheaper — the Rek'Sai Origin::Revealed row — recycle the rest, then may empower it), Shadows of the Past (up to two units from any trash to their owners' hands: bounce from the trash zone), Acceleration Gate (ready up to four among units, gear and runes: Filter::Or over Unit, Gear, Rune) | Dredge Up, Lightning Rush, Clairvoyance, Death Mark, Iterative Design, Wild Claw, Shadows of the Past, Acceleration Gate |
| `gear-equipment-watchers` (9) | the four Equipment, the gear with continuous effects and the two turn watchers (Doran's Shield / Boneshiver / Rage Amplifier-as-Forbidding-Waste shape: gear(name, &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]) with while_attached(&[Grant::Might(n), ...]) carrying the attached line read from the card image): Pendulum Blade (+1; the wearer's move trigger is the Grant::Ability wearer row), Hand Hammer (+1; +2 while exactly one other unit of yours is here, Grant::MightIf), Jagged Cutlass (+2; can't be moved by enemy items, a Grant::Static seam beside Determined Sentry's NoMoveToBase), Shady Spectacles (+0; the wearer becomes a copy of a chosen friendly unit — keeper_of_masks::become_copy_of, the Ctx::become_copy row); Rage Amplifier (Empower plus Static::Aura +1 or +2 over friendly units), Helm of Suppression (Empower plus an opponents' spell surcharge, the Vex - Cheerless SpellSurcharge seam), Endless Riches (a play trigger that banishes hand and trash then Burn 7 runs; skip your Draw Phase, play cards from your trash and banish-instead-of-trash are three game-rule statics, each a named seam); Bottled Constellation (start of your Main Phase is the Iascylla When::MainPhaseOf row; kill three other friendly units and/or gear as the cost is the Commander Ledros pay-stage row; score_point runs), Forgotten Relic (Burn 1 on play and at each Beginning Phase through Trigger::Play and Trigger::BeginningPhase, then +Might equal to the burned unit's Might this turn: burn_cards() returns the ids and a burned card the fold has no face for parks on await_faces — the trash is public, so the host pays the reveal in the same frame — before the kind and Might are read, the Ravenbloom Conservatory pattern) | Pendulum Blade, Hand Hammer, Jagged Cutlass, Shady Spectacles, Rage Amplifier, Helm of Suppression, Endless Riches, Bottled Constellation, Forgotten Relic |
| `empower-economy` (12) | empowerment as an effect, a trigger and a cost, across units and gear (Tail-Cloaked Matriarch / Hextech Formula-as-Sun-Disc shape: prelude::empower(COST), on_empowered, ctx.empower / disempower, activated(Timing, cost, targets, run) with exhausting_self, Static::EntersExhausted): the become-Empowered triggers run today through on_empowered — Apprentice Mage's Predict 2 through the eclipse / scryers_bloom predict flow, Mel, Defiant Soul's banish of a 3-or-less enemy unit (her discard-a-spell Empower cost is the pay-stage seam; the offer gates on hand size alone, since the plugin cannot read hand faces, and the kind is checked after the discard reveals it), Kharox's opponent Burn 3 then a play from their trash through the_harrowing::play_from_trash (the burned cards park on await_faces before the pick, so the units he just burned are offered); Profiteer (the unit or gear to empower is a target chosen on the chain, so Deflect taxes it there; the disempower of something you control is a Resume pick paid at resolution, the pay-stage seam), Tornado Warrior (Hidden, on_play_from_facedown empower here with at_end_of_turn disempower) and Renekton, Brute (a Might-becomes-10 trigger: the Fiora - Worthy BecameMighty row at threshold 10; the pay-1 activation and the Empowered keywords run) empower without the keyword; Questionable Tome and Hextech Disc (Empower paid by exhausting is Keyword::Empower(Cost::FREE) with paying_with(SelfCost::Exhaust) on the empower ability; disempower-this as an activation cost is one SelfCost seam shared with the Zed / Mel / Ambessa / Kennen legends — usable_if(empowered) with the run disempowering first until it lands; the Disc's Mech through ferrous_forerunner::spawn_mech), Platewyrm Egg (enters exhausted; Empower for one energy plus the exhaust; the Reaction [Add] 1 or 2 is the Seals adds_while_paying seam), Tools of Empire (+2, or +4 while Empowered), Hextech Formula (enters exhausted, exhaust to empower another gear), Glowstone (disempower and exhaust to hand control to a chosen player who recalls it — set_controller — then an end-of-turn kill and 5 to all their units through at_end_of_turn) | Apprentice Mage, Mel, Defiant Soul, Kharox, Profiteer, Tornado Warrior, Renekton, Brute, Questionable Tome, Hextech Disc, Platewyrm Egg, Tools of Empire, Hextech Formula, Glowstone |
| `battlefields` (9) | the nine battlefields: triggers (Zaun Warrens / Star Spring shape: Trigger::Conquer(Who::You), Hold(Who::You) with Ability.cost for Protective Sands' optional pay-1 draw, whose four-or-fewer-runes if is the trigger Condition alone (383.2.a.1) so the energy paid at finalization always buys the draw, and Shadow Temple's Burn 3), an aura (Kinkou Temple: Static::Aura scope UnitsHere, when has Tank, Grant::Might(1) — the Forbidding Waste shape), and game-rule statics named as seams: Heisho, Shell of the World (ignore Deflect for items choosing something here: Static::IgnoresDeflect is per-item today), Mystic Vortex (Reaction cards cost a rainbow more during showdowns here, the Vex - Cheerless surcharge row; a Hidden card played from hand is played as normal, 811.3, and has Reaction only facedown or played from facedown, 811.6), Risen Altar and Piltovan Forge (a discount on other cards' activated abilities — Empower costs of units here, the first friendly gear ability each turn — cost::of_item prices abilities off the script alone: one activation-discount seam plus the per-turn counter), Dragon Roost (an optional two-rainbow additional cost any player may pay to play a Dragon here: the Card.additional row from another card plus the play-locations row, over herald_of_scales::is_dragon), Threshold of the Gray (when combat starts here each side adds one energy: the Diana - Lunari ShowdownBegins trigger row plus jhin_murderous_artist::add_to_rune_pool) | Dragon Roost, Heisho, Shell of the World, Kinkou Temple, Mystic Vortex, Piltovan Forge, Protective Sands, Risen Altar, Shadow Temple, Threshold of the Gray |
| `legends` (8) | the eight legends (Nasus - Curator of the Sands / Kha'Zix - Voidreaver / Jinx - Loose Cannon shape: legend(name, &[], &[...]) with activated(Timing, cost, targets, run), exhausting_self, usable_if, YouPlayCard, Trigger::Empowered): Shen - Eye of Twilight (Action exhaust: grant Tank this turn), Jayce - Defender of Tomorrow (Empower on a legend through prelude::empower, ready one gear or two when Empowered: two activations gated by usable_if), Akali - Rogue Assassin (Empower; Action exhaust on your turn: recall a friendly unit from a showdown, ready it when Empowered), Renekton - Butcher of the Sands (a Reaction [Add] 2 for units and their abilities only: the Seals adds_while_paying seam with the Ornn kind filter), Zed - Master of Shadows (a you-banish-a-card-you-own trigger subject is a missing Trigger arm; disempower-me as the activation cost is the SelfCost seam shared with the gear group), Mel - Soul's Reflection and Ambessa - Matriarch of War (when you empower something else: Trigger::Empowered carries no Who, a Who::Friendly-not-me arm is the seam; their disempower-me activations as above), Kennen - Heart of the Tempest — the catalog names the legend `Yordle, Kennen - Heart of the Tempest` and the row keeps that name — (YouPlayCard filtered to an origin other than the hand off Event::Played, then the disempower activation granting Assault 2 this turn) | Shen - Eye of Twilight, Jayce - Defender of Tomorrow, Akali - Rogue Assassin, Renekton - Butcher of the Sands, Zed - Master of Shadows, Mel - Soul's Reflection, Ambessa - Matriarch of War, Yordle, Kennen - Heart of the Tempest |

**Engine gaps.** The triage's reading of where the vocabulary ends, by
row of the M10–M12 tables; the scripting pass supersedes it with the
seams the card files actually name. Every card is scripted as far as the
vocabulary allows, the rest is a `pub fn` seam and an ignored test.

| Mechanic | Cards | Primitive the engine owes |
|---|---|---|
| non-resource costs at the pay stage (355.10.c, 357) | Punching Poro (discard 1), Escaped Grayback (kill a friendly unit), Mel, Defiant Soul (discard a spell), Legion Marauder (one energy or a Body rune, a pick) as Empower costs; Ruthless Strike and Zed, From the Shadows (an optional discard as the additional cost, the Brazen Buccaneer row); Sky Cruiser (discard a gear as the activation cost, the Unlicensed Armory row); Bottled Constellation (kill three other friendly units and/or gear, the Commander Ledros row); Questionable Tome, Hextech Disc, Glowstone, Zed - Master of Shadows, Mel - Soul's Reflection, Ambessa - Matriarch of War, Yordle, Kennen - Heart of the Tempest (disempower this as an activation cost) | the M10 pick-prompt design, unchanged, plus one `SelfCost::Disempower` arm beside `KillSelf` and `BanishSelf` that `activate` pays by `Ctx::disempower` and refuses while not Empowered — until then `usable_if(empowered)` with the run disempowering first. Exhaust as an Empower cost (Questionable Tome, Hextech Disc, Platewyrm Egg) is `paying_with(SelfCost::Exhaust)` on the empower ability and runs today. |
| discounts and surcharges from other cards | Risen Altar (Empower costs of your units here one energy or a rainbow less), Piltovan Forge (the first friendly gear activated ability each turn one energy less), Helm of Suppression (opponents' spells one energy more, one energy and a rainbow while Empowered), Mystic Vortex (cards with Reaction cost a rainbow more during showdowns here), Applied Researchers (Empowered: your spells one energy and a rainbow less, floor one), Shock Blast, Stargazer, Plaza Guardian, Shadowblade Lurker, Keeper of Law (self discounts, live through `Static::SelfDiscount` / `SpellDiscount`; the `legal::classify` pricing gap of Eager Apprentice applies) | `Static::AbilityDiscount(ItemDiscount)` consulted by `cost::of_item` for `ItemKind::Ability` (today it prices an ability off its own script alone) with a per-turn gear-ability counter for the Forge, and `Static::SpellSurcharge` (the Vex - Cheerless seam) extended to a Reaction-keyword filter and a showdown-here condition. |
| enters ready (369.3) | Shadow Assassin (a namesake in your trash), Corrupted Dragon (your score not within three of the Victory Score) | `Static::EntersReady(Applies)`, unchanged; both `enters_ready` predicates are live. |
| missing trigger subjects | Affectionate Poro and Mournful Witness (`CombatEnded(Who::Me)`: `CombatWon` / `CombatLost` carry only the outcome; the Poro also reads whether it was dealt damage this turn), Renekton, Brute (`MightReached(Who::Me, 10)`, the Fiora - Worthy BecameMighty row at a threshold), Ravenbloom Prefect (`OpponentPlaysGear`), Mask Mother (`Discarded(Who::Me)` heard from the hand, the Flame Chompers off-the-board row), Zed - Master of Shadows (`Banished(Who::You)` over a card you own), Mel - Soul's Reflection and Ambessa - Matriarch of War (`Empowered` of another friendly card: the arm carries no `Who`), Illaoi (`Scored(Who::Me)`, the Sumpworks Map row), Threshold of the Gray (`ShowdownBegins(Where::Here)`, the Diana - Lunari row), Bottled Constellation (`When::MainPhaseOf(seat)`, the Iascylla row) | `Trigger` variants and `triggers::matches` arms for each subject; `combat` and `cleanup` raising a combat-ended event with the participants, `Ctx::might` writers raising a threshold event, `play::finalize` raising the opponent's gear, `discard` raising from the hand, `Ctx::banish` raising with the owner, `Ctx::empower` carrying the card for a `Who` filter, `Ctx::score` raising, `showdown::open` raising, `phases` queueing the Main Phase delay. |
| game-rule statics | Ol' Poro (unplayable on your first three turns: a per-seat turn counter, the Obelisk of Power row, read by `legal::classify`), Sandstone Chimera (players channel one rune at the Channel Phase while it is at a battlefield), Otterpus (a point from a conquer or hold on a player's first or second turn draws instead: the Tianna Crownguard `Ctx::score` consult with a replacement), Fallen Feline (name a spell as it is played — The List name-prompt row — then opponents cannot play spells of that name while it is at a battlefield, the Brynhir Thundersong lock keyed on a name), Endless Riches (skip your Draw Phase; play cards from your trash; a card bound for your trash from anywhere but the main deck is banished instead — three hooks: `phases`, `legal::locations_for` over the trash zone, `Ctx::trash`), Dune Surfer (you ignore Tank while assigning combat damage here: `combat` assignment), Esteemed Hierophant (prevent all damage from enemy spells and abilities while you control seven or more runes: a static prevention beside `prevent::amount`), Ambessa, The Wolf (Empowered: cannot be dealt damage unless in combat, the Kayn - Unleashed `Static::NoDamage` row), Sacred Protector (own `NoCombatDamageFrom` consult, the Galio row), Heisho, Shell of the World (players ignore Deflect while paying for items choosing something here: `cost::deflect` reads `Static::IgnoresDeflect` off the paying item only), Mel, Newly Awakened (Empowered: your spells and abilities cannot be countered, and a chosen -Might from your items is one more), Decree of Rage (this spell cannot be countered: a per-item uncounterable flag `counter_spell` consults), Gangplank, Naval (Empowered: an item choosing him that would stun, -Might or bounce him gives +3 instead — a replacement on three effect paths, today only the kill path has one), Kayle, Justified (Empowered up to three times: `Ctx::empower` is a flag, the counter is `COUNTER_EMPOWERED` capped at three with `is_empowered` reading it as nonzero), Jagged Cutlass (the wearer cannot be moved by enemy spells and abilities: a `Grant::Static(NoMoveByEnemy)` beside Determined Sentry's `NoMoveToBase`) | one hook each, named in the card; every predicate is live and tested. Spiderling's "any number of copies" is a deck rule for `agni_riftbound::legality`'s copy limit, not the engine. |
| Might layers (476) | Dragon Form (base Might becomes 5 this turn), Dominus (double Might this turn, the Vi - Hotheaded shape that runs), Dame the Despoiler (Might set to the chosen unit's then +1, computed through `might_this_turn`), Spiderling, Repair Specialist, Illaoi (a dynamic Might or Assault: `MightIf` ladders until `Grant::MightBy`) | a base-Might override on `CardState` for the turn read first by `current_might`, and `Grant::MightBy`, unchanged. |
| floating turn effects | Dominus (a granted ability this turn: "two rainbow: ready me", the Relentless Pursuit row), Kennen, Storm of Shuriken (a Flow equal to its cost granted to a trash spell this turn: `activate::flow_playable` reads printed keywords only, the Syndra - Transcendent granted-keyword row), Siphoning Strike (when it dies this turn, channel one exhausted: the Deadly Flourish death-watcher row); Sanction and Tornado Warrior's end-of-turn reverts run through `at_end_of_turn` | the M10 primitive, unchanged. |
| per-turn counters | Swain, Visionary (a non-token unit, a non-token gear and a spell played this turn), Hungry Wolf (an enemy unit chosen this turn, the Ezreal - Prodigal Explorer row), Piltovan Forge (gear abilities activated this turn), Affectionate Poro (damage dealt to it this turn), Ol' Poro and Otterpus (a seat's turn number, the Obelisk row); Blade Twirler and Jayce, Brilliant Inventor run on `once_each_turn` | counters on `SeatState` and `CardState` reset at Expiration; every reader works within one request today. |
| tokens | Iterative Design and Hextech Disc (a 3 Might Mech: `ferrous_forerunner::spawn_mech` until `Token::Mech`), Noxian Emissary (two Recruits: `faithful_manufactor::spawn_recruit`), Shady Spectacles (the wearer becomes a copy of a chosen friendly unit while attached: `keeper_of_masks::become_copy_of`, the `Ctx::become_copy` row, with a revert on detach); Zed, From the Shadows, Death Mark (Shadow Clone) and Illaoi (Tentacle) spawn through `Token::ShadowClone` / `Token::Tentacle` today | `Token::Mech`, `Token::Recruit`, `Ctx::become_copy`, unchanged. |
| play locations | Ocean Drake (you may play me to an open battlefield: the Sneaky Deckhand `open_play_locations` seam), Dragon Roost (any player may pay two rainbow as an additional cost to play a Dragon, then plays it here: an additional cost offered by another card plus a location it dictates, over `herald_of_scales::is_dragon`) | `Ctx::play_locations` / `legal::play_to` consulting in-play statics, and `Card.additional` generalised to a cost another card offers. |
| prompts, origins and limited plays | Mesmerize, Sanction, Minah Swiftfoot, Jayce, Hammer in Hand (named modes, the Rocket Barrage `mode_of` row), Cataclysmic Duel (each player picks one unit of theirs in turn order, the Party Favors row), Fallen Feline (a name prompt, The List row), Wild Claw (look at five, banish a unit or gear and play it five energy cheaper — `Origin::Revealed`, the Rek'Sai row — recycle the rest), Kharox (a unit played from an opponent's trash ignoring its cost: `the_harrowing::play_from_trash` with another owner's trash), Rebuttal (pay a rainbow to take control of a spell of energy cost four or less and re-choose its targets, else counter it: the Mystic Reversal chain-item control row) | the M10–M12 list, unchanged. |
| target filters (355) | Decree of Discord (any number of enemy Order units with total Might five or less: the Fox-Fire group-total seam), Public Execution (an enemy unit with less Might than the chosen friendly unit: the Angle Shot anchor-relative row, judged again at resolution), Shadow Dash (a battlefield where you have units: the Moonfall zone-filter row), Shadows of the Past (units in any trash: `Filter::InTrash` without `Friendly`, which `targets` should already serve); Noxian Demolitionist and Ambessa, Respected and Feared judge Might and energy in a candidates fn and run today | the M11 `Filter` arms, unchanged. |
| prevention (437) | Ki Barrier (prevent the next seven damage to a unit this turn: the Counter Strike per-unit `Amount::Next` seam), Esteemed Hierophant (the static prevention above) | `Prevention` carrying a unit, unchanged. |
| [Add] payment sources (429) and the rune pool (167) | Renekton - Butcher of the Sands (a Reaction [Add] two energy for units and units' abilities only: the Seals `adds_while_paying` seam with the Ornn kind filter), Platewyrm Egg (a Reaction [Add] one, two while Empowered), Threshold of the Gray (as combat starts here the attacker and the defender each add one energy: the banked pool of `jhin_murderous_artist::add_to_rune_pool`) | the M10–M12 generalisation, unchanged. |
| equipment abilities on the wearer (818) | Pendulum Blade (the wearer's move to a battlefield gives +2 this turn: `Grant::Ability`), Hand Hammer (+2 while exactly one other unit of yours is here: `Grant::MightIf`, runs), Jagged Cutlass and Shady Spectacles (the rows above) | `Grant::Ability` resolved by `triggers::sources`, unchanged. |
| keywords the engine does not read | Swain, Visionary (Vision), Kennen, Storm of Shuriken (a granted Flow) | `IMPLICIT_VISION` and the granted-keyword row, unchanged. |

Of the rows, the pay-stage design (non-resource costs, `SelfCost::Disempower`)
touches the most cards — eighteen, eleven of them Empower or disempower
costs, which is the mechanic Vendetta is built on — and the surcharge and
ability-discount statics the next most; both are the M10 designs with one
more arm each. The only new shapes the set opens are small: a base-Might
override for the turn (Dragon Form), a replacement on the stun, -Might and
bounce paths (Gangplank, Naval), an Empower counter (Kayle, Justified), a
combat-ended trigger with its participants, and the three Endless Riches
hooks.

### M13 — what Vendetta still needs

The scripting pass landed 148 card files beside the 18 the six deck files
already scripted, one for each of the 166 rows of `vendetta.md`: 130 run
end to end on the M0–M12 vocabulary, 2 are vanillas whose printed
keywords are the whole script (Horns of the Dragon, Soulspinner) and 16
stay stubs — `CARD` is the generic constructor of its kind with the
printed keywords — because their entire text is one mechanic the engine
does not have yet. `vendetta.md` therefore stays `Scripted: partial`; the
coverage test pins the 16 by name (`SEAM_STUBS` in
`the_vendetta_set_file_is_partial_lists_every_base_print_and_its_stubs_wait_on_named_seams`)
and the vanillas by name, so a stub that gains a script or a script that
regresses to a stub fails the build.

The rules are the M10 rules. Every gap is a *seam*: a `pub fn` in the card
file whose name says what the engine still owes, tested for what it reads,
and an `#[ignore = "engine gap · …"]` test that states the behaviour the
card wants end to end and fails today for the stated reason — all 79 fail
when run with `--ignored` (380 across the four sets), none is a
placeholder. A Vendetta card that needs a primitive an Origins,
Spiritforged or Unleashed sibling already named gets the sibling's seam,
never a second private workaround, and the integration pass folded the
duplicates the parallel scripting had grown into one holder each:
`herald_of_scales::DRAGONS` carries the four Vendetta Dragons (Cloud Drake,
Corrupted Dragon, Eclipse Dragon, Ocean Drake) beside the fifteen it had,
so Ocean Drake's `NON_DRAGON_UNIT` and Dragon Roost read one list;
`prelude::{is_empowered, empowered, when_empowered}` are the one Empowered
predicate in its three shapes (`Applies`, `Usable`, `Condition`) that the
thirty "Empowered: I have …" statics, triggers and usable gates read
instead of a wrapper each; `questionable_tome::disempower_this_as_the_cost`
is the one disempower-me cost, which `ambessa_matriarch_of_war::disempower_me`
delegates to for the four legends; `brazen_buccaneer::a_discard_can_be_offered_as_the_additional_cost`
gates Ruthless Strike and Zed, From the Shadows alike;
`ol_poro::turn_number_of` is the one seat-turn counter Otterpus imports;
`cruel_patron::{kill_candidates, kill_cost_payable, pay_kill_cost}` carry
Escaped Grayback's kill; `flame_chompers`, `the_zero_drive`,
`sneaky_deckhand`, `diana_lunari`, `keeper_of_masks`,
`jhin_murderous_artist`, `the_harrowing`, `rek_sai_void_burrower` and
`ferrous_forerunner` lend their seams to Mask Mother, Ravenbloom Prefect,
Ocean Drake, Mystic Vortex, Shady Spectacles, Threshold of the Gray,
Kharox, Wild Claw, Iterative Design and Hextech Disc. Two cards the triage
expected to open a gap did not: Aurok General's both-Empowered aura runs
today as `Static::Aura { FriendlyUnits, when: both_empowered,
Grant::Might(2) }` (the general included), and Wild Claw's look-banish-play
finalizes inline (419.3) so the may-Empower lands on a unit already on the
board — the card really visits Banishment, so the Rek'Sai `Origin::Revealed`
row does not apply to it.

The table merges the M10, M11 and M12 rows where the primitive is the same
— the Vendetta cards are added to the row and the primitive column says
what, if anything, the new cards ask of it beyond the earlier wording —
and adds the rows Vendetta opened. "Mechanic → cards → primitive" is the
reading order; a card in **bold** is one of the 16 stubs.

| Mechanic | Cards | Primitive the engine owes |
|---|---|---|
| non-resource costs at the pay stage (355.10.c, 357, 383.3.b) | M10, M11 and M12 as listed; Vendetta: Punching Poro (discard 1 as the Empower cost: `discarded_as_the_empower_cost_until_the_pay_stage_asks_for_it` gates the offer, `discard_cost_payable` and `prelude::ask_discard` pay it at resolution, where a response can empty the hand), Escaped Grayback (kill a friendly unit as the Empower cost, himself included, through `cruel_patron::{kill_candidates, kill_cost_payable, pay_kill_cost}` — a target killed at resolution that raises `Chosen`), Mel, Defiant Soul (discard a spell: the hand's size gates — `spells_in_hand` cannot be read by the plugin — and the engine's Discard prompt at resolution empowers only when `discarded_kind` is a spell), Legion Marauder (`ALTERNATIVES` — one energy or a Body rune: `chosen_alternative_until_the_pay_stage_asks_energy_or_body` decides for the player since `ExtraCost` returns one `Cost`, the energy unless only a Body rune can pay; the Irelia - Graceful REDUCTIONS pick), Ruthless Strike and Zed, From the Shadows (an optional discard as the additional cost, the Brazen Buccaneer row: `Card.additional` is energy and power only, so the play never asks; the Strike's `damage_for(paid_additional(item))` reads five once `STAGE_ADDITIONAL` records the discard, Zed's paid play conjures the Shadow Clone), Sky Cruiser (discard a gear as the base cost, the Unlicensed Armory row: `can_discard_a_gear` gates on the hand's size, `ask_discard` at resolution, a non-gear pick leaves the cost unpaid and nothing dealt), Profiteer (disempower something you control as the cost within instructions after the you may, 383.3.b: `empowered_things_you_control` is the candidate list, picked and paid at resolution through a min-1 Resume, while the empower target rides the chain as `EMPOWER_TARGET`), Bottled Constellation (kill three other friendly units and/or gear, the Commander Ledros row: `kill_candidates` / `kill_cost_payable` / `pay_kills` through a 0..3 `Resume` pick at resolution), `SelfCost::Disempower` — Questionable Tome, Hextech Disc, Glowstone (`questionable_tome::disempower_this_as_the_cost`), Ambessa - Matriarch of War, Zed - Master of Shadows, Mel - Soul's Reflection, Yordle, Kennen - Heart of the Tempest (`ambessa_matriarch_of_war::disempower_me` over it; `usable_if(prelude::empowered)` gates the offer and the run disempowers first, which on a legend `Ctx::set_counter` refuses), **Ravenbloom Prefect** (`SelfCost::BanishSelf`, the Zero Drive row, through `the_zero_drive::banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization`) | the M10 design — a pick prompt at the pay stage before the item exists, recorded on the item, uncounterable — plus a `SelfCost::Disempower` arm beside `KillSelf` and `BanishTarget` that `activate::pay_self` pays through `Ctx::disempower` and refuses while not Empowered (Ravenbloom Prefect's `BanishSelf` is a second missing arm: no `SelfCost` banishes the source), Gust Monk's banish-a-card-from-any-trash is no gap — it is the Shadow Clone shape, `paying_with(.., SelfCost::BanishTarget)` paying target 0 at finalization —, a discard-N additional-cost kind asked at `STAGE_ADDITIONAL` and recorded as `paid_additional`, a kind-filtered hand pick, and the either-or pick between alternative costs. Every candidate list and pay is live in the card file. |
| Empowered legends and the Empower counter | Akali - Rogue Assassin, Jayce - Defender of Tomorrow, Ambessa - Matriarch of War, Mel - Soul's Reflection, Zed - Master of Shadows, Yordle, Kennen - Heart of the Tempest (a legend cannot be Empowered: `Ctx::set_counter` refuses any card that is not `on_board`, and the legend zone is not, so `Ctx::empower` on a legend emits no counter and raises no `Empowered` event), Profiteer (`legend_can_be_empowered` keeps legends out of both offers until it can), Kayle, Justified (`COUNTER_EMPOWERED` is capped at one in the riftbound manifest, `games/riftbound/src/lib.rs`, mirrored by the fixture's counter table, and `Ctx::empower` sets a flag; `times_empowered`, `counter_room`, `can_be_empowered_again` and `empower_once_more` count, ladder her Might and raise `Event::Empowered` per Empower, and run end to end on a fixture whose cap is raised to three) | `Ctx::set_counter` reading `face_in_play` instead of `on_board`, and the manifest's cap on `COUNTER_EMPOWERED` raised to `kayle_justified::TIMES` (3) with `Ctx::empower` incrementing to the cap (`is_empowered` already reads it as nonzero). |
| missing trigger subjects | M10, M11 and M12 as listed; Vendetta: **Affectionate Poro** and Mournful Witness (`CombatEnded(Who::Me)`: `cleanup::after_combat` raises `CombatWon` / `CombatLost` with the zone and the seat only — no participants, nothing on a tie; the Poro's `draw_if_undamaged_after_my_combat` and the Witness's `empower_me_after_my_combat` are live on a hand-made trigger item), Renekton, Brute (`MightReached(Who::Me, 10)`, the Fiora - Worthy BecameMighty row at a threshold: `WHEN_MY_MIGHT_BECOMES_TEN`, `might_reached_ten(before, after)`, `empower_me`), **Ravenbloom Prefect** (`OpponentPlaysGear`: `an_opponent_played_a_gear_until_trigger_opponent_plays_gear_matches_it` and `confiscate` are live), **Mask Mother** (`Discarded(Who::Me)` heard from the hand into the trash, the Flame Chompers off-the-board row; `embolden` is live), Zed - Master of Shadows (`Banished(Who::You)` for `Event::Banished` over a card you own), Mel - Soul's Reflection and Ambessa - Matriarch of War and Mel - Soul's Reflection ("when you empower something else" keys on who performs the empower, not on the card's controller — your Profiteer empowering an enemy unit counts, an opponent's Sanction on your unit does not — so `Event::Empowered` wants the empowering item's controller beside the card; `Trigger::Empowered` matches the source's own event only, and `you_empower_something_else` reads the card's controller as the stand-in condition until the event carries the actor), Yordle, Kennen - Heart of the Tempest (`Event::PlayedSpell` carries the item id and no origin, so a Flow spell from the trash is invisible to his condition — the `PlayedSpell` row), **Threshold of the Gray** (`Trigger::ShowdownBegins(Where::Here)` raised with the zone and both seats as a combat opens, 464.2.b, the Diana - Lunari row; `each_side_adds_one` is live) | `Trigger` variants and `triggers::matches` arms for each subject; `after_combat` raising `CombatEnded` with the participants *before* `heal_all`, a Might writer raising an event as `current_might` crosses a threshold, `Ctx::banish` and `play::begin` raising the seat-scoped events. Each card carries the condition and the run; the ignore reason names the exact wiring. |
| enters ready (369.3) | M10, M11 and M12 as listed; Vendetta: **Shadow Assassin** (`enters_ready` over `namesakes_in_my_trash`, by `base_name`), Corrupted Dragon (`enters_ready` / `far_from_victory`: your score not within three of the Victory Score, tested against 0/4/5/7/8 of 8 and a 6-point table; the attack trigger runs end to end) | `Static::EntersReady(Applies)` consulted by `play::finalize`, unchanged. |
| damage modifiers (712–715), prevention (437) and lethal (142.3) | M10, M11 and M12 as listed; Vendetta: Ambessa, The Wolf (`takes_no_damage`: the Kayn - Unleashed `Static::NoDamage(Applies)` row — no damage out of combat while Empowered, exempt from lethal assignment), **Esteemed Hierophant** (`prevents_all_damage_from(ctx, me, cause)` over `you_control_seven_runes` and `from_an_enemy_spell_or_ability`: a static prevention beside `prevent::amount` that prevents an enemy item's damage in full, 437.1.b.1.b, and exempts the unit from its lethal), Ki Barrier (`prevent_the_next_damage_to_this_turn(ctx, unit, 7)`: the Counter Strike per-unit row, now `Amount::N` spent across instances and cleared at Expiration; `Prevention` carries no unit today), Sacred Protector (`deals_no_combat_damage(ctx, source, unit)`: `Ctx::deals_combat_damage` consults every other card's `NoCombatDamageFrom` and skips the unit's own — the Galio - Indefatigable / Ezreal - Dashing row; true for itself unless exactly one other friendly unit shares its battlefield), **Affectionate Poro** (`undamaged_this_turn`: `after_combat` heals every unit before any trigger resolves, so `damage_on` reads 0 for a Poro that was hit — a per-turn damage record) | `Ctx::damage` consulting in-play `NoDamage` statics and static preventions before `prevent::amount`, a unit plus `Amount::N` on `Prevention`, `deals_combat_damage` reading a unit's own static, and a per-unit damage-this-turn record that survives the heal. |
| discounts and surcharges from other cards | M10, M11 and M12 as listed; Vendetta: Stargazer (`flow_discount` is live through `cost::of_item` / `activate::flow_cost`, but `legal::from_trash` prices the Flow drag with `legal::flow_cost` — the printed Flow cost alone — the Eager Apprentice pricing gap), Applied Researchers (`researching` / `discount`: live through `Static::SpellDiscount` gated on `is_empowered` inside the discount fn, since `cost::spell_discounts` reads `script.statics` directly and a `Static::While` over `Grant::Static(SpellDiscount)` would never be folded; the remaining gap is the same `legal::classify` pricing), Helm of Suppression (`spell_surcharge(ctx, item, helm) -> Cost` with the `ItemDiscount` signature: `Static::SpellSurcharge` summed into `cost::base_of_item`, the Vex - Cheerless row — one energy, one energy and a rainbow while Empowered), **Mystic Vortex** (`reaction_surcharge(ctx, item, vortex)`: a rainbow while `diana_lunari::a_showdown_is_open_here` and the item is a spell or permanent with Reaction, Hidden, `hide::reacts` or a facedown origin — the same `SpellSurcharge`), **Risen Altar** (`empower_discount`: `cost::discounts_of` returns nothing for `ItemKind::Ability` and `cost::of_item` prices an ability off its own script alone; one energy *or* one rainbow off the Empower of any unit here is also the Irelia - Graceful pick), **Piltovan Forge** (`gear_ability_discount` over `gear_abilities_played_this_turn`: the same ability discount plus the per-seat counter below), Plaza Guardian, Shadowblade Lurker (a same-name trash count), Keeper of Law, Shock Blast (self discounts, live through `Static::SelfDiscount`) | `Static::SpellSurcharge(ItemDiscount)` summed into `cost::base_of_item` and `Static::AbilityDiscount(ItemDiscount)` consulted by `cost::of_item` for abilities, both folded by `cost::discounts_of`, and `legal::classify` / `legal::from_trash` pricing with the same arithmetic the item is charged. |
| game-rule statics | M10, M11 and M12 as listed; Vendetta: **Ol' Poro** (`cannot_be_played`: a `Static::PlayVeto(Applies)` consulted by `legal::classify`, refusing the play with its own `Reason` on a seat's first three turns), **Sandstone Chimera** (`throttles_the_channel`, `a_chimera_stands_at_a_battlefield`, `runes_this_turn`: `phases::continue_beginning` channels `rules::runes_this_turn` and consults no in-play card — a `Static::ChannelCount` consult), **Otterpus** (`is_an_early_turn_of`, `an_otterpus_is_in_play`, `draws_instead_of_scoring`, `score_or_draw`: the Tianna Crownguard `Ctx::score` consult, as a replacement that draws one on a player's first two turns), Fallen Feline (`named_spell`, `is_a_spell_named`, `locks`, `cannot_play`: `legal::classify` consulting a lock keyed on a name — the Brynhir Thundersong lock — once the name prompt below exists), Mel, Newly Awakened and Decree of Rage (`Static::Uncounterable(Applies)` consulted by `chain::counter` / `Ctx::counter_item` and the counter spells' candidate lists: `decree_of_rage::cant_be_countered` is the per-item flag, `mel_newly_awakened::cannot_be_countered` the one she grants her controller's spells and abilities while Empowered; her `amplified` also wants `prelude::might_this_turn` / `Ctx::might` passing a negative delta through it for the item's controller), Gangplank, Naval (`shrugs_off` / `plus_three_instead`: a replacement on the stun, -Might and bounce paths when `Ctx::stun` / `Ctx::might` / `Ctx::bounce` run for a chain item — today only the kill path has one; the +3 is `Expiry::Permanent`, 477.3.b), **Dune Surfer** (`you_ignore_tank_assigning_here(ctx, surfer, assigner, zone)`: `combat::candidates` / `combat::ordered` honour Tank for every assigner and want a per-assigner consult over the in-play Surfers), **Heisho, Shell of the World** (`deflect_ignored_here(ctx, item, extra)` / `chooses_something_here`: `cost::deflect` reads `Static::IgnoresDeflect` off the paying item only — a table-wide consult before summing the chosen cards' Deflect), Endless Riches (three hooks: `skips_draw_phase(ctx, seat)` consulted by `phases::continue_beginning` before the Draw Phase draw; `may_play_cards_from_trash(ctx, seat)` consulted by `legal::from_trash` / `legal::locations_for`, a trash card priced as a hand play with `Origin::Trash { leave: Banish }`; `banishes_instead_of_trash(ctx, card, from)` consulted by `Ctx::trash`, discard and the kill path on every trash-bound move except the Burn from the Main Deck), Jagged Cutlass (`cannot_be_moved_by_enemy_items(ctx, unit)` / `item_cannot_move(ctx, item, unit)`: no `Static::NoMoveByEnemy` beside `NoMoveToBase`, and `march::effect_move`, `Filter::Movable` and the group-move prompt consult no item controller — the Determined Sentry moves row), Kharox (runs end to end: `ctx.set_controller` then `the_harrowing::play_from_trash`; the owner's-zones gap on the stolen unit's death is the M10 Possession row) | one hook each, named in the card; every predicate is live and tested. |
| floating turn effects | M10, M11 and M12 as listed; Vendetta: Dominus (`readies_for_two_rainbow_this_turn`, `READY_ME`, `READY_ME_LABEL`: a granted activated ability for the turn, the Relentless Pursuit row — `activate::offers` reads the source's own script and `Grant` carries no `Ability`; the double-Might half runs), Kennen, Storm of Shuriken (`grants_flow(ctx, seat, spell)`, `flow_for_its_cost`, `flow_granted_this_turn_until_card_state_carries_a_costed_keyword`: `Keyword::Flow(Cost)` has no `from_codes` arm so `CardState.granted` cannot carry it, and `legal::flow_cost` / `activate::flow_playable` read printed keywords only — the Syndra - Transcendent granted-keyword row), Siphoning Strike (`channel_when_it_dies_this_turn`: a turn-scoped death watcher sourced from a resolved item, the Deadly Flourish row; the `AfterKillsBy` path pays when the strike itself kills), Dragon Form (`base_might_becomes` / `base_might_delta`: a base-Might override on `CardState` for the turn read first by `current_might` — the Might-layers row; today a this-turn delta off the printed Might, so a second Dragon Form the same turn stacks); Sanction's and Tornado Warrior's end-of-turn reverts run through `at_end_of_turn` | the M10 primitive with a granted activated ability, a this-turn Flow on `CardState` consulted by both readers, a watcher sourced from a resolved spell, and a base-Might layer. The seams narrate today. |
| per-turn counters | M10, M11 and M12 as listed; Vendetta: Swain, Visionary (`non_token_unit_played_this_turn` / `non_token_gear_played_this_turn` read `FLAG_ENTERED_THIS_TURN` and `CardState.entered` on the board plus this request's `Event::Played`, so a card played earlier this turn and gone since is forgotten; `spell_played_this_turn` reads `SeatState.spells_played`, already per-turn), Hungry Wolf (`chose_an_enemy_unit_this_turn` reads this request's `Event::Chosen` only, the Ezreal - Prodigal Explorer row — an activation is always a later request than the spell that chose, so the gate never opens in play and the ability is dead until the counter lands), **Piltovan Forge** (`gear_abilities_played_this_turn` reads this request's `Activated` events and the chain), **Ol' Poro** and **Otterpus** (`ol_poro::turn_number_of`: a seat's turn number, the Obelisk of Power row; an extra turn in the opening rounds shifts it), **Affectionate Poro** (the damage record above); Blade Twirler and Jayce, Brilliant Inventor run on `once_each_turn` | counters on `SeatState` and `CardState` reset at Expiration: kinds played, enemy choices, gear abilities activated, a per-seat turn number, a per-unit damage record. Every reader works within one request today. |
| prompts, origins and limited plays | M10, M11 and M12 as listed; Vendetta: Mesmerize and Sanction (`mode_of` reads the mode off the pick, the Rocket Barrage / Party Favors named-modes row; Sanction's Empower-then-disempower on an already Empowered unit cannot be chosen), Cataclysmic Duel (no gap: a Resume addressed to a seat other than the item's controller skips the untargetable filter, 355.10.e — a shrouded unit is offered to its own controller — and a mandatory Resume with nothing to offer answers itself `Done` in `prompts::next_auto` instead of stalling the game), Akali, Deadly Weapon and Ambessa, Respected and Feared (a Resume over enemy units passes `prelude::choosable` — untargetable and Deflect-affordable — and Akali's pick pays the Deflect through `prelude::pay_deflect`; the unit is a target chosen at finalization by 355.5.b, the Akali ignore names it), Minah Swiftfoot (`mode_options` / `mode_of`: the two modes ride `TargetRef::Zone` stand-ins — the trash for "each player discards 1", the main deck for "each player draws 1"), Jayce, Hammer in Hand (`Mode`, `MODES`, `keyword_of`, `label_of`, `mode_of`, `give` are live and `on_readied` fires; `choose_one` narrates and gives nothing until a named-mode prompt exists), Fallen Feline (a name prompt kind plus a stored name on `CardState`, The List row; the play trigger narrates the owed prompt), Kharox (`play_from_their_trash` runs), Wild Claw (no gap, see above) | named modes on `Resume`, a name prompt kind and a `CardState` slot for the answer; the rest unchanged. |
| target filters (355) | M11 and M12 as listed; Vendetta: Decree of Discord (`fits_total_might`: the 355.11 group-total Might filter, the Fox-Fire row; 3 + 3 is offered at play and trimmed at resolution through a `SUBSET` resume), Public Execution (`has_less_might_than`: `Filter::MightLessThan(anchor)`, the Angle Shot anchor-relative row; judged again at resolution), Shadow Dash and Moonfall (`shadow_dash::your_units_at` / `moonfall::you_have_units_at`: a zone filter for a battlefield where you have units — every battlefield elsewhere is offered at play and the empty one refused as the spell resolves), Akali - Rogue Assassin (`Filter::InShowdown`: only units at the showdown's battlefield; the run refuses the others at resolution), Shadows of the Past (`Filter::InTrash` without `Friendly`, served today) | three more `Filter` arms read by `targets::candidates_with`; each spell judges the picks again at resolution and leaves a mismatch alone. |
| play locations and moves | M10, M11 and M12 as listed; Vendetta: Ocean Drake (`open_play_locations`: the Sneaky Deckhand / Sai Scout seam, not consulted by `Ctx::play_locations` or `legal::play_to`; the non-Dragon bounce runs), **Dragon Roost** (`roost_additional_cost` at `STAGE_ADDITIONAL` for a Dragon of any player over every roost in play, and `roost_play_location` in place of the location prompt when the two rainbows are paid: an additional cost offered by *another* card plus the location it dictates), Jagged Cutlass (the moves row above) | `Ctx::play_locations` and `legal::play_to` consulting in-play statics, `play::advance` consulting other cards' additional costs at `STAGE_ADDITIONAL`, and `march` consulting the moving item's controller. |
| [Add] payment sources (429) and the rune pool (167) | M10, M11 and M12 as listed; Vendetta: **Renekton - Butcher of the Sands** (`adds_while_paying`: two energy for a unit play or a unit's ability only, by recycling two runes for his rainbow and exhausting, as a Reaction — the Seals seam with the Ornn - Fire Below the Mountain kind filter), Platewyrm Egg (`adds_while_paying(ctx, seat, egg) -> Option<Cost>`: one energy ready, two while Empowered, as a Reaction), **Threshold of the Gray** (as combat starts here the attacker and the defender each add one energy: the banked pool of `jhin_murderous_artist::add_to_rune_pool` for both seats, plus the `ShowdownBegins` subject above) | the `adds_while_paying` generalisation of the Gold path and the per-seat pool, unchanged. |
| tokens and copies | M10 Token::Recruit, M11 Token::Mech, M12 Token::Bird / Reflection and the copy; Vendetta: Iterative Design and Hextech Disc (a 3 Might Mech through `ferrous_forerunner::{play_mechs, spawn_mech}` until `Token::Mech` lands in `engine/ctx.rs` and `cards/mod.rs` knows the name — Iterative Design carries the ignored test), Noxian Emissary (two Recruits through `faithful_manufactor::spawn_recruit`), Shady Spectacles (`keeper_of_masks::become_copy_of` narrates: `Ctx::become_copy`, 477.1.b, rewriting the wearer's name, script binding and Might while attached and reverting on detach); Zed, From the Shadows and Death Mark (`Token::ShadowClone`) and Illaoi, Prophet of the Great Kraken (`Token::Tentacle`) spawn through the engine today | `Token::Mech`, `Token::Recruit` and `Ctx::become_copy`, now with a revert on detach. |
| equipment abilities on the wearer (818) | M11 and M12 as listed; Vendetta: Pendulum Blade (the move listener `when(triggered(Move { Friendly, Battlefield }), cull::wearer_is_subject)`: `triggers::sources` skips attached gear; the +2 run is live), Hand Hammer (`Grant::MightIf`, runs), Jagged Cutlass and Shady Spectacles (the rows above) | `Grant::Ability` on a `WhileAttached` grant list resolved by `triggers::sources` as the wearer's own, unchanged. |
| delayed triggers (316.4, 359.3.f) | M12: Iascylla, Blue Sentinel, Ashe - Focused; Vendetta: Bottled Constellation (`wishes_at_the_start_of_the_main_phase_of(ctx, seat)` lists the bottles to queue — ability `WISH`, `Trigger::Reflexive` — and is consulted by nothing since there is no `When::MainPhaseOf(seat)`; the three kills are the pay-stage row above) | `When::MainPhaseOf(seat)` queued by `phases::continue_beginning` as the Action phase opens, unchanged. |
| keywords the engine does not read | M10 Vision, M11 Quick-Draw / granted Weaponmaster / granted Accelerate, M12 granted Repeat; Vendetta: Swain, Visionary (Vision, 817: `IMPLICIT_VISION`, the Mystic Poro row — the keyword is printed, no look, no recycle), Kennen, Storm of Shuriken (a granted Flow, the row above) | `IMPLICIT_VISION` and a costed keyword on `CardState.granted`. |
| tags on the face | M10, M11 and M12 as listed; Vendetta: Ocean Drake and Dragon Roost (Dragon, through `herald_of_scales::{DRAGONS, is_dragon}`, which grew the set's four) | a `tags` row on `CardInfo` and `Filter::Tag`; until then a Dragon printed under a name the list does not know is invisible. |
| Might layers (476) | M10 as listed; Vendetta: Dragon Form (the base-Might layer above), Dame the Despoiler (Might set to the chosen unit's plus one, computed through `might_this_turn` — runs), Spiderling, Repair Specialist, Illaoi, Prophet of the Great Kraken (`MightIf` ladders until `Grant::MightBy`) | a base-Might override read first by `current_might` and `Grant::MightBy`, unchanged. |
| legend replacements, triggers off the board, Hidden rules, control in place, owner's zones | M10, M11 and M12 as listed; Mask Mother joins the off-the-board row and Kharox the owner's-zones row | unchanged. |

Of the 16 stubs, five wait on a game-rule static alone (Ol' Poro,
Sandstone Chimera, Otterpus, Dune Surfer, Heisho, Shell of the World) and
one on a static prevention (Esteemed Hierophant), four on a missing
trigger subject (Affectionate Poro, Ravenbloom Prefect, Mask Mother,
Threshold of the Gray), three on the ability-discount and surcharge
statics (Risen Altar, Piltovan Forge, Mystic Vortex), one on enters-ready
(Shadow Assassin), one on [Add] payment (Renekton - Butcher of the Sands)
and one on another card's additional cost (Dragon Roost). The row Vendetta
leans on hardest is the pay stage: sixteen cards pay something that is
not a resource, eleven of them as an Empower or disempower cost, and
`SelfCost::Disempower` alone — one arm beside `KillSelf` and `BanishTarget`
paid through `Ctx::disempower` — takes seven scripts off the
disempower-at-resolution workaround, while the `face_in_play` read in
`Ctx::set_counter` is a one-line change that lets six legends be Empowered
at all. The new shapes the set opens are small and each is named in one
file: a base-Might layer (Dragon Form), a replacement on the stun, -Might
and bounce paths (Gangplank, Naval), a capped Empower counter (Kayle,
Justified), `CombatEnded` with its participants raised before the heal
(Affectionate Poro, Mournful Witness), a this-turn Flow on `CardState`
(Kennen, Storm of Shuriken), an additional cost offered by another card
(Dragon Roost), and the three Endless Riches hooks.

## Rulings and deviations

Choices the rules leave open or this design makes, recorded so they are not
re-litigated per card. Every entry was written by the milestone that met the
case (M1–M8) and names the code that enforces it; the groups follow the
engine's own modules. Where a reading deviates from a plain reading of Core
Rules v1.2 the deviation is said so, with the rule number and the reason.
M9's rulings live in its own section above and cite the 2026-07-16 edition;
they move here, renumbered into these groups, when M9 lands.

### Turn structure and the opening

- The Awaken step is the turn player readying their objects (315.1.a,
  402.3.a: "a player Readies all non-spell Game Objects they Control during
  the Ready Step"), so `Ctx::awaken` raises `Readied { by: turn player }`
  for each object it actually readies (402.1.c: an already-ready object is
  not readied) and "when you ready me" (Irelia - Fervent) fires at the start
  of her controller's turn, queued with the Temporary kill at the Beginning
  phase. No rule or FAQ excludes the Ready step; if one appears, the change
  is the one `raise` in `awaken`. Accelerate enters ready without `Readied`.
- The Beginning Phase runs in two steps since M4: `phases::start_turn`
  awakens, raises `BeginningPhase` and proceeds the chain, so Temporary kills
  and Dusk Rose Lab land *before* scoring and can lose a hold;
  `phases::continue_beginning` then scores holds, channels and draws as one
  step. Triggers raised by the scoring itself (a `Held`) still resolve after
  the draw.
- The opening hand and a mulligan redraw are dealt (`Ctx::deal`), not drawn:
  no `Drew` event, no draw count, so a "second card each turn" trigger such
  as Frigid Jewel starts counting at the Beginning-phase draw of turn 1.

### The chain, priority and focus

- Focus after a countered play passes as after any play: 343 passes focus
  when the last chain item resolves, and a countered spell leaves the chain
  the same way; 412.1.b's "not considered to have been played" governs
  play triggers (406.4.b) and Legion, not focus.
- Focus passes when a play creates a chain (the chain was empty at finalize),
  never for a reaction added to an existing chain, so a chain of A's spell and
  B's reaction leaves focus with B as 343 orders it.

### Targets

- A single legal target is still a choice and raises `Chosen`, even when the
  prompt is answered automatically.
- A target is validated positionally: `targets::valid(ctx, item, index)` maps
  the index to its `TargetSpec` by cumulative `max` and re-runs the filter at
  resolution; scripts read targets through the prelude's `card_target` /
  `item_target` / `card_targets`, which return nothing for a target that no
  longer matches (356.3.e), and the rest of the effect runs.
- A play's mandatory target keeps its printed minimum: a spec with no legal
  candidate opens a cancel-only prompt and the card cannot be put on the
  chain without one (352.7); a trigger with fewer candidates than its
  printed minimum is removed from the chain (390.3), never asked for less.
  `choose_targets` also refuses more picks than the spec's `max`,
  independent of the prompt.
- Each chain item records how many targets every spec took
  (`ChainItem::spec_counts`), so a spec after a variable-count one is
  validated against its own filter however many were picked;
  `targets::of_spec(ctx, item, n)` returns the n-th spec's picks.
- `targets::valid` re-checks `Untargetable` as well as the spec's filter, so
  Akali leaving combat makes an enemy spell that legally chose her in combat
  find no target at resolution (356.3.e.2, 356.3.e.5). Deflect stays out of
  it: it is an additional cost paid when the spell is played.
- `Filter::ItemTargets(&Filter)` matches a chain item when one of its chosen
  cards satisfies the inner filter from the reacting item's point of view;
  Not So Fast's "chooses a friendly unit or gear" is `ItemTargets(&And[Or[Unit,
  Gear], Friendly])`, so an enemy spell that chose only a friendly legend or
  rune is never offered.

### Triggers

- Trigger batches of one controller are ordered by that controller; across
  controllers the turn player's batch goes first (376.3.b, 376.3.b.1).
- A batch whose order cannot change the outcome is placed in queue order
  without an `OrderTriggers` prompt: `triggers::interchangeable` holds when
  every pending item of the batch is a `Trigger` of the same script pointer
  (so the same printed card) and the same ability index, the ability has no
  `TargetSpec` and the item carries no targets, and the items agree on
  controller, `subject`, `noted` and `origin` — two Astral Herons at one
  battlefield firing on your first card, two Ravenbloom Students on one
  spell. 383.3.d (v2026-07-16; 376.3.b in v1.2) and 808.2.a give the
  controller the right to select the order, and a selection among options
  that are the same ability of the same card from the same event with no
  per-instance data has one result whichever way it is made, so answering it
  for the controller takes no rule-granted choice away. It is the policy the
  engine already applies to a prompt with one option (`prompts::auto_answer`)
  and to a seat that cannot act (`priority::can_act`), and 383.1.b shows the
  rules themselves treating indistinguishable simultaneous instances as one
  choice. The criterion is deliberately narrow: two Stellacorn Herders (one
  `Moved` event per unit, so different subjects), two Fae Fawns (one `Moved`
  event per unit, so different subjects, and possibly different noted
  origins), two Jewels (targets) and a Heron beside a Pit Crew (different
  scripts) still ask. The one thing it does not read is the
  ability's body: an effect that names its own source (a Student's "+1 might
  to me") reaches the same final board either way but exposes a different
  copy first to a response, and the engine treats that as not worth a
  question. Answering it in the engine rather than a client means kai-cli,
  the web client, the bot and a human with "order my triggers myself" all
  skip the vacuous prompt, and no client needs to see targets or subjects to
  know it was vacuous.
- A delayed trigger scheduled for the current `EndOfTurn` (or `BeginningOf`)
  while that step's chain is already resolving is queued when the chain
  empties: `open_state` re-runs `queue_delayed` for the current `When` and
  only finishes the turn (or continues the beginning phase) when nothing was
  due.
- A queued trigger carries the subject of the event that fired it
  (`ChainItem::subject`, read by `prelude::trigger_subject`), so Vex stuns
  the unit whose play triggered it rather than re-deriving a candidate from
  the board, and a trigger whose subject has left the board does nothing.
- A queued trigger also carries a `Noted` for the events that have one to
  carry: a death's snapshot (734.1.d.3) and, since M4, a move's origin —
  `triggers::noted_of` puts the from-zone in `Noted.zone` with the moved
  card's current Might and controller, because `ctx.events` lives for one
  decide and a Move trigger resolves in a later one. Lillia - Fae Fawn reads
  it to plant her Sprite at the location she left; two moves of one card in
  one decide queue two triggers, each with its own origin.
- `Trigger::EnemyUnitDies` asks whether the dead card was a *unit*
  (`Event::Died.unit`, snapshotted in `kill::kill` before the body is trashed
  or despawned — `Noted` cannot answer for a despawned token). Without it
  Zhonya's, which replaces a unit's death by killing itself, would still pay
  an opponent's Pyke for the death it erased.
- A triggered ability's own `cost` is priced exactly like an activation's:
  `cost::base_of_item` reads `ItemKind::Trigger` through the same
  `extra`/`cost` path, so Sunken Temple's "you may pay 1 energy to draw 1"
  opens the 392.2 optional-cost confirm at `play::STAGE_PAY` and a declined
  trigger is removed from the chain. Every other scripted trigger declares no
  cost and is unaffected.
- A triggered ability may carry a self cost (`prelude::exhausting_self`):
  `activate::pay_self` honours an explicit `SelfCost::Exhaust`/`KillSelf` on a
  trigger while `SelfCost::Auto` stays free for triggers (a unit's play
  trigger never exhausts it). A trigger whose source is already exhausted is
  removed before the 392.2 confirm is asked ("its source is exhausted"), and
  so is one whose energy or power cannot be planned from the controller's
  ready runes ("its cost can't be paid"), since a cost that cannot be paid is
  not offered; the confirm is only asked when a yes could be honoured.

### Costs and payment

- `Filter::EnergyAtMost` / `PowerAtMost` read a missing cost as 0, so a token
  satisfies them even though 179.2.a says tokens have no costs. Pickpocket can
  therefore eat a Gold. Recorded rather than fixed: nothing in the pool wants
  the other reading, and 130.4's base cost is 0 for a costless object.
- `PromptWhy::PayWith` is a cancellable prompt like every other stage of a
  play: it lists the ready Golds, "recycle a rune", and the closers through
  `Prompt::numbered`, and its status names the card and the power it is
  paying (`pay 1 power for {card N} with`).

### Movement, contest and control

- Group standard moves are a prompt after the first drag, not a multi-select
  drag.
- Contested is sticky, as 184.3.a.1 and 184.3.b require: `ctx::arrived`
  applies it only to a battlefield that is not already contested, so the
  Attacker stays the seat whose units applied it (442.1.a.1) however many
  seats arrive afterwards. Control follows: `cleanup::settle` never strips a
  holder with no units left while the battlefield is Contested (184.3.c,
  184.4.c), so control changes only through `establish`.
- A swap (`march::swap_units`, `MoveCause::Swap`) is one batch: the 427.2 cap
  is checked for both destinations against the pre-swap board, both `Move`
  effects are emitted before either arrival is processed, and only then are
  both contests marked and both `Moved` events raised, so the next `collect`
  sees two triggers with two origins (two Fae Fawns plant two Sprites) and
  the cleanup stages both showdowns for the turn player's pick. Either unit
  capped refuses the whole swap rather than recalling one side.

### Combat

- Lethal damage in combat counts damage already marked: `max(1, Might −
  damage)` — the amount 443.1.d.3 requires in full before the next unit and
  443.1.d.4 caps at, read against the damage a unit already carries.
- The Combat Cleanup clears Contested only through `establish` — the `[only]`
  and `[]` arms of 444.2.a. A battlefield that still holds two seats' units
  after a combat therefore stays Contested and `cleanup::run` re-stages it,
  instead of being left unresolvable. `after_combat` still establishes before
  running the rest of the cleanup rather than after it as 444.1 → 444.2
  reads: running the staging steps first would stage a fresh showdown at the
  battlefield the lone surviving attacker is about to conquer.
- Tank and Backline are exclusionary: a unit carrying both is offered only
  where it can satisfy one of them — first, while no non-Tank opposing unit
  has taken an assignment (741.1.b), or last, when it is the only unassigned
  unit (443.1.d.7's Caitlyn example forbids the middle).
- A damage-step `Assign` prompt never outlives its combat: `showdown::abandon`
  and `combat::resolve` close it, so a seat leaving mid-combat cannot strand
  the other behind a prompt with no answers.

### Kills, replacement and cleanup

- Cleanup's 322.12 precedes 322.13/14 on every path: `showdown::owed`
  counts events raised but not yet collected as well as the queue and
  pending deaths, so a showdown never opens (and auto-passes) before the
  action's own Moved/Conquered triggers are on the chain — a companion Fae
  Fawn's trigger is finalized before the combat her group move stages.
- A Cleanup repeats until it changes nothing (321): `cleanup::run` re-runs
  `win_check` and `lethal_kills` while `dying(ctx)` is not empty, capped by
  `CLEANUP_LIMIT`, and only then refreshes combat, settles control and stages.
  This is what makes Zhonya's honest: it recalls without healing (436.1), so
  a unit saved from a Singularity is still lethally damaged and the next
  cleanup of the same window trashes it — the Hourglass really saves a unit
  only where the damage is wiped, i.e. a combat's step 2c (444.1.a.1). The
  Deathknell of that second, real death fires.
- Simultaneous deaths are planned as one batch (322.3 → 322.4): `kill::batch`
  takes every dying unit's `Noted` snapshot and matches its Deathknells while
  all of them are still on the board, and only then runs the replacement
  windows and the trashes, so a conditional Deathknell never reads a board an
  earlier member of its own batch has already left. `cleanup::dying` is sorted
  by card id, so which unit a single one-shot replacement answers is the
  lowest id of the batch rather than an artifact of `Snapshot.cards` move
  history. Asking the dying unit's controller which replacement applies (368)
  is `kill::park`: when more than one replacement applies outside combat the
  kill is parked as a `Resolving` trigger item on the chain (source = the
  dying unit, `targets` = the applicable sources, the cause encoded in
  `picks`) whose `Resume` prompt offers the sources to the owner; `kill::decide`
  runs the pick, or the first applicable when the choice is moot, and lets the
  unit die when none is left. Mid-combat (322.4 inside a combat's cleanup)
  the owner's own replacement is taken without asking so the combat cannot
  strand on a prompt. Choosing *which* of several simultaneously dying units
  a single one-shot replacement saves is still the lowest id of the batch.
- A replacement source that dies in the same batch still applies (370.4,
  whose own example is Soraka - Wanderer beside a weaker ally): `kill::batch`
  judges `applicable()` for every planned death against the pre-batch board,
  then runs the replaced deaths before the unmodified ones (373.1.a), so the
  ward is healed and recalled before Soraka is trashed. A snapshot source is
  dropped again only when it has left the board on its own — a one-shot
  Hourglass that killed itself answering the first death does not answer the
  second — or when it is a batch member already in the trash. The parked
  choice of 368 (two replacements over one death) still re-reads the live
  board at `kill::decide`, so a Soraka who is herself one of two applicable
  sources and dies in the batch is not offered by the time the owner picks.
- A card entering a player's zone enters its owner's (056.2): `Ctx::trash`
  and `Ctx::bounce` read `Ctx::owner`, not the controller, so a unit stolen
  by Hostile Takeover dies into and returns to its owner's trash and hand;
  `Ctx::recall` keeps the controller, since the base a unit recalls to is its
  controller's.
- Steps 322.12–322.14 are ordered by `showdown::owed`: `open_next` refuses
  while `blob.queue` or `ctx.deaths` still holds work, so a Deathknell owed by
  a cleanup kill reaches the chain before a showdown or a combat opens, and
  `chain::open_state` opens it once the chain is empty again.

### Hidden cards

- A card the opponent has seen in your hand may still be hidden, and hiding
  it makes it secret again (the physical game lets you reorder your hand
  before laying one down): the `hidden` flag on the Move strips the face
  in the fold. Nothing in the log says which of the shown cards went down.
- A hide is charged and accepted blind; a non-Hidden card hidden this way
  is stuck, which `hide::play_legal` enforces at play time, when the face is
  public, rather than at hide time, when it is not.
- The facedown card's playability is readable from the blob.
- A facedown card has no abilities (408.3): `activate::playable` refuses its
  activations with `Reason::Facedown` before any other check, so an Equip
  gear hidden at a battlefield is neither offered nor activatable by the
  seat that knows its face.

### Reveals, peeks and public faces

- A reveal of a card in a private zone shows it in place until the card moves
  (`LogState.shown`); a peek is remembered for the seat while the card is on
  the table, whatever zone it moves through, and forgotten the moment the card
  becomes public. Both are recorded in the fold, not the wire.
- `Effect::Reveal` on a card in a face-down zone is accepted and recorded
  (`owed_reveals`) but not paid there: the host cannot append a `Reveal`
  in a face-down zone. The debt survives moves and `reveal_surfaced` pays
  it the moment the card reaches an owner-visible or public zone — so a
  future "reveal the top card of your deck, then draw it" publishes the
  face when the card lands in hand, which is what such a text means. No
  pool card reveals from a deck; a reveal that must show a deck card while
  it stays in the deck wants `await_face` and a host path.
- A cancelled play leaves the face public.

### Equipment

- Attachment (M7) lives on the gear's row: `CardState.attached_to` names the
  Top-Most unit and the `attached` annotation mirrors it (little-endian unit
  id bytes) for kai's chip. What the unit has while the gear is on it is data
  on the gear's script — `Static::WhileAttached(&[Grant])` — and
  `attach::attach` copies every `Grant::Keyword` into the unit's `granted`
  with `Expiry::WhileAttached(gear)`, keyed by the *gear* so `detach` is one
  `expire`. The Might Bonus (136) is `Grant::Might(n)` on the same slice:
  `attach` sums the gear's `Might` grants and applies them as one
  `MightMod` with the same expiry (skipped for a wearer with no printed
  Might, 136.3.b), so a re-equip moves the bonus with the gear and every
  detach path reverses it. `Grant::Static` is read at query time through
  `attach::granted_statics` and is wired only where a unit could carry one
  (`targets::untargetable`); nothing in the pool grants a static.
- An attached gear's own text is inactive (134.4, 718.2): `activate` refuses
  its activations with `Reason::Attached` and `triggers::sources` skips it,
  so a gear cannot re-equip by paying Equip again and an attached Frigid
  Jewel would not fire. Detaching happens through effects, the unit leaving
  the board (719.5) or the gear leaving it.
- `attach::sync` runs inside every cleanup, after the kill loop: it expires
  grants whose gear no longer sits on that unit, detaches gear whose unit
  left the board (at the unit's last location, 422.4.b), moves attached gear
  under a unit that moved (719.3.a; standard moves and swaps also follow at
  once so the group-move prompt never shows a lagging gear), and recalls
  every loose gear standing at a battlefield (435.1, 422.4.a). `sync` is the
  only reader of the rule, so an effect that moves a unit needs no attachment
  knowledge.
- Equip is `prelude::equip(cost)`: an `Activated(Sorcery)` ability with
  `SelfCost::Free`, one `FRIENDLY_UNIT` target and `attach` as its body, so
  the chosen unit raises `Chosen` (744.1.b.1) and Irelia - Fervent grows when
  equipped. The gear's keyword list still prints `Equip(cost)` for the
  presenter; the engine reads only the ability.

### Prompts and their answers

- `Resume` prompts carry their answers in `ctx.picks()`; their option set is
  the closers plus whatever the ability's `candidates` hook returns
  (`prelude::with_candidates`, M3). Pickpocket did not need it: 352.12/352.13
  make its "you may kill a gear" a targeted choice, so it is a min-0/max-1
  `TargetSpec` asked in the choices step, not a `Resume`; Hwei's typed branch
  is the discard ruling below.
  An ability without the hook offers only the closers, so a script must
  not `ask_resume` with a minimum before it names candidates.
- A `Resume` prompt asks the ability's own question: `prelude::asking` puts a
  `&'static str` on the ability and the presenter reads it (Hwei's runes,
  Edge of Night's wearer, Abandon's Predict); a `Target` prompt reads the
  spec's label the same way, so Charm and Ride The Wind say "choose where it
  goes" over their zone options.
- A discard during resolution is `prelude::ask_discard` → `PromptWhy::Discard
  { item, stage }`: the options are the hand, and a hand-to-trash drag answers
  it as a gesture (the host reveals before such a move, so the face is known
  when the script resumes). When the pick path leaves the face blank — the
  plugin's own `Move` to the trash precedes the host's `Reveal` — the card
  carries `FLAG_DISCARDED` and the item stays `Resolving` at its branch stage
  until `Action::Reveal` arrives, when `discard::revealed` resumes it with the
  card in `ctx.picks()`. `prelude::discarded_kind` is the typed branch.

### The legal list, greying and the presenter

- The `Play` rim promises that the table will take the Move, not that the
  spell resolves: `legal::play_to` gates on cost and timing only, so a spell
  whose mandatory target has no legal candidate is still highlighted and then
  opens the cancel-only prompt named under Targets. Making the rim truthful would
  mean the highlight and `decide` disagreed, which is the one thing M5 is
  built not to do.
- An activated ability the seat cannot currently pay for is offered greyed out
  (`Offer.enabled`, `Affordance.enabled`) rather than hidden, so a price that
  moves with the board — Lillia's Sprite — stays visible. Timing, seat,
  in-play and once-per-turn still remove the offer entirely.
- The narration window is 12 lines (`NARRATION_LINES`). M4's Beginning Phase
  spends two lines per priority window, and at 4 the kills and Deathknells of
  an entry were evicted before the player could read them; the three M4 net
  scenarios assert the whole slice so the window cannot silently narrow.
- A legal row's `zones` are the destinations a face-up Move of that card is
  accepted for and `hidden` the battlefields a `MoveHidden` is accepted at
  (M8, `Legal.hidden`). Until M8 the row carried the union, so kai's
  `Rims::hides_at` took the first zone of a `[Play, Hide]` row as the hide
  drop target and the soak's random brain hid at whichever shared
  battlefield the row named — a battlefield that already held a facedown
  card, or a play destination, was then refused, the one refusal the M5
  promise forbids. `legal::note` files a `Hide` destination on `hidden` and
  everything else on `zones`; the parity test compares both lists against
  `decide` with the flag both ways on every fixture.
- `decide` accepts a hide blind (see Hidden cards) of any card in hand, but
  the legal list offers a `Hide` only for a card whose face the viewer holds
  and that prints Hidden (`legal::hideable`) — the one place the list is
  deliberately a subset of what `decide` takes, since offering a hide of a
  plain unit would light up a move that strands the card. The parity test
  filters the engine's side the same way.
- The `hidden` flag on a Move is honoured only for a hide from hand or from
  the champion zone: `classify` refuses it on a board unit
  (`Reason::HideFromHand`) and on the champion card going to base or the
  chain (`Reason::Unrevealed`, as from hand). Before M8 both fell through to
  a face-up march or play while the fold, which strips the face of every
  `hidden` Move, left a public unit faceless on the board. On a mulligan
  gesture the flag is inert (the card is going into the deck either way).
- A hand card is greyed (kai `highlight::greyed`) only while its seat is
  *acting* — the status line ends "rules enforced" and an enabled `pass` or
  `end turn` affordance is on offer — and no legal row of an actionable
  kind (`Play`, `React`, `Hide`, `Answer`) names it. The set is derived from
  the legal list alone: kai does no cost arithmetic, so the greying is the
  engine's affordability verdict by construction. Nothing greys while
  waiting for the other seat, during a prompt, in the lobby or after a win,
  because the whole hand would otherwise grey on every opponent turn and
  read as "unaffordable"; the free table never greys (its status line ends
  "free table"). A greyed card stays hoverable and draggable and the
  engine's refusal reason remains the explanation; the strip adds a hover
  hint. The grey is the card's own material dimmed to `GREY_LEVEL` and
  restored exactly, chosen over an overlay because fanned hand cards overlap.

### Randomness and fairness

- The mulligan recycle (and Burn Out, when it is built) uses an in-game
  commit-reveal roll; the
  resulting order is public once revealed (see open questions). The roll
  opens only for two or more recycled cards (403.5 has nothing to randomise
  for one), seeds xorshift64 with the pooled secret xor the roll id, and the
  two mulligan paths differ only before the roll: a gesture sinks each card
  as it is dragged, the buttons sink them after the redraw, and the roll
  reorders the deck bottom either way.
- The in-game and lobby rolls are fair against honest peers only: the
  8-byte `dice::commitment` is a non-cryptographic invertible hash (FNV
  multiply plus a splitmix finalizer), binding — a reveal is refused until
  every seat committed and must match — but hiding only to about 2^32 work,
  so a seat that commits last could recover the other secret and steer the
  order. Accepted while the SDK stays dependency-free; a 256-bit hash
  replaces `commitment` when one is allowed in.

### Keywords without rules text

- XP is counted and unused; Empowered is a toggle nothing sets; Backline is
  the inverse of Tank per the 443.1.d examples.
- Legion counts every Main Deck card (738.1.c.1, 738.1.c.2): `played_main`
  is set when a unit or gear finalizes and when a spell resolves (a
  countered spell never sets it, 412.1.b); a permanent's play triggers are
  collected before the flag is set so their Legion condition reads the
  earlier plays only, the "another … before this one" of 738.1.c.1.

### Card-specific readings

- Irelia - Blade Dancer's two paid "may" triggers are suppressed when a yes
  could change nothing: the ChosenFriendly trigger fires only for a chosen
  friendly *unit* (gear chosen by Pickpocket or Adaptatron never asks for
  her rune) that is exhausted, and the Conquer trigger only while she is
  herself exhausted. The card prints neither condition; the reading follows
  392.2's spirit that a cost which cannot buy anything is not offered, and
  no outcome in this pool differs.
- Tideturner's "you may choose a friendly unit" is narrowed to a friendly
  unit at another location from hand as well as from facedown: the lift of
  737.1.d.2 is derived statically from the filter (`hide::lifted_by`), and
  a same-location choice is a no-op swap whose only effect would be a
  `Chosen` event (Irelia - Fervent's +1, Blade Dancer's rune). Recorded
  rather than split per origin; revisit if a card makes the fishing line
  matter.
- Zhonya's uses the printed card text (kill the gear, recall exhausted), so
  Deathknell does not fire and Pyke does not count the death: the
  replacement (366.1's own example) turns the kill into a recall (436), and
  734.1.d triggers Deathknell only on a permanent being killed.
- Abandoned Hall's "when a player plays a spell, they may…" belongs to the
  spell's controller: `Trigger::AnyonePlaysSpell` hands the item to the
  event's controller, every other battlefield trigger to the holder (or the
  turn player when uncontrolled) as 184.6.a and 184.6.b assign them. The
  Hall is the "unless otherwise specified" of 184.6: its text names who
  chooses, so the item is theirs rather than the holder's — a deviation from
  a plain reading that keeps the holder as controller, taken so the "may"
  and the draw land on the player the card addresses.
- Charm and Ride The Wind choose the move destination while the spell is
  played, as 352.3 orders (Charm with M6, Ride The Wind with M7): the
  destination is a second `TargetKind::Zone` target, `CHARM_DESTINATION` =
  `Filter::DifferentLocationFrom(0)`, whose candidates are the chosen unit's
  own base plus every battlefield it does not stand on and that 427.2 does
  not cap. No blob change was needed: the one base zone reads as the chosen
  unit's controller's base through `targets::zone_location`, so a seat
  reacting on the chain sees both choices on the item. At resolution
  `prelude::charm_destination` re-validates the zone against
  `march::effect_destinations`; a destination that closed in the meantime
  moves nothing and Ride The Wind still readies the unit.
- Dusk Rose Lab's "you may kill a unit you control here to draw 1" targets
  nothing (352.10.c.1 uses this very card as its example): the trigger goes on
  the chain with no target and no `Chosen` event, and the ability asks a
  `Resume` prompt over its own `candidates` at resolution, so the choice is
  made from the board as it then stands. Adaptatron and Pickpocket keep their
  `TargetSpec`: their "you may kill a gear" is a genuine target under 352.12.
- Riposte's "Counter that spell and give that unit +Might equal to that
  spell's Energy cost" is read as contingent: the unit gains nothing when
  the chosen spell has already left the chain (countered or resolved by a
  response), because "that spell's Energy cost" is read off the chain item
  at resolution and no item is left to read. The looser reading — 359.3.e.8
  executes the one instruction on the targets still available, so the unit
  gets the printed cost even after a fizzle — needs the spell's card
  captured on Riposte's item when its targets lock (a `TargetRef::Item` to
  card lookup at finalization, not at resolution); revisit if a game turns
  on it.
- Buhru Captain's "draw 1 or buff me" is asked in two `Resume` stages —
  the main deck (draw) or skip, then the captain (buff) or skip — instead
  of one prompt over both, because `Ctx::picked` keeps a pick as a bare
  `u32` (`Answer::value` flattens Zone/Card/Seat/Item), so a Zone and a
  Card ref in one candidate list collide when the captain's card id equals
  the main-deck zone id (ids start at 0, `ZONE_MAIN_DECK` is 1). The engine
  seam is a `Vec<TargetRef>` on `picked`; until then a script offers one
  ref kind per prompt (Qiyana - Victorious offers two zones).
- Ezreal - Prodigy's "optional additional costs you pay" are three
  components of one item: the script's `additional` when `paid_additional`,
  the `Keyword::Repeat` cost when `repeated` (820.1, 356.4.c's own example
  is a repeated Frigid Touch) and the Accelerate cost when `accelerated`
  (805.2); each paid component is one energy less when it has energy, else
  one rainbow less, summed. `cost::discounts_of` still consults
  `SpellDiscount` for spells only, so the Accelerate half is live in
  `discount_for` and dormant at the pay stage until that gap closes.
- `Filter::Named` compares `base_name` (the print suffix stripped), the
  same reading `cards::resolve` and `rumble_mechanized_menace::is_mech`
  use, so a "Mega-Mech (Alternate Art)" print is a Mech to Bubble Bot's
  target list as it is to the Mech auras.
- Arise! asks where each Sand Soldier is played, one `Resume` location
  prompt per soldier (185.2.a: a token play follows every step of playing
  a card, Vanguard Armory's reminder spells the default out), and only
  when more than one play location is open; with a single location the
  soldiers land there unasked.
- Switcheroo is `prelude::swap_might_this_turn`: two `EndOfTurn` deltas of
  the difference between the units' *current* Might, designation bonuses
  included, so a later Stupefy reads the swapped value and both reverse at
  the Expiration step. A unit whose Assault designation clears mid-turn keeps
  the delta, which is the layered reading of 454.

### Table options

- The victory score, the battlefield count and the enforced flag are the
  three table options (`victory_score`, `battlefields`, `rules_enforced`), a
  CBOR map of string keys to integers inside `TableConfig.options` so the SDK
  stays dependency-free and other plugins can carry their own keys; an
  unknown key is tolerated by kai's decoder and kept by the SDK's
  `Snapshot.options` and the plugin, and nothing in kai re-encodes a decoded
  map into a new genesis. They are defined twice —
  `agni_riftbound::TableOptions` on the host side and
  `rules::OPTION_*`/`DEFAULT_*` in the plugin — because the plugin crate is
  SDK-only; a test in `plugins/riftbound` pins the names and defaults equal
  through real `DecideRequest` bytes, the enforced flag included.
- `rules_enforced` is written only when the host switched it on
  (`TableOptions::genesis(chosen, enforced)`: `None` when neither the
  options nor the flag were chosen, the flag alone otherwise, so a flag-only
  map still plays by seat count); any value ≥ 1 means enforced, 0, a
  negative value or a missing key means a free table. The plugin reads it as
  `Options::enforced` and seeds the lobby's mode from it
  (`Options::starting_mode`, `GameBlob::lobby_in`) whenever it opens a fresh
  blob — the first game event on a table and again after a Reset — so an
  enforced table's dice roll opens in enforced mode without anyone pressing
  the switch; the roll winner can still switch the mode either way at the
  start, as before. Both seats read the genesis, never their own toggle
  (`kai::net::rules_enforced`), and a joiner runs the host's hash-pinned
  plugin, so an older kai cannot disagree in the fold — it only lacks the
  pinned-deck lobby.
- Under rules enforced kai pins the decks to the scripted pool: the lobby
  offers Lillia and Irelia (`deck::pinned`), each side resolved from the
  saved-deck history by label (a held row whose label starts with the
  legend's name), else built from the house pool files under
  `games/riftbound/rules/pool/` by `ai::soak::pool_deck` — a legal 40-card main deck per side with the pool's
  Order card left out of Irelia's Calm/Chaos identity, 12 runes split across
  the legend's domains, the section's three battlefields, the legend and its
  champion, pinned by tests on both sides — and on the web, which bundles no
  pool, a "missing" note asking for an import. The host defaults to Lillia
  and stays in the lobby when the table opens; a joiner is taken to the
  table on landing and takes the side the host has not dealt (its own
  legend if it is reconnecting to a seat already dealt, the other side of
  the host's legend, Irelia when nothing is dealt yet), and its deck deals
  itself once unless its legend is already on the table.
- An option the plugin cannot use falls back rather than refusing: a
  victory score below 1 plays to 8, a battlefield count below 1 (or one
  that overflows) plays `max(2, seats)`, which keeps every pre-M8 table,
  log and fixture at one battlefield per seat while honouring "default 2".
  The final-point rule moves with the configured score: a seat at score − 1
  scores a Hold, and a Conquer only when it has scored every battlefield
  this turn, otherwise it draws (448.1.b); Burn Out is unbuilt.
- A table with more battlefields than seats splits them so the seats after
  the first player place the extra ones (`contribution_of`,
  `Battlefields::Many`): the sanctioned War rule where the first player
  sits out, generalised. A seat placing several uses its chosen battlefield
  first and its deck order after; the chooser stays a single pick and its
  window text says how many it places.
- The desktop table lays out `SessionInfo::battlefields_in_play(players)`
  contested slots (`zones::contested_in_play` takes the count, not the seat
  count), so a duel on three battlefields — which the lobby offers, the deal
  stages and the plugin enforces — has an anchor, felt, drop target and
  card placement for the third; the layout tests pin the 2-seat,
  3-battlefield case.
- kai edits the options only while Solo, before a table opens, and only
  writes them into the genesis when the host chose them (a mode preset or
  house rules); an untouched lobby is "by seat count" and writes none, so a
  3- or 4-seat table hosted without a preset still stages three battlefields
  as it did at M7. Once a table is open (host, re-host or joiner)
  `SessionInfo.options` holds the genesis bytes and
  `SessionInfo::options_in_play(players)` resolves them with the plugin's
  own fallback, so both seats show the genesis truth and the table lays out
  as many battlefields as are in play. The lobby's mode presets fill the
  options and the new-game hint names the sanctioned mode for the seat
  count or says "house rules".

### The self-play soak

- `kai-cli soak` holds one `HostSession` and a `ClientSession` replica per
  seat in one process, exactly as `net/tests/riftbound_turns.rs` drives an
  enforced game, because the `--ai` plumbing sends every intent through the
  one global agni-net bridge and cannot carry two seats. The plugin always
  runs under wasmi with the harden crate's gas budget, so gas exhaustion is
  an `EngineFault`; the engine is native by default (`--engine wasm` loads
  the bundled or store engine). After every fold the host's blob is compared
  with each replica's, and a replica refusing an entry the host folded, or
  diverging, is an `EngineFault`.
- A game is *stuck* when no seat has a legal option after automatic reveals,
  when eight consecutive moves the legal list offered are refused, or when
  the AI seat folds nothing for eight decisions running; a refusal of a
  command the model invented is not counted, so a model's confusion cannot
  be reported as an engine failure. Reaching the turn cap is an ending, not
  a failure; only `Stuck` and `EngineFault` fail the run. The panic buttons
  (`free table`, `confirm free table`) are excluded from the random brain
  and refused when a model presses them, since the soak measures the
  enforced engine. A game that cannot be set up (the plugin refuses to
  load, the roll never settles) is carried into the summary as a
  `could not be set up` FAILURE line and fails the run the same way, so the
  last line and the exit code never disagree.
- Brains are bound to decks, not seats: deck A sits at seat 0 on odd games
  and the first player alternates every two, so four games cover each deck
  in each seat going first once from each. A game replays move for move
  from `--seed` and `--start`: the deal is pre-shuffled from the game seed
  (the host's own shuffle is seeded from the face order), the roll secrets
  and the brain are seeded from it, and the random brain draws uniformly
  over the enabled affordances plus every legal-list destination — every
  `Legal.zones` entry and every `Legal.hidden` entry as the presenter names
  them, with no filter of its own, so a wrong destination is a counted
  refusal rather than a silently untried one. Each JSON line carries the
  base seed, the derived game seed, the turn cap, the engine and the plugin
  origin, and the summary's replay hint spells the flags out
  (`--seed S --turn-cap C --engine E --start N --games 1`). The bin's smoke
  test requires at least one of its two games to reach a winner, not only
  to avoid a fault.

### Engine hygiene

- `CARDS` is sorted by byte order, so "Smoke Screen" precedes "Smoke and
  Mirrors"; the registry test pins it.
- Board scans read the `CardInfo` they iterate (`Ctx::face_on_board`,
  `face_in_play`, `face_location`, `faces_on_board`) rather than re-finding
  each card by id through the linear `Snapshot::card`; `collect_ordered`
  computes the trigger sources once per collection. Semantics are
  unchanged (ids are unique, so `card(card.id)` was always the card
  itself); the O(n²) pass it removed cost 23.9% of the gas budget on the
  worst fixture and 6.2% after, see Gas under open questions.

## Open questions

- **Edge of Night's Effect Text.** Reading the pool lines through 135/136,
  an Equipment prints Rules Text (Equip, and for Edge the facedown clause),
  Effect Text granted to the wearer (135.2.c) and a Might Bonus (136). Boots
  of Swiftness prints "[Ganking] … +2 Might", so its `EFFECT_TEXT` is
  `[Grant::Keyword(Ganking), Grant::Might(2)]`. Edge of Night prints only
  "+2 Might" after Equip, so its `EFFECT_TEXT` is `[Grant::Might(2)]` and
  any further granted text is unconfirmed in the pool file and Riftcodex:
  add the `Grant` to `edge_of_night::EFFECT_TEXT` when a scan of the card is
  held; nothing else in the script changes.
- **A swap that stages two showdowns is a plugin-level fixture only.** 184.3
  contests a battlefield when a unit arrives at one its controller does not
  control, and a swap moves two friendly units into each other's current
  locations — which their controller controls (184.4.a) unless a contest is
  already open there. So in a two-player game a swap can never stage two
  fresh showdowns and the `PickStaged` prompt after a swap is reachable only
  from `march.rs`'s hand-built fixture (both battlefields held by the other
  seat with the units standing on them). The net scenario in
  `net/tests/riftbound_turns.rs` covers the reachable shape instead: Smoke
  and Mirrors played inside an open showdown swaps the contesting unit with a
  Temporary one at base as one batch, the showdown stays open with the new
  arrival, both replicas fold the same blob and the arrival conquers. A
  three-seat table (cap 427.2 aside) would not change the reachability.
- **Ride The Wind with no open destination.** Since the destination is a
  mandatory play-time target, a friendly unit whose every destination is
  427.2-capped (three or more players) is no target at all rather than a
  unit the spell readies in place. The two-player pool cannot reach it.
- **A secret shuffle for Burn Out.** The roll-seeded permutation is public.
  The honest fix is a host-executed re-deal: an `Effect::Reshuffle { seat }`
  the host answers by despawning the trashed cards and dealing fresh hidden
  ids from its dealer map in a secret order, exactly like the opening deal.
  Engine and host work; deferred until a game actually burns out.
- **Rules text for XP/Level, Empowered and Backline** is absent from v1.2;
  the 2026-07-16 edition supplies it (729–733, 824, 827–828, 826) and M9
  rules XP, Level and Empowered from that text. Backline stays as M3 read
  it until a card in the pool prints it.
- **A queued ability of a despawned token.** `targets::ability_of` resolves a
  trigger's script through the source card, and a token is gone from the table
  the moment it is despawned, so a token's own queued ability would evaporate
  across a decide boundary. Latent: neither Sprite nor Gold has an ability.
  The fix is to carry the source's face name on the `ChainItem` — a blob
  format change that rides with M5's targeting work.
- **Gas.** The projection, trigger collection over every card and cleanup to
  quiescence are a few dozen passes over ~150 cards per entry plus a CBOR
  re-encode of the blob. The budget is baked into the hashed module, so
  raising it is a plugin version bump. An iteration cap in the cleanup and
  trigger loops should refuse with a reason rather than trap on gas.
  Measured under wasmi by `net/tests/gas_bench.rs` (ignored by default:
  `cargo test -p agni-net --test gas_bench --release -- --ignored
  --nocapture`), which seats a metered `WasmModule` as the host's plugin and
  reads `gas_left` after every `decide` and `view` on a 150-card table. On
  the worst fixture — Unchecked Power resolving with twelve units at the
  battlefields and four Unsung Hero Deathknells — the resolving pass costs
  6.2% of the 100M budget and the busiest `view` 6.6%; a plain game to the
  victory score averages 3.1% per `decide` (floor ≈ 2.7%, which is the CBOR
  parse of the ~13 KB request) and 4.6% per `view`, with the blob between
  202 and 517 bytes per entry. Before M8 the resolving pass cost 23.9%: the
  board scans in `triggers::sources`, `kill::applicable`, `cleanup::dying`,
  `Ctx::units_at` and the prelude's `friendly_units`/`enemy_units` walked
  every card and then re-found each one by id through the linear
  `Snapshot::card`, an O(n²) pass repeated per event; they now read the
  `CardInfo` they are iterating (`Ctx::face_on_board`, `face_in_play`,
  `face_location`, `faces_on_board`) and `collect_ordered` computes the
  trigger sources once per collection rather than once per event. To
  profile a call natively, set `AGNI_GAS_DUMP_DIR` while running the
  benchmark to dump every request as CBOR, replay one through
  `agni_riftbound_turns::decide_bytes` under callgrind, and read the
  inclusive tree: instruction counts track wasmi gas closely enough to rank
  hot spots.
- **`Kind::Auto` continuations** are not needed by the pool (gesture-answered
  discards and `await_face` on the host's Reveal cover Hwei and future
  "reveal the top card" cards); the arm is reserved, not built.
- **Phase D commitments** would make hides verifiable and facedown
  playability private; the decider API needs no change when the log grows
  them.
- **Multi-select drags** in kai (one intent, one cleanup) would make the
  group-move prompt disappear from the common case.
- **Reset in enforced mode** removes today's escape hatch of editing the
  table by hand; `FreeTable` is the replacement and needs both seats.

## Recovered trigger support, blob v14

The five-cluster trigger recovery extends the v13 Damage baseline. This
section records the implemented recovery; the earlier milestone inventories
remain historical and do not imply that every listed gap is closed.

Queued effect text distinguishes the unit that holds an ability from the
permanent that lends it. `ItemKind::Granted { holder, lender, index }` records
attached effect text, and `ItemKind::Lent { holder, lender, index }` records
borrowed activations. The holder supplies the ability's controller, payments
and references to its own unit. The retained lender/index identifies the
script for resolution after the original grant becomes inactive. Dynamic
copied-ability identity remains a separate limitation; this representation
does not implement the deferred stable ability-instance ledger.

The recovered trigger paths retain death subjects and their saved properties,
match attached and projected grants, and preserve movement attribution.
Multiple pending triggers retain a controller ordering choice. This does not
complete the broader captured-event or simultaneous-event redesign.

Combat excess records use the captured assigner when computing lethal damage.
The serialized excess rows have canonical ordering and reject duplicate keys.
`Noted` additionally preserves whether the subject was buffed. Blob v14 writes
these fields and the Granted/Lent item forms while retaining migrations for
earlier saved items, including v12 Limited costs and v13 damage state.

Acceptance includes independently authored v12/v13/v14 fixtures and a real
Warmog's Armor/Gardens of Becoming sequence. Equip pays its printed cost,
Conquer queues the wearer's Granted buff, and the later Gardens activation
queues a Lent item that exhausts its holder and grants XP. Replay from both
saved item boundaries uses fresh native and hardened engine/plugin instances.
General payment rollback, replacement continuations, stable copied identities
and controller-selected damage ordering remain separate roadmap work.

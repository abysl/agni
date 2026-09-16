use agni_core::{CardFace, Zone};
use agni_plugin_sdk::decide::{self as sdk, Action, Effect as SdkEffect, BOTTOM, TOP};
use agni_plugin_sdk::table::{Face, Snapshot, Target, BOARD};
use agni_sim::abi::encode;
use agni_sim::engine::native_decide_request;
use agni_sim::log::{
    fold_entry, fold_entry_with, Decider, Effect, FoldError, LogAction, LogEntry, LogState,
    TableConfig, Verdict,
};
use agni_sim::wire::CounterTarget;
use serde_bytes::ByteBuf;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }
}

struct Scripted(Vec<Effect>);

impl Decider for Scripted {
    fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
        Verdict::accept().with_effects(self.0.clone())
    }
}

fn step(state: &mut LogState, seat: u8, action: LogAction) {
    let entry = LogEntry::new(state.next_seq, seat, action);
    fold_entry(state, &entry).unwrap_or_else(|error| panic!("the opening table folds: {error}"));
}

fn opening_table() -> LogState {
    let mut state = LogState::new();
    step(
        &mut state,
        0,
        LogAction::Genesis {
            name: "rae".into(),
            config: TableConfig {
                engine: None,
                plugin: Some("riftbound".into()),
                zones: agni_riftbound::zone_table(),
                options: None,
                counters: agni_riftbound::counter_table(),
                despawn_any: false,
            },
        },
    );
    step(&mut state, 1, LogAction::Join { name: "ada".into() });
    let base = Zone::Plugin(agni_riftbound::ZONE_BASE);
    let deck = Zone::Plugin(agni_riftbound::ZONE_MAIN_DECK);
    step(
        &mut state,
        0,
        LogAction::Deal {
            cards: vec![1, 2, 3],
            to: base,
        },
    );
    step(
        &mut state,
        1,
        LogAction::Deal {
            cards: vec![4, 5, 6],
            to: base,
        },
    );
    step(
        &mut state,
        0,
        LogAction::Deal {
            cards: vec![7, 8],
            to: deck,
        },
    );
    step(
        &mut state,
        1,
        LogAction::Deal {
            cards: vec![9],
            to: deck,
        },
    );
    for (seat, card) in [(0, 1), (0, 2), (1, 4), (1, 5)] {
        step(
            &mut state,
            seat,
            LogAction::Reveal {
                card,
                face: CardFace::named(format!("Unit {card}"))
                    .with_kind("Unit")
                    .with_might(Some(card as u8)),
            },
        );
    }
    state
}

fn projected(state: &LogState) -> Snapshot {
    let probe = LogEntry::new(
        state.next_seq,
        0,
        LogAction::Game {
            data: ByteBuf::from(vec![0]),
        },
    );
    let request = native_decide_request(state, &probe).expect("a game entry is always valid");
    sdk::parse(&encode(&request))
        .expect("the sdk parses the engine's request")
        .table
}

fn zone_ids() -> Vec<u16> {
    agni_riftbound::zone_table()
        .iter()
        .map(|decl| decl.id)
        .collect()
}

fn face_of(face: &Face) -> CardFace {
    let mut sim = CardFace::named(&face.name);
    sim.kind = face.kind.clone();
    sim.energy = face.energy;
    sim.power = face.power;
    sim.might = face.might;
    sim.domain = face.domain.clone();
    sim
}

fn zone_of(zone: Option<u16>) -> Zone {
    match zone {
        None => Zone::Hand,
        Some(BOARD) => Zone::Board,
        Some(id) => Zone::Plugin(id),
    }
}

fn to_sim_target(target: Target) -> CounterTarget {
    match target {
        Target::Table => CounterTarget::Table,
        Target::Seat(seat) => CounterTarget::Seat(seat),
        Target::Card(card) => CounterTarget::Card(card),
    }
}

fn to_sim_effect(effect: &SdkEffect) -> Effect {
    match effect {
        SdkEffect::Move {
            card,
            zone,
            seat,
            index,
        } => Effect::Move {
            card: *card,
            to: zone_of(Some(*zone)),
            seat: *seat,
            index: *index,
        },
        SdkEffect::Annotate { card, key, value } => Effect::Annotate {
            card: *card,
            key: key.clone(),
            value: value.clone().map(ByteBuf::from),
        },
        SdkEffect::Counter {
            target,
            counter,
            delta,
        } => Effect::Counter {
            target: to_sim_target(*target),
            counter: *counter,
            delta: *delta,
        },
        SdkEffect::Spawn {
            face,
            zone,
            seat,
            owner,
        } => Effect::Spawn {
            face: face_of(face),
            to: zone_of(Some(*zone)),
            seat: *seat,
            owner: *owner,
        },
        SdkEffect::Despawn { card } => Effect::Despawn { card: *card },
        SdkEffect::Reveal { card } => Effect::Reveal { card: *card },
        SdkEffect::Conceal { card } => Effect::Conceal { card: *card },
        SdkEffect::Peek { card, seat } => Effect::Peek {
            card: *card,
            seat: *seat,
        },
    }
}

fn to_sim_action(action: &Action) -> LogAction {
    match action {
        Action::Game(data) => LogAction::Game {
            data: ByteBuf::from(data.clone()),
        },
        Action::Move {
            card,
            to,
            seat,
            index,
            hidden,
        } => LogAction::Move {
            card: *card,
            to: zone_of(*to),
            seat: *seat,
            index: *index,
            hidden: *hidden,
        },
        Action::Spawn { face, zone, seat } => LogAction::Spawn {
            face: face_of(face),
            to: zone_of(*zone),
            seat: *seat,
        },
        Action::Reveal { card, face } => LogAction::Reveal {
            card: *card,
            face: face_of(face),
        },
        Action::Clear { seat } => LogAction::Clear { seat: *seat },
        Action::Reset => LogAction::Reset,
        Action::Annotate { card, key, value } => LogAction::Annotate {
            card: *card,
            key: key.clone(),
            value: value.clone().map(ByteBuf::from),
        },
        Action::Counter {
            target,
            counter,
            delta,
        } => LogAction::Counter {
            target: to_sim_target(*target),
            counter: *counter,
            delta: *delta,
        },
        other => panic!("the script does not generate {other:?}"),
    }
}

struct Script {
    rng: Lcg,
    zones: Vec<u16>,
}

impl Script {
    fn card(&mut self, state: &LogState) -> u32 {
        if self.rng.chance(4) {
            return 999;
        }
        let ids: Vec<u32> = state.table.cards().iter().map(|card| card.id.0).collect();
        if ids.is_empty() {
            return 999;
        }
        self.rng.pick(&ids)
    }

    fn zone(&mut self) -> u16 {
        if self.rng.chance(3) {
            return 77;
        }
        if self.rng.chance(4) {
            return BOARD;
        }
        let zones = self.zones.clone();
        self.rng.pick(&zones)
    }

    fn entry_zone(&mut self) -> Option<u16> {
        if self.rng.chance(4) {
            return None;
        }
        Some(self.zone())
    }

    fn seat(&mut self) -> u8 {
        if self.rng.chance(3) {
            2
        } else {
            self.rng.pick(&[0, 1])
        }
    }

    fn index(&mut self) -> u32 {
        self.rng.pick(&[BOTTOM, 1, 2, TOP])
    }

    fn annotation(&mut self) -> (String, Option<Vec<u8>>) {
        let key = self
            .rng
            .pick(&["exhausted", "hidden", "stunned"])
            .to_string();
        let value = match self.rng.below(3) {
            0 => None,
            1 => Some(vec![1]),
            _ => Some(vec![9, 0]),
        };
        (key, value)
    }

    fn counter(&mut self, state: &LogState) -> (Target, u16, i32) {
        let counter = if self.rng.chance(4) {
            9
        } else {
            self.rng.below(7) as u16
        };
        let target = match self.rng.below(5) {
            0 => Target::Table,
            1 | 2 => Target::Seat(self.seat()),
            _ => Target::Card(self.card(state)),
        };
        let delta = self.rng.below(7) as i32 - 3;
        (target, counter, delta)
    }

    fn spawn_face(&mut self) -> Face {
        if self.rng.chance(3) {
            return Face::default();
        }
        if self.rng.chance(50) {
            Face::named("Sprite").with_kind("Unit").with_might(Some(3))
        } else {
            Face::named("Gold")
                .with_kind("Gear")
                .with_cost(Some(1), Some(0))
                .with_domain(vec!["Order".into()])
        }
    }

    fn reveal_face(&mut self, card: u32) -> Face {
        Face::named(format!("Shown {card}"))
            .with_kind("Spell")
            .with_cost(Some(2), Some(1))
            .with_might(None)
            .with_domain(vec!["Fury".into(), "Mind".into()])
    }

    fn effect(&mut self, state: &LogState) -> SdkEffect {
        match self.rng.below(100) {
            0..=34 => SdkEffect::Move {
                card: self.card(state),
                zone: self.zone(),
                seat: self.seat(),
                index: self.index(),
            },
            35..=54 => {
                let (key, value) = self.annotation();
                SdkEffect::Annotate {
                    card: self.card(state),
                    key,
                    value,
                }
            }
            55..=74 => {
                let (target, counter, delta) = self.counter(state);
                SdkEffect::Counter {
                    target,
                    counter,
                    delta,
                }
            }
            75..=86 => SdkEffect::Spawn {
                face: self.spawn_face(),
                zone: self.zone(),
                seat: self.seat(),
                owner: self.rng.pick(&[None, None, Some(0), Some(1)]),
            },
            87..=90 => SdkEffect::Reveal {
                card: self.card(state),
            },
            91..=94 => SdkEffect::Peek {
                card: self.card(state),
                seat: self.rng.pick(&[0, 1, 1, 4]),
            },
            95..=97 => SdkEffect::Conceal {
                card: self.card(state),
            },
            _ => SdkEffect::Despawn {
                card: self.card(state),
            },
        }
    }

    fn entry(&mut self, state: &LogState) -> Action {
        match self.rng.below(100) {
            0..=50 => Action::Game(vec![self.rng.below(256) as u8]),
            52..=66 => Action::Move {
                card: self.card(state),
                to: self.entry_zone(),
                seat: self.seat(),
                index: self.index(),
                hidden: false,
            },
            67..=76 => Action::Spawn {
                face: self.spawn_face(),
                zone: self.entry_zone(),
                seat: self.seat(),
            },
            77..=84 => {
                let (key, value) = self.annotation();
                Action::Annotate {
                    card: self.card(state),
                    key,
                    value,
                }
            }
            85..=91 => {
                let (target, counter, delta) = self.counter(state);
                Action::Counter {
                    target,
                    counter,
                    delta,
                }
            }
            92..=95 => {
                let card = self.card(state);
                Action::Reveal {
                    card,
                    face: self.reveal_face(card),
                }
            }
            96..=98 => Action::Clear { seat: self.seat() },
            _ => Action::Reset,
        }
    }
}

#[derive(Default, Debug)]
struct Tally {
    folded: usize,
    refused_effects: usize,
    invalid_entries: usize,
    spawned: usize,
    despawned: usize,
    revealed: usize,
    swept: usize,
    core_zones: usize,
}

fn run(seed: u64, steps: usize) -> Tally {
    let mut state = opening_table();
    let mut script = Script {
        rng: Lcg(seed),
        zones: zone_ids(),
    };
    let mut tally = Tally::default();
    for _ in 0..steps {
        let seat = script.rng.pick(&[0u8, 1]);
        let action = script.entry(&state);
        let effects: Vec<SdkEffect> = (0..script.rng.below(5))
            .map(|_| script.effect(&state))
            .collect();
        let entry = LogEntry::new(state.next_seq, seat, to_sim_action(&action));
        let Some(request) = native_decide_request(&state, &entry) else {
            let mut nothing = Scripted(Vec::new());
            assert!(fold_entry_with(&mut state, &entry, &mut nothing).is_err());
            tally.invalid_entries += 1;
            continue;
        };
        let parsed = sdk::parse(&encode(&request)).expect("the sdk parses the request");
        assert_eq!(parsed.seat, seat);
        assert_eq!(parsed.action, action);
        let mut projection = parsed.table.clone();
        projection
            .apply_entry(&parsed.action, seat)
            .unwrap_or_else(|error| panic!("a validated entry projects: {error:?} {action:?}"));
        let projected = projection.apply_all(&effects, seat);
        let mut decider = Scripted(effects.iter().map(to_sim_effect).collect());
        let folded = fold_entry_with(&mut state, &entry, &mut decider);
        match (folded, projected) {
            (Ok(()), Ok(())) => {
                assert_eq!(projected_snapshot_matches(&state, &projection), Ok(()));
                tally.folded += 1;
                match &action {
                    Action::Reveal { .. } => tally.revealed += 1,
                    Action::Clear { .. } | Action::Reset => tally.swept += 1,
                    Action::Move { to, .. } | Action::Spawn { zone: to, .. }
                        if matches!(to, None | Some(BOARD)) =>
                    {
                        tally.core_zones += 1
                    }
                    _ => {}
                }
                for effect in &effects {
                    match effect {
                        SdkEffect::Spawn { .. } => tally.spawned += 1,
                        SdkEffect::Despawn { .. } => tally.despawned += 1,
                        _ => {}
                    }
                }
            }
            (Err(FoldError::BadEffect { index }), Err((at, _))) => {
                assert_eq!(index, at, "both sides refuse the same effect: {effects:?}");
                tally.refused_effects += 1;
            }
            (folded, projected) => panic!(
                "the fold and the projection disagree on {action:?} + {effects:?}: {folded:?} vs {projected:?}"
            ),
        }
    }
    tally
}

fn projected_snapshot_matches(state: &LogState, projection: &Snapshot) -> Result<(), String> {
    let refolded = projected(state);
    if refolded == *projection {
        return Ok(());
    }
    Err(format!(
        "the projection drifted from the fold\nfold: {refolded:#?}\nprojection: {projection:#?}"
    ))
}

#[test]
fn the_sdk_projection_folds_random_sequences_exactly_like_agni_sim() {
    let mut total = Tally::default();
    for seed in 1..=24u64 {
        let tally = run(seed, 120);
        total.folded += tally.folded;
        total.refused_effects += tally.refused_effects;
        total.invalid_entries += tally.invalid_entries;
        total.spawned += tally.spawned;
        total.despawned += tally.despawned;
        total.revealed += tally.revealed;
        total.swept += tally.swept;
        total.core_zones += tally.core_zones;
    }
    assert!(total.folded > 800, "{total:?}");
    assert!(total.refused_effects > 100, "{total:?}");
    assert!(total.invalid_entries > 20, "{total:?}");
    assert!(total.spawned > 100, "{total:?}");
    assert!(total.despawned > 10, "{total:?}");
    assert!(total.revealed > 15, "{total:?}");
    assert!(total.swept > 5, "{total:?}");
    assert!(total.core_zones > 15, "{total:?}");
}

#[test]
fn an_entry_level_spawn_takes_the_id_before_the_effect_spawn_that_follows() {
    let mut state = opening_table();
    let battlefield = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
    let action = Action::Spawn {
        face: Face::named("Sprite").with_kind("Unit").with_might(Some(3)),
        zone: Some(battlefield),
        seat: 1,
    };
    let effects = vec![
        SdkEffect::Spawn {
            face: Face::named("Gold")
                .with_kind("Gear")
                .with_cost(Some(1), Some(0))
                .with_domain(vec!["Order".into()]),
            zone: agni_riftbound::ZONE_BASE,
            seat: 0,
            owner: Some(0),
        },
        SdkEffect::Spawn {
            face: Face::named("Gold").with_kind("Gear"),
            zone: agni_riftbound::ZONE_BASE,
            seat: 1,
            owner: None,
        },
        SdkEffect::Counter {
            target: Target::Card(10),
            counter: agni_riftbound::COUNTER_DAMAGE,
            delta: 2,
        },
        SdkEffect::Despawn { card: 12 },
    ];
    let entry = LogEntry::new(state.next_seq, 1, to_sim_action(&action));
    let request = native_decide_request(&state, &entry).unwrap();
    let parsed = sdk::parse(&encode(&request)).unwrap();
    let mut projection = parsed.table.clone();
    assert_eq!(projection.next_id, 10);
    projection.apply_entry(&parsed.action, 1).unwrap();
    projection.apply_all(&effects, 1).unwrap();
    let mut decider = Scripted(effects.iter().map(to_sim_effect).collect());
    fold_entry_with(&mut state, &entry, &mut decider).unwrap();
    assert_eq!(projected(&state), projection);
    let sprite = projection.card(10).unwrap();
    assert_eq!(
        (sprite.owner, sprite.seat, sprite.zone),
        (1, 0, Some(battlefield))
    );
    let gold = projection.card(11).unwrap();
    assert_eq!((gold.owner, gold.seat), (0, 0));
    assert_eq!((gold.energy, gold.power), (Some(1), Some(0)));
    assert_eq!(gold.domain, ["Order"]);
    assert!(projection.card(12).is_none());
    assert_eq!(projection.next_id, 13);
    assert_eq!(
        projection.counter(Target::Card(10), agni_riftbound::COUNTER_DAMAGE),
        Some(2)
    );
    assert!(projection.is_token(10));
    assert!(!projection.is_token(12));
    assert!(state.is_token(10));
    assert!(state.table.get(agni_core::CardId(12)).is_none());
}

#[test]
fn a_despawn_of_a_dealt_card_is_refused_by_both_sides_at_its_index() {
    let mut state = opening_table();
    let effects = vec![SdkEffect::exhaust(1), SdkEffect::Despawn { card: 2 }];
    let entry = LogEntry::new(
        state.next_seq,
        0,
        LogAction::Game {
            data: ByteBuf::from(vec![1]),
        },
    );
    let request = native_decide_request(&state, &entry).unwrap();
    let mut projection = sdk::parse(&encode(&request)).unwrap().table;
    assert_eq!(
        projection.apply_all(&effects, 0),
        Err((1, agni_plugin_sdk::table::ApplyError::NotAToken))
    );
    let mut decider = Scripted(effects.iter().map(to_sim_effect).collect());
    assert_eq!(
        fold_entry_with(&mut state, &entry, &mut decider),
        Err(FoldError::BadEffect { index: 1 })
    );
    assert!(state.annotation(1, "exhausted").is_none());
}

#[test]
fn a_reveal_entry_projects_the_face_the_engine_folds() {
    let mut state = opening_table();
    let face = Face::named("Defy")
        .with_kind("Spell")
        .with_cost(Some(2), Some(1))
        .with_domain(vec!["Fury".into()]);
    let action = Action::Reveal {
        card: 3,
        face: face.clone(),
    };
    let entry = LogEntry::new(state.next_seq, 0, to_sim_action(&action));
    let request = native_decide_request(&state, &entry).unwrap();
    let parsed = sdk::parse(&encode(&request)).unwrap();
    assert_eq!(parsed.action, action);
    assert!(parsed.table.card(3).unwrap().is_hidden());
    let mut projection = parsed.table.clone();
    projection.apply_entry(&parsed.action, 0).unwrap();
    let shown = projection.card(3).unwrap();
    assert_eq!(shown.face(), face);
    assert!(projection.is_revealed(3));
    fold_entry(&mut state, &entry).unwrap();
    assert_eq!(projected(&state), projection);
}

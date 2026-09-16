use crate::state::{GameBlob, Mode, Phase, Showdown};
use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
use agni_plugin_sdk::table::{CardInfo, Snapshot, Target};

pub const ZONE_HAND: &str = "hand";
pub const ZONE_MAIN_DECK: &str = "main-deck";
pub const ZONE_RUNE_DECK: &str = "rune-deck";
pub const ZONE_RUNE_POOL: &str = "rune-pool";
pub const ZONE_BASE: &str = "base";
pub const ZONE_CHAIN: &str = "chain";
pub const KIND_UNIT: &str = "Unit";
pub const KIND_BATTLEFIELD: &str = "Battlefield";
pub const COUNTER_POINTS: u16 = 0;
pub const COUNTER_XP: u16 = 1;
pub const COUNTER_TEMPORARY: u16 = 4;
pub const ZONE_TRASH: &str = "trash";
pub const TEMPORARY_TOKENS: &[&str] = &["Sprite"];
pub const OPTION_VICTORY_SCORE: &str = "victory_score";
pub const OPTION_BATTLEFIELDS: &str = "battlefields";
pub const OPTION_RULES_ENFORCED: &str = "rules_enforced";
pub const DEFAULT_VICTORY_SCORE: i32 = 8;
pub const DEFAULT_BATTLEFIELDS: usize = 2;
pub const RUNES_PER_TURN: usize = 2;
pub const CARDS_PER_TURN: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub victory_score: i32,
    pub battlefields: usize,
    pub enforced: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            victory_score: DEFAULT_VICTORY_SCORE,
            battlefields: DEFAULT_BATTLEFIELDS,
            enforced: false,
        }
    }
}

impl Options {
    pub fn of(table: &Snapshot) -> Self {
        let victory_score = table
            .option(OPTION_VICTORY_SCORE)
            .and_then(|value| i32::try_from(value).ok())
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_VICTORY_SCORE);
        let battlefields = table
            .option(OPTION_BATTLEFIELDS)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value >= 1)
            .unwrap_or_else(|| DEFAULT_BATTLEFIELDS.max(usize::from(table.players)));
        let enforced = table
            .option(OPTION_RULES_ENFORCED)
            .is_some_and(|value| value >= 1);
        Self {
            victory_score,
            battlefields,
            enforced,
        }
    }

    pub fn starting_mode(&self) -> Mode {
        if self.enforced {
            Mode::Enforced
        } else {
            Mode::Free
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostRefusal {
    NotEnoughRunes { needed: u8, ready: u8 },
    NoPowerOf { domain: String },
}

pub fn is_unit(card: &CardInfo) -> bool {
    match card.kind.as_deref() {
        Some(kind) => kind == KIND_UNIT,
        None => true,
    }
}

pub fn zone_id(table: &Snapshot, name: &str) -> Option<u16> {
    table.zone_named(name).map(|zone| zone.id)
}

pub fn contested(table: &Snapshot) -> Vec<u16> {
    let keep = Options::of(table).battlefields;
    table
        .zones
        .iter()
        .filter(|zone| zone.shared && zone.battlefield)
        .map(|zone| zone.id)
        .take(keep)
        .collect()
}

pub fn units_of(table: &Snapshot, zone: u16, seat: u8) -> impl Iterator<Item = &CardInfo> {
    table.owned_at(zone, seat).filter(|card| is_unit(card))
}

pub fn seats_with_units(table: &Snapshot, zone: u16) -> Vec<u8> {
    let mut seats: Vec<u8> = table
        .in_zone(zone)
        .filter(|card| is_unit(card))
        .map(|card| card.owner)
        .collect();
    seats.sort_unstable();
    seats.dedup();
    seats
}

pub fn awaken(table: &Snapshot, seat: u8) -> Vec<Effect> {
    table
        .cards
        .iter()
        .filter(|card| card.owner == seat && card.exhausted)
        .map(|card| Effect::ready(card.id))
        .collect()
}

fn from_top(table: &Snapshot, zone: u16, seat: u8, count: usize) -> Vec<u32> {
    let held: Vec<u32> = table.held(zone, seat).map(|card| card.id).collect();
    held.iter().rev().take(count).copied().collect()
}

pub fn channel(table: &Snapshot, seat: u8, count: usize) -> Vec<Effect> {
    let (Some(deck), Some(pool)) = (
        zone_id(table, ZONE_RUNE_DECK),
        zone_id(table, ZONE_RUNE_POOL),
    ) else {
        return Vec::new();
    };
    from_top(table, deck, seat, count)
        .into_iter()
        .map(|card| Effect::Move {
            card,
            zone: pool,
            seat,
            index: TOP,
        })
        .collect()
}

pub fn draw(table: &Snapshot, seat: u8, count: usize) -> Vec<Effect> {
    let (Some(deck), Some(hand)) = (zone_id(table, ZONE_MAIN_DECK), zone_id(table, ZONE_HAND))
    else {
        return Vec::new();
    };
    from_top(table, deck, seat, count)
        .into_iter()
        .map(|card| Effect::Move {
            card,
            zone: hand,
            seat,
            index: TOP,
        })
        .collect()
}

pub fn runes_this_turn(state: &GameBlob) -> usize {
    let players = usize::from(state.players());
    let turn = usize::from(state.turn());
    if turn == players && players > 1 {
        RUNES_PER_TURN + 1
    } else {
        RUNES_PER_TURN
    }
}

pub fn kill_temporary(table: &Snapshot, seat: u8) -> Vec<Effect> {
    let Some(trash) = zone_id(table, ZONE_TRASH) else {
        return Vec::new();
    };
    table
        .cards
        .iter()
        .filter(|card| card.owner == seat)
        .filter(|card| {
            table
                .counter(Target::Card(card.id), COUNTER_TEMPORARY)
                .unwrap_or(0)
                > 0
        })
        .map(|card| Effect::Move {
            card: card.id,
            zone: trash,
            seat,
            index: TOP,
        })
        .collect()
}

pub fn on_spawn(table: &Snapshot, name: &str) -> Vec<Effect> {
    if TEMPORARY_TOKENS
        .iter()
        .any(|token| token.eq_ignore_ascii_case(name))
    {
        vec![Effect::Counter {
            target: Target::Card(table.next_id),
            counter: COUNTER_TEMPORARY,
            delta: 1,
        }]
    } else {
        Vec::new()
    }
}

pub fn score(
    table: &Snapshot,
    state: &GameBlob,
    seat: u8,
    by_hold: bool,
    awarded: &mut i32,
) -> Vec<Effect> {
    let standing = points(table, seat) + *awarded;
    let victory = Options::of(table).victory_score;
    if standing < victory - 1 || by_hold || scored_every_battlefield(table, state, seat) {
        *awarded += 1;
        return vec![Effect::score(seat, COUNTER_POINTS, 1)];
    }
    draw(table, seat, 1)
}

pub fn scored_every_battlefield(table: &Snapshot, state: &GameBlob, seat: u8) -> bool {
    let zones = contested(table);
    !zones.is_empty() && zones.iter().all(|zone| state.scored(*zone, seat))
}

pub fn begin_turn(table: &Snapshot, state: &mut GameBlob) -> Vec<Effect> {
    let seat = state.turn_player();
    let mut effects = kill_temporary(table, seat);
    effects.extend(awaken(table, seat));
    let mut awarded = 0;
    for zone in contested(table) {
        settle(table, state, zone);
        if state.holder(zone) == Some(seat) && !state.scored(zone, seat) {
            state.mark_scored(zone, seat);
            effects.extend(score(table, state, seat, true, &mut awarded));
        }
    }
    effects.extend(channel(table, seat, runes_this_turn(state)));
    effects.extend(draw(table, seat, CARDS_PER_TURN));
    effects
}

pub fn settle(table: &Snapshot, state: &mut GameBlob, zone: u16) -> Option<u8> {
    let before = state.holder(zone);
    match seats_with_units(table, zone).as_slice() {
        [] => {
            state.set_holder(zone, None);
            None
        }
        [only] => {
            state.set_holder(zone, Some(*only));
            (before != Some(*only)).then_some(*only)
        }
        _ => None,
    }
}

pub fn establish(table: &Snapshot, state: &mut GameBlob, zone: u16) -> Vec<Effect> {
    match settle(table, state, zone) {
        Some(taker) if !state.scored(zone, taker) => {
            state.mark_scored(zone, taker);
            score(table, state, taker, false, &mut 0)
        }
        _ => Vec::new(),
    }
}

pub fn resolve_all(table: &Snapshot, state: &mut GameBlob) -> Vec<Effect> {
    contested(table)
        .into_iter()
        .flat_map(|zone| establish(table, state, zone))
        .collect()
}

pub fn close_showdown(table: &Snapshot, state: &mut GameBlob, showdown: Showdown) -> Vec<Effect> {
    establish(table, state, showdown.zone)
}

pub fn arrival(table: &Snapshot, state: &mut GameBlob, seat: u8, zone: u16) {
    if !contested(table).contains(&zone)
        || state.phase() != Some(Phase::Action)
        || !state.is_turn_player(seat)
        || state.showdown.is_some()
        || state.holder(zone) == Some(seat)
    {
        return;
    }
    let defender = state
        .holder(zone)
        .filter(|holder| *holder != seat)
        .or_else(|| {
            seats_with_units(table, zone)
                .into_iter()
                .find(|other| *other != seat)
        })
        .unwrap_or_else(|| (seat + 1) % state.players().max(1));
    if defender == seat {
        return;
    }
    state.showdown = Some(Showdown::open(zone, seat, defender));
}

pub fn is_unit_play_onto_a_battlefield(table: &Snapshot, card: &CardInfo, to: u16) -> bool {
    let from_hand = zone_id(table, ZONE_HAND).is_some_and(|hand| card.zone == Some(hand));
    from_hand && is_unit(card) && !card.is_hidden() && contested(table).contains(&to)
}

pub fn is_unit_play(table: &Snapshot, card: &CardInfo, to: u16) -> bool {
    let from_hand = zone_id(table, ZONE_HAND).is_some_and(|hand| card.zone == Some(hand));
    from_hand && is_unit(card) && !card.is_hidden() && zone_id(table, ZONE_BASE) == Some(to)
}

pub fn is_march(table: &Snapshot, card: &CardInfo, to: u16) -> bool {
    let board = |zone: Option<u16>| {
        zone.is_some_and(|zone| {
            zone_id(table, ZONE_BASE) == Some(zone) || contested(table).contains(&zone)
        })
    };
    is_unit(card)
        && !card.is_hidden()
        && board(card.zone)
        && board(Some(to))
        && card.zone != Some(to)
}

pub fn is_play(table: &Snapshot, card: &CardInfo, to: u16) -> bool {
    let from_hand = zone_id(table, ZONE_HAND).is_some_and(|hand| card.zone == Some(hand));
    let onto = zone_id(table, ZONE_BASE) == Some(to)
        || zone_id(table, ZONE_CHAIN) == Some(to)
        || contested(table).contains(&to);
    from_hand && onto
}

pub fn rune_domain(rune: &CardInfo) -> Option<&str> {
    rune.domain
        .first()
        .map(String::as_str)
        .or_else(|| rune.name.strip_suffix(" Rune"))
}

pub fn power_domains(card: &CardInfo) -> Vec<&str> {
    let power = usize::from(card.power.unwrap_or(0));
    let domains: Vec<&str> = card.domain.iter().map(String::as_str).collect();
    match domains.len() {
        0 => Vec::new(),
        1 => vec![domains[0]; power],
        _ if domains.len() == power => domains,
        _ => Vec::new(),
    }
}

pub fn pay(table: &Snapshot, seat: u8, card: &CardInfo) -> Result<Vec<Effect>, CostRefusal> {
    let energy = usize::from(card.energy.unwrap_or(0));
    let power = usize::from(card.power.unwrap_or(0));
    if energy + power == 0 {
        return Ok(Vec::new());
    }
    let (Some(pool), Some(deck)) = (
        zone_id(table, ZONE_RUNE_POOL),
        zone_id(table, ZONE_RUNE_DECK),
    ) else {
        return Ok(Vec::new());
    };
    let mut ready: Vec<&CardInfo> = table
        .held(pool, seat)
        .filter(|rune| !rune.exhausted)
        .collect();
    if ready.len() < energy + power {
        return Err(CostRefusal::NotEnoughRunes {
            needed: (energy + power) as u8,
            ready: ready.len() as u8,
        });
    }
    let mut recycled = Vec::new();
    let wanted = power_domains(card);
    for domain in &wanted {
        let Some(index) = ready
            .iter()
            .position(|rune| rune_domain(rune) == Some(domain))
        else {
            return Err(CostRefusal::NoPowerOf {
                domain: (*domain).to_string(),
            });
        };
        recycled.push(ready.remove(index).id);
    }
    for _ in wanted.len()..power {
        let index = ready
            .iter()
            .position(|rune| {
                card.domain
                    .iter()
                    .any(|domain| rune_domain(rune) == Some(domain))
            })
            .unwrap_or(0);
        recycled.push(ready.remove(index).id);
    }
    let mut effects: Vec<Effect> = ready[..energy]
        .iter()
        .map(|rune| Effect::exhaust(rune.id))
        .collect();
    effects.extend(recycled.into_iter().map(|id| Effect::Move {
        card: id,
        zone: deck,
        seat,
        index: BOTTOM,
    }));
    Ok(effects)
}

pub fn points(table: &Snapshot, seat: u8) -> i32 {
    table
        .counter(Target::Seat(seat), COUNTER_POINTS)
        .unwrap_or(0)
}

pub fn winner(table: &Snapshot) -> Option<u8> {
    let victory = Options::of(table).victory_score;
    (0..table.players).find(|seat| points(table, *seat) >= victory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Mode;
    use agni_plugin_sdk::table::{CounterInfo, ZoneKind, ZoneSummary};

    fn zone(id: u16, name: &str, kind: ZoneKind, shared: bool) -> ZoneSummary {
        ZoneSummary {
            id,
            name: name.into(),
            kind,
            shared,
            battlefield: kind == ZoneKind::Battlefield,
            label: name.into(),
            ..Default::default()
        }
    }

    fn card(id: u32, zone: u16, seat: u8, name: &str, kind: &str) -> CardInfo {
        CardInfo {
            id,
            zone: Some(zone),
            seat: if zone >= 9 { 0 } else { seat },
            owner: seat,
            name: name.into(),
            kind: Some(kind.into()),
            ..Default::default()
        }
    }

    fn rune(id: u32, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            exhausted,
            ..card(id, 7, seat, "Fury Rune", "Rune")
        }
    }

    fn rune_of(id: u32, seat: u8, domain: &str) -> CardInfo {
        CardInfo {
            domain: vec![domain.into()],
            ..card(id, 7, seat, &format!("{domain} Rune"), "Rune")
        }
    }

    fn table() -> Snapshot {
        Snapshot {
            players: 2,
            zones: vec![
                zone(0, ZONE_HAND, ZoneKind::Hand, false),
                zone(1, ZONE_MAIN_DECK, ZoneKind::Deck, false),
                zone(2, ZONE_RUNE_DECK, ZoneKind::Deck, false),
                zone(7, ZONE_RUNE_POOL, ZoneKind::Aux, false),
                zone(8, ZONE_BASE, ZoneKind::Battlefield, false),
                zone(5, ZONE_TRASH, ZoneKind::Discard, false),
                zone(9, "battlefield-1", ZoneKind::Battlefield, true),
                zone(10, "battlefield-2", ZoneKind::Battlefield, true),
                zone(11, "battlefield-3", ZoneKind::Battlefield, true),
                zone(12, ZONE_CHAIN, ZoneKind::Stack, true),
            ],
            cards: vec![
                card(20, 1, 0, "", ""),
                card(21, 1, 0, "", ""),
                card(30, 2, 0, "", ""),
                card(31, 2, 0, "", ""),
                card(32, 2, 0, "", ""),
                rune(40, 0, true),
                rune(41, 0, false),
                rune(42, 0, false),
                rune(43, 0, false),
                CardInfo {
                    exhausted: true,
                    ..card(50, 8, 0, "Noxus Hopeful", KIND_UNIT)
                },
                card(51, 9, 0, "Proving Grounds", KIND_BATTLEFIELD),
                card(60, 10, 1, "Sprite", KIND_UNIT),
            ],
            counters: vec![CounterInfo {
                target: Target::Seat(1),
                counter: COUNTER_POINTS,
                value: 2,
            }],
            ..Default::default()
        }
    }

    fn at_turn(turn: u16, player: u8) -> GameBlob {
        let mut state = GameBlob::start(2, 0, Mode::Free);
        let core = state.core_mut().unwrap();
        core.turn = turn;
        core.player = player;
        state
    }

    #[test]
    fn a_temporary_card_is_killed_at_its_owners_beginning_phase_and_sprites_arrive_temporary() {
        let mut table = table();
        table.next_id = 200;
        table.cards.push(card(90, 10, 1, "Sprite", KIND_UNIT));
        table.counters.push(CounterInfo {
            target: Target::Card(90),
            counter: COUNTER_TEMPORARY,
            value: 1,
        });
        assert_eq!(
            kill_temporary(&table, 1),
            [Effect::Move {
                card: 90,
                zone: 5,
                seat: 1,
                index: TOP
            }]
        );
        assert!(kill_temporary(&table, 0).is_empty());
        let mut state = at_turn(2, 1);
        let first = begin_turn(&table, &mut state).into_iter().next().unwrap();
        assert!(matches!(first, Effect::Move { card: 90, .. }));
        assert_eq!(
            on_spawn(&table, "Sprite"),
            [Effect::Counter {
                target: Target::Card(200),
                counter: COUNTER_TEMPORARY,
                delta: 1
            }]
        );
        assert!(on_spawn(&table, "Gold").is_empty());
    }

    #[test]
    fn the_shared_battlefields_follow_the_table_option_or_one_per_player() {
        assert_eq!(contested(&table()), [9, 10]);
        let mut three = table();
        three.options = vec![(OPTION_BATTLEFIELDS.into(), 3)];
        assert_eq!(contested(&three), [9, 10, 11]);
        assert_eq!(Options::of(&three).victory_score, DEFAULT_VICTORY_SCORE);
        let mut crowd = table();
        crowd.players = 3;
        assert_eq!(contested(&crowd), [9, 10, 11]);
        let mut odd = table();
        odd.options = vec![
            (OPTION_VICTORY_SCORE.into(), -4),
            (OPTION_BATTLEFIELDS.into(), i64::from(u32::MAX) + 1),
            ("handicap".into(), 1),
        ];
        assert_eq!(Options::of(&odd).victory_score, DEFAULT_VICTORY_SCORE);
        assert_eq!(contested(&odd), [9, 10, 11]);
    }

    #[test]
    fn the_rules_enforced_option_picks_the_starting_mode() {
        assert!(!Options::of(&table()).enforced);
        assert_eq!(Options::of(&table()).starting_mode(), Mode::Free);
        let mut enforced = table();
        enforced.options = vec![(OPTION_RULES_ENFORCED.into(), 1)];
        assert!(Options::of(&enforced).enforced);
        assert_eq!(Options::of(&enforced).starting_mode(), Mode::Enforced);
        assert_eq!(Options::of(&enforced).victory_score, DEFAULT_VICTORY_SCORE);
        for odd in [0, -1] {
            let mut table = table();
            table.options = vec![(OPTION_RULES_ENFORCED.into(), odd)];
            assert_eq!(Options::of(&table).starting_mode(), Mode::Free, "{odd}");
        }
        let mut loud = table();
        loud.options = vec![(OPTION_RULES_ENFORCED.into(), 3)];
        assert_eq!(Options::of(&loud).starting_mode(), Mode::Enforced);
    }

    #[test]
    fn free_table_scoring_reads_the_victory_score_from_the_options() {
        let mut table = table();
        table.options = vec![(OPTION_VICTORY_SCORE.into(), 6)];
        table.cards.push(card(22, 1, 1, "", ""));
        let state = GameBlob::start(2, 0, Mode::Free);
        let mut awarded = 0;
        table.counters[0].value = 5;
        assert!(
            matches!(
                score(&table, &state, 1, false, &mut awarded).as_slice(),
                [Effect::Move {
                    card: 22,
                    zone: 0,
                    ..
                }]
            ),
            "the final point of six needs a hold or every battlefield"
        );
        assert_eq!(awarded, 0);
        assert_eq!(
            score(&table, &state, 1, true, &mut awarded),
            [Effect::score(1, COUNTER_POINTS, 1)]
        );
        assert_eq!(winner(&table), None);
        table.counters[0].value = 6;
        assert_eq!(winner(&table), Some(1));
    }

    #[test]
    fn terrain_is_not_a_unit_but_an_unknown_kind_is() {
        let table = table();
        assert!(!is_unit(table.card(51).unwrap()));
        assert!(is_unit(table.card(60).unwrap()));
        assert!(is_unit(&CardInfo {
            kind: None,
            ..card(1, 9, 0, "", "")
        }));
        assert_eq!(seats_with_units(&table, 9), Vec::<u8>::new());
        assert_eq!(seats_with_units(&table, 10), [1]);
    }

    #[test]
    fn the_beginning_phase_readies_channels_and_draws_from_the_top() {
        let table = table();
        let mut state = at_turn(3, 0);
        let effects = begin_turn(&table, &mut state);
        assert_eq!(
            effects,
            [
                Effect::ready(40),
                Effect::ready(50),
                Effect::Move {
                    card: 32,
                    zone: 7,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 31,
                    zone: 7,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 21,
                    zone: 0,
                    seat: 0,
                    index: TOP
                },
            ]
        );
        assert_eq!(state.holder(10), Some(1));
        assert_eq!(state.holder(9), None);
    }

    #[test]
    fn the_seat_going_last_channels_an_extra_rune_on_its_first_turn() {
        let mut second = at_turn(2, 1);
        assert_eq!(runes_this_turn(&second), 3);
        let first = at_turn(1, 0);
        assert_eq!(runes_this_turn(&first), 2);
        second.core_mut().unwrap().turn = 4;
        assert_eq!(runes_this_turn(&second), 2);
        let mut four = GameBlob::start(4, 0, Mode::Free);
        four.core_mut().unwrap().turn = 4;
        four.core_mut().unwrap().player = 3;
        assert_eq!(runes_this_turn(&four), 3);
    }

    #[test]
    fn holding_a_battlefield_with_a_unit_scores_once_per_turn() {
        let table = table();
        let mut state = at_turn(2, 1);
        state.set_holder(10, Some(1));
        let effects = begin_turn(&table, &mut state);
        assert!(effects.contains(&Effect::score(1, COUNTER_POINTS, 1)));
        assert!(state.scored(10, 1));
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::Counter { .. }))
                .count(),
            1
        );
        let again = establish(&table, &mut state, 10);
        assert!(again.is_empty());
    }

    #[test]
    fn establishing_control_scores_a_conquer_for_a_new_holder_only() {
        let table = table();
        let mut state = at_turn(3, 1);
        assert_eq!(
            establish(&table, &mut state, 10),
            [Effect::score(1, COUNTER_POINTS, 1)]
        );
        assert_eq!(state.holder(10), Some(1));
        assert!(establish(&table, &mut state, 10).is_empty());
        assert!(establish(&table, &mut state, 9).is_empty());
        assert_eq!(state.holder(9), None);
        let mut both = table.clone();
        both.cards.push(card(61, 10, 0, "Jinx", KIND_UNIT));
        assert!(establish(&both, &mut state, 10).is_empty());
        assert_eq!(state.holder(10), Some(1));
    }

    #[test]
    fn a_unit_arriving_on_someone_elses_battlefield_opens_a_showdown() {
        let table = table();
        let mut state = at_turn(3, 0);
        arrival(&table, &mut state, 0, 10);
        let showdown = state.showdown.expect("a showdown opened");
        assert_eq!(
            (showdown.zone, showdown.attacker, showdown.defender),
            (10, 0, 1)
        );
        let mut quiet = at_turn(3, 0);
        arrival(&table, &mut quiet, 0, 9);
        assert_eq!(quiet.showdown.unwrap().defender, 1);
        let mut mine = at_turn(3, 0);
        mine.set_holder(9, Some(0));
        arrival(&table, &mut mine, 0, 9);
        assert!(mine.showdown.is_none());
        let mut theirs = at_turn(3, 1);
        arrival(&table, &mut theirs, 0, 10);
        assert!(theirs.showdown.is_none());
        let mut beginning = at_turn(3, 0);
        beginning.core_mut().unwrap().phase = Phase::Beginning;
        arrival(&table, &mut beginning, 0, 10);
        assert!(beginning.showdown.is_none());
    }

    #[test]
    fn a_unit_from_hand_lands_on_the_base_never_on_a_battlefield() {
        let table = table();
        let unit = card(70, 0, 0, "Shadow Order Disciple", KIND_UNIT);
        assert!(is_unit_play_onto_a_battlefield(&table, &unit, 9));
        assert!(!is_unit_play_onto_a_battlefield(&table, &unit, 8));
        let spell = card(71, 0, 0, "Rebuke", "Spell");
        assert!(!is_unit_play_onto_a_battlefield(&table, &spell, 9));
        let from_base = card(72, 8, 0, "Jinx", KIND_UNIT);
        assert!(!is_unit_play_onto_a_battlefield(&table, &from_base, 9));
        let hidden = card(73, 0, 0, "", KIND_UNIT);
        assert!(!is_unit_play_onto_a_battlefield(&table, &hidden, 9));
    }

    #[test]
    fn a_unit_enters_from_hand_and_marches_between_board_zones() {
        let table = table();
        let played = card(70, 0, 0, "Shadow Order Disciple", KIND_UNIT);
        assert!(is_unit_play(&table, &played, 8));
        assert!(!is_unit_play(&table, &played, 9));
        assert!(!is_unit_play(&table, &card(71, 0, 0, "Rebuke", "Spell"), 8));
        let at_base = card(72, 8, 0, "Jinx", KIND_UNIT);
        assert!(is_march(&table, &at_base, 9));
        assert!(!is_march(&table, &at_base, 8));
        assert!(!is_march(&table, &at_base, 0));
        assert!(!is_march(&table, &played, 9));
        let afield = card(73, 10, 1, "Sprite", KIND_UNIT);
        assert!(is_march(&table, &afield, 8));
        assert!(is_march(&table, &afield, 9));
        assert!(!is_march(&table, table.card(51).unwrap(), 8));
    }

    #[test]
    fn playing_from_hand_exhausts_energy_and_recycles_power() {
        let table = table();
        let unit = CardInfo {
            energy: Some(2),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..card(70, 0, 0, "Shadow Order Disciple", KIND_UNIT)
        };
        assert!(is_play(&table, &unit, 8));
        assert!(is_play(&table, &unit, 12));
        assert!(!is_play(&table, &unit, 0));
        assert!(!is_play(&table, table.card(50).unwrap(), 9));
        assert_eq!(
            pay(&table, 0, &unit).unwrap(),
            [
                Effect::exhaust(42),
                Effect::exhaust(43),
                Effect::Move {
                    card: 41,
                    zone: 2,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        let mut mixed = table.clone();
        mixed
            .cards
            .retain(|held| held.kind.as_deref() != Some("Rune"));
        mixed.cards.push(rune_of(80, 0, "Calm"));
        mixed.cards.push(rune_of(81, 0, "Chaos"));
        mixed.cards.push(rune_of(82, 0, "Fury"));
        assert_eq!(
            pay(&mixed, 0, &unit).unwrap(),
            [
                Effect::exhaust(80),
                Effect::exhaust(81),
                Effect::Move {
                    card: 82,
                    zone: 2,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        let chaos_only = CardInfo {
            domain: vec!["Chaos".into()],
            ..unit.clone()
        };
        let mut no_chaos = mixed.clone();
        no_chaos.cards.retain(|held| held.id != 81);
        no_chaos.cards.push(rune_of(83, 0, "Mind"));
        assert_eq!(
            pay(&no_chaos, 0, &chaos_only),
            Err(CostRefusal::NoPowerOf {
                domain: "Chaos".into()
            })
        );
        let two_domains = CardInfo {
            power: Some(2),
            domain: vec!["Calm".into(), "Chaos".into()],
            ..unit.clone()
        };
        mixed.cards.push(rune_of(84, 0, "Body"));
        let recycled = pay(&mixed, 0, &two_domains)
            .unwrap()
            .into_iter()
            .filter(|effect| matches!(effect, Effect::Move { .. }))
            .count();
        assert_eq!(recycled, 2);
        assert_eq!(rune_domain(&rune(1, 0, false)), Some("Fury"));
        assert_eq!(power_domains(&two_domains), ["Calm", "Chaos"]);
        let pricey = CardInfo {
            energy: Some(7),
            power: Some(1),
            ..unit.clone()
        };
        assert_eq!(
            pay(&table, 0, &pricey),
            Err(CostRefusal::NotEnoughRunes {
                needed: 8,
                ready: 3
            })
        );
        let free = CardInfo {
            energy: None,
            power: None,
            ..unit
        };
        assert_eq!(pay(&table, 0, &free).unwrap(), []);
    }

    #[test]
    fn the_final_point_needs_a_hold_or_every_battlefield_conquered_this_turn() {
        let mut table = table();
        table.counters[0].value = DEFAULT_VICTORY_SCORE - 1;
        table.cards.push(card(61, 9, 1, "Jinx", KIND_UNIT));
        table.cards.push(card(22, 1, 1, "", ""));
        let mut state = at_turn(4, 1);
        state.set_holder(10, Some(1));
        state.mark_scored(10, 1);
        let mut first = state.clone();
        first.control.clear();
        let draws = establish(&table, &mut first, 9);
        assert!(matches!(
            draws.as_slice(),
            [Effect::Move {
                card: 22,
                zone: 0,
                ..
            }]
        ));
        assert!(first.scored(9, 1));
        let wins = establish(&table, &mut state, 9);
        assert_eq!(wins, [Effect::score(1, COUNTER_POINTS, 1)]);
        let mut held = at_turn(5, 1);
        held.set_holder(10, Some(1));
        let effects = begin_turn(&table, &mut held);
        assert!(effects.contains(&Effect::score(1, COUNTER_POINTS, 1)));
    }

    #[test]
    fn points_and_the_winner_read_from_the_seat_counter() {
        let mut table = table();
        assert_eq!(points(&table, 1), 2);
        assert_eq!(points(&table, 0), 0);
        assert_eq!(winner(&table), None);
        table.counters[0].value = DEFAULT_VICTORY_SCORE;
        assert_eq!(winner(&table), Some(1));
    }
}

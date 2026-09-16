use crate::log::{LogState, Seat};
use crate::wire::{
    zone_visibility, CounterDecl, CounterScope, CounterTarget, CounterValue, ZoneDecl,
    ZoneVisibility,
};
use agni_core::{Card, CardFace, CardId, PlayerId, Table, Zone};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Badge {
    pub key: String,
    pub value: ByteBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCard {
    pub id: u32,
    pub zone: Zone,
    pub seat: u8,
    pub owner: u8,
    pub face_visible: bool,
    pub badges: Vec<Badge>,
}

impl ViewCard {
    pub fn badge(&self, key: &str) -> Option<&[u8]> {
        self.badges
            .iter()
            .find(|badge| badge.key == key)
            .map(|badge| badge.value.as_slice())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableView {
    pub zones: Vec<ZoneDecl>,
    pub seats: Vec<Seat>,
    pub cards: Vec<ViewCard>,
    pub revealed: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peeked: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counter_table: Vec<CounterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<CounterValue>,
    #[serde(default, skip_serializing_if = "no_bytes")]
    pub plugin_state: ByteBuf,
    pub next_seq: u64,
}

fn no_bytes(bytes: &ByteBuf) -> bool {
    bytes.is_empty()
}

impl TableView {
    pub fn card(&self, id: u32) -> Option<&ViewCard> {
        self.cards.iter().find(|card| card.id == id)
    }

    pub fn counter_decl(&self, counter: u16) -> Option<&CounterDecl> {
        crate::wire::counter_decl(&self.counter_table, counter)
    }

    pub fn counter(&self, target: CounterTarget, counter: u16) -> Option<i32> {
        self.counters
            .iter()
            .find(|held| held.target == target && held.counter == counter)
            .map(|held| held.value)
    }

    pub fn counters_for(&self, target: CounterTarget) -> Vec<&CounterValue> {
        self.counters
            .iter()
            .filter(|held| held.target == target)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewDelta {
    Zones(Vec<ZoneDecl>),
    Seats(Vec<Seat>),
    Cards(Vec<ViewCard>),
    Revealed(Vec<u32>),
    Peeked(Vec<u32>),
    Counters(Vec<CounterValue>),
    CounterTable(Vec<CounterDecl>),
    PluginState(ByteBuf),
    NextSeq(u64),
}

pub fn table_view(state: &LogState, viewer: u8) -> TableView {
    let cards: Vec<ViewCard> = state
        .table
        .cards()
        .iter()
        .map(|card| {
            let visibility = zone_visibility(&state.zones, card.zone);
            let granted = match visibility {
                Some(ZoneVisibility::All) => true,
                Some(ZoneVisibility::Owner) => card.seat.0 == viewer,
                Some(ZoneVisibility::None) | None => false,
            };
            let still_public = !matches!(
                visibility,
                Some(ZoneVisibility::Owner) | Some(ZoneVisibility::None)
            );
            let badges: Vec<Badge> = state
                .annotations
                .get(&card.id.0)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|(key, value)| Badge {
                            key: key.clone(),
                            value: value.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            ViewCard {
                id: card.id.0,
                zone: card.zone,
                seat: card.seat.0,
                owner: card.owner.0,
                face_visible: granted
                    || (still_public && state.revealed.contains(&card.id.0))
                    || state.shown_in_place(card.id.0)
                    || state.peeked_by(card.id.0, viewer),
                badges,
            }
        })
        .collect();
    TableView {
        zones: state.zones.clone(),
        seats: state.seats.clone(),
        counters: visible_counters(state, &cards),
        cards,
        counter_table: state.counter_table.clone(),
        revealed: state.revealed.iter().copied().collect(),
        peeked: state.peeks_of(viewer).collect(),
        plugin_state: state.plugin_state.clone(),
        next_seq: state.next_seq,
    }
}

fn visible_counters(state: &LogState, cards: &[ViewCard]) -> Vec<CounterValue> {
    let mut out = Vec::new();
    for decl in &state.counter_table {
        match decl.scope {
            CounterScope::Table => out.push(CounterValue {
                target: CounterTarget::Table,
                counter: decl.id,
                value: state
                    .counter(CounterTarget::Table, decl.id)
                    .unwrap_or(decl.start),
            }),
            CounterScope::Seat => {
                for seat in &state.seats {
                    let target = CounterTarget::Seat(seat.seat);
                    out.push(CounterValue {
                        target,
                        counter: decl.id,
                        value: state.counter(target, decl.id).unwrap_or(decl.start),
                    });
                }
            }
            CounterScope::Card => {
                for card in cards.iter().filter(|card| card.face_visible) {
                    let target = CounterTarget::Card(card.id);
                    if let Some(value) = state
                        .counters
                        .iter()
                        .find(|held| held.target == target && held.counter == decl.id)
                    {
                        out.push(*value);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

pub fn diff_views(prev: &TableView, next: &TableView) -> Vec<ViewDelta> {
    let mut deltas = Vec::new();
    if prev.zones != next.zones {
        deltas.push(ViewDelta::Zones(next.zones.clone()));
    }
    if prev.seats != next.seats {
        deltas.push(ViewDelta::Seats(next.seats.clone()));
    }
    if prev.cards != next.cards {
        deltas.push(ViewDelta::Cards(next.cards.clone()));
    }
    if prev.counter_table != next.counter_table {
        deltas.push(ViewDelta::CounterTable(next.counter_table.clone()));
    }
    if prev.counters != next.counters {
        deltas.push(ViewDelta::Counters(next.counters.clone()));
    }
    if prev.revealed != next.revealed {
        deltas.push(ViewDelta::Revealed(next.revealed.clone()));
    }
    if prev.peeked != next.peeked {
        deltas.push(ViewDelta::Peeked(next.peeked.clone()));
    }
    if prev.plugin_state != next.plugin_state {
        deltas.push(ViewDelta::PluginState(next.plugin_state.clone()));
    }
    if prev.next_seq != next.next_seq {
        deltas.push(ViewDelta::NextSeq(next.next_seq));
    }
    deltas
}

pub fn apply_deltas(view: &mut TableView, deltas: &[ViewDelta]) {
    for delta in deltas {
        match delta {
            ViewDelta::Zones(zones) => view.zones.clone_from(zones),
            ViewDelta::Seats(seats) => view.seats.clone_from(seats),
            ViewDelta::Cards(cards) => view.cards.clone_from(cards),
            ViewDelta::Revealed(revealed) => view.revealed.clone_from(revealed),
            ViewDelta::Peeked(peeked) => view.peeked.clone_from(peeked),
            ViewDelta::Counters(counters) => view.counters.clone_from(counters),
            ViewDelta::CounterTable(table) => view.counter_table.clone_from(table),
            ViewDelta::PluginState(bytes) => view.plugin_state.clone_from(bytes),
            ViewDelta::NextSeq(next_seq) => view.next_seq = *next_seq,
        }
    }
}

pub fn view_to_table(view: &TableView, faces: &BTreeMap<u32, CardFace>) -> Table {
    let mut table = Table::new();
    for card in &view.cards {
        let face = if card.face_visible {
            faces
                .get(&card.id)
                .cloned()
                .unwrap_or_else(CardFace::hidden)
        } else {
            CardFace::hidden()
        };
        table.insert_card(Card {
            id: CardId(card.id),
            owner: PlayerId(card.owner),
            seat: PlayerId(card.seat),
            zone: card.zone,
            face,
        });
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{fold_entry, render_table, LogAction, LogEntry, LogState, TableConfig};
    use crate::wire::{ZoneKind, ZoneLayout, ZoneOwner, ZonePlace};

    fn counter_state(scope: CounterScope, visibility: ZoneVisibility) -> LogState {
        let mut state = LogState::new();
        let zones = vec![ZoneDecl {
            id: 0,
            name: "hand".into(),
            kind: ZoneKind::Hand,
            owner: ZoneOwner::PerSeat,
            visibility,
            layout: ZoneLayout::Fan,
            place: ZonePlace::Fan,
            span: 1,
            label: "Hand".into(),
        }];
        let counters = vec![CounterDecl {
            id: 1,
            name: "life".into(),
            label: "Life".into(),
            scope,
            start: 20,
            min: None,
            max: None,
            step: 1,
            place: crate::wire::CounterPlace::SeatPlate,
            color: crate::wire::DEFAULT_COUNTER_COLOR,
        }];
        let entries = vec![
            LogEntry::new(
                0,
                0,
                LogAction::Genesis {
                    name: "rae".into(),
                    config: TableConfig {
                        engine: None,
                        plugin: None,
                        zones,
                        options: None,
                        counters,
                        despawn_any: false,
                    },
                },
            ),
            LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
            LogEntry::new(
                2,
                0,
                LogAction::Deal {
                    cards: vec![7],
                    to: agni_core::Zone::Plugin(0),
                },
            ),
        ];
        for entry in &entries {
            fold_entry(&mut state, entry).expect("the script folds");
        }
        state
    }

    #[test]
    fn a_reveal_stops_counting_once_the_card_is_back_in_a_hidden_zone() {
        let mut hand = counter_state(CounterScope::Seat, ZoneVisibility::Owner);
        hand.revealed.insert(7);
        assert!(table_view(&hand, 0).card(7).unwrap().face_visible);
        assert!(!table_view(&hand, 1).card(7).unwrap().face_visible);
        let mut public = counter_state(CounterScope::Seat, ZoneVisibility::All);
        public.revealed.insert(7);
        assert!(table_view(&public, 1).card(7).unwrap().face_visible);
        let mut deck = counter_state(CounterScope::Seat, ZoneVisibility::None);
        deck.revealed.insert(7);
        assert!(!table_view(&deck, 0).card(7).unwrap().face_visible);
        hand.shown.insert(7);
        assert!(
            table_view(&hand, 1).card(7).unwrap().face_visible,
            "a card shown in its hand stays visible to the table until it moves"
        );
    }

    #[test]
    fn every_seat_carries_a_seat_counter_at_its_start_value() {
        let state = counter_state(CounterScope::Seat, ZoneVisibility::All);
        let view = table_view(&state, 0);
        assert_eq!(view.counter_table.len(), 1);
        assert_eq!(view.counters.len(), 2);
        assert_eq!(view.counter(CounterTarget::Seat(0), 1), Some(20));
        assert_eq!(view.counter(CounterTarget::Seat(1), 1), Some(20));
    }

    #[test]
    fn a_card_counter_follows_the_cards_own_visibility() {
        let mut state = counter_state(CounterScope::Card, ZoneVisibility::Owner);
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Counter {
                target: CounterTarget::Card(7),
                counter: 1,
                delta: 2,
            },
        );
        fold_entry(&mut state, &entry).expect("the counter folds");

        let owner = table_view(&state, 0);
        assert_eq!(owner.counter(CounterTarget::Card(7), 1), Some(22));

        let stranger = table_view(&state, 1);
        assert_eq!(stranger.counter(CounterTarget::Card(7), 1), None);
    }

    #[test]
    fn a_counter_change_shows_up_as_its_own_delta() {
        let before = table_view(&counter_state(CounterScope::Seat, ZoneVisibility::All), 0);
        let mut state = counter_state(CounterScope::Seat, ZoneVisibility::All);
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: -3,
            },
        );
        fold_entry(&mut state, &entry).expect("the counter folds");
        let after = table_view(&state, 0);

        let deltas = diff_views(&before, &after);
        assert!(deltas
            .iter()
            .any(|delta| matches!(delta, ViewDelta::Counters(_))));

        let mut replayed = before.clone();
        apply_deltas(&mut replayed, &deltas);
        assert_eq!(replayed, after);
    }

    fn scripted_state() -> LogState {
        let mut state = LogState::new();
        let zones = vec![ZoneDecl {
            id: 0,
            name: "deck".into(),
            kind: ZoneKind::Deck,
            owner: ZoneOwner::PerSeat,
            visibility: ZoneVisibility::None,
            layout: ZoneLayout::Pile,
            place: ZonePlace::Outer,
            span: 1,
            label: "Deck".into(),
        }];
        let entries = vec![
            LogEntry::new(
                0,
                0,
                LogAction::Genesis {
                    name: "rae".into(),
                    config: TableConfig {
                        engine: None,
                        plugin: None,
                        zones,
                        options: None,
                        counters: Vec::new(),
                        despawn_any: false,
                    },
                },
            ),
            LogEntry::new(1, 1, LogAction::Join { name: "ada".into() }),
            LogEntry::new(
                2,
                0,
                LogAction::Deal {
                    cards: vec![0, 1],
                    to: Zone::Hand,
                },
            ),
            LogEntry::new(
                3,
                1,
                LogAction::Deal {
                    cards: vec![2],
                    to: Zone::Plugin(0),
                },
            ),
            LogEntry::new(
                4,
                0,
                LogAction::Reveal {
                    card: 0,
                    face: CardFace::named("played"),
                },
            ),
            LogEntry::new(
                5,
                0,
                LogAction::Move {
                    card: 0,
                    to: Zone::Board,
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            ),
            LogEntry::new(
                6,
                0,
                LogAction::Annotate {
                    card: 0,
                    key: "exhausted".into(),
                    value: Some(ByteBuf::from(vec![0xf5])),
                },
            ),
        ];
        for entry in &entries {
            fold_entry(&mut state, entry).unwrap();
        }
        state
    }

    #[test]
    fn the_view_matches_render_table_for_every_seat() {
        let state = scripted_state();
        let mut faces = BTreeMap::new();
        faces.insert(0u32, CardFace::named("played"));
        faces.insert(1u32, CardFace::named("held"));
        faces.insert(2u32, CardFace::named("decked"));
        for viewer in 0u8..3 {
            let view = table_view(&state, viewer);
            let rebuilt = view_to_table(&view, &faces);
            let rendered = render_table(&state, &faces, viewer);
            assert_eq!(rebuilt.cards(), rendered.cards());
        }
    }

    #[test]
    fn a_hidden_zone_face_is_never_marked_visible() {
        let state = scripted_state();
        for viewer in 0u8..3 {
            let view = table_view(&state, viewer);
            let decked = view.card(2).unwrap();
            assert!(!decked.face_visible);
        }
    }

    #[test]
    fn a_peeked_face_is_visible_to_the_peeking_seat_alone() {
        let mut state = scripted_state();
        state.peeks.insert((2, 0));
        let mut faces = BTreeMap::new();
        faces.insert(0u32, CardFace::named("played"));
        faces.insert(1u32, CardFace::named("held"));
        faces.insert(2u32, CardFace::named("decked"));
        for viewer in 0u8..3 {
            let view = table_view(&state, viewer);
            assert_eq!(view.card(2).unwrap().face_visible, viewer == 0);
            assert_eq!(view.peeked, if viewer == 0 { vec![2] } else { Vec::new() });
            let mut mirror = TableView::default();
            let deltas = diff_views(&mirror, &view);
            apply_deltas(&mut mirror, &deltas);
            assert_eq!(mirror, view);
            let rebuilt = view_to_table(&view, &faces);
            assert_eq!(
                rebuilt.get(CardId(2)).unwrap().face.name,
                if viewer == 0 { "decked" } else { "" }
            );
            assert_eq!(
                rebuilt.cards(),
                render_table(&state, &faces, viewer).cards()
            );
        }
    }

    #[test]
    fn annotations_surface_as_badges() {
        let state = scripted_state();
        let view = table_view(&state, 1);
        let played = view.card(0).unwrap();
        assert_eq!(played.badge("exhausted"), Some(&[0xf5][..]));
        assert_eq!(played.badges.len(), 1);
        assert!(played.face_visible);
        assert!(view.card(1).unwrap().badge("exhausted").is_none());
    }

    #[test]
    fn deltas_rebuild_the_view() {
        let state = scripted_state();
        let full = table_view(&state, 1);
        let mut mirror = TableView::default();
        let deltas = diff_views(&mirror, &full);
        apply_deltas(&mut mirror, &deltas);
        assert_eq!(mirror, full);
        assert!(diff_views(&mirror, &full).is_empty());
    }
}

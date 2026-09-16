use serde::{Deserialize, Serialize};

pub mod rng;

pub use rng::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Zone {
    Hand,
    Board,
    Plugin(u16),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardFace {
    pub name: String,
    pub tint: [u8; 3],
    pub foil: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub might: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domain: Vec<String>,
}

impl CardFace {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tint: [128; 3],
            foil: false,
            kind: None,
            energy: None,
            power: None,
            might: None,
            domain: Vec::new(),
        }
    }

    pub fn with_domain(mut self, domain: Vec<String>) -> Self {
        self.domain = domain;
        self
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn with_cost(mut self, energy: Option<u8>, power: Option<u8>) -> Self {
        self.energy = energy;
        self.power = power;
        self
    }

    pub fn with_might(mut self, might: Option<u8>) -> Self {
        self.might = might;
        self
    }

    pub fn hidden() -> Self {
        Self::default()
    }

    pub fn is_hidden(&self) -> bool {
        self.name.is_empty()
    }
}

impl From<&CardFace> for CardFace {
    fn from(face: &CardFace) -> Self {
        face.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub owner: PlayerId,
    pub seat: PlayerId,
    pub zone: Zone,
    pub face: CardFace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    cards: Vec<Card>,
    next_id: u32,
}

impl Table {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn add(
        &mut self,
        owner: PlayerId,
        zone: Zone,
        name: impl Into<String>,
        tint: [u8; 3],
    ) -> CardId {
        self.add_face(
            owner,
            zone,
            CardFace {
                name: name.into(),
                tint,
                foil: false,
                kind: None,
                energy: None,
                power: None,
                might: None,
                domain: Vec::new(),
            },
        )
    }

    pub fn add_face(&mut self, owner: PlayerId, zone: Zone, face: CardFace) -> CardId {
        let id = CardId(self.next_id);
        self.next_id += 1;
        self.cards.push(Card {
            id,
            owner,
            seat: owner,
            zone,
            face,
        });
        id
    }

    pub fn insert_card(&mut self, card: Card) -> bool {
        if self.cards.iter().any(|c| c.id == card.id) {
            return false;
        }
        self.next_id = self.next_id.max(card.id.0 + 1);
        self.cards.push(card);
        true
    }

    pub fn get(&self, id: CardId) -> Option<&Card> {
        self.cards.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: CardId) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.id == id)
    }

    pub fn faces_mut(&mut self) -> impl Iterator<Item = (CardId, &mut CardFace)> {
        self.cards.iter_mut().map(|c| (c.id, &mut c.face))
    }

    pub fn retain(&mut self, keep: impl FnMut(&Card) -> bool) {
        self.cards.retain(keep);
    }

    pub fn clear(&mut self) {
        self.cards.clear();
    }

    pub fn in_area(&self, seat: PlayerId, zone: Zone) -> impl Iterator<Item = &Card> {
        self.cards
            .iter()
            .filter(move |c| c.seat == seat && c.zone == zone)
    }

    pub fn apply(&mut self, intent: Intent) -> bool {
        match intent {
            Intent::MoveCard {
                card,
                to,
                seat,
                index,
            } => {
                if !self.changes(intent) {
                    return false;
                }
                let Some(from) = self.cards.iter().position(|c| c.id == card) else {
                    return false;
                };
                let mut moved = self.cards.remove(from);
                moved.zone = to;
                moved.seat = seat;
                let at = self.insertion_point(seat, to, index);
                self.cards.insert(at, moved);
                true
            }
        }
    }

    pub fn changes(&self, intent: Intent) -> bool {
        match intent {
            Intent::MoveCard {
                card,
                to,
                seat,
                index,
            } => {
                let Some(from) = self.cards.iter().position(|c| c.id == card) else {
                    return false;
                };
                let moved = &self.cards[from];
                if moved.zone != to || moved.seat != seat {
                    return true;
                }
                self.insertion_point_without(from, seat, to, index) != from
            }
        }
    }

    fn insertion_point(&self, seat: PlayerId, zone: Zone, index: usize) -> usize {
        let mut ordinal = 0;
        for (i, card) in self.cards.iter().enumerate() {
            if card.seat == seat && card.zone == zone {
                if ordinal == index {
                    return i;
                }
                ordinal += 1;
            }
        }
        self.cards.len()
    }

    fn insertion_point_without(
        &self,
        without: usize,
        seat: PlayerId,
        zone: Zone,
        index: usize,
    ) -> usize {
        let mut ordinal = 0;
        for (i, card) in self.cards.iter().enumerate() {
            if i == without {
                continue;
            }
            if card.seat == seat && card.zone == zone {
                if ordinal == index {
                    return if i > without { i - 1 } else { i };
                }
                ordinal += 1;
            }
        }
        self.cards.len() - 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    MoveCard {
        card: CardId,
        to: Zone,
        seat: PlayerId,
        index: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: PlayerId = PlayerId(0);
    const THEM: PlayerId = PlayerId(2);

    #[test]
    fn spirit_surface_is_reachable() {
        #[allow(unused_imports)]
        use spirit_sdk::{spirit_core, spirit_index, spirit_routing, spirit_schema};
    }

    fn table_of(zones: &[Zone]) -> (Table, Vec<CardId>) {
        let mut t = Table::new();
        let ids = zones
            .iter()
            .enumerate()
            .map(|(i, &z)| t.add(ME, z, format!("card {i}"), [128; 3]))
            .collect();
        (t, ids)
    }

    fn order(t: &Table, seat: PlayerId, zone: Zone) -> Vec<CardId> {
        t.in_area(seat, zone).map(|c| c.id).collect()
    }

    #[test]
    fn moving_a_card_changes_its_zone() {
        let (mut t, ids) = table_of(&[Zone::Hand]);
        assert!(t.apply(Intent::MoveCard {
            card: ids[0],
            to: Zone::Board,
            seat: ME,
            index: 0
        }));
        assert_eq!(t.get(ids[0]).unwrap().zone, Zone::Board);
    }

    #[test]
    fn moving_a_card_to_its_current_position_is_not_a_change() {
        let (mut t, ids) = table_of(&[Zone::Hand, Zone::Hand]);
        assert!(!t.apply(Intent::MoveCard {
            card: ids[0],
            to: Zone::Hand,
            seat: ME,
            index: 0
        }));
        assert_eq!(order(&t, ME, Zone::Hand), ids);
    }

    #[test]
    fn reordering_within_a_zone() {
        let (mut t, ids) = table_of(&[Zone::Hand, Zone::Hand, Zone::Hand]);
        assert!(t.apply(Intent::MoveCard {
            card: ids[0],
            to: Zone::Hand,
            seat: ME,
            index: 2
        }));
        assert_eq!(order(&t, ME, Zone::Hand), vec![ids[1], ids[2], ids[0]]);
    }

    #[test]
    fn playing_to_another_seat_keeps_ownership() {
        let (mut t, ids) = table_of(&[Zone::Hand]);
        assert!(t.apply(Intent::MoveCard {
            card: ids[0],
            to: Zone::Board,
            seat: THEM,
            index: 0
        }));
        let card = t.get(ids[0]).unwrap();
        assert_eq!(card.owner, ME);
        assert_eq!(card.seat, THEM);
        assert_eq!(order(&t, THEM, Zone::Board), vec![ids[0]]);
        assert!(order(&t, ME, Zone::Board).is_empty());
    }

    #[test]
    fn an_index_past_the_end_appends() {
        let (mut t, ids) = table_of(&[Zone::Hand, Zone::Hand, Zone::Board]);
        assert!(t.apply(Intent::MoveCard {
            card: ids[2],
            to: Zone::Hand,
            seat: ME,
            index: 99
        }));
        assert_eq!(order(&t, ME, Zone::Hand), vec![ids[0], ids[1], ids[2]]);
    }

    #[test]
    fn inserting_a_card_with_an_explicit_id_keeps_fresh_ids_fresh() {
        let mut t = Table::new();
        let inserted = Card {
            id: CardId(5),
            owner: ME,
            seat: ME,
            zone: Zone::Hand,
            face: CardFace::named("wired"),
        };
        assert!(t.insert_card(inserted.clone()));
        assert!(!t.insert_card(inserted));
        let next = t.add(ME, Zone::Hand, "fresh", [128; 3]);
        assert_eq!(next, CardId(6));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn changes_predicts_apply_without_mutating() {
        let (t, ids) = table_of(&[Zone::Hand, Zone::Hand, Zone::Board, Zone::Board]);
        for &card in &ids {
            for to in [Zone::Hand, Zone::Board] {
                for seat in [ME, THEM] {
                    for index in 0..5 {
                        let intent = Intent::MoveCard {
                            card,
                            to,
                            seat,
                            index,
                        };
                        let mut mutated = t.clone();
                        assert_eq!(t.changes(intent), mutated.apply(intent));
                        if !t.changes(intent) {
                            assert_eq!(mutated, t);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn ids_stay_stable_across_moves() {
        let (mut t, ids) = table_of(&[Zone::Hand, Zone::Hand]);
        t.apply(Intent::MoveCard {
            card: ids[0],
            to: Zone::Board,
            seat: ME,
            index: 0,
        });
        let back = t.add(ME, Zone::Hand, "new", [128; 3]);
        assert_ne!(back, ids[0]);
        assert_ne!(back, ids[1]);
    }

    #[test]
    fn a_hidden_face_is_the_default_and_carries_no_name() {
        assert!(CardFace::hidden().is_hidden());
        assert_eq!(CardFace::hidden(), CardFace::default());
        assert!(!CardFace::named("Emberwing Scout").is_hidden());
    }
}

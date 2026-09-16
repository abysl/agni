use crate::{total, DeckEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCard {
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub signature: bool,
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotZone {
    pub cards: Vec<SnapshotCard>,
    pub zone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub game: String,
    pub zones: Vec<SnapshotZone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityCard {
    pub count: u32,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityZone {
    pub cards: Vec<IdentityCard>,
    pub zone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckIdentity {
    pub game: String,
    pub zones: Vec<IdentityZone>,
}

impl SnapshotZone {
    pub fn new(zone: impl Into<String>, cards: Vec<SnapshotCard>) -> Self {
        Self {
            cards,
            zone: zone.into(),
        }
    }

    pub fn from_entries<C>(
        zone: impl Into<String>,
        entries: &[DeckEntry<C>],
        key: impl Fn(&C) -> String,
        name: impl Fn(&C) -> String,
        image_url: impl Fn(&C) -> Option<String>,
    ) -> Self {
        Self::new(
            zone,
            entries
                .iter()
                .map(|entry| SnapshotCard {
                    count: entry.count,
                    image_url: image_url(&entry.card),
                    kind: None,
                    key: key(&entry.card),
                    name: name(&entry.card),
                    energy: None,
                    power: None,
                    might: None,
                    domain: Vec::new(),
                    tags: Vec::new(),
                    signature: false,
                })
                .collect(),
        )
    }

    pub fn total(&self) -> u32 {
        self.cards.iter().map(|card| card.count).sum()
    }
}

impl Snapshot {
    pub fn new(game: impl Into<String>, zones: Vec<SnapshotZone>) -> Self {
        Self {
            game: game.into(),
            zones,
        }
    }

    pub fn total(&self) -> u32 {
        self.zones.iter().map(SnapshotZone::total).sum()
    }

    pub fn zone(&self, zone: &str) -> Option<&SnapshotZone> {
        self.zones.iter().find(|held| held.zone == zone)
    }

    pub fn identity(&self) -> DeckIdentity {
        let mut zones: Vec<IdentityZone> = self
            .zones
            .iter()
            .filter(|zone| !zone.cards.is_empty())
            .map(|zone| {
                let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
                for card in &zone.cards {
                    *counts.entry(card.key.as_str()).or_default() += card.count;
                }
                IdentityZone {
                    cards: counts
                        .into_iter()
                        .filter(|(_, count)| *count > 0)
                        .map(|(key, count)| IdentityCard {
                            count,
                            key: key.to_string(),
                        })
                        .collect(),
                    zone: zone.zone.clone(),
                }
            })
            .filter(|zone| !zone.cards.is_empty())
            .collect();
        zones.sort_by(|left, right| left.zone.cmp(&right.zone));
        DeckIdentity {
            game: self.game.clone(),
            zones,
        }
    }
}

pub fn entries_total<C>(entries: &[DeckEntry<C>]) -> u32 {
    total(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(key: &str, count: u32) -> SnapshotCard {
        SnapshotCard {
            count,
            image_url: Some(format!("https://art.example/{key}.png")),
            kind: None,
            energy: None,
            power: None,
            might: None,
            key: key.into(),
            name: format!("Card {key}"),
            domain: Vec::new(),
            tags: Vec::new(),
            signature: false,
        }
    }

    fn deck(zones: Vec<SnapshotZone>) -> Snapshot {
        Snapshot::new("riftbound", zones)
    }

    #[test]
    fn card_order_does_not_change_the_identity() {
        let straight = deck(vec![SnapshotZone::new(
            "main",
            vec![card("a", 2), card("b", 1)],
        )]);
        let shuffled = deck(vec![SnapshotZone::new(
            "main",
            vec![card("b", 1), card("a", 2)],
        )]);
        assert_eq!(straight.identity(), shuffled.identity());
    }

    #[test]
    fn zone_order_does_not_change_the_identity() {
        let straight = deck(vec![
            SnapshotZone::new("main", vec![card("a", 1)]),
            SnapshotZone::new("sideboard", vec![card("b", 1)]),
        ]);
        let swapped = deck(vec![
            SnapshotZone::new("sideboard", vec![card("b", 1)]),
            SnapshotZone::new("main", vec![card("a", 1)]),
        ]);
        assert_eq!(straight.identity(), swapped.identity());
    }

    #[test]
    fn a_split_entry_folds_into_one_count() {
        let split = deck(vec![SnapshotZone::new(
            "main",
            vec![card("a", 1), card("a", 2)],
        )]);
        let merged = deck(vec![SnapshotZone::new("main", vec![card("a", 3)])]);
        assert_eq!(split.identity(), merged.identity());
        assert_eq!(split.total(), 3);
    }

    #[test]
    fn tags_and_signature_are_not_identity_and_stay_optional_on_the_wire() {
        let mine = deck(vec![SnapshotZone::new("main", vec![card("a", 1)])]);
        let mut tagged = mine.clone();
        tagged.zones[0].cards[0].tags = vec!["Lillia".into()];
        tagged.zones[0].cards[0].signature = true;
        assert_eq!(mine.identity(), tagged.identity());
        assert_ne!(mine, tagged);
        let bare = serde_json::to_value(&mine).unwrap();
        let card = &bare["zones"][0]["cards"][0];
        assert!(card.get("tags").is_none());
        assert!(card.get("signature").is_none());
        let rich = serde_json::to_value(&tagged).unwrap();
        assert_eq!(rich["zones"][0]["cards"][0]["tags"][0], "Lillia");
        assert_eq!(rich["zones"][0]["cards"][0]["signature"], true);
        let before_the_fields = serde_json::json!({
            "game": "riftbound",
            "zones": [{"zone": "main", "cards": [{"count": 1, "key": "a", "name": "Card a"}]}]
        });
        let decoded: Snapshot = serde_json::from_value(before_the_fields).unwrap();
        assert!(decoded.zones[0].cards[0].tags.is_empty());
        assert!(!decoded.zones[0].cards[0].signature);
        assert_eq!(decoded.identity(), mine.identity());
    }

    #[test]
    fn art_and_display_names_are_not_identity() {
        let mine = deck(vec![SnapshotZone::new("main", vec![card("a", 1)])]);
        let mut theirs = mine.clone();
        theirs.zones[0].cards[0].image_url = None;
        theirs.zones[0].cards[0].name = "Anything Else".into();
        assert_eq!(mine.identity(), theirs.identity());
        assert_ne!(mine, theirs);
    }

    #[test]
    fn an_empty_zone_is_the_same_as_an_absent_one() {
        let bare = deck(vec![SnapshotZone::new("main", vec![card("a", 1)])]);
        let padded = deck(vec![
            SnapshotZone::new("main", vec![card("a", 1)]),
            SnapshotZone::new("sideboard", Vec::new()),
        ]);
        assert_eq!(bare.identity(), padded.identity());
    }

    #[test]
    fn a_changed_count_or_card_is_a_different_deck() {
        let base = deck(vec![SnapshotZone::new("main", vec![card("a", 2)])]);
        let recount = deck(vec![SnapshotZone::new("main", vec![card("a", 3)])]);
        let swapped = deck(vec![SnapshotZone::new("main", vec![card("z", 2)])]);
        let sided = deck(vec![
            SnapshotZone::new("main", vec![card("a", 2)]),
            SnapshotZone::new("sideboard", vec![card("b", 1)]),
        ]);
        assert_ne!(base.identity(), recount.identity());
        assert_ne!(base.identity(), swapped.identity());
        assert_ne!(base.identity(), sided.identity());
    }

    #[test]
    fn the_same_list_in_another_game_is_another_deck() {
        let riftbound = deck(vec![SnapshotZone::new("main", vec![card("a", 1)])]);
        let mtg = Snapshot::new("mtg", vec![SnapshotZone::new("main", vec![card("a", 1)])]);
        assert_ne!(riftbound.identity(), mtg.identity());
    }

    #[test]
    fn zones_read_back_by_name() {
        let built = deck(vec![
            SnapshotZone::new("legend", vec![card("l", 1)]),
            SnapshotZone::new("main", vec![card("a", 40)]),
        ]);
        assert_eq!(built.zone("legend").unwrap().total(), 1);
        assert_eq!(built.zone("main").unwrap().total(), 40);
        assert!(built.zone("runes").is_none());
        assert_eq!(built.total(), 41);
    }
}

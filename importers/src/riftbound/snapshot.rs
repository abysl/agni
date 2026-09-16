use agni_deck::{Snapshot, SnapshotCard, SnapshotZone};
use agni_riftbound::{DeckEntry, ResolvedCard, ResolvedDeck, GAME};

pub const LEGEND: &str = "legend";
pub const CHAMPION: &str = "champion";
pub const MAIN: &str = "main";
pub const RUNES: &str = "runes";
pub const BATTLEFIELDS: &str = "battlefields";
pub const SIDEBOARD: &str = "sideboard";

fn snapshot_card(card: &ResolvedCard, count: u32) -> SnapshotCard {
    SnapshotCard {
        count,
        image_url: card.image_url.clone(),
        kind: card.kind.clone(),
        key: card.riftbound_id.clone(),
        name: card.name.clone(),
        energy: card.energy,
        power: card.power,
        might: card.might,
        domain: card.domain.clone(),
        tags: card.tags.clone(),
        signature: card.signature,
    }
}

fn resolved_card(card: &SnapshotCard) -> ResolvedCard {
    ResolvedCard {
        name: card.name.clone(),
        riftbound_id: card.key.clone(),
        image_url: card.image_url.clone(),
        kind: card.kind.clone(),
        energy: card.energy,
        power: card.power,
        might: card.might,
        domain: card.domain.clone(),
        tags: card.tags.clone(),
        signature: card.signature,
    }
}

fn zone(name: &str, entries: &[DeckEntry]) -> SnapshotZone {
    SnapshotZone::new(
        name,
        entries
            .iter()
            .map(|entry| snapshot_card(&entry.card, entry.count))
            .collect(),
    )
}

fn single(name: &str, card: Option<&ResolvedCard>) -> SnapshotZone {
    SnapshotZone::new(
        name,
        card.map(|card| snapshot_card(card, 1))
            .into_iter()
            .collect(),
    )
}

pub fn snapshot(deck: &ResolvedDeck) -> Snapshot {
    Snapshot::new(
        GAME,
        vec![
            single(LEGEND, deck.legend.as_ref()),
            single(CHAMPION, deck.chosen_champion.as_ref()),
            zone(MAIN, &deck.main_deck),
            zone(RUNES, &deck.runes),
            zone(BATTLEFIELDS, &deck.battlefields),
            zone(SIDEBOARD, &deck.sideboard),
        ],
    )
}

fn entries(snapshot: &Snapshot, name: &str) -> Vec<DeckEntry> {
    snapshot
        .zone(name)
        .map(|zone| {
            zone.cards
                .iter()
                .map(|card| DeckEntry {
                    card: resolved_card(card),
                    count: card.count,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first(snapshot: &Snapshot, name: &str) -> Option<ResolvedCard> {
    entries(snapshot, name)
        .into_iter()
        .next()
        .map(|entry| entry.card)
}

pub fn deck(snapshot: &Snapshot) -> Option<ResolvedDeck> {
    if snapshot.game != GAME {
        return None;
    }
    Some(ResolvedDeck {
        legend: first(snapshot, LEGEND),
        chosen_champion: first(snapshot, CHAMPION),
        main_deck: entries(snapshot, MAIN),
        runes: entries(snapshot, RUNES),
        battlefields: entries(snapshot, BATTLEFIELDS),
        sideboard: entries(snapshot, SIDEBOARD),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, id: &str, tags: &[&str], signature: bool) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            image_url: Some(format!("https://img.example/{id}.png")),
            kind: Some("Unit".into()),
            energy: Some(3),
            power: Some(1),
            might: Some(3),
            domain: vec!["Mind".into()],
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            signature,
        }
    }

    fn sample() -> ResolvedDeck {
        ResolvedDeck {
            legend: Some(card(
                "Lillia - Bashful Bloom",
                "unl-189-219",
                &["Lillia"],
                false,
            )),
            chosen_champion: Some(card(
                "Lillia - Fae Fawn",
                "unl-082-219",
                &["Fae", "Lillia", "Ionia"],
                false,
            )),
            main_deck: vec![
                DeckEntry {
                    card: card(
                        "Lillia - Fae Fawn",
                        "unl-082-219",
                        &["Fae", "Lillia"],
                        false,
                    ),
                    count: 2,
                },
                DeckEntry {
                    card: card("Daisy!", "unl-196-219", &["Ivern", "Ionia"], true),
                    count: 1,
                },
            ],
            runes: vec![DeckEntry {
                card: card("Mind Rune", "ogn-089-298", &[], false),
                count: 12,
            }],
            battlefields: vec![DeckEntry {
                card: card("Seat of Power", "sfd-217-221", &[], false),
                count: 1,
            }],
            sideboard: vec![DeckEntry {
                card: card("Pickpocket", "ogn-100-298", &[], false),
                count: 1,
            }],
        }
    }

    #[test]
    fn a_deck_round_trips_through_its_snapshot_with_every_field() {
        let deck = sample();
        let snapshot = snapshot(&deck);
        assert_eq!(snapshot.game, GAME);
        assert_eq!(snapshot.zone(LEGEND).unwrap().total(), 1);
        assert_eq!(snapshot.zone(MAIN).unwrap().total(), 3);
        assert_eq!(snapshot.zone(SIDEBOARD).unwrap().total(), 1);
        let main = &snapshot.zone(MAIN).unwrap().cards;
        assert_eq!(main[0].tags, vec!["Fae".to_string(), "Lillia".to_string()]);
        assert!(main[1].signature);
        assert_eq!(super::deck(&snapshot), Some(deck));
    }

    #[test]
    fn an_empty_deck_keeps_its_six_zones_and_another_game_is_refused() {
        let snapshot = snapshot(&ResolvedDeck::default());
        assert_eq!(snapshot.zones.len(), 6);
        assert_eq!(snapshot.total(), 0);
        assert_eq!(super::deck(&snapshot), Some(ResolvedDeck::default()));
        let mtg = Snapshot::new("mtg", Vec::new());
        assert_eq!(super::deck(&mtg), None);
    }

    #[test]
    fn a_snapshot_saved_before_tags_decodes_with_empty_tags_and_the_same_identity() {
        let deck = sample();
        let mut stripped = deck.clone();
        for entry in stripped.main_deck.iter_mut() {
            entry.card.tags.clear();
            entry.card.signature = false;
        }
        assert_eq!(snapshot(&stripped).identity(), snapshot(&deck).identity());
        let mut encoded = Vec::new();
        ciborium::into_writer(&snapshot(&stripped), &mut encoded).unwrap();
        let decoded: Snapshot = ciborium::from_reader(encoded.as_slice()).unwrap();
        assert_eq!(super::deck(&decoded), Some(stripped));
    }
}

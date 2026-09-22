use super::{Finding, Grade, Mode, Rule, Zone};
use crate::{DeckEntry, ResolvedCard, ResolvedDeck};
use std::collections::BTreeSet;

const SOURCE: &str = "https://playriftbound.com/en-us/news/announcements/september-ban-list-updates-effective-september-18-2026";
const EFFECTIVE: &str = "2026-09-18";
const BANS: &[(&str, &str)] = &[
    ("Ekko, Recurrent", "ogn-110-298"),
    ("Stacked Deck", "ogn-183-298"),
];

fn name_key(name: &str) -> String {
    name.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_key(id: &str) -> String {
    id.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn matches(card: &ResolvedCard, name: &str, reference: &str) -> bool {
    name_key(&card.name) == name_key(name) || print_key(&card.riftbound_id) == print_key(reference)
}

fn present(entries: &[DeckEntry]) -> Vec<&ResolvedCard> {
    entries
        .iter()
        .filter(|entry| entry.count > 0)
        .map(|entry| &entry.card)
        .collect()
}

fn zones(deck: &ResolvedDeck) -> [(Zone, Vec<&ResolvedCard>); 6] {
    [
        (Zone::Legend, deck.legend.iter().collect()),
        (Zone::Champion, deck.chosen_champion.iter().collect()),
        (Zone::Main, present(&deck.main_deck)),
        (Zone::Runes, present(&deck.runes)),
        (Zone::Battlefields, present(&deck.battlefields)),
        (Zone::Sideboard, present(&deck.sideboard)),
    ]
}

pub(super) fn findings(deck: &ResolvedDeck, mode: Mode) -> Vec<Finding> {
    let format = match mode {
        Mode::Standard => "Standard",
        Mode::Constructed2v2 => "Constructed 2v2",
    };
    let mut findings = Vec::new();
    for (zone, cards) in zones(deck) {
        for &(name, reference) in BANS {
            let ids: BTreeSet<String> = cards
                .iter()
                .filter(|card| matches(card, name, reference))
                .map(|card| card.riftbound_id.clone())
                .collect();
            if ids.is_empty() {
                continue;
            }
            findings.push(Finding {
                rule: Rule::Banned { name: name.into() },
                cite: SOURCE,
                grade: Grade::Break,
                zone,
                cards: ids.into_iter().collect(),
                detail: format!("{name} is banned in {format} (effective {EFFECTIVE})"),
            });
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, id: &str) -> ResolvedCard {
        ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            ..Default::default()
        }
    }

    fn entry(name: &str, id: &str, count: u32) -> DeckEntry {
        DeckEntry {
            card: card(name, id),
            count,
        }
    }

    #[test]
    fn both_formats_reject_each_named_card_with_a_dated_source() {
        for mode in [Mode::Standard, Mode::Constructed2v2] {
            for &(name, id) in BANS {
                let deck = ResolvedDeck {
                    main_deck: vec![entry(name, id, 3)],
                    ..Default::default()
                };
                let report = findings(&deck, mode);
                assert_eq!(report.len(), 1);
                assert_eq!(report[0].rule, Rule::Banned { name: name.into() });
                assert_eq!(report[0].grade, Grade::Break);
                assert_eq!(report[0].zone, Zone::Main);
                assert_eq!(report[0].cards, [id]);
                assert_eq!(report[0].cite, SOURCE);
                assert!(report[0].detail.contains(EFFECTIVE));
            }
        }
    }

    #[test]
    fn alternate_prints_and_punctuation_share_the_named_card_ban() {
        let deck = ResolvedDeck {
            main_deck: vec![
                entry("Ekko - Recurrent", "ogn-110a-298", 1),
                entry("  EKKO — RECURRENT  ", "alternate-ekko", 1),
                entry("Stacked  Deck", "ogn-183a-298", 1),
                entry("stacked deck", "alternate-stacked", 1),
            ],
            ..Default::default()
        };
        let report = findings(&deck, Mode::Standard);
        assert_eq!(report.len(), 2);
        assert_eq!(report[0].cards, ["alternate-ekko", "ogn-110a-298"]);
        assert_eq!(report[1].cards, ["alternate-stacked", "ogn-183a-298"]);
    }

    #[test]
    fn reference_ids_cannot_bypass_the_ban_with_missing_or_different_names() {
        for (name, id) in [("", "OGN-110/298"), ("unresolved", "OGN-183-298")] {
            let deck = ResolvedDeck {
                main_deck: vec![entry(name, id, 1)],
                ..Default::default()
            };
            assert_eq!(findings(&deck, Mode::Standard).len(), 1);
        }
    }

    #[test]
    fn another_ekko_or_a_partial_name_is_not_banned() {
        for name in [
            "Ekko",
            "Ekko - Another Title",
            "Ekko - Recurrent Echo",
            "Stacked",
            "Stacked Deckhand",
        ] {
            let deck = ResolvedDeck {
                main_deck: vec![entry(name, "unrelated-print", 1)],
                ..Default::default()
            };
            assert!(findings(&deck, Mode::Standard).is_empty(), "{name}");
        }
    }

    #[test]
    fn all_registered_zones_are_checked_and_zero_count_rows_are_absent() {
        let banned = card("Ekko, Recurrent", "ogn-110-298");
        let entries = vec![entry("Stacked Deck", "ogn-183-298", 1)];
        let deck = ResolvedDeck {
            legend: Some(banned.clone()),
            chosen_champion: Some(banned),
            main_deck: entries.clone(),
            runes: entries.clone(),
            battlefields: entries.clone(),
            sideboard: entries,
        };
        let report = findings(&deck, Mode::Standard);
        let found: Vec<Zone> = report.iter().map(|finding| finding.zone).collect();
        assert_eq!(
            found,
            [
                Zone::Legend,
                Zone::Champion,
                Zone::Main,
                Zone::Runes,
                Zone::Battlefields,
                Zone::Sideboard
            ]
        );
        let zero = ResolvedDeck {
            main_deck: vec![entry("Ekko, Recurrent", "ogn-110-298", 0)],
            sideboard: vec![entry("Stacked Deck", "ogn-183-298", 0)],
            ..Default::default()
        };
        assert!(findings(&zero, Mode::Standard).is_empty());
    }

    #[test]
    fn moving_a_banned_card_to_the_sideboard_does_not_make_it_legal() {
        let mut deck = ResolvedDeck {
            main_deck: vec![entry("Stacked Deck", "ogn-183-298", 1)],
            ..Default::default()
        };
        assert_eq!(findings(&deck, Mode::Standard)[0].zone, Zone::Main);
        deck.sideboard = std::mem::take(&mut deck.main_deck);
        assert_eq!(findings(&deck, Mode::Standard)[0].zone, Zone::Sideboard);
    }
}

use super::catalog::{CardLookup, LookupError};
use super::{Game, ParsedDeck};

pub const NO_MATCH: &str = "no card in the catalog matches";
pub const LOOKUP_FAILURE_FUSE: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub identifier: String,
    pub reason: String,
}

impl Unresolved {
    pub fn lookup_failed(&self) -> bool {
        self.reason != NO_MATCH
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolution<G: Game> {
    pub deck: G::Deck,
    pub unresolved: Vec<Unresolved>,
}

impl<G: Game> Resolution<G> {
    pub fn first_failure(&self) -> Option<&Unresolved> {
        self.unresolved.iter().find(|entry| entry.lookup_failed())
    }
}

pub fn resolve<G: Game>(
    parsed: &ParsedDeck<G>,
    cards: &mut dyn CardLookup<G>,
) -> Result<Resolution<G>, LookupError> {
    let mut resolution = Resolution::default();
    let mut failures: Vec<LookupError> = Vec::new();
    for entry in &parsed.entries {
        let identifier = G::describe(&entry.identifier);
        if let Some(last) = failures.get(LOOKUP_FAILURE_FUSE - 1) {
            resolution.unresolved.push(Unresolved {
                identifier,
                reason: format!(
                    "not looked up: {} lookups in a row failed, the last with: {last}",
                    failures.len()
                ),
            });
            continue;
        }
        match cards.find(&entry.identifier) {
            Ok(Some(card)) => {
                failures.clear();
                G::place(&mut resolution.deck, entry.section, card, entry.count);
            }
            Ok(None) => {
                failures.clear();
                resolution.unresolved.push(Unresolved {
                    identifier,
                    reason: NO_MATCH.into(),
                });
            }
            Err(error) => {
                resolution.unresolved.push(Unresolved {
                    identifier,
                    reason: error.to_string(),
                });
                failures.push(error);
            }
        }
    }
    Ok(resolution)
}

#[cfg(test)]
mod tests {
    use super::super::testing::{card, Card, Section, TestGame};
    use super::super::{parse_text, ParsedEntry};
    use super::*;

    struct Fixture;

    impl CardLookup<TestGame> for Fixture {
        fn find(&mut self, name: &String) -> Result<Option<Card>, LookupError> {
            Ok(match name.as_str() {
                "Scout" => Some(card("Scout", true)),
                "Warden" => Some(card("Warden", false)),
                "Broken" => return Err(LookupError("the wire fell over".into())),
                _ => None,
            })
        }
    }

    #[test]
    fn sections_land_in_their_zones_and_repeats_merge() {
        let parsed =
            parse_text::<TestGame>("3 Scout\nSideboard\n2 Warden\nDeck\n1 Scout\n").unwrap();
        let resolution = resolve(&parsed, &mut Fixture).unwrap();
        assert!(resolution.unresolved.is_empty());
        assert_eq!(resolution.deck.main.len(), 1);
        assert_eq!(resolution.deck.main[0].count, 4);
        assert_eq!(resolution.deck.side.len(), 1);
        assert_eq!(resolution.deck.side[0].card.name, "Warden");
        assert_eq!(resolution.deck.side[0].count, 2);
    }

    #[test]
    fn unknown_cards_land_in_unresolved_with_a_reason() {
        let parsed = ParsedDeck::<TestGame> {
            entries: vec![
                ParsedEntry {
                    identifier: "Completely Unknown".into(),
                    count: 2,
                    section: None,
                },
                ParsedEntry {
                    identifier: "Scout".into(),
                    count: 1,
                    section: Some(Section::Main),
                },
            ],
        };
        let resolution = resolve(&parsed, &mut Fixture).unwrap();
        assert_eq!(resolution.unresolved.len(), 1);
        assert_eq!(resolution.unresolved[0].identifier, "Completely Unknown");
        assert_eq!(resolution.unresolved[0].reason, NO_MATCH);
        assert!(!resolution.unresolved[0].lookup_failed());
        assert!(resolution.first_failure().is_none());
        assert_eq!(resolution.deck.main.len(), 1);
    }

    #[test]
    fn a_lookup_failure_names_the_card_and_the_rest_still_resolves() {
        let parsed = parse_text::<TestGame>("1 Scout\n1 Broken\n2 Warden\n").unwrap();
        let resolution = resolve(&parsed, &mut Fixture).unwrap();
        assert_eq!(resolution.deck.main.len(), 2);
        assert_eq!(resolution.unresolved.len(), 1);
        assert_eq!(resolution.unresolved[0].identifier, "Broken");
        assert_eq!(
            resolution.unresolved[0].reason,
            "card lookup failed: the wire fell over"
        );
        assert!(resolution.unresolved[0].lookup_failed());
        assert_eq!(resolution.first_failure().unwrap().identifier, "Broken");
    }

    #[test]
    fn repeated_lookup_failures_stop_asking_and_say_so() {
        let parsed =
            parse_text::<TestGame>("1 Broken\n1 Broken\n1 Broken\n1 Scout\n1 Nobody\n").unwrap();
        let resolution = resolve(&parsed, &mut Fixture).unwrap();
        assert!(resolution.deck.main.is_empty());
        assert_eq!(resolution.unresolved.len(), 5);
        for entry in &resolution.unresolved[..3] {
            assert_eq!(entry.reason, "card lookup failed: the wire fell over");
        }
        assert_eq!(resolution.unresolved[3].identifier, "Scout");
        assert_eq!(
            resolution.unresolved[3].reason,
            "not looked up: 3 lookups in a row failed, the last with: card lookup failed: the wire fell over"
        );
        assert_eq!(resolution.unresolved[4].identifier, "Nobody");
        assert!(resolution.unresolved[4].lookup_failed());
    }

    #[test]
    fn a_success_between_failures_resets_the_fuse() {
        let parsed =
            parse_text::<TestGame>("1 Broken\n1 Broken\n1 Scout\n1 Broken\n1 Broken\n1 Warden\n")
                .unwrap();
        let resolution = resolve(&parsed, &mut Fixture).unwrap();
        assert_eq!(resolution.deck.main.len(), 2);
        assert_eq!(resolution.unresolved.len(), 4);
        assert!(resolution
            .unresolved
            .iter()
            .all(|entry| entry.reason == "card lookup failed: the wire fell over"));
    }
}

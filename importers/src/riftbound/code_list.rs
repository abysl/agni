use super::card_code::CardCode;
use super::text_list::ordered;
use super::{Identifier, ParsedDeck, ParsedEntry};
use agni_riftbound::{DeckEntry, ResolvedCard, ResolvedDeck};
use std::fmt;

pub const NO_SIDEBOARD: &str = "the code list has no sideboard — copy the deck code instead";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeListError {
    Empty,
    BadToken(String),
    ZeroCount(String),
}

impl fmt::Display for CodeListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the code list is empty"),
            Self::BadToken(token) => {
                write!(f, "{token:?} is not a card code token like OGN-045-2")
            }
            Self::ZeroCount(token) => write!(f, "{token:?} asks for zero copies"),
        }
    }
}

impl std::error::Error for CodeListError {}

fn parse_token(token: &str) -> Result<(CardCode, u32), CodeListError> {
    let bad = || CodeListError::BadToken(token.into());
    let mut parts = token.split('-');
    let set = parts.next().ok_or_else(bad)?;
    let number = parts.next().ok_or_else(bad)?;
    let count = match parts.next() {
        None => 1,
        Some(count_text) if parts.next().is_none() => {
            let count: u32 = count_text.parse().map_err(|_| bad())?;
            if count == 0 {
                return Err(CodeListError::ZeroCount(token.into()));
            }
            count
        }
        Some(_) => return Err(bad()),
    };
    let code = CardCode::parse(&format!("{set}-{number}")).map_err(|_| bad())?;
    Ok((code, count))
}

pub fn parse_code_list(input: &str) -> Result<ParsedDeck, CodeListError> {
    let mut deck = ParsedDeck::default();
    for token in input.split_whitespace() {
        let (code, count) = parse_token(token)?;
        deck.entries.push(ParsedEntry {
            identifier: Identifier::Code(code),
            count,
            section: None,
        });
    }
    if deck.entries.is_empty() {
        return Err(CodeListError::Empty);
    }
    Ok(deck)
}

fn token(card: &ResolvedCard, count: u32) -> Result<String, String> {
    let code = CardCode::base_print_of_id(&card.riftbound_id).map_err(|error| error.to_string())?;
    if count == 1 {
        Ok(code.to_string())
    } else {
        Ok(format!("{code}-{count}"))
    }
}

fn tokens(out: &mut Vec<String>, entries: &[DeckEntry]) -> Result<(), String> {
    for entry in ordered(entries) {
        out.push(token(&entry.card, entry.count)?);
    }
    Ok(())
}

pub fn render(deck: &ResolvedDeck) -> Result<String, String> {
    if !deck.sideboard.is_empty() {
        return Err(NO_SIDEBOARD.into());
    }
    let mut out = Vec::new();
    if let Some(legend) = &deck.legend {
        out.push(token(legend, 1)?);
    }
    if let Some(champion) = &deck.chosen_champion {
        out.push(token(champion, 1)?);
    }
    tokens(&mut out, &deck.main_deck)?;
    tokens(&mut out, &deck.runes)?;
    tokens(&mut out, &deck.battlefields)?;
    Ok(out.join(" "))
}

fn tts_token(token: &str) -> Option<CardCode> {
    let mut parts = token.split('-');
    let set = parts.next()?;
    let number = parts.next()?;
    let variant = parts.next()?;
    if parts.next().is_some() || variant.is_empty() || !variant.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    CardCode::parse(&format!("{set}-{number}")).ok()
}

pub fn looks_like_tts_list(input: &str) -> bool {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 2 || tokens.iter().any(|token| tts_token(token).is_none()) {
        return false;
    }
    let mut seen = std::collections::BTreeSet::new();
    tokens.iter().any(|token| !seen.insert(*token))
}

pub fn from_tts(input: &str) -> String {
    let mut counts: Vec<(CardCode, u32)> = Vec::new();
    for code in input.split_whitespace().filter_map(tts_token) {
        match counts.iter_mut().find(|(held, _)| *held == code) {
            Some((_, count)) => *count += 1,
            None => counts.push((code, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(code, count)| {
            if count == 1 {
                code.to_string()
            } else {
                format!("{code}-{count}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn looks_like_code_list(input: &str) -> bool {
    let mut tokens = input.split_whitespace().peekable();
    tokens.peek().is_some()
        && input
            .split_whitespace()
            .all(|token| parse_token(token).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_carry_code_and_count() {
        let deck = parse_code_list("OGN-045-2 OGN-R05-12 UNL-116a-3 OGN-260").unwrap();
        assert_eq!(deck.entries.len(), 4);
        let render: Vec<(String, u32)> = deck
            .entries
            .iter()
            .map(|entry| match &entry.identifier {
                Identifier::Code(code) => (code.to_string(), entry.count),
                other => panic!("expected a code, got {other:?}"),
            })
            .collect();
        assert_eq!(
            render,
            vec![
                ("OGN-045".to_string(), 2),
                ("OGN-R05".to_string(), 12),
                ("UNL-116a".to_string(), 3),
                ("OGN-260".to_string(), 1),
            ]
        );
    }

    #[test]
    fn lists_split_across_lines_and_extra_spaces() {
        let deck = parse_code_list("OGN-045-2\n  OGN-046-1\tOGN-047-3").unwrap();
        assert_eq!(deck.entries.len(), 3);
    }

    #[test]
    fn bad_tokens_are_named_in_the_error() {
        assert_eq!(
            parse_code_list("OGN-045-2 nonsense"),
            Err(CodeListError::BadToken("nonsense".into()))
        );
        assert_eq!(
            parse_code_list("OGN-045-0"),
            Err(CodeListError::ZeroCount("OGN-045-0".into()))
        );
        assert_eq!(
            parse_code_list("OGN-045-2-9"),
            Err(CodeListError::BadToken("OGN-045-2-9".into()))
        );
        assert_eq!(parse_code_list("  "), Err(CodeListError::Empty));
    }

    fn lillia() -> ResolvedDeck {
        super::super::resolve::fixtures::pool_decks()
            .into_iter()
            .find(|(slug, _)| slug == "lillia-jonnynick")
            .map(|(_, deck)| deck)
            .expect("the lillia pool deck")
    }

    #[test]
    fn the_lillia_list_renders_legend_champion_main_runes_and_battlefields_print_exact() {
        assert_eq!(
            render(&lillia()).unwrap(),
            "UNL-189 UNL-082 SFD-036-3 OGN-103-3 SFD-032 SFD-053 OGN-043-2 OGN-045-3 OGN-046-3 OGN-095-3 OGN-058-3 UNL-083-2 OGN-093-2 UNL-069-3 OGN-105 OGN-123 SFD-042-2 OGN-060-3 UNL-078-3 OGN-042-7 OGN-089-5 UNL-209 SFD-215 SFD-217"
        );
        assert_eq!(render(&ResolvedDeck::default()).unwrap(), "");
    }

    #[test]
    fn a_sideboard_refuses_the_code_list_with_the_reason() {
        let deck = super::super::resolve::fixtures::with_sideboard(&lillia());
        assert_eq!(render(&deck), Err(NO_SIDEBOARD.to_string()));
    }

    #[test]
    fn an_alternate_print_stays_itself_and_an_unparseable_id_is_named() {
        let card = |id: &str| ResolvedCard {
            name: "Poppy - Paragon".into(),
            riftbound_id: id.into(),
            kind: Some("Unit".into()),
            ..Default::default()
        };
        let deck = ResolvedDeck {
            main_deck: vec![
                DeckEntry {
                    card: card("unl-116a-219"),
                    count: 3,
                },
                DeckEntry {
                    card: card("unl-116-219"),
                    count: 1,
                },
            ],
            ..Default::default()
        };
        assert_eq!(render(&deck).unwrap(), "UNL-116 UNL-116a-3");
        let reprint = ResolvedDeck {
            main_deck: vec![
                DeckEntry {
                    card: ResolvedCard {
                        name: "Stacked Deck".into(),
                        riftbound_id: "opp-183-298".into(),
                        ..Default::default()
                    },
                    count: 2,
                },
                DeckEntry {
                    card: ResolvedCard {
                        name: "Bare Promo".into(),
                        riftbound_id: "pr-003".into(),
                        ..Default::default()
                    },
                    count: 1,
                },
            ],
            ..Default::default()
        };
        assert_eq!(render(&reprint).unwrap(), "PR-003 OGN-183-2");
        let odd = ResolvedDeck {
            main_deck: vec![DeckEntry {
                card: card("nonsense"),
                count: 1,
            }],
            ..Default::default()
        };
        assert!(render(&odd).unwrap_err().contains("nonsense"));
    }

    #[test]
    fn every_pool_deck_round_trips_through_its_code_list_with_the_champion_rejoining_main() {
        use super::super::resolve::fixtures::{folded_catalog, pool_decks};
        use super::super::snapshot::snapshot;
        let mut catalog = folded_catalog();
        for (slug, deck) in pool_decks() {
            let list = render(&deck).unwrap_or_else(|error| panic!("{slug}: {error}"));
            assert!(looks_like_code_list(&list), "{slug}");
            let parsed = parse_code_list(&list).unwrap_or_else(|error| panic!("{slug}: {error}"));
            let resolution = super::super::resolve::resolve(&parsed, &mut catalog).unwrap();
            assert!(
                resolution.unresolved.is_empty(),
                "{slug}: {:?}",
                resolution.unresolved
            );
            assert_eq!(
                snapshot(&resolution.deck).identity(),
                snapshot(&deck).identity(),
                "{slug}: the champion unit rejoins the champion slot"
            );
            assert_eq!(
                render(&resolution.deck).unwrap(),
                render(&deck).unwrap(),
                "{slug}"
            );
        }
    }

    #[test]
    fn detection_accepts_only_full_code_lists() {
        assert!(looks_like_code_list("OGN-045-2 OGN-046-1"));
        assert!(!looks_like_code_list("3 Emberwing Scout"));
        assert!(!looks_like_code_list("OGN-045-2 and friends"));
        assert!(!looks_like_code_list(""));
    }
}

pub mod card_code;
pub mod catalog;
pub mod code_list;
pub mod deck_code;
pub mod json;
pub mod link;
pub mod resolve;
pub mod snapshot;
pub mod tcg_arena;
pub mod text_list;

#[cfg(feature = "riftbound-native")]
pub mod ingest;
#[cfg(feature = "riftbound-native")]
pub mod query;
#[cfg(feature = "riftbound-native")]
pub mod riftcodex;

#[cfg(feature = "riftbound-gateway")]
pub mod gateway;

use crate::deck::Game;
use crate::naming::normalize_name;
use agni_deck::push;
use agni_riftbound::ResolvedDeck;
use card_code::CardCode;
use catalog::{CardKind, CatalogCard};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Riftbound;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Legend,
    Champion,
    Main,
    Runes,
    Battlefields,
    Sideboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    Name(String),
    Code(CardCode),
    Id(String),
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => f.write_str(name),
            Self::Code(code) => code.fmt(f),
            Self::Id(id) => f.write_str(id),
        }
    }
}

pub type ParsedEntry = crate::deck::ParsedEntry<Riftbound>;
pub type ParsedDeck = crate::deck::ParsedDeck<Riftbound>;

impl Game for Riftbound {
    type Section = Section;
    type Identifier = Identifier;
    type Card = CatalogCard;
    type Deck = ResolvedDeck;

    fn describe(identifier: &Identifier) -> String {
        identifier.to_string()
    }

    fn cache_key(identifier: &Identifier) -> String {
        match identifier {
            Identifier::Name(name) => format!("name:{}", normalize_name(name)),
            Identifier::Code(code) => format!("id:{}", code.id_fragment()),
            Identifier::Id(id) => format!("id:{}", id.to_ascii_lowercase()),
        }
    }

    fn place(deck: &mut ResolvedDeck, section: Option<Section>, card: CatalogCard, count: u32) {
        let section = section.or(match card.kind {
            CardKind::Legend => Some(Section::Legend),
            CardKind::Rune => Some(Section::Runes),
            CardKind::Battlefield => Some(Section::Battlefields),
            _ => None,
        });
        let card = card.resolved();
        match section {
            Some(Section::Legend) => {
                if deck.legend.is_none() {
                    deck.legend = Some(card.clone());
                    if count > 1 {
                        push(&mut deck.main_deck, card, count - 1);
                    }
                } else {
                    push(&mut deck.main_deck, card, count);
                }
            }
            Some(Section::Champion) => {
                if deck.chosen_champion.is_none() {
                    deck.chosen_champion = Some(card.clone());
                    if count > 1 {
                        push(&mut deck.main_deck, card, count - 1);
                    }
                } else {
                    push(&mut deck.main_deck, card, count);
                }
            }
            Some(Section::Runes) => push(&mut deck.runes, card, count),
            Some(Section::Battlefields) => push(&mut deck.battlefields, card, count),
            Some(Section::Sideboard) => push(&mut deck.sideboard, card, count),
            Some(Section::Main) | None => push(&mut deck.main_deck, card, count),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckSource {
    Code(String),
    Text(String),
    CodeList(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Code(deck_code::DeckCodeError),
    Text(text_list::TextError),
    CodeList(code_list::CodeListError),
    TcgArena(tcg_arena::Error),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(error) => error.fmt(f),
            Self::Text(error) => error.fmt(f),
            Self::CodeList(error) => error.fmt(f),
            Self::TcgArena(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_deck(source: &DeckSource) -> Result<ParsedDeck, ParseError> {
    match source {
        DeckSource::Code(code) => {
            let decoded = deck_code::decode(code).map_err(ParseError::Code)?;
            Ok(parsed_from_decoded(&decoded))
        }
        DeckSource::Text(text) => text_list::parse_text(text).map_err(ParseError::Text),
        DeckSource::CodeList(list) => {
            code_list::parse_code_list(list).map_err(ParseError::CodeList)
        }
    }
}

pub fn parse_any(text: &str) -> Result<ParsedDeck, ParseError> {
    parse_any_with_title(text).map(|(deck, _)| deck)
}

pub fn parse_any_with_title(text: &str) -> Result<(ParsedDeck, Option<String>), ParseError> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        let imported = tcg_arena::parse_json(text).map_err(ParseError::TcgArena)?;
        return Ok((imported.deck, imported.title));
    }
    let mut tokens = trimmed.split_whitespace();
    if let (Some(single), None) = (tokens.next(), tokens.next()) {
        if let Some(code) = link::code_in_url(single) {
            return parse_deck(&DeckSource::Code(code)).map(|deck| (deck, None));
        }
        if link::classify(single) == Some(link::Site::TcgArena) {
            let imported = tcg_arena::parse_url(single).map_err(ParseError::TcgArena)?;
            return Ok((imported.deck, imported.title));
        }
        if deck_code::decode(single).is_ok() {
            return parse_deck(&DeckSource::Code(single.to_string())).map(|deck| (deck, None));
        }
    }
    if code_list::looks_like_tts_list(trimmed) {
        return parse_deck(&DeckSource::CodeList(code_list::from_tts(trimmed)))
            .map(|deck| (deck, None));
    }
    if code_list::looks_like_code_list(trimmed) {
        return parse_deck(&DeckSource::CodeList(trimmed.to_string())).map(|deck| (deck, None));
    }
    parse_deck(&DeckSource::Text(text.to_string())).map(|deck| (deck, None))
}

pub fn parsed_from_decoded(decoded: &deck_code::DecodedDeck) -> ParsedDeck {
    let mut deck = ParsedDeck::default();
    let mut champion_taken = false;
    for entry in &decoded.main {
        let mut count = entry.count;
        if !champion_taken && decoded.champion == Some(entry.code) {
            champion_taken = true;
            count -= 1;
        }
        if count == 0 {
            continue;
        }
        deck.entries.push(ParsedEntry {
            identifier: Identifier::Code(entry.code),
            count,
            section: None,
        });
    }
    for entry in &decoded.sideboard {
        deck.entries.push(ParsedEntry {
            identifier: Identifier::Code(entry.code),
            count: entry.count,
            section: Some(Section::Sideboard),
        });
    }
    if let Some(champion) = decoded.champion {
        deck.entries.push(ParsedEntry {
            identifier: Identifier::Code(champion),
            count: 1,
            section: Some(Section::Champion),
        });
    }
    deck
}

#[cfg(test)]
mod site_format_tests {
    use super::resolve::fixtures::folded_catalog;
    use super::{link, parse_any, parse_any_with_title, resolve};
    use agni_riftbound::ResolvedDeck;

    fn seated(text: &str) -> ResolvedDeck {
        let parsed = parse_any(text).unwrap_or_else(|error| panic!("{error}: {text:?}"));
        let resolution = resolve::resolve(&parsed, &mut folded_catalog()).unwrap();
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        resolution.deck
    }

    #[test]
    fn a_pasted_tcg_arena_import_url_uses_the_url_parser() {
        let url = "https://tcg-arena.fr/import?game=Riftbound&name=Synthetic&deck=MSBTeW50aGV0aWMgTGVnZW5kCg==";
        let parsed = parse_any(url).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(
            parsed.entries[0].identifier,
            super::Identifier::Name("Synthetic Legend".into())
        );
    }

    #[test]
    fn a_pasted_tcg_arena_json_uses_the_json_parser() {
        let json = r#"{"game":"Riftbound","title":"Synthetic","deckList":{"categoriesOrder":["Legend"],"Legend":[{"count":1,"id":"SYN-001"}]}}"#;
        let (parsed, title) = parse_any_with_title(json).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(title.as_deref(), Some("Synthetic"));
    }

    fn totals(deck: &ResolvedDeck) -> (u32, u32, u32, u32) {
        let sum = |zone: &[agni_riftbound::DeckEntry]| zone.iter().map(|e| e.count).sum();
        (
            sum(&deck.main_deck),
            sum(&deck.runes),
            sum(&deck.battlefields),
            sum(&deck.sideboard),
        )
    }

    const PILTOVER_TEXT: &str = "Legend:\n1 Lillia, Bashful Bloom\n\nChampion:\n1 Lillia, Fae Fawn\n\nMainDeck:\n2 Charm\n2 Hwei, Brooding Painter\n\nBattlefields:\n1 Rockfall Path\n\nRunes:\n6 Calm Rune\n";

    const RIFT_ATLAS_TEXT: &str = "Legend:\n1 Lillia - Bashful Bloom [UNL-189]\n\nChampion:\n1 Lillia - Fae Fawn [UNL-082]\n\nMainDeck:\n2 Charm [OGN-043]\n2 Hwei - Brooding Painter [UNL-080]\n\nBattlefields:\n1 Rockfall Path [SFD-216]\n\nRunes:\n6 Calm Rune [OGN-042]\n\nSideboard:\n1 Charm [OGN-043]\n";

    #[test]
    fn piltover_archives_text_export_seats_with_comma_names_and_a_champion_section() {
        let deck = seated(PILTOVER_TEXT);
        assert_eq!(deck.legend.as_ref().unwrap().name, "Lillia - Bashful Bloom");
        assert_eq!(
            deck.chosen_champion.as_ref().unwrap().name,
            "Lillia - Fae Fawn"
        );
        assert_eq!(totals(&deck), (4, 6, 1, 0));
    }

    #[test]
    fn rift_atlas_text_export_seats_by_its_bracketed_card_codes() {
        let deck = seated(RIFT_ATLAS_TEXT);
        assert_eq!(deck.legend.as_ref().unwrap().riftbound_id, "unl-189-219");
        assert_eq!(
            deck.chosen_champion.as_ref().unwrap().riftbound_id,
            "unl-082-219"
        );
        assert_eq!(totals(&deck), (4, 6, 1, 1));
        let by_code_only = seated("Legend:\n1 Nobody Here [UNL-189]\nChampion:\n1 Also Nobody [UNL-082]\nMainDeck:\n3 Whoever [OGN-043]\n");
        assert_eq!(
            by_code_only.legend.as_ref().unwrap().name,
            "Lillia - Bashful Bloom"
        );
        assert_eq!(by_code_only.main_deck[0].count, 3);
    }

    #[test]
    fn tilde_and_markdown_headers_and_every_count_spelling_are_read() {
        let deck = seated("# my list\n~~Legend~~\nLillia, Bashful Bloom\n## Champion\n1 Lillia, Fae Fawn\n## Main Deck\n2x Charm\n1 x Charm\nHwei - Brooding Painter x2\nHwei - Brooding Painter x 1\n// a note\n~~Runes~~\n12 Calm Rune\n### Battlefields\n1 Rockfall Path\n");
        assert_eq!(deck.legend.as_ref().unwrap().name, "Lillia - Bashful Bloom");
        assert_eq!(
            deck.chosen_champion.as_ref().unwrap().name,
            "Lillia - Fae Fawn"
        );
        assert_eq!(totals(&deck), (6, 12, 1, 0));
    }

    #[test]
    fn a_tabletop_simulator_list_seats_with_the_champion_inferred() {
        let deck = seated(
            "UNL-189-2 UNL-082-1 OGN-043-1 OGN-043-1 UNL-080-1 SFD-216-1 OGN-042-2 OGN-042-2",
        );
        assert_eq!(deck.legend.as_ref().unwrap().name, "Lillia - Bashful Bloom");
        assert_eq!(
            deck.chosen_champion.as_ref().map(|card| card.name.clone()),
            Some("Lillia - Fae Fawn".to_string()),
            "a list without a champion section takes the legend's champion unit"
        );
        assert_eq!(totals(&deck), (3, 2, 1, 0));
    }

    #[test]
    fn a_deck_code_pasted_as_a_site_link_seats_without_a_fetch() {
        let code = super::deck_code::encode_deck(&seated(PILTOVER_TEXT)).unwrap();
        for url in [
            link::piltover_url(&code),
            link::riftatlas_url(&code),
            format!("https://play.riftatlas.com/?savedDeckId=x&deckCode={code}#hub"),
        ] {
            let deck = seated(&url);
            assert_eq!(
                deck.chosen_champion.as_ref().unwrap().name,
                "Lillia - Fae Fawn",
                "{url}"
            );
            assert_eq!(totals(&deck), (4, 6, 1, 0), "{url}");
        }
        assert_eq!(link::code_in_url("https://riftdecks.com/deck/1"), None);
        assert_eq!(
            link::code_in_url("https://piltoverarchive.com/deckbuilder?code=nope"),
            None
        );
    }

    #[test]
    fn a_code_we_write_reads_back_as_the_same_forty_on_either_site() {
        let deck = seated(PILTOVER_TEXT);
        let decoded =
            super::deck_code::decode(&super::deck_code::encode_deck(&deck).unwrap()).unwrap();
        let champion_copies: u32 = decoded
            .main
            .iter()
            .filter(|entry| Some(entry.code) == decoded.champion)
            .map(|entry| entry.count)
            .sum();
        assert_eq!(
            champion_copies, 1,
            "the champion is listed among the main cards, as the sites read it"
        );
        let back = seated(&super::deck_code::encode_deck(&deck).unwrap());
        assert_eq!(
            back.chosen_champion.as_ref().unwrap().name,
            "Lillia - Fae Fawn"
        );
        assert_eq!(totals(&back), totals(&deck));
    }
}

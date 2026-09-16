pub mod catalog;
#[cfg(feature = "art")]
pub mod history;
pub mod resolve;
pub mod text_list;

pub use catalog::{unique_prefix, Cached, CardLookup, LookupError, NameIndex};
pub use resolve::{resolve, Resolution, Unresolved};
pub use text_list::{parse_text, split_count, TextError, TextVocabulary};

use std::fmt;

pub trait Game: Copy + Eq + Default + fmt::Debug + 'static {
    type Section: Copy + Eq + fmt::Debug;
    type Identifier: Clone + Eq + fmt::Debug;
    type Card: Clone + Eq + fmt::Debug;
    type Deck: Default + Clone + Eq + fmt::Debug;

    fn describe(identifier: &Self::Identifier) -> String;
    fn cache_key(identifier: &Self::Identifier) -> String;
    fn place(deck: &mut Self::Deck, section: Option<Self::Section>, card: Self::Card, count: u32);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEntry<G: Game> {
    pub identifier: G::Identifier,
    pub count: u32,
    pub section: Option<G::Section>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedDeck<G: Game> {
    pub entries: Vec<ParsedEntry<G>>,
}

#[cfg(test)]
pub(crate) mod testing {
    use super::{Game, TextVocabulary};
    use crate::naming::normalize_name;
    use agni_deck::{push, DeckEntry};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct TestGame;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Section {
        Main,
        Side,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Card {
        pub name: String,
        pub art: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct Deck {
        pub main: Vec<DeckEntry<Card>>,
        pub side: Vec<DeckEntry<Card>>,
    }

    impl Game for TestGame {
        type Section = Section;
        type Identifier = String;
        type Card = Card;
        type Deck = Deck;

        fn describe(identifier: &String) -> String {
            identifier.clone()
        }

        fn cache_key(identifier: &String) -> String {
            normalize_name(identifier)
        }

        fn place(deck: &mut Deck, section: Option<Section>, card: Card, count: u32) {
            match section {
                Some(Section::Side) => push(&mut deck.side, card, count),
                Some(Section::Main) | None => push(&mut deck.main, card, count),
            }
        }
    }

    impl TextVocabulary for TestGame {
        type Game = TestGame;

        fn section(header: &str) -> Option<Section> {
            match header {
                "deck" | "main" | "main deck" => Some(Section::Main),
                "side" | "sideboard" => Some(Section::Side),
                _ => None,
            }
        }

        fn identifier(name: &str) -> String {
            name.to_string()
        }
    }

    pub fn card(name: &str, art: bool) -> Card {
        Card {
            name: name.into(),
            art,
        }
    }
}

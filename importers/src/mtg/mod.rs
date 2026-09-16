pub mod catalog;
pub mod resolve;
pub mod text_list;

#[cfg(feature = "mtg-native")]
pub mod ingest;
#[cfg(feature = "mtg-native")]
pub mod scryfall_named;

pub use text_list::{parse_text, TextError};

use crate::deck::Game;
use crate::naming::normalize_name;
use agni_deck::push;
use agni_mtg::{ResolvedCard, ResolvedDeck};
use catalog::MtgCard;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mtg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Commander,
    Main,
    Sideboard,
    Ignored,
}

pub type ParsedEntry = crate::deck::ParsedEntry<Mtg>;
pub type ParsedDeck = crate::deck::ParsedDeck<Mtg>;

impl Game for Mtg {
    type Section = Section;
    type Identifier = String;
    type Card = MtgCard;
    type Deck = ResolvedDeck;

    fn describe(name: &String) -> String {
        name.clone()
    }

    fn cache_key(name: &String) -> String {
        normalize_name(name)
    }

    fn place(deck: &mut ResolvedDeck, section: Option<Section>, card: MtgCard, count: u32) {
        let card = ResolvedCard {
            name: card.name,
            image_url: card.image_url,
        };
        match section {
            Some(Section::Commander) => {
                if deck.commander.is_none() {
                    deck.commander = Some(card);
                } else {
                    push(&mut deck.main_deck, card, count);
                }
            }
            Some(Section::Sideboard) => push(&mut deck.sideboard, card, count),
            Some(Section::Ignored) => {}
            Some(Section::Main) | None => push(&mut deck.main_deck, card, count),
        }
    }
}

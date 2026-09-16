use super::{Mtg, ParsedDeck};
use crate::deck::{CardLookup, LookupError};

pub use crate::deck::Unresolved;

pub type Resolution = crate::deck::Resolution<Mtg>;

pub fn resolve(
    parsed: &ParsedDeck,
    cards: &mut dyn CardLookup<Mtg>,
) -> Result<Resolution, LookupError> {
    crate::deck::resolve(parsed, cards)
}

pub fn is_empty(resolution: &Resolution) -> bool {
    let deck = &resolution.deck;
    deck.commander.is_none() && deck.main_deck.is_empty() && deck.sideboard.is_empty()
}

#[cfg(test)]
mod tests {
    use super::super::catalog::test_catalog;
    use super::super::parse_text;
    use super::*;

    #[test]
    fn sections_land_in_their_zones() {
        let deck = parse_text(
            "Commander\n1 Serelith, Tidebound Oracle\nDeck\n4 Thornspire Adept\n2 Mistfen Causeway\nSideboard\n3 Cinderveil Ward\n",
        )
        .unwrap();
        let resolution = resolve(&deck, &mut test_catalog()).unwrap();
        assert!(resolution.unresolved.is_empty());
        let deck = resolution.deck;
        assert_eq!(deck.commander.unwrap().name, "Serelith, Tidebound Oracle");
        assert_eq!(deck.main_deck.len(), 2);
        assert_eq!(deck.main_deck[0].count, 4);
        assert_eq!(deck.sideboard.len(), 1);
        assert_eq!(deck.sideboard[0].count, 3);
    }

    #[test]
    fn a_second_commander_falls_into_the_main_deck() {
        let deck =
            parse_text("Commander\n1 Serelith, Tidebound Oracle\n1 Thornspire Adept\n").unwrap();
        let resolution = resolve(&deck, &mut test_catalog()).unwrap();
        assert_eq!(resolution.deck.main_deck.len(), 1);
        assert_eq!(resolution.deck.main_deck[0].card.name, "Thornspire Adept");
    }

    #[test]
    fn a_bare_list_is_all_main_deck() {
        let deck = parse_text("4 Thornspire Adept\n4 Thornspire Adept\n").unwrap();
        let resolution = resolve(&deck, &mut test_catalog()).unwrap();
        assert_eq!(resolution.deck.main_deck.len(), 1);
        assert_eq!(resolution.deck.main_deck[0].count, 8);
        assert!(resolution.deck.commander.is_none());
        assert!(!is_empty(&resolution));
    }

    #[test]
    fn a_deck_that_resolves_nothing_reads_as_empty() {
        let deck = parse_text("2 Completely Unknown\n").unwrap();
        let resolution = resolve(&deck, &mut test_catalog()).unwrap();
        assert_eq!(resolution.unresolved[0].identifier, "Completely Unknown");
        assert!(is_empty(&resolution));
    }
}

pub const GAME: &str = "abyss-walker";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AbyssWalkerDeck {
    pub cards: Vec<CardName>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deck_holds_cards() {
        let deck = AbyssWalkerDeck {
            cards: vec![CardName("Walker".into())],
        };
        assert_eq!(deck.cards.len(), 1);
    }
}

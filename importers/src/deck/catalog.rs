use super::Game;
use crate::naming::normalize_name;
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupError(pub String);

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "card lookup failed: {}", self.0)
    }
}

impl std::error::Error for LookupError {}

pub trait CardLookup<G: Game> {
    fn find(&mut self, identifier: &G::Identifier) -> Result<Option<G::Card>, LookupError>;
}

pub fn unique_prefix<'a, V>(
    index: &'a BTreeMap<String, V>,
    key: &str,
    separator: char,
) -> Option<&'a V> {
    if let Some(value) = index.get(key) {
        return Some(value);
    }
    let prefix = format!("{key}{separator}");
    let mut matches = index
        .range(key.to_string()..)
        .take_while(|(candidate, _)| candidate.starts_with(key))
        .filter(|(candidate, _)| candidate.starts_with(&prefix));
    let first = matches.next();
    match (first, matches.next()) {
        (Some((_, value)), None) => Some(value),
        _ => None,
    }
}

pub struct NameIndex<C> {
    cards: Vec<C>,
    by_normalized_name: BTreeMap<String, usize>,
}

impl<C> NameIndex<C> {
    pub fn new(cards: Vec<C>, name: impl Fn(&C) -> &str) -> Self {
        let mut by_normalized_name = BTreeMap::new();
        for (index, card) in cards.iter().enumerate() {
            by_normalized_name
                .entry(normalize_name(name(card)))
                .or_insert(index);
        }
        Self {
            cards,
            by_normalized_name,
        }
    }

    pub fn cards(&self) -> &[C] {
        &self.cards
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn find(&self, name: &str) -> Option<&C> {
        unique_prefix(&self.by_normalized_name, &normalize_name(name), ' ')
            .map(|&index| &self.cards[index])
    }
}

pub struct Cached<G: Game, L> {
    inner: L,
    hits: BTreeMap<String, Option<G::Card>>,
    game: PhantomData<G>,
}

impl<G: Game, L> Cached<G, L> {
    pub fn new(inner: L) -> Self {
        Self {
            inner,
            hits: BTreeMap::new(),
            game: PhantomData,
        }
    }

    pub fn inner(&self) -> &L {
        &self.inner
    }
}

impl<G: Game, L: CardLookup<G>> CardLookup<G> for Cached<G, L> {
    fn find(&mut self, identifier: &G::Identifier) -> Result<Option<G::Card>, LookupError> {
        let key = G::cache_key(identifier);
        if let Some(hit) = self.hits.get(&key) {
            return Ok(hit.clone());
        }
        let result = self.inner.find(identifier)?;
        self.hits.insert(key, result.clone());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{card, Card, TestGame};
    use super::*;

    fn index() -> NameIndex<Card> {
        NameIndex::new(
            vec![
                card("Emberwing Scout", true),
                card("Emberwing Scout (Alternate Art)", true),
                card("Yorick, Keeper of the 1000 Graves", true),
                card("Ember Rune", false),
            ],
            |card| &card.name,
        )
    }

    impl CardLookup<TestGame> for NameIndex<Card> {
        fn find(&mut self, name: &String) -> Result<Option<Card>, LookupError> {
            Ok(NameIndex::find(self, name).cloned())
        }
    }

    #[test]
    fn names_match_case_and_punctuation_insensitively() {
        let index = index();
        assert_eq!(
            index.find("emberwing  SCOUT!").unwrap().name,
            "Emberwing Scout"
        );
        assert_eq!(
            index.find("YORICK KEEPER OF THE 1000 GRAVES").unwrap().name,
            "Yorick, Keeper of the 1000 Graves"
        );
        assert!(index.find("Unknown Card").is_none());
        assert_eq!(index.len(), 4);
        assert!(!index.is_empty());
    }

    #[test]
    fn a_unique_prefix_resolves_but_an_ambiguous_one_does_not() {
        let index = index();
        assert_eq!(
            index.find("Yorick").unwrap().name,
            "Yorick, Keeper of the 1000 Graves"
        );
        assert_eq!(index.find("Ember").unwrap().name, "Ember Rune");
        assert!(index.find("Emberwing").is_none());
    }

    #[test]
    fn prefixes_only_match_at_a_separator() {
        let mut ids = BTreeMap::new();
        ids.insert("ogn-007-298".to_string(), 1);
        ids.insert("ogn-007a-298".to_string(), 2);
        ids.insert("ven-r01".to_string(), 3);
        assert_eq!(unique_prefix(&ids, "ogn-007", '-'), Some(&1));
        assert_eq!(unique_prefix(&ids, "ven-r01", '-'), Some(&3));
        assert_eq!(unique_prefix(&ids, "ogn", '-'), None);
        assert_eq!(unique_prefix(&ids, "ogn-00", '-'), None);
    }

    #[test]
    fn a_variant_sorting_before_the_separator_does_not_hide_the_base_printing() {
        let mut ids = BTreeMap::new();
        ids.insert("unl-230*-219".to_string(), "signature");
        ids.insert("unl-230-219".to_string(), "base");
        ids.insert("unl-230a-219".to_string(), "alternate");
        assert_eq!(unique_prefix(&ids, "unl-230", '-'), Some(&"base"));
        assert_eq!(unique_prefix(&ids, "unl-230*", '-'), Some(&"signature"));
        assert_eq!(unique_prefix(&ids, "unl-230a", '-'), Some(&"alternate"));
        assert_eq!(unique_prefix(&ids, "unl-23", '-'), None);
    }

    #[test]
    fn the_cache_serves_repeats_without_asking_again() {
        struct Counting {
            inner: NameIndex<Card>,
            calls: usize,
        }
        impl CardLookup<TestGame> for Counting {
            fn find(&mut self, name: &String) -> Result<Option<Card>, LookupError> {
                self.calls += 1;
                CardLookup::find(&mut self.inner, name)
            }
        }
        let mut cached: Cached<TestGame, Counting> = Cached::new(Counting {
            inner: index(),
            calls: 0,
        });
        for _ in 0..3 {
            assert!(cached.find(&"Ember Rune".to_string()).unwrap().is_some());
            assert!(cached.find(&"ember  RUNE!".to_string()).unwrap().is_some());
            assert!(cached.find(&"Nobody".to_string()).unwrap().is_none());
        }
        assert_eq!(cached.inner().calls, 2);
    }
}

use super::Mtg;
use crate::deck::{Cached as Memo, NameIndex};

pub use crate::deck::LookupError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtgCard {
    pub name: String,
    pub image_url: Option<String>,
}

pub trait CardLookup: crate::deck::CardLookup<Mtg> {
    fn by_name(&mut self, name: &str) -> Result<Option<MtgCard>, LookupError> {
        self.find(&name.to_string())
    }
}

impl<L: crate::deck::CardLookup<Mtg> + ?Sized> CardLookup for L {}

pub type Cached<L> = Memo<Mtg, L>;

pub struct StaticCatalog {
    index: NameIndex<MtgCard>,
}

impl StaticCatalog {
    pub fn new(cards: Vec<MtgCard>) -> Self {
        Self {
            index: NameIndex::new(cards, |card| &card.name),
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

impl crate::deck::CardLookup<Mtg> for StaticCatalog {
    fn find(&mut self, name: &String) -> Result<Option<MtgCard>, LookupError> {
        Ok(self.index.find(name).cloned())
    }
}

pub struct Layered<First, Second> {
    pub first: First,
    pub second: Second,
}

impl<First, Second> crate::deck::CardLookup<Mtg> for Layered<First, Second>
where
    First: crate::deck::CardLookup<Mtg>,
    Second: crate::deck::CardLookup<Mtg>,
{
    fn find(&mut self, name: &String) -> Result<Option<MtgCard>, LookupError> {
        match self.first.find(name) {
            Ok(Some(card)) if card.image_url.is_some() => Ok(Some(card)),
            Ok(local) => Ok(self.second.find(name)?.or(local)),
            Err(_) => self.second.find(name),
        }
    }
}

#[cfg(test)]
pub(crate) fn test_catalog() -> StaticCatalog {
    use crate::naming::normalize_name;
    let card = |name: &str, art: bool| MtgCard {
        name: name.into(),
        image_url: art.then(|| format!("https://img.example/{}.jpg", normalize_name(name))),
    };
    StaticCatalog::new(vec![
        card("Thornspire Adept", true),
        card("Mistfen Causeway", true),
        card("Serelith, Tidebound Oracle", true),
        card("Cinderveil Ward", false),
        card("Krellik, Keeper of the 1000 Embers", true),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_legendary_first_name_resolves_through_the_comma() {
        let mut catalog = test_catalog();
        assert_eq!(
            catalog.by_name("Serelith").unwrap().unwrap().name,
            "Serelith, Tidebound Oracle"
        );
        assert_eq!(
            catalog
                .by_name("KRELLIK KEEPER OF THE 1000 EMBERS")
                .unwrap()
                .unwrap()
                .name,
            "Krellik, Keeper of the 1000 Embers"
        );
        assert!(catalog.by_name("Unknown Card").unwrap().is_none());
    }

    #[test]
    fn the_local_catalog_answers_first_and_the_remote_covers_its_misses() {
        struct Remote {
            calls: usize,
        }
        impl crate::deck::CardLookup<Mtg> for Remote {
            fn find(&mut self, name: &String) -> Result<Option<MtgCard>, LookupError> {
                self.calls += 1;
                Ok(Some(MtgCard {
                    name: name.clone(),
                    image_url: Some("https://cards.example/remote.jpg".into()),
                }))
            }
        }
        let mut layered = Layered {
            first: test_catalog(),
            second: Remote { calls: 0 },
        };
        assert_eq!(
            layered
                .by_name("Thornspire Adept")
                .unwrap()
                .unwrap()
                .image_url
                .as_deref(),
            Some("https://img.example/thornspire adept.jpg")
        );
        assert_eq!(layered.second.calls, 0);
        assert_eq!(
            layered
                .by_name("Cinderveil Ward")
                .unwrap()
                .unwrap()
                .image_url
                .as_deref(),
            Some("https://cards.example/remote.jpg")
        );
        assert_eq!(layered.second.calls, 1);
    }
}

use super::card_code::CardCode;
use super::{Identifier, Riftbound};
use crate::deck::{unique_prefix, Cached as Memo, NameIndex};
use agni_riftbound::ResolvedCard;
use std::collections::BTreeMap;

pub use crate::deck::LookupError;
pub use crate::naming::normalize_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CardKind {
    Legend,
    Unit,
    Spell,
    Gear,
    Rune,
    Battlefield,
    #[default]
    Other,
}

impl CardKind {
    pub fn parse(text: &str) -> Self {
        match text.to_ascii_lowercase().as_str() {
            "legend" => Self::Legend,
            "unit" => Self::Unit,
            "spell" => Self::Spell,
            "gear" => Self::Gear,
            "rune" => Self::Rune,
            "battlefield" => Self::Battlefield,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legend => "Legend",
            Self::Unit => "Unit",
            Self::Spell => "Spell",
            Self::Gear => "Gear",
            Self::Rune => "Rune",
            Self::Battlefield => "Battlefield",
            Self::Other => "Other",
        }
    }
}

pub const SUPERTYPE_CHAMPION: &str = "Champion";
pub const SUPERTYPE_SIGNATURE: &str = "Signature";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CatalogCard {
    pub name: String,
    pub riftbound_id: String,
    pub kind: CardKind,
    pub champion: bool,
    pub image_url: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
    pub tags: Vec<String>,
    pub signature: bool,
    pub set_id: Option<String>,
    pub text: Option<String>,
}

impl CatalogCard {
    pub fn is_padded(&self) -> bool {
        self.kind == CardKind::Other && self.text.is_none() && self.domain.is_empty()
    }

    pub fn resolved(&self) -> ResolvedCard {
        ResolvedCard {
            name: self.name.clone(),
            riftbound_id: self.riftbound_id.clone(),
            image_url: self.image_url.clone(),
            kind: Some(self.kind.as_str().to_string()),
            energy: self.energy,
            power: self.power,
            might: self.might,
            domain: self.domain.clone(),
            tags: self.tags.clone(),
            signature: self.signature,
        }
    }
}

pub trait CardLookup: crate::deck::CardLookup<Riftbound> {
    fn by_name(&mut self, name: &str) -> Result<Option<CatalogCard>, LookupError> {
        self.find(&Identifier::Name(name.to_string()))
    }

    fn by_code(&mut self, code: &CardCode) -> Result<Option<CatalogCard>, LookupError> {
        self.find(&Identifier::Code(*code))
    }

    fn by_id(&mut self, id: &str) -> Result<Option<CatalogCard>, LookupError> {
        self.find(&Identifier::Id(id.to_string()))
    }
}

impl<L: crate::deck::CardLookup<Riftbound> + ?Sized> CardLookup for L {}

pub type Cached<L> = Memo<Riftbound, L>;

pub struct StaticCatalog {
    index: NameIndex<CatalogCard>,
    by_id: BTreeMap<String, usize>,
}

impl StaticCatalog {
    pub fn new(cards: Vec<CatalogCard>) -> Self {
        let by_id = cards
            .iter()
            .enumerate()
            .map(|(index, card)| (card.riftbound_id.to_ascii_lowercase(), index))
            .collect();
        Self {
            index: NameIndex::new(cards, |card| &card.name),
            by_id,
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn find_id_fragment(&self, fragment: &str) -> Option<&CatalogCard> {
        unique_prefix(&self.by_id, fragment, '-').map(|&index| &self.index.cards()[index])
    }
}

impl crate::deck::CardLookup<Riftbound> for StaticCatalog {
    fn find(&mut self, identifier: &Identifier) -> Result<Option<CatalogCard>, LookupError> {
        Ok(match identifier {
            Identifier::Name(name) => self.index.find(name),
            Identifier::Code(code) => self.find_id_fragment(&code.id_fragment()),
            Identifier::Id(id) => self.find_id_fragment(&id.to_ascii_lowercase()),
        }
        .cloned())
    }
}

pub struct Layered<First, Second> {
    pub first: First,
    pub second: Second,
}

impl<First, Second> crate::deck::CardLookup<Riftbound> for Layered<First, Second>
where
    First: crate::deck::CardLookup<Riftbound>,
    Second: crate::deck::CardLookup<Riftbound>,
{
    fn find(&mut self, identifier: &Identifier) -> Result<Option<CatalogCard>, LookupError> {
        match self.first.find(identifier) {
            Ok(Some(card)) if !card.is_padded() => Ok(Some(card)),
            Ok(local) => Ok(self.second.find(identifier)?.or(local)),
            Err(_) => self.second.find(identifier),
        }
    }
}

#[cfg(test)]
pub(crate) fn test_catalog() -> StaticCatalog {
    let card = |name: &str, id: &str, kind: CardKind, champion: bool| CatalogCard {
        name: name.into(),
        riftbound_id: id.into(),
        kind,
        champion,
        image_url: Some(format!("https://img.example/{id}.png")),
        ..Default::default()
    };
    StaticCatalog::new(vec![
        card("Vanguard Sentinel", "ogn-201-298", CardKind::Legend, false),
        card("Emberwing Scout", "ogn-007-298", CardKind::Unit, true),
        card(
            "Emberwing Scout (Alternate Art)",
            "ogn-007a-298",
            CardKind::Unit,
            true,
        ),
        card("Gloomvale Trickster", "ogn-101-298", CardKind::Unit, false),
        card(
            "Yorick, Keeper of the 1000 Graves",
            "ogn-118-298",
            CardKind::Unit,
            false,
        ),
        card("Ember Rune", "ogn-042-298", CardKind::Rune, false),
        card("Tempest Rune", "ven-r01", CardKind::Rune, false),
        card(
            "Sunken Causeway",
            "ogn-260-298",
            CardKind::Battlefield,
            false,
        ),
        card("Duskwatch Warden", "ogn-088-298", CardKind::Gear, false),
        card("Sudden Undertow", "ogn-045-298", CardKind::Spell, false),
        card(
            "Marsh Envoy (Signature)",
            "unl-229*-219",
            CardKind::Unit,
            true,
        ),
        card(
            "Lillia - Bashful Bloom",
            "unl-189-219",
            CardKind::Legend,
            false,
        ),
        card(
            "Lillia - Bashful Bloom (Signature)",
            "unl-230*-219",
            CardKind::Legend,
            false,
        ),
        card(
            "Lillia - Bashful Bloom (Overnumbered)",
            "unl-230-219",
            CardKind::Legend,
            false,
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_uniquely_by_prefix() {
        let mut catalog = test_catalog();
        let hit = catalog.by_name("emberwing scout").unwrap().unwrap();
        assert_eq!(hit.riftbound_id, "ogn-007-298");
        let hit = catalog.by_name("Yorick").unwrap().unwrap();
        assert_eq!(hit.riftbound_id, "ogn-118-298");
        assert!(catalog.by_name("Emberwing").unwrap().is_none());
        assert!(catalog.by_name("Unknown Card").unwrap().is_none());
    }

    #[test]
    fn codes_resolve_through_id_fragments() {
        let mut catalog = test_catalog();
        let code = CardCode::parse("OGN-045").unwrap();
        let hit = catalog.by_code(&code).unwrap().unwrap();
        assert_eq!(hit.name, "Sudden Undertow");
        let rune = CardCode::parse("VEN-R01").unwrap();
        assert_eq!(
            catalog.by_code(&rune).unwrap().unwrap().name,
            "Tempest Rune"
        );
        let signed = CardCode::parse("UNL-229s").unwrap();
        assert_eq!(
            catalog.by_code(&signed).unwrap().unwrap().name,
            "Marsh Envoy (Signature)"
        );
        let base_does_not_match_alternate = CardCode::parse("UNL-229").unwrap();
        assert!(catalog
            .by_code(&base_does_not_match_alternate)
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_legend_resolves_beside_its_signature_printing() {
        let mut catalog = test_catalog();
        let base = CardCode::parse("UNL-230").unwrap();
        assert_eq!(
            catalog.by_code(&base).unwrap().unwrap().name,
            "Lillia - Bashful Bloom (Overnumbered)"
        );
        let signed = CardCode::parse("UNL-230s").unwrap();
        assert_eq!(
            catalog.by_code(&signed).unwrap().unwrap().name,
            "Lillia - Bashful Bloom (Signature)"
        );
        assert_eq!(
            catalog.by_id("unl-230").unwrap().unwrap().riftbound_id,
            "unl-230-219"
        );
        assert_eq!(
            catalog
                .by_name("Lillia - Bashful Bloom")
                .unwrap()
                .unwrap()
                .riftbound_id,
            "unl-189-219"
        );
        assert!(catalog.by_name("Lillia").unwrap().is_none());
    }

    #[test]
    fn ids_resolve_with_or_without_set_size() {
        let mut catalog = test_catalog();
        assert_eq!(
            catalog.by_id("ogn-007-298").unwrap().unwrap().name,
            "Emberwing Scout"
        );
        assert_eq!(
            catalog.by_id("OGN-007").unwrap().unwrap().name,
            "Emberwing Scout"
        );
        assert!(catalog.by_id("ogn-999-298").unwrap().is_none());
    }

    #[test]
    fn the_cache_keys_names_and_ids_apart() {
        struct Counting {
            inner: StaticCatalog,
            calls: usize,
        }
        impl crate::deck::CardLookup<Riftbound> for Counting {
            fn find(
                &mut self,
                identifier: &Identifier,
            ) -> Result<Option<CatalogCard>, LookupError> {
                self.calls += 1;
                self.inner.find(identifier)
            }
        }
        let mut cached = Cached::new(Counting {
            inner: test_catalog(),
            calls: 0,
        });
        for _ in 0..3 {
            assert!(cached.by_name("Emberwing Scout").unwrap().is_some());
            assert!(cached.by_name("emberwing  SCOUT!").unwrap().is_some());
            assert!(cached.by_id("ogn-042-298").unwrap().is_some());
            assert!(cached
                .by_code(&CardCode::parse("OGN-042").unwrap())
                .unwrap()
                .is_some());
        }
        assert_eq!(cached.inner().calls, 3);
    }

    #[test]
    fn a_layered_lookup_falls_through_on_a_miss_and_on_a_padded_hit() {
        let padded = CatalogCard {
            name: "Emberwing Scout".into(),
            riftbound_id: "ogn-007-298".into(),
            ..Default::default()
        };
        assert!(padded.is_padded());
        let mut layered = Layered {
            first: StaticCatalog::new(vec![padded.clone()]),
            second: test_catalog(),
        };
        let hit = layered.by_id("ogn-007-298").unwrap().unwrap();
        assert_eq!(
            hit.kind,
            CardKind::Unit,
            "the padded record defers to the full one"
        );
        let hit = layered.by_name("Gloomvale Trickster").unwrap().unwrap();
        assert_eq!(
            hit.riftbound_id, "ogn-101-298",
            "a miss reaches the second lookup"
        );
        let mut only_padded = Layered {
            first: StaticCatalog::new(vec![padded.clone()]),
            second: StaticCatalog::new(Vec::new()),
        };
        let hit = only_padded.by_id("ogn-007-298").unwrap().unwrap();
        assert!(
            hit.is_padded(),
            "a padded record still answers when nothing better exists"
        );
        assert!(only_padded.by_name("Unknown Card").unwrap().is_none());
    }
}

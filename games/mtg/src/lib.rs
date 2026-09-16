use agni_core::CardFace;
use agni_sim::wire::{
    CounterDecl, CounterPlace, CounterScope, CounterSpec, DealGroup, DealTarget, ZoneDecl,
    ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneSpec, ZoneVisibility,
};

pub const GAME: &str = "mtg";
pub const CARD_BACK_URL: &str =
    "https://backs.scryfall.io/large/0/a/0aeebaf5-8c7d-4636-9e82-8c27447861f7.jpg";

pub const MIN_MAIN_DECK: usize = 60;
pub const MAX_SIDEBOARD: usize = 15;
pub const OPENING_HAND_SIZE: u32 = 7;

pub const ZONE_NAME_HAND: &str = "hand";
pub const ZONE_NAME_LIBRARY: &str = "library";
pub const ZONE_NAME_GRAVEYARD: &str = "graveyard";
pub const ZONE_NAME_EXILE: &str = "exile";
pub const ZONE_NAME_BATTLEFIELD: &str = "battlefield";
pub const ZONE_NAME_COMMAND: &str = "command";

pub const ZONE_HAND: u16 = 0;
pub const ZONE_LIBRARY: u16 = 1;
pub const ZONE_GRAVEYARD: u16 = 2;
pub const ZONE_EXILE: u16 = 3;
pub const ZONE_BATTLEFIELD: u16 = 4;
pub const ZONE_COMMAND: u16 = 5;

const ZONES: [ZoneSpec; 6] = [
    ZoneSpec {
        id: ZONE_HAND,
        name: ZONE_NAME_HAND,
        label: "Hand",
        kind: ZoneKind::Hand,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::Owner,
        layout: ZoneLayout::Fan,
        place: ZonePlace::Fan,
        span: 1,
    },
    ZoneSpec {
        id: ZONE_LIBRARY,
        name: ZONE_NAME_LIBRARY,
        label: "Library",
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_GRAVEYARD,
        name: ZONE_NAME_GRAVEYARD,
        label: "Graveyard",
        kind: ZoneKind::Discard,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_EXILE,
        name: ZONE_NAME_EXILE,
        label: "Exile",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Outer,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_BATTLEFIELD,
        name: ZONE_NAME_BATTLEFIELD,
        label: "Battlefield",
        kind: ZoneKind::Battlefield,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Inner,
        span: 16,
    },
    ZoneSpec {
        id: ZONE_COMMAND,
        name: ZONE_NAME_COMMAND,
        label: "Command",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Inner,
        span: 4,
    },
];

pub fn zone_table() -> Vec<ZoneDecl> {
    ZONES.iter().map(ZoneDecl::from).collect()
}

pub const COUNTER_LIFE: u16 = 0;
pub const COUNTER_POISON: u16 = 1;
pub const COUNTER_PLUS_ONE: u16 = 2;
pub const COUNTER_MINUS_ONE: u16 = 3;
pub const COUNTER_LOYALTY: u16 = 4;

pub const STARTING_LIFE: i32 = 20;
pub const LETHAL_POISON: i32 = 10;

const COUNTERS: [CounterSpec; 5] = [
    CounterSpec {
        id: COUNTER_LIFE,
        name: "life",
        color: [232, 230, 224],
        label: "Life",
        scope: CounterScope::Seat,
        start: STARTING_LIFE,
        min: None,
        max: None,
        step: 1,
        place: CounterPlace::SeatPlate,
    },
    CounterSpec {
        id: COUNTER_POISON,
        name: "poison",
        color: [26, 26, 30],
        label: "Poison",
        scope: CounterScope::Seat,
        start: 0,
        min: Some(0),
        max: Some(LETHAL_POISON),
        step: 1,
        place: CounterPlace::SeatPlate,
    },
    CounterSpec {
        id: COUNTER_PLUS_ONE,
        name: "plus-one",
        color: [92, 170, 104],
        label: "+1/+1",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_MINUS_ONE,
        name: "minus-one",
        color: [190, 80, 70],
        label: "-1/-1",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_LOYALTY,
        name: "loyalty",
        color: [124, 112, 194],
        label: "Loyalty",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
];

pub fn counter_table() -> Vec<CounterDecl> {
    COUNTERS.iter().map(CounterDecl::from).collect()
}

pub use agni_deck::CardName;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MtgDeck {
    pub main_deck: Vec<CardName>,
    pub sideboard: Vec<CardName>,
    pub commander: Option<CardName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCard {
    pub name: String,
    pub image_url: Option<String>,
}

pub type DeckEntry = agni_deck::DeckEntry<ResolvedCard>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedDeck {
    pub commander: Option<ResolvedCard>,
    pub main_deck: Vec<DeckEntry>,
    pub sideboard: Vec<DeckEntry>,
}

fn flatten(entries: &[DeckEntry]) -> Vec<CardName> {
    agni_deck::flatten(entries, |card| card.name.as_str())
}

impl ResolvedDeck {
    pub fn names(&self) -> MtgDeck {
        MtgDeck {
            main_deck: flatten(&self.main_deck),
            sideboard: flatten(&self.sideboard),
            commander: self
                .commander
                .as_ref()
                .map(|card| CardName(card.name.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeckFaces {
    pub commander: Option<CardFace>,
    pub library: Vec<CardFace>,
}

pub fn deal_plan(deck: &DeckFaces) -> Vec<DealGroup> {
    let mut plan = Vec::new();
    if let Some(commander) = &deck.commander {
        plan.push(DealGroup {
            target: DealTarget::Zone(ZONE_NAME_COMMAND.into()),
            faces: vec![commander.clone()],
            shuffle: false,
            draw: 0,
        });
    }
    if !deck.library.is_empty() {
        plan.push(DealGroup {
            target: DealTarget::Zone(ZONE_NAME_LIBRARY.into()),
            faces: deck.library.clone(),
            shuffle: true,
            draw: OPENING_HAND_SIZE,
        });
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{decode_zone_table, encode_zone_table};
    use std::collections::BTreeSet;

    #[test]
    fn the_zone_table_pins_the_full_mtg_anatomy() {
        let zones = zone_table();
        assert_eq!(zones.len(), 6);
        let ids: BTreeSet<u16> = zones.iter().map(|decl| decl.id).collect();
        assert_eq!(ids.len(), zones.len());
        assert!(zones.iter().all(|decl| decl.owner == ZoneOwner::PerSeat));
        let by_name = |name: &str| zones.iter().find(|decl| decl.name == name).unwrap();
        assert_eq!(by_name("hand").visibility, ZoneVisibility::Owner);
        assert_eq!(by_name("hand").layout, ZoneLayout::Fan);
        assert_eq!(by_name("library").visibility, ZoneVisibility::None);
        assert_eq!(by_name("library").kind, ZoneKind::Deck);
        assert_eq!(by_name("graveyard").visibility, ZoneVisibility::All);
        assert_eq!(by_name("graveyard").kind, ZoneKind::Discard);
        assert_eq!(by_name("exile").visibility, ZoneVisibility::All);
        assert_eq!(by_name("exile").layout, ZoneLayout::Row);
        assert_eq!(by_name("battlefield").kind, ZoneKind::Battlefield);
        assert_eq!(by_name("command").visibility, ZoneVisibility::All);
    }

    #[test]
    fn the_zone_table_round_trips_through_its_cbor_schema() {
        let zones = zone_table();
        let bytes = encode_zone_table(&zones);
        assert_eq!(decode_zone_table(&bytes).unwrap(), zones);
        assert_eq!(
            encode_zone_table(&decode_zone_table(&bytes).unwrap()),
            bytes
        );
    }

    #[test]
    fn a_deck_holds_its_zones() {
        let deck = MtgDeck {
            main_deck: vec![CardName("Thornspire Adept".into()); 4],
            sideboard: vec![CardName("Cinderveil Ward".into())],
            commander: None,
        };
        assert_eq!(deck.main_deck.len(), 4);
        assert_eq!(deck.sideboard.len(), 1);
        assert!(deck.commander.is_none());
    }

    #[test]
    fn a_resolved_deck_flattens_to_names() {
        let card = |name: &str| ResolvedCard {
            name: name.into(),
            image_url: Some(format!("https://img.example/{name}.jpg")),
        };
        let deck = ResolvedDeck {
            commander: Some(card("Serelith, Tidebound Oracle")),
            main_deck: vec![
                DeckEntry {
                    card: card("Thornspire Adept"),
                    count: 4,
                },
                DeckEntry {
                    card: card("Mistfen Causeway"),
                    count: 2,
                },
            ],
            sideboard: vec![DeckEntry {
                card: card("Cinderveil Ward"),
                count: 3,
            }],
        };
        let names = deck.names();
        assert_eq!(
            names.commander,
            Some(CardName("Serelith, Tidebound Oracle".into()))
        );
        assert_eq!(names.main_deck.len(), 6);
        assert_eq!(names.main_deck[0], CardName("Thornspire Adept".into()));
        assert_eq!(names.main_deck[4], CardName("Mistfen Causeway".into()));
        assert_eq!(names.sideboard.len(), 3);
    }

    #[test]
    fn the_deal_plan_seats_commander_then_library_with_a_seven_card_draw() {
        let face = |name: &str| CardFace::named(name);
        let deck = DeckFaces {
            commander: Some(face("Serelith, Tidebound Oracle")),
            library: (0..MIN_MAIN_DECK)
                .map(|i| face(&format!("main {i}")))
                .collect(),
        };
        let plan = deal_plan(&deck);
        let targets: Vec<(&DealTarget, usize, bool, u32)> = plan
            .iter()
            .map(|group| (&group.target, group.faces.len(), group.shuffle, group.draw))
            .collect();
        assert_eq!(
            targets,
            vec![
                (&DealTarget::Zone(ZONE_NAME_COMMAND.into()), 1, false, 0),
                (
                    &DealTarget::Zone(ZONE_NAME_LIBRARY.into()),
                    MIN_MAIN_DECK,
                    true,
                    OPENING_HAND_SIZE
                ),
            ]
        );
        let table = zone_table();
        for group in &plan {
            let DealTarget::Zone(name) = &group.target else {
                panic!("mtg deals target named zones only");
            };
            assert!(table.iter().any(|decl| &decl.name == name));
        }
        let commanderless = DeckFaces {
            commander: None,
            library: vec![face("solo card")],
        };
        assert_eq!(deal_plan(&commanderless).len(), 1);
        assert!(deal_plan(&DeckFaces::default()).is_empty());
    }
}

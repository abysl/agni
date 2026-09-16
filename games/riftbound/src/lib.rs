use agni_core::CardFace;
use agni_sim::wire::{
    CounterDecl, CounterPlace, CounterScope, CounterSpec, DealGroup, DealTarget, TokenDecl,
    ZoneDecl, ZoneKind, ZoneLayout, ZoneOwner, ZonePlace, ZoneSpec, ZoneVisibility,
};
use std::collections::BTreeMap;

pub mod legality;

pub const GAME: &str = "riftbound";
pub const CARD_BACK_URL: &str = "https://cdn.piltoverarchive.com/Cardback.webp";

pub const ZONE_NAME_HAND: &str = "hand";
pub const ZONE_NAME_MAIN_DECK: &str = "main-deck";
pub const ZONE_NAME_RUNE_DECK: &str = "rune-deck";
pub const ZONE_NAME_RUNE_POOL: &str = "rune-pool";
pub const ZONE_NAME_BASE: &str = "base";
pub const ZONE_NAME_LEGEND: &str = "legend";
pub const ZONE_NAME_CHAMPION: &str = "champion";
pub const ZONE_NAME_TRASH: &str = "trash";
pub const ZONE_NAME_SIDEBOARD: &str = "sideboard";
pub const ZONE_NAME_CHAIN: &str = "chain";
pub const ZONE_NAME_BANISHMENT: &str = "banishment";
pub const BATTLEFIELD_PREFIX: &str = "battlefield-";

pub const MAIN_DECK_SIZE: usize = 40;
pub const RUNE_DECK_SIZE: usize = 12;
pub const BATTLEFIELD_COUNT: usize = 3;
pub const OPENING_HAND_SIZE: u32 = 4;

pub const ZONE_HAND: u16 = 0;
pub const ZONE_MAIN_DECK: u16 = 1;
pub const ZONE_RUNE_DECK: u16 = 2;
pub const ZONE_LEGEND: u16 = 3;
pub const ZONE_CHAMPION: u16 = 4;
pub const ZONE_TRASH: u16 = 5;
pub const ZONE_SIDEBOARD: u16 = 6;
pub const ZONE_RUNE_POOL: u16 = 7;
pub const ZONE_BASE: u16 = 8;
pub const ZONE_BATTLEFIELD_FIRST: u16 = 9;
pub const ZONE_CHAIN: u16 = ZONE_BATTLEFIELD_FIRST + BATTLEFIELD_COUNT as u16;
pub const ZONE_BANISHMENT: u16 = ZONE_CHAIN + 1;

const SEAT_ZONES: [ZoneSpec; 10] = [
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
        id: ZONE_BASE,
        name: ZONE_NAME_BASE,
        label: "Base",
        kind: ZoneKind::Battlefield,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Inner,
        span: 16,
    },
    ZoneSpec {
        id: ZONE_LEGEND,
        name: ZONE_NAME_LEGEND,
        label: "Legend",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Inner,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_CHAMPION,
        name: ZONE_NAME_CHAMPION,
        label: "Champion",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Inner,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_TRASH,
        name: ZONE_NAME_TRASH,
        label: "Trash",
        kind: ZoneKind::Discard,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 3,
    },
    ZoneSpec {
        id: ZONE_BANISHMENT,
        name: ZONE_NAME_BANISHMENT,
        label: "Banished",
        kind: ZoneKind::Discard,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 3,
    },
    ZoneSpec {
        id: ZONE_RUNE_POOL,
        name: ZONE_NAME_RUNE_POOL,
        label: "Runes",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Outer,
        span: 13,
    },
    ZoneSpec {
        id: ZONE_RUNE_DECK,
        name: ZONE_NAME_RUNE_DECK,
        label: "Rune Deck",
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_MAIN_DECK,
        name: ZONE_NAME_MAIN_DECK,
        label: "Main Deck",
        kind: ZoneKind::Deck,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::None,
        layout: ZoneLayout::Pile,
        place: ZonePlace::Outer,
        span: 4,
    },
    ZoneSpec {
        id: ZONE_SIDEBOARD,
        name: ZONE_NAME_SIDEBOARD,
        label: "Sideboard",
        kind: ZoneKind::Aux,
        owner: ZoneOwner::PerSeat,
        visibility: ZoneVisibility::Owner,
        layout: ZoneLayout::Grid,
        place: ZonePlace::Offstage,
        span: 1,
    },
];

pub fn zone_table() -> Vec<ZoneDecl> {
    let mut zones: Vec<ZoneDecl> = SEAT_ZONES.iter().map(ZoneDecl::from).collect();
    for slot in 0..BATTLEFIELD_COUNT as u16 {
        zones.push(ZoneDecl {
            id: ZONE_BATTLEFIELD_FIRST + slot,
            name: format!("{BATTLEFIELD_PREFIX}{}", slot + 1),
            kind: ZoneKind::Battlefield,
            owner: ZoneOwner::Shared,
            visibility: ZoneVisibility::All,
            layout: ZoneLayout::Row,
            place: ZonePlace::Center,
            span: 1,
            label: format!("Battlefield {}", slot + 1),
        });
    }
    zones.push(ZoneDecl {
        id: ZONE_CHAIN,
        name: ZONE_NAME_CHAIN.into(),
        kind: ZoneKind::Stack,
        owner: ZoneOwner::Shared,
        visibility: ZoneVisibility::All,
        layout: ZoneLayout::Row,
        place: ZonePlace::Offstage,
        span: 1,
        label: "Chain".into(),
    });
    zones
}

pub const COUNTER_POINTS: u16 = 0;
pub const COUNTER_XP: u16 = 1;
pub const COUNTER_MIGHT: u16 = 2;
pub const COUNTER_DAMAGE: u16 = 3;
pub const COUNTER_TEMPORARY: u16 = 4;
pub const COUNTER_BUFFED: u16 = 5;
pub const COUNTER_EMPOWERED: u16 = 6;

const COUNTERS: [CounterSpec; 7] = [
    CounterSpec {
        id: COUNTER_POINTS,
        name: "points",
        color: [214, 176, 80],
        label: "Points",
        scope: CounterScope::Seat,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::SeatPlate,
    },
    CounterSpec {
        id: COUNTER_XP,
        name: "xp",
        color: [92, 150, 212],
        label: "XP",
        scope: CounterScope::Seat,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::SeatPlate,
    },
    CounterSpec {
        id: COUNTER_MIGHT,
        name: "might",
        color: [96, 176, 108],
        label: "Might",
        scope: CounterScope::Card,
        start: 0,
        min: None,
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_DAMAGE,
        name: "damage",
        color: [200, 70, 62],
        label: "Damage",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_TEMPORARY,
        name: "temporary",
        color: [160, 120, 210],
        label: "Temporary",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: Some(1),
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_BUFFED,
        name: "buffed",
        color: [80, 170, 200],
        label: "Buffed",
        scope: CounterScope::Card,
        start: 0,
        min: Some(0),
        max: None,
        step: 1,
        place: CounterPlace::CardBadge,
    },
    CounterSpec {
        id: COUNTER_EMPOWERED,
        name: "empowered",
        color: [230, 150, 60],
        label: "Empowered",
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
pub struct RiftboundDeck {
    pub legend: Option<CardName>,
    pub chosen_champion: Option<CardName>,
    pub main_deck: Vec<CardName>,
    pub runes: Vec<CardName>,
    pub battlefields: Vec<CardName>,
    pub sideboard: Vec<CardName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckShape {
    pub has_legend: bool,
    pub has_chosen_champion: bool,
    pub main_deck: usize,
    pub runes: usize,
    pub battlefields: usize,
    pub sideboard: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedCard {
    pub name: String,
    pub riftbound_id: String,
    pub image_url: Option<String>,
    pub kind: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
    pub tags: Vec<String>,
    pub signature: bool,
}

pub const KIND_UNIT: &str = "Unit";
pub const KIND_LEGEND: &str = "Legend";
pub const KIND_RUNE: &str = "Rune";
pub const KIND_BATTLEFIELD: &str = "Battlefield";

pub fn token_table() -> Vec<TokenDecl> {
    let unit = |name: &str, might: u8, art: Option<&str>| TokenDecl {
        name: name.into(),
        kind: KIND_UNIT.into(),
        might: Some(might),
        art: art.map(str::to_string),
        temporary: false,
    };
    vec![
        TokenDecl {
            temporary: true,
            ..unit("Sprite", 3, Some("ogn-274-298"))
        },
        unit("Recruit", 1, Some("ogn-272-298")),
        unit("Bird", 1, Some("unl-t02")),
        unit("Sand Soldier", 2, Some("sfd-t02")),
        unit("Mech", 3, Some("sfd-t01")),
        unit("Shadow Clone", 0, Some("ven-t05")),
        unit("Tentacle", 1, Some("ven-t06")),
        TokenDecl {
            name: "Gold".into(),
            kind: "Gear".into(),
            might: None,
            art: Some("sfd-t03".into()),
            temporary: false,
        },
    ]
}

impl ResolvedCard {
    pub fn is_unit(&self) -> bool {
        self.kind.as_deref() == Some(KIND_UNIT)
    }
}

pub type DeckEntry = agni_deck::DeckEntry<ResolvedCard>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedDeck {
    pub legend: Option<ResolvedCard>,
    pub chosen_champion: Option<ResolvedCard>,
    pub main_deck: Vec<DeckEntry>,
    pub runes: Vec<DeckEntry>,
    pub battlefields: Vec<DeckEntry>,
    pub sideboard: Vec<DeckEntry>,
}

fn flatten(entries: &[DeckEntry]) -> Vec<CardName> {
    agni_deck::flatten(entries, |card| card.name.as_str())
}

impl ResolvedDeck {
    pub fn names(&self) -> RiftboundDeck {
        RiftboundDeck {
            legend: self.legend.as_ref().map(|card| CardName(card.name.clone())),
            chosen_champion: self
                .chosen_champion
                .as_ref()
                .map(|card| CardName(card.name.clone())),
            main_deck: flatten(&self.main_deck),
            runes: flatten(&self.runes),
            battlefields: flatten(&self.battlefields),
            sideboard: flatten(&self.sideboard),
        }
    }
}

impl RiftboundDeck {
    pub fn shape(&self) -> DeckShape {
        DeckShape {
            has_legend: self.legend.is_some(),
            has_chosen_champion: self.chosen_champion.is_some(),
            main_deck: self.main_deck.len(),
            runes: self.runes.len(),
            battlefields: self.battlefields.len(),
            sideboard: self.sideboard.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeckFaces {
    pub legend: Option<CardFace>,
    pub chosen_champion: Option<CardFace>,
    pub main_deck: Vec<CardFace>,
    pub runes: Vec<CardFace>,
    pub battlefields: Vec<CardFace>,
    pub sideboard: Vec<CardFace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub name: &'static str,
    pub players: u8,
    pub victory_score: i32,
    pub battlefield_count: u8,
    pub first_player_contributes: bool,
    pub random_selection: bool,
}

pub const MODES: [Mode; 5] = [
    Mode {
        name: "1v1 (Duel)",
        players: 2,
        victory_score: 8,
        battlefield_count: 2,
        first_player_contributes: true,
        random_selection: true,
    },
    Mode {
        name: "1v1 (Match)",
        players: 2,
        victory_score: 8,
        battlefield_count: 2,
        first_player_contributes: true,
        random_selection: false,
    },
    Mode {
        name: "FFA3 (Skirmish)",
        players: 3,
        victory_score: 8,
        battlefield_count: 3,
        first_player_contributes: true,
        random_selection: true,
    },
    Mode {
        name: "FFA4 (War)",
        players: 4,
        victory_score: 8,
        battlefield_count: 3,
        first_player_contributes: false,
        random_selection: true,
    },
    Mode {
        name: "2v2 (Magma Chamber)",
        players: 4,
        victory_score: 11,
        battlefield_count: 3,
        first_player_contributes: false,
        random_selection: true,
    },
];

pub fn default_mode(players: u8) -> Option<&'static Mode> {
    MODES.iter().find(|mode| mode.players == players)
}

pub const OPTION_VICTORY_SCORE: &str = "victory_score";
pub const OPTION_BATTLEFIELDS: &str = "battlefields";
pub const OPTION_RULES_ENFORCED: &str = "rules_enforced";
pub const DEFAULT_VICTORY_SCORE: i32 = 8;
pub const DEFAULT_BATTLEFIELDS: u8 = 2;
pub const VICTORY_SCORE_RANGE: std::ops::RangeInclusive<i32> = 1..=20;
pub const BATTLEFIELDS_RANGE: std::ops::RangeInclusive<u8> = 1..=BATTLEFIELD_COUNT as u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableOptions {
    pub victory_score: i32,
    pub battlefields: u8,
}

impl Default for TableOptions {
    fn default() -> Self {
        Self {
            victory_score: DEFAULT_VICTORY_SCORE,
            battlefields: DEFAULT_BATTLEFIELDS,
        }
    }
}

impl TableOptions {
    pub fn of_mode(mode: &Mode) -> Self {
        Self {
            victory_score: mode.victory_score,
            battlefields: mode.battlefield_count,
        }
    }

    pub fn for_players(players: u8) -> Self {
        default_mode(players).map(Self::of_mode).unwrap_or_default()
    }

    pub fn clamped(self) -> Self {
        Self {
            victory_score: self
                .victory_score
                .clamp(*VICTORY_SCORE_RANGE.start(), *VICTORY_SCORE_RANGE.end()),
            battlefields: self
                .battlefields
                .clamp(*BATTLEFIELDS_RANGE.start(), *BATTLEFIELDS_RANGE.end()),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut map: BTreeMap<&str, i64> = BTreeMap::new();
        map.insert(OPTION_VICTORY_SCORE, i64::from(self.victory_score));
        map.insert(OPTION_BATTLEFIELDS, i64::from(self.battlefields));
        agni_sim::abi::encode(&map)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let map: BTreeMap<String, i64> = agni_sim::abi::decode(bytes)?;
        let defaults = Self::default();
        Some(Self {
            victory_score: map
                .get(OPTION_VICTORY_SCORE)
                .map(|value| i32::try_from(*value).unwrap_or(defaults.victory_score))
                .unwrap_or(defaults.victory_score),
            battlefields: map
                .get(OPTION_BATTLEFIELDS)
                .map(|value| u8::try_from(*value).unwrap_or(defaults.battlefields))
                .unwrap_or(defaults.battlefields),
        })
    }

    pub fn of_config<B: AsRef<[u8]>>(options: Option<B>) -> Self {
        options
            .and_then(|bytes| Self::decode(bytes.as_ref()))
            .unwrap_or_default()
    }

    pub fn genesis(chosen: Option<Self>, enforced: bool) -> Option<Vec<u8>> {
        if chosen.is_none() && !enforced {
            return None;
        }
        let mut map: BTreeMap<&str, i64> = BTreeMap::new();
        if let Some(options) = chosen {
            map.insert(OPTION_VICTORY_SCORE, i64::from(options.victory_score));
            map.insert(OPTION_BATTLEFIELDS, i64::from(options.battlefields));
        }
        if enforced {
            map.insert(OPTION_RULES_ENFORCED, 1);
        }
        Some(agni_sim::abi::encode(&map))
    }

    pub fn enforced_in<B: AsRef<[u8]>>(options: Option<B>) -> bool {
        let map: BTreeMap<String, i64> = options
            .and_then(|bytes| agni_sim::abi::decode(bytes.as_ref()))
            .unwrap_or_default();
        map.get(OPTION_RULES_ENFORCED)
            .is_some_and(|value| *value >= 1)
    }

    pub fn in_play<B: AsRef<[u8]>>(options: Option<B>, players: u8) -> Self {
        let map: BTreeMap<String, i64> = options
            .and_then(|bytes| agni_sim::abi::decode(bytes.as_ref()))
            .unwrap_or_default();
        let victory_score = map
            .get(OPTION_VICTORY_SCORE)
            .and_then(|value| i32::try_from(*value).ok())
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_VICTORY_SCORE);
        let battlefields = map
            .get(OPTION_BATTLEFIELDS)
            .and_then(|value| usize::try_from(*value).ok())
            .filter(|value| *value >= 1)
            .unwrap_or_else(|| usize::from(DEFAULT_BATTLEFIELDS).max(usize::from(players)))
            .min(BATTLEFIELD_COUNT);
        Self {
            victory_score,
            battlefields: battlefields as u8,
        }
    }

    pub fn label(&self) -> String {
        let plural = if self.battlefields == 1 { "" } else { "s" };
        format!(
            "first to {} · {} battlefield{plural}",
            self.victory_score, self.battlefields
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Battlefields {
    All,
    One(usize),
    Many { pick: usize, count: usize },
    None,
}

pub fn contribution(mode: &Mode, seat: u8, first_player: u8, pick: usize) -> Battlefields {
    contribution_of(
        mode.battlefield_count,
        mode.players,
        seat,
        first_player,
        pick,
    )
}

pub fn contribution_of(
    count: u8,
    players: u8,
    seat: u8,
    first_player: u8,
    pick: usize,
) -> Battlefields {
    if players == 0 {
        return Battlefields::One(pick);
    }
    let base = count / players;
    let extra = count % players;
    let rank = (u16::from(seat) + u16::from(players) - u16::from(first_player % players) - 1)
        % u16::from(players);
    let share = base + u8::from(rank < u16::from(extra));
    match usize::from(share) {
        0 => Battlefields::None,
        1 => Battlefields::One(pick),
        count => Battlefields::Many { pick, count },
    }
}

pub fn deal_plan(deck: &DeckFaces) -> Vec<DealGroup> {
    deal_plan_with(deck, Battlefields::All)
}

pub fn deal_plan_with(deck: &DeckFaces, battlefields: Battlefields) -> Vec<DealGroup> {
    fn single(face: &Option<CardFace>) -> Option<&[CardFace]> {
        face.as_ref().map(std::slice::from_ref)
    }
    let mut plan = Vec::new();
    let mut push = |target: DealTarget, faces: Option<&[CardFace]>, shuffle: bool, draw: u32| {
        let Some(faces) = faces else {
            return;
        };
        if faces.is_empty() {
            return;
        }
        plan.push(DealGroup {
            target,
            faces: faces.to_vec(),
            shuffle,
            draw,
        });
    };
    push(
        DealTarget::Zone(ZONE_NAME_LEGEND.into()),
        single(&deck.legend),
        false,
        0,
    );
    push(
        DealTarget::Zone(ZONE_NAME_CHAMPION.into()),
        single(&deck.chosen_champion),
        false,
        0,
    );
    push(
        DealTarget::Zone(ZONE_NAME_RUNE_DECK.into()),
        Some(&deck.runes),
        true,
        0,
    );
    push(
        DealTarget::Zone(ZONE_NAME_MAIN_DECK.into()),
        Some(&deck.main_deck),
        true,
        OPENING_HAND_SIZE,
    );
    let contributed: Vec<CardFace> = match battlefields {
        Battlefields::All => deck.battlefields.clone(),
        Battlefields::None => Vec::new(),
        Battlefields::One(index) => deck
            .battlefields
            .get(index)
            .or_else(|| deck.battlefields.first())
            .cloned()
            .into_iter()
            .collect(),
        Battlefields::Many { pick, count } => {
            let lead = if pick < deck.battlefields.len() {
                pick
            } else {
                0
            };
            let mut order = vec![lead];
            order.extend((0..deck.battlefields.len()).filter(|index| *index != lead));
            order
                .into_iter()
                .take(count)
                .filter_map(|index| deck.battlefields.get(index).cloned())
                .collect()
        }
    };
    push(
        DealTarget::Spread(BATTLEFIELD_PREFIX.into()),
        Some(&contributed),
        false,
        0,
    );
    push(
        DealTarget::Zone(ZONE_NAME_SIDEBOARD.into()),
        Some(&deck.sideboard),
        false,
        0,
    );
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{decode_zone_table, encode_zone_table};
    use std::collections::BTreeSet;

    #[test]
    fn the_zone_table_pins_the_full_riftbound_anatomy() {
        let zones = zone_table();
        assert_eq!(zones.len(), 10 + BATTLEFIELD_COUNT + 1);
        let banishment = zones
            .iter()
            .find(|decl| decl.id == ZONE_BANISHMENT)
            .unwrap();
        assert_eq!(banishment.name, ZONE_NAME_BANISHMENT);
        assert_eq!(banishment.kind, ZoneKind::Discard);
        assert_eq!(banishment.owner, ZoneOwner::PerSeat);
        assert_eq!(banishment.visibility, ZoneVisibility::All);
        assert_eq!(banishment.layout, ZoneLayout::Pile);
        assert_eq!(banishment.label, "Banished");
        assert_eq!(ZONE_BANISHMENT, 13);
        let chain = zones.iter().find(|decl| decl.id == ZONE_CHAIN).unwrap();
        assert_eq!(chain.kind, ZoneKind::Stack);
        assert_eq!(chain.place, ZonePlace::Offstage);
        assert_eq!(chain.owner, ZoneOwner::Shared);
        let ids: BTreeSet<u16> = zones.iter().map(|decl| decl.id).collect();
        assert_eq!(ids.len(), zones.len());
        let by_name = |name: &str| zones.iter().find(|decl| decl.name == name).unwrap();
        assert_eq!(by_name("hand").visibility, ZoneVisibility::Owner);
        assert_eq!(by_name("main-deck").visibility, ZoneVisibility::None);
        assert_eq!(by_name("rune-deck").visibility, ZoneVisibility::None);
        assert_eq!(by_name("rune-pool").visibility, ZoneVisibility::All);
        assert_eq!(by_name("base").visibility, ZoneVisibility::All);
        assert_eq!(by_name("legend").visibility, ZoneVisibility::All);
        assert_eq!(by_name("champion").visibility, ZoneVisibility::All);
        assert_eq!(by_name("trash").visibility, ZoneVisibility::All);
        assert_eq!(by_name("sideboard").visibility, ZoneVisibility::Owner);
        for slot in 1..=BATTLEFIELD_COUNT {
            let battlefield = by_name(&format!("battlefield-{slot}"));
            assert_eq!(battlefield.owner, ZoneOwner::Shared);
            assert_eq!(battlefield.visibility, ZoneVisibility::All);
        }
        assert!(zones
            .iter()
            .filter(
                |decl| !decl.name.starts_with(BATTLEFIELD_PREFIX) && decl.name != ZONE_NAME_CHAIN
            )
            .all(|decl| decl.owner == ZoneOwner::PerSeat));
    }

    #[test]
    fn the_zone_table_seats_every_zone_where_riftatlas_puts_it() {
        let zones = zone_table();
        let place = |name: &str| zones.iter().find(|decl| decl.name == name).unwrap().place;
        assert_eq!(place(ZONE_NAME_HAND), ZonePlace::Fan);
        assert_eq!(place(ZONE_NAME_SIDEBOARD), ZonePlace::Offstage);
        for name in [ZONE_NAME_BASE, ZONE_NAME_LEGEND, ZONE_NAME_CHAMPION] {
            assert_eq!(place(name), ZonePlace::Inner);
        }
        for name in [
            ZONE_NAME_TRASH,
            ZONE_NAME_BANISHMENT,
            ZONE_NAME_RUNE_POOL,
            ZONE_NAME_RUNE_DECK,
            ZONE_NAME_MAIN_DECK,
        ] {
            assert_eq!(place(name), ZonePlace::Outer);
        }
        for slot in 1..=BATTLEFIELD_COUNT {
            assert_eq!(
                place(&format!("{BATTLEFIELD_PREFIX}{slot}")),
                ZonePlace::Center
            );
        }
        let row: Vec<&str> = zones
            .iter()
            .filter(|decl| decl.place == ZonePlace::Inner)
            .map(|decl| decl.name.as_str())
            .collect();
        assert_eq!(row, ["base", "legend", "champion"]);
        let row: Vec<&str> = zones
            .iter()
            .filter(|decl| decl.place == ZonePlace::Outer)
            .map(|decl| decl.name.as_str())
            .collect();
        assert_eq!(
            row,
            ["trash", "banishment", "rune-pool", "rune-deck", "main-deck"],
            "the banish pile sits beside the trash"
        );
    }

    #[test]
    fn the_token_table_declares_the_shadow_clone_and_the_tentacle() {
        let tokens = token_table();
        let by_name = |name: &str| tokens.iter().find(|decl| decl.name == name).unwrap();
        assert_eq!(by_name("Shadow Clone").might, Some(0));
        assert_eq!(by_name("Tentacle").might, Some(1));
        assert_eq!(by_name("Sand Soldier").might, Some(2));
        for name in ["Shadow Clone", "Tentacle", "Sand Soldier"] {
            let decl = by_name(name);
            assert_eq!(decl.kind, KIND_UNIT);
            assert!(decl.art.is_some());
            assert!(!decl.temporary);
        }
        let mut names: Vec<&str> = tokens.iter().map(|decl| decl.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tokens.len(), "every token name is unique");
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
    fn a_deck_shape_reports_every_zone() {
        let deck = RiftboundDeck {
            legend: Some(CardName("Viktor".into())),
            chosen_champion: Some(CardName("Viktor, Herald of Progress".into())),
            main_deck: (0..MAIN_DECK_SIZE)
                .map(|i| CardName(format!("main {i}")))
                .collect(),
            runes: (0..RUNE_DECK_SIZE)
                .map(|i| CardName(format!("rune {i}")))
                .collect(),
            battlefields: (0..BATTLEFIELD_COUNT)
                .map(|i| CardName(format!("battlefield {i}")))
                .collect(),
            sideboard: Vec::new(),
        };
        let shape = deck.shape();
        assert!(shape.has_legend);
        assert!(shape.has_chosen_champion);
        assert_eq!(shape.main_deck, MAIN_DECK_SIZE);
        assert_eq!(shape.runes, RUNE_DECK_SIZE);
        assert_eq!(shape.battlefields, BATTLEFIELD_COUNT);
        assert_eq!(shape.sideboard, 0);
    }

    #[test]
    fn the_sanctioned_modes_match_the_core_rules() {
        let duel = default_mode(2).expect("2 players is a sanctioned mode");
        assert_eq!(duel.name, "1v1 (Duel)");
        assert_eq!(duel.battlefield_count, 2);
        assert_eq!(duel.victory_score, 8);
        assert!(duel.first_player_contributes);

        let skirmish = default_mode(3).expect("3 players is a sanctioned mode");
        assert_eq!(skirmish.battlefield_count, 3);
        assert!(skirmish.first_player_contributes);

        let war = default_mode(4).expect("4 players is a sanctioned mode");
        assert_eq!(war.name, "FFA4 (War)");
        assert_eq!(war.battlefield_count, 3);
        assert!(!war.first_player_contributes);

        assert!(default_mode(1).is_none());
        assert!(default_mode(5).is_none());
    }

    #[test]
    fn the_two_v_two_mode_scores_to_eleven() {
        let magma = MODES
            .iter()
            .find(|mode| mode.name == "2v2 (Magma Chamber)")
            .expect("magma chamber is sanctioned");
        assert_eq!(magma.victory_score, 11);
        assert_eq!(magma.battlefield_count, 3);
        assert!(!magma.first_player_contributes);
    }

    #[test]
    fn the_points_counter_has_no_cap_because_the_victory_score_is_the_tables() {
        let points = counter_table()
            .into_iter()
            .find(|decl| decl.id == COUNTER_POINTS)
            .expect("points is declared");
        assert_eq!(points.scope, CounterScope::Seat);
        assert_eq!(points.min, Some(0));
        assert_eq!(points.max, None);
        let highest = MODES.iter().map(|mode| mode.victory_score).max().unwrap();
        assert!(highest > DEFAULT_VICTORY_SCORE);
        assert!(VICTORY_SCORE_RANGE.contains(&(highest + 1)));
    }

    #[test]
    fn every_seat_contributes_one_battlefield_except_the_first_player_at_four() {
        let duel = default_mode(2).unwrap();
        assert_eq!(contribution(duel, 0, 0, 1), Battlefields::One(1));
        assert_eq!(contribution(duel, 1, 0, 2), Battlefields::One(2));

        let war = default_mode(4).unwrap();
        assert_eq!(contribution(war, 0, 0, 1), Battlefields::None);
        assert_eq!(contribution(war, 1, 0, 0), Battlefields::One(0));
        assert_eq!(contribution(war, 2, 0, 2), Battlefields::One(2));
    }

    #[test]
    fn a_contributing_seat_places_exactly_one_of_its_three_battlefields() {
        let face = |name: &str| CardFace::named(name);
        let deck = DeckFaces {
            legend: Some(face("Legend")),
            chosen_champion: Some(face("Champ")),
            main_deck: (0..MAIN_DECK_SIZE)
                .map(|i| face(&format!("main {i}")))
                .collect(),
            runes: (0..RUNE_DECK_SIZE)
                .map(|i| face(&format!("rune {i}")))
                .collect(),
            battlefields: vec![face("bf 0"), face("bf 1"), face("bf 2")],
            sideboard: Vec::new(),
        };

        let spread = |plan: &[DealGroup]| {
            plan.iter()
                .find(|group| matches!(&group.target, DealTarget::Spread(prefix) if prefix == BATTLEFIELD_PREFIX))
                .map(|group| group.faces.clone())
                .unwrap_or_default()
        };

        let one = deal_plan_with(&deck, Battlefields::One(1));
        assert_eq!(spread(&one), vec![face("bf 1")]);

        let none = deal_plan_with(&deck, Battlefields::None);
        assert!(spread(&none).is_empty());

        let all = deal_plan_with(&deck, Battlefields::All);
        assert_eq!(spread(&all).len(), 3);

        let two = deal_plan_with(&deck, Battlefields::Many { pick: 2, count: 2 });
        assert_eq!(spread(&two), vec![face("bf 2"), face("bf 0")]);

        let past = deal_plan_with(&deck, Battlefields::Many { pick: 9, count: 5 });
        assert_eq!(
            spread(&past),
            vec![face("bf 0"), face("bf 1"), face("bf 2")]
        );
    }

    #[test]
    fn a_table_with_more_battlefields_than_seats_splits_them_after_the_first_player() {
        assert_eq!(contribution_of(3, 2, 0, 0, 1), Battlefields::One(1));
        assert_eq!(
            contribution_of(3, 2, 1, 0, 2),
            Battlefields::Many { pick: 2, count: 2 }
        );
        assert_eq!(
            contribution_of(3, 2, 0, 1, 0),
            Battlefields::Many { pick: 0, count: 2 }
        );
        assert_eq!(contribution_of(3, 2, 1, 1, 0), Battlefields::One(0));
        assert_eq!(contribution_of(1, 2, 0, 0, 0), Battlefields::None);
        assert_eq!(contribution_of(1, 2, 1, 0, 0), Battlefields::One(0));
        assert_eq!(contribution_of(2, 0, 1, 0, 4), Battlefields::One(4));
        for mode in &MODES {
            for first in 0..mode.players {
                let first_gets = contribution(mode, first, first, 0) != Battlefields::None;
                assert_eq!(first_gets, mode.first_player_contributes, "{}", mode.name);
                let placed: usize = (0..mode.players)
                    .map(|seat| match contribution(mode, seat, first, 0) {
                        Battlefields::None => 0,
                        Battlefields::One(_) => 1,
                        Battlefields::Many { count, .. } => count,
                        Battlefields::All => usize::MAX,
                    })
                    .sum();
                assert_eq!(placed, usize::from(mode.battlefield_count), "{}", mode.name);
            }
        }
    }

    #[test]
    fn the_table_options_round_trip_and_keep_their_defaults_around_unknown_keys() {
        let options = TableOptions {
            victory_score: 6,
            battlefields: 3,
        };
        assert_eq!(TableOptions::decode(&options.encode()), Some(options));
        assert_eq!(
            TableOptions::of_config::<&[u8]>(None),
            TableOptions::default()
        );
        assert_eq!(
            TableOptions::default(),
            TableOptions {
                victory_score: 8,
                battlefields: 2
            }
        );
        let mut map: BTreeMap<&str, i64> = BTreeMap::new();
        map.insert("victory_score", 6);
        map.insert("handicap", -2);
        map.insert("mulligans", 1);
        let foreign = agni_sim::abi::encode(&map);
        assert_eq!(
            TableOptions::decode(&foreign),
            Some(TableOptions {
                victory_score: 6,
                battlefields: 2
            })
        );
        assert_eq!(
            TableOptions::of_config(Some(&[0xff])),
            TableOptions::default()
        );
        assert_eq!(
            TableOptions {
                victory_score: 99,
                battlefields: 0
            }
            .clamped(),
            TableOptions {
                victory_score: 20,
                battlefields: 1
            }
        );
        assert_eq!(options.label(), "first to 6 · 3 battlefields");
        assert_eq!(
            TableOptions::for_players(4),
            TableOptions {
                victory_score: 8,
                battlefields: 3
            }
        );
        assert_eq!(TableOptions::for_players(7), TableOptions::default());
    }

    #[test]
    fn the_enforced_flag_rides_the_options_map_without_pinning_the_rest() {
        let none: Option<&[u8]> = None;
        assert_eq!(TableOptions::genesis(None, false), None);
        assert!(!TableOptions::enforced_in(none));
        let only_enforced = TableOptions::genesis(None, true).expect("the flag alone is written");
        assert!(TableOptions::enforced_in(Some(&only_enforced)));
        assert_eq!(
            TableOptions::in_play(Some(&only_enforced), 4),
            TableOptions {
                victory_score: 8,
                battlefields: 3
            },
            "a flag-only map still plays by seat count"
        );
        let chosen = TableOptions {
            victory_score: 6,
            battlefields: 3,
        };
        let both = TableOptions::genesis(Some(chosen), true).expect("written");
        assert!(TableOptions::enforced_in(Some(&both)));
        assert_eq!(TableOptions::decode(&both), Some(chosen));
        assert_eq!(TableOptions::in_play(Some(&both), 2), chosen);
        let free = TableOptions::genesis(Some(chosen), false).expect("written");
        assert!(!TableOptions::enforced_in(Some(&free)));
        assert_eq!(free, chosen.encode());
        let mut map: BTreeMap<&str, i64> = BTreeMap::new();
        map.insert("rules_enforced", 0);
        assert!(!TableOptions::enforced_in(Some(&agni_sim::abi::encode(
            &map
        ))));
        map.insert("rules_enforced", -1);
        assert!(!TableOptions::enforced_in(Some(&agni_sim::abi::encode(
            &map
        ))));
        map.insert("rules_enforced", 7);
        assert!(TableOptions::enforced_in(Some(&agni_sim::abi::encode(
            &map
        ))));
        assert!(!TableOptions::enforced_in(Some(&[0xff])));
    }

    #[test]
    fn the_options_in_play_fall_back_the_way_the_plugin_does() {
        let none: Option<&[u8]> = None;
        assert_eq!(
            TableOptions::in_play(none, 2),
            TableOptions {
                victory_score: 8,
                battlefields: 2
            }
        );
        assert_eq!(TableOptions::in_play(none, 1).battlefields, 2);
        assert_eq!(TableOptions::in_play(none, 3).battlefields, 3);
        assert_eq!(TableOptions::in_play(none, 4).battlefields, 3);
        let mut map: BTreeMap<&str, i64> = BTreeMap::new();
        map.insert("victory_score", 0);
        map.insert("battlefields", 0);
        let zeros = agni_sim::abi::encode(&map);
        assert_eq!(
            TableOptions::in_play(Some(&zeros), 4),
            TableOptions {
                victory_score: 8,
                battlefields: 3
            },
            "a zero falls back to the seat count, it is never clamped to one"
        );
        map.insert("victory_score", 30);
        map.insert("battlefields", 200);
        let wild = agni_sim::abi::encode(&map);
        assert_eq!(
            TableOptions::in_play(Some(&wild), 2),
            TableOptions {
                victory_score: 30,
                battlefields: BATTLEFIELD_COUNT as u8
            },
            "the plugin plays any score and takes as many battlefields as are declared"
        );
        map.insert("victory_score", -3);
        map.remove("battlefields");
        let negative = agni_sim::abi::encode(&map);
        assert_eq!(
            TableOptions::in_play(Some(&negative), 2),
            TableOptions::default()
        );
        assert_eq!(
            TableOptions::in_play(Some(&[0xff]), 3),
            TableOptions {
                victory_score: 8,
                battlefields: 3
            }
        );
        let options = TableOptions {
            victory_score: 2,
            battlefields: 3,
        };
        assert_eq!(TableOptions::in_play(Some(options.encode()), 2), options);
    }

    #[test]
    fn a_pick_past_the_end_falls_back_to_the_first_battlefield() {
        let face = |name: &str| CardFace::named(name);
        let deck = DeckFaces {
            legend: None,
            chosen_champion: None,
            main_deck: Vec::new(),
            runes: Vec::new(),
            battlefields: vec![face("only")],
            sideboard: Vec::new(),
        };
        let plan = deal_plan_with(&deck, Battlefields::One(7));
        let spread = plan
            .iter()
            .find(|group| matches!(&group.target, DealTarget::Spread(_)))
            .map(|group| group.faces.clone())
            .unwrap_or_default();
        assert_eq!(spread, vec![face("only")]);
    }

    #[test]
    fn the_deal_plan_covers_every_deck_zone_in_riftbound_order() {
        let face = |name: &str| CardFace::named(name);
        let deck = DeckFaces {
            legend: Some(face("Legend")),
            chosen_champion: Some(face("Champ")),
            main_deck: (0..MAIN_DECK_SIZE)
                .map(|i| face(&format!("main {i}")))
                .collect(),
            runes: (0..RUNE_DECK_SIZE)
                .map(|i| face(&format!("rune {i}")))
                .collect(),
            battlefields: (0..BATTLEFIELD_COUNT)
                .map(|i| face(&format!("field {i}")))
                .collect(),
            sideboard: vec![face("spare")],
        };
        let plan = deal_plan(&deck);
        let targets: Vec<(&DealTarget, usize, bool, u32)> = plan
            .iter()
            .map(|group| (&group.target, group.faces.len(), group.shuffle, group.draw))
            .collect();
        assert_eq!(
            targets,
            vec![
                (&DealTarget::Zone(ZONE_NAME_LEGEND.into()), 1, false, 0),
                (&DealTarget::Zone(ZONE_NAME_CHAMPION.into()), 1, false, 0),
                (
                    &DealTarget::Zone(ZONE_NAME_RUNE_DECK.into()),
                    RUNE_DECK_SIZE,
                    true,
                    0
                ),
                (
                    &DealTarget::Zone(ZONE_NAME_MAIN_DECK.into()),
                    MAIN_DECK_SIZE,
                    true,
                    OPENING_HAND_SIZE
                ),
                (
                    &DealTarget::Spread(BATTLEFIELD_PREFIX.into()),
                    BATTLEFIELD_COUNT,
                    false,
                    0
                ),
                (&DealTarget::Zone(ZONE_NAME_SIDEBOARD.into()), 1, false, 0),
            ]
        );
        let table = zone_table();
        let names: BTreeSet<&str> = table.iter().map(|decl| decl.name.as_str()).collect();
        for group in &plan {
            match &group.target {
                DealTarget::Zone(zone) => assert!(names.contains(zone.as_str())),
                DealTarget::Spread(prefix) => assert!(table
                    .iter()
                    .any(|decl| decl.name.starts_with(prefix.as_str()))),
            }
        }
        assert!(deal_plan(&DeckFaces::default()).is_empty());
    }

    #[test]
    fn the_deal_plan_leaves_the_base_and_the_rune_pool_empty() {
        let face = |name: &str| CardFace::named(name);
        let deck = DeckFaces {
            legend: Some(face("Legend")),
            chosen_champion: Some(face("Champ")),
            main_deck: (0..MAIN_DECK_SIZE)
                .map(|i| face(&format!("main {i}")))
                .collect(),
            runes: (0..RUNE_DECK_SIZE)
                .map(|i| face(&format!("rune {i}")))
                .collect(),
            battlefields: (0..BATTLEFIELD_COUNT)
                .map(|i| face(&format!("field {i}")))
                .collect(),
            sideboard: vec![face("spare")],
        };
        for group in deal_plan(&deck) {
            let DealTarget::Zone(zone) = &group.target else {
                continue;
            };
            assert_ne!(zone.as_str(), ZONE_NAME_BASE);
            assert_ne!(zone.as_str(), ZONE_NAME_RUNE_POOL);
        }
        assert!(deal_plan(&deck)
            .iter()
            .any(|group| group.target == DealTarget::Zone(ZONE_NAME_RUNE_DECK.into())));
    }

    #[test]
    fn a_resolved_deck_flattens_to_names() {
        let card = |name: &str, id: &str| ResolvedCard {
            name: name.into(),
            riftbound_id: id.into(),
            image_url: Some(format!("https://img.example/{id}.png")),
            ..Default::default()
        };
        let deck = ResolvedDeck {
            legend: Some(card("Vanguard Sentinel", "ogn-201-298")),
            chosen_champion: Some(card("Emberwing Scout", "ogn-007-298")),
            main_deck: vec![
                DeckEntry {
                    card: card("Emberwing Scout", "ogn-007-298"),
                    count: 3,
                },
                DeckEntry {
                    card: card("Gloomvale Trickster", "ogn-101-298"),
                    count: 2,
                },
            ],
            runes: vec![DeckEntry {
                card: card("Ember Rune", "ogn-042-298"),
                count: 12,
            }],
            battlefields: vec![DeckEntry {
                card: card("Sunken Causeway", "ogn-260-298"),
                count: 1,
            }],
            sideboard: Vec::new(),
        };
        let names = deck.names();
        assert_eq!(names.legend, Some(CardName("Vanguard Sentinel".into())));
        assert_eq!(
            names.chosen_champion,
            Some(CardName("Emberwing Scout".into()))
        );
        assert_eq!(names.main_deck.len(), 5);
        assert_eq!(names.runes.len(), 12);
        assert_eq!(names.battlefields.len(), 1);
        assert_eq!(names.main_deck[0], CardName("Emberwing Scout".into()));
        assert_eq!(names.main_deck[3], CardName("Gloomvale Trickster".into()));
    }
}

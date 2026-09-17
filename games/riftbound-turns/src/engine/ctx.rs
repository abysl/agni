use crate::cards::{
    self, Card, Cost, Domain, Grant, Keyword, Resolved, Source, Static, Suppresses,
    KIND_BATTLEFIELD, KIND_GEAR, KIND_LEGEND, KIND_RUNE, KIND_SPELL, KIND_UNIT, RUNE_SUFFIX,
    TOKEN_BARON_PIT, TOKEN_BRUSH, TOKEN_GOLD, TOKEN_SAND_SOLDIER, TOKEN_SHADOW_CLONE, TOKEN_SPRITE,
    TOKEN_TENTACLE,
};
use crate::engine::{attach, hide, kill, prevent, statics, triggers};
use crate::rules::{
    self, Options, COUNTER_POINTS, COUNTER_TEMPORARY, COUNTER_XP, ZONE_BASE, ZONE_CHAIN, ZONE_HAND,
    ZONE_MAIN_DECK, ZONE_RUNE_DECK, ZONE_RUNE_POOL, ZONE_TRASH,
};
use crate::state::{
    Amount, Ask, CardState, ChainItem, CostedGrant, DamageSource, Death, Delayed, Expiry, GameBlob,
    ItemKind, NameKind, Noted, Origin, Pool, Prevention, Promise, PromiseEffect, PromiseKind,
    PromptWhy, TargetRef, When, FLAG_ATTACKER, FLAG_DEFENDER, FLAG_NOT_PLAYED,
    FLAG_NO_MOVE_BY_OWNER, FLAG_REVEALING, FLAG_SHROUDED, FLAG_STUNNED,
};
use agni_plugin_sdk::decide::{Action, Effect, TOP};
use agni_plugin_sdk::prompt::Prompt;
use agni_plugin_sdk::table::{ApplyError, CardInfo, Face, Snapshot, Target, ZoneKind};

pub const ZONE_LEGEND: &str = "legend";
pub const ZONE_CHAMPION: &str = "champion";
pub const ZONE_SIDEBOARD: &str = "sideboard";
pub const ZONE_BANISHMENT: &str = "banishment";
pub const COUNTER_MIGHT: u16 = 2;
pub const COUNTER_DAMAGE: u16 = 3;
pub const COUNTER_BUFFED: u16 = 5;
pub const COUNTER_EMPOWERED: u16 = 6;
pub const ANNOTATION_STUNNED: &str = "stunned";
pub const ANNOTATION_ATTACKER: &str = "attacker";
pub const ANNOTATION_DEFENDER: &str = "defender";
pub const ANNOTATION_REPLACED: &str = "replaced";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Base(u8),
    Battlefield(u16),
}

impl Location {
    pub fn of_zone(zone: u16, seat: u8, zones: &Zones) -> Option<Self> {
        if zones.base == Some(zone) {
            Some(Location::Base(seat))
        } else if zones.battlefields.contains(&zone) {
            Some(Location::Battlefield(zone))
        } else {
            None
        }
    }

    pub fn battlefield(self) -> Option<u16> {
        match self {
            Location::Battlefield(zone) => Some(zone),
            Location::Base(_) => None,
        }
    }

    pub fn is_base(self) -> bool {
        matches!(self, Location::Base(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Item(u16),
    Ability(Source),
    Replacement,
    Combat,
    Cleanup { last_item: Option<u16> },
    Cost,
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveCause {
    Standard,
    Effect,
    Swap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    Moved,
    Recalled,
    NotAUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Killed {
    Yes,
    Replaced,
    NotOnBoard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trashed {
    Trashed,
    Banished { by: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Sprite,
    Gold,
    SandSoldier,
    ShadowClone,
    Tentacle,
    Brush,
    BaronPit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterDest {
    Trash,
    Hand,
}

impl Token {
    pub fn face(self) -> Face {
        match self {
            Token::Sprite => Face::named(TOKEN_SPRITE)
                .with_kind(KIND_UNIT)
                .with_might(Some(3)),
            Token::Gold => Face::named(TOKEN_GOLD).with_kind(KIND_GEAR),
            Token::SandSoldier => Face::named(TOKEN_SAND_SOLDIER)
                .with_kind(KIND_UNIT)
                .with_might(Some(2)),
            Token::ShadowClone => Face::named(TOKEN_SHADOW_CLONE)
                .with_kind(KIND_UNIT)
                .with_might(Some(0)),
            Token::Tentacle => Face::named(TOKEN_TENTACLE)
                .with_kind(KIND_UNIT)
                .with_might(Some(1)),
            Token::Brush => Face::named(TOKEN_BRUSH).with_kind(KIND_BATTLEFIELD),
            Token::BaronPit => Face::named(TOKEN_BARON_PIT).with_kind(KIND_BATTLEFIELD),
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            Token::Gold => KIND_GEAR,
            Token::Sprite | Token::SandSoldier | Token::ShadowClone | Token::Tentacle => KIND_UNIT,
            Token::Brush | Token::BaronPit => KIND_BATTLEFIELD,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Played {
        card: u32,
        controller: u8,
        kind: String,
        origin: Origin,
        paid_additional: bool,
    },
    PlayedSpell {
        item: u16,
        controller: u8,
        nth: u8,
    },
    Moved {
        card: u32,
        from: Option<Location>,
        to: Location,
        cause: MoveCause,
        by: Option<u8>,
    },
    Entered {
        card: u32,
        at: Location,
    },
    Died {
        card: u32,
        controller: u8,
        unit: bool,
        noted: Noted,
    },
    Drew {
        seat: u8,
        nth: u8,
    },
    Chosen {
        card: u32,
        by: u8,
        item: u16,
    },
    Readied {
        card: u32,
        by: u8,
    },
    Conquered {
        zone: u16,
        seat: u8,
        units: Vec<u32>,
    },
    Held {
        zone: u16,
        seat: u8,
        units: Vec<u32>,
    },
    BeginningPhase {
        seat: u8,
    },
    EndingStep {
        seat: u8,
    },
    Attacks {
        card: u32,
    },
    Defends {
        card: u32,
    },
    DamageDealt {
        card: u32,
        n: u8,
        source: Cause,
    },
    Empowered {
        card: u32,
        by: u8,
    },
    Disempowered {
        card: u32,
    },
    CombatWon {
        zone: u16,
        seat: u8,
    },
    CombatLost {
        zone: u16,
        seat: u8,
    },
    CombatEnded {
        zone: u16,
        units: Vec<u32>,
    },
    Activated {
        item: u16,
        source: u32,
        index: u8,
        controller: u8,
    },
    Burned {
        seat: u8,
        card: u32,
    },
    Banished {
        card: u32,
        owner: u8,
        by: u8,
        token: bool,
    },
    TurnQueued {
        seat: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Zones {
    pub hand: Option<u16>,
    pub main_deck: Option<u16>,
    pub rune_deck: Option<u16>,
    pub rune_pool: Option<u16>,
    pub base: Option<u16>,
    pub legend: Option<u16>,
    pub champion: Option<u16>,
    pub trash: Option<u16>,
    pub chain: Option<u16>,
    pub sideboard: Option<u16>,
    pub banishment: Option<u16>,
    pub battlefields: Vec<u16>,
}

impl Zones {
    pub fn of(table: &Snapshot) -> Self {
        let id = |name: &str| table.zone_named(name).map(|zone| zone.id);
        let shared: Vec<u16> = table
            .zones
            .iter()
            .filter(|zone| zone.shared && zone.kind == ZoneKind::Battlefield)
            .map(|zone| zone.id)
            .collect();
        let mut battlefields: Vec<u16> = shared
            .iter()
            .copied()
            .filter(|zone| {
                table
                    .in_zone(*zone)
                    .any(|card| card.is_kind(KIND_BATTLEFIELD))
            })
            .collect();
        if battlefields.is_empty() {
            battlefields = shared
                .into_iter()
                .take(Options::of(table).battlefields)
                .collect();
        }
        Self {
            hand: id(ZONE_HAND),
            main_deck: id(ZONE_MAIN_DECK),
            rune_deck: id(ZONE_RUNE_DECK),
            rune_pool: id(ZONE_RUNE_POOL),
            base: id(ZONE_BASE),
            legend: id(ZONE_LEGEND),
            champion: id(ZONE_CHAMPION),
            trash: id(ZONE_TRASH),
            chain: id(ZONE_CHAIN),
            sideboard: id(ZONE_SIDEBOARD),
            banishment: id(ZONE_BANISHMENT),
            battlefields,
        }
    }

    pub fn is_battlefield(&self, zone: u16) -> bool {
        self.battlefields.contains(&zone)
    }

    pub fn spare_battlefield(&self, table: &Snapshot) -> Option<u16> {
        table
            .zones
            .iter()
            .filter(|zone| zone.shared && zone.kind == ZoneKind::Battlefield)
            .map(|zone| zone.id)
            .find(|zone| !self.is_battlefield(*zone))
    }

    pub fn is_deck(&self, zone: u16) -> bool {
        self.main_deck == Some(zone) || self.rune_deck == Some(zone)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryMove {
    pub card: u32,
    pub from: Option<u16>,
    pub from_seat: u8,
    pub to: Option<u16>,
    pub to_seat: u8,
    pub index: u32,
    pub hidden: bool,
}

pub struct Ctx<'a> {
    pub table: Snapshot,
    pub origin: Snapshot,
    index: std::cell::RefCell<Vec<(u32, usize)>>,
    pub blob: &'a mut GameBlob,
    pub scripts: &'a Resolved,
    pub effects: Vec<Effect>,
    pub events: Vec<Event>,
    pub seat: u8,
    pub actor: u8,
    pub spawned: u32,
    pub zones: Zones,
    pub options: Options,
    pub entry: Option<EntryMove>,
    pub fault: Option<(usize, ApplyError)>,
    pub won: Option<u8>,
    pub collected: usize,
    pub picked: Vec<u32>,
    pub deaths: Vec<triggers::Match>,
    pub awaiting: Vec<u32>,
    pub remembered: Vec<TargetRef>,
    pub resolving: Option<ChainItem>,
    pub defer_limited: bool,
    pub projected_move_card: Option<u32>,
    pub departed: Vec<CardInfo>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        table: &Snapshot,
        blob: &'a mut GameBlob,
        scripts: &'a Resolved,
        seat: u8,
        action: &Action,
    ) -> Self {
        let mut shadow = table.clone();
        let fault = shadow
            .apply_entry(action, seat)
            .err()
            .map(|error| (0, error));
        let mut ctx = Self::assemble(table, shadow, blob, scripts, seat);
        ctx.fault = fault;
        if let Action::Move {
            card,
            to,
            seat: to_seat,
            index,
            hidden,
        } = action
        {
            let before = table.card(*card);
            ctx.entry = Some(EntryMove {
                card: *card,
                from: before.and_then(|held| held.zone),
                from_seat: before.map(|held| held.seat).unwrap_or(0),
                to: *to,
                to_seat: *to_seat,
                index: *index,
                hidden: *hidden,
            });
        }
        ctx
    }

    pub fn fresh(
        table: &Snapshot,
        blob: &'a mut GameBlob,
        scripts: &'a Resolved,
        seat: u8,
    ) -> Self {
        Self::assemble(table, table.clone(), blob, scripts, seat)
    }

    fn assemble(
        table: &Snapshot,
        shadow: Snapshot,
        blob: &'a mut GameBlob,
        scripts: &'a Resolved,
        seat: u8,
    ) -> Self {
        Self {
            origin: shadow.clone(),
            table: shadow,
            index: std::cell::RefCell::new(Vec::new()),
            blob,
            scripts,
            effects: Vec::new(),
            events: Vec::new(),
            seat,
            actor: seat,
            spawned: 0,
            zones: Zones::of(table),
            options: Options::of(table),
            entry: None,
            fault: None,
            won: None,
            collected: 0,
            picked: Vec::new(),
            deaths: Vec::new(),
            awaiting: Vec::new(),
            remembered: Vec::new(),
            resolving: None,
            defer_limited: false,
            projected_move_card: None,
            departed: Vec::new(),
        }
    }

    pub fn turn(&self) -> u16 {
        self.blob.turn()
    }

    pub fn turn_player(&self) -> u8 {
        self.blob.turn_player()
    }

    pub fn deaths_this_turn(&self) -> &[Death] {
        &self.blob.deaths_this_turn
    }

    pub fn players(&self) -> u8 {
        self.blob.players().max(self.table.players).max(1)
    }

    pub fn card(&self, id: u32) -> Option<&CardInfo> {
        let position = {
            let mut index = self.index.borrow_mut();
            if index.is_empty() {
                index.extend(
                    self.table
                        .cards
                        .iter()
                        .enumerate()
                        .map(|(at, card)| (card.id, at)),
                );
                index.sort_unstable_by_key(|(id, _)| *id);
            }
            index
                .binary_search_by_key(&id, |(held, _)| *held)
                .ok()
                .map(|found| index[found].1)
        };
        position
            .and_then(|at| self.table.cards.get(at))
            .filter(|card| card.id == id)
            .or_else(|| self.table.card(id))
    }

    fn forget_index(&self) {
        self.index.borrow_mut().clear();
    }

    pub fn script(&self, id: u32) -> Option<&'static Card> {
        if self.is_token(id)
            && self
                .origin
                .card(id)
                .zip(self.card(id))
                .is_some_and(|(before, after)| before.face() != after.face())
        {
            return self.card(id).and_then(cards::resolve);
        }
        self.scripts
            .of_card(id)
            .or_else(|| self.card(id).and_then(cards::resolve))
    }

    pub fn deflect_of(&self, card: u32) -> u8 {
        let printed = self
            .script(card)
            .map(|script| {
                script
                    .keywords
                    .iter()
                    .filter_map(|kw| match kw {
                        Keyword::Deflect(n) => Some(*n),
                        _ => None,
                    })
                    .sum::<u8>()
            })
            .unwrap_or(0);
        let granted = self
            .blob
            .card_state(card)
            .map(|row| {
                row.granted
                    .iter()
                    .filter_map(|(kw, _)| match kw {
                        Keyword::Deflect(n) => Some(*n),
                        _ => None,
                    })
                    .sum::<u8>()
            })
            .unwrap_or(0);
        let projected = self.projected_keyword_sum(card, |kw| match kw {
            Keyword::Deflect(n) => Some(n),
            _ => None,
        });
        printed.saturating_add(granted).saturating_add(projected)
    }

    fn projected_keyword_sum(&self, card: u32, pick: fn(Keyword) -> Option<u8>) -> u8 {
        statics::grants_on(self, card)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Keyword(kw) => pick(kw),
                _ => None,
            })
            .fold(0u8, u8::saturating_add)
    }

    pub fn keyword_instances(&self, card: u32, keyword: Keyword) -> usize {
        let printed = self
            .script(card)
            .map(|script| {
                script
                    .keywords
                    .iter()
                    .filter(|held| held.same_kind(keyword))
                    .count()
            })
            .unwrap_or(0);
        let granted = self
            .blob
            .card_state(card)
            .map(|row| {
                let ordinary = row
                    .granted
                    .iter()
                    .filter(|(held, _)| held.same_kind(keyword))
                    .count();
                ordinary
                    + row
                        .granted_costed
                        .iter()
                        .filter(|grant| grant.kind.matches(keyword))
                        .count()
            })
            .unwrap_or(0);
        let projected = statics::grants_on(self, card)
            .into_iter()
            .filter(|grant| matches!(grant, Grant::Keyword(held) if held.same_kind(keyword)))
            .count();
        printed + granted + projected
    }

    pub fn projected_keyword(&self, card: u32, keyword: Keyword) -> bool {
        statics::grants_on(self, card)
            .into_iter()
            .any(|grant| matches!(grant, Grant::Keyword(granted) if granted.same_kind(keyword)))
    }

    pub fn projected_might(&self, card: u32) -> i32 {
        statics::grants_on(self, card)
            .into_iter()
            .map(|grant| match grant {
                Grant::Might(n) | Grant::MightIf(_, n) => i32::from(n),
                Grant::Keyword(_)
                | Grant::Static(_)
                | Grant::Ability(_)
                | Grant::Copied(_)
                | Grant::Mirror(_)
                | Grant::Borrowed(_) => 0,
            })
            .sum()
    }

    pub fn has_static(&self, card: u32, wanted: Static) -> bool {
        self.script(card)
            .is_some_and(|script| script.has_static(wanted))
            || self
                .projected_statics(card)
                .iter()
                .any(|held| held.same_kind(wanted))
    }

    pub fn projected_statics(&self, card: u32) -> Vec<Static> {
        statics::grants_on(self, card)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Static(held) => Some(held),
                _ => None,
            })
            .collect()
    }

    pub fn ignores_deflect(&self, card: u32) -> bool {
        self.script(card)
            .is_some_and(|script| script.has_static(cards::Static::IgnoresDeflect))
    }

    pub fn has_keyword(&self, card: u32, keyword: Keyword) -> bool {
        self.script(card)
            .is_some_and(|script| script.has_keyword(keyword))
            || self.blob.card_state(card).is_some_and(|row| {
                row.granted
                    .iter()
                    .any(|(granted, _)| granted.same_kind(keyword))
                    || row
                        .granted_costed
                        .iter()
                        .any(|grant| grant.kind.matches(keyword))
            })
            || self.projected_keyword(card, keyword)
    }

    pub fn granted_cost(&self, card: u32, keyword: Keyword) -> Option<crate::engine::cost::Cost> {
        self.blob.card_state(card).and_then(|row| {
            row.granted_costed
                .iter()
                .find(|grant| grant.kind.matches(keyword))
                .map(|grant| crate::engine::cost::of_grant(grant, &self.domains_of(card)))
        })
    }

    pub fn flow_of(&self, card: u32) -> Option<crate::engine::cost::Cost> {
        if !self.is_spell(card) {
            return None;
        }
        self.script(card)
            .and_then(Card::flow_cost)
            .map(|cost| crate::engine::cost::of_script(&cost, &self.domains_of(card)))
            .or_else(|| self.granted_cost(card, Keyword::Flow(cards::Cost::FREE)))
    }

    pub fn hunt_value(&self, card: u32) -> u8 {
        let printed = self.script(card).map(Card::hunt).unwrap_or(0);
        let granted = self
            .blob
            .card_state(card)
            .map(|row| {
                row.granted
                    .iter()
                    .filter_map(|(kw, _)| match kw {
                        Keyword::Hunt(n) => Some(*n),
                        _ => None,
                    })
                    .fold(0u8, u8::saturating_add)
            })
            .unwrap_or(0);
        let projected = self.projected_keyword_sum(card, |kw| match kw {
            Keyword::Hunt(n) => Some(n),
            _ => None,
        });
        printed.saturating_add(granted).saturating_add(projected)
    }

    pub fn kind_of(&self, card: u32) -> Option<&str> {
        self.card(card).and_then(|held| held.kind.as_deref())
    }

    pub fn is_unit(&self, card: u32) -> bool {
        self.card(card).is_some_and(is_unit_face)
    }

    pub fn is_gear(&self, card: u32) -> bool {
        self.card(card).is_some_and(|held| held.is_kind(KIND_GEAR))
    }

    pub fn is_spell(&self, card: u32) -> bool {
        self.card(card).is_some_and(|held| held.is_kind(KIND_SPELL))
    }

    pub fn is_rune(&self, card: u32) -> bool {
        self.card(card).is_some_and(is_rune_face)
    }

    pub fn is_legend(&self, card: u32) -> bool {
        self.card(card)
            .is_some_and(|held| held.is_kind(KIND_LEGEND))
    }

    pub fn is_battlefield_card(&self, card: u32) -> bool {
        self.card(card)
            .is_some_and(|held| held.is_kind(KIND_BATTLEFIELD))
    }

    pub fn is_token(&self, card: u32) -> bool {
        self.table.is_token(card)
    }

    pub fn is_temporary(&self, card: u32) -> bool {
        self.table
            .counter(Target::Card(card), COUNTER_TEMPORARY)
            .unwrap_or(0)
            > 0
            || self.has_keyword(card, Keyword::Temporary)
    }

    pub fn controller(&self, card: u32) -> u8 {
        if let Some(seat) = self.blob.card_state(card).and_then(|row| row.controlled_by) {
            return seat;
        }
        self.card(card).map(|held| held.owner).unwrap_or(0)
    }

    pub fn owner(&self, card: u32) -> u8 {
        self.card(card).map(|held| held.owner).unwrap_or(0)
    }

    pub fn set_controller(&mut self, card: u32, seat: u8, source: u32) -> bool {
        self.take_control(card, seat, source, false)
    }

    pub fn set_controller_in_place(&mut self, card: u32, seat: u8, source: u32) -> bool {
        self.take_control(card, seat, source, true)
    }

    fn take_control(&mut self, card: u32, seat: u8, source: u32, in_place: bool) -> bool {
        if self.card(card).is_none() || seat >= self.players() {
            return false;
        }
        {
            let row = self.state_mut(card);
            row.controlled_by = Some(seat);
            row.control_source = Some(source);
        }
        self.narrate(format!("{{seat {seat}}} takes control of {{card {card}}}"));
        match self.location(card) {
            Some(Location::Battlefield(zone)) if in_place => {
                if self.contest(zone, seat) {
                    self.narrate(format!(
                        "{{card {card}}} stands its ground · {{zone {zone}}} is contested by {{seat {seat}}}"
                    ));
                }
            }
            Some(Location::Base(home)) if home == seat => {}
            _ if self.on_board(card) => {
                if let Some(base) = self.zones.base {
                    self.emit(Effect::Move {
                        card,
                        zone: base,
                        seat,
                        index: TOP,
                    });
                }
            }
            _ => {}
        }
        true
    }

    pub fn attached_turn(&self, gear: u32) -> u16 {
        self.blob
            .card_state(gear)
            .map(|row| row.attached_turn)
            .unwrap_or(0)
    }

    pub fn location(&self, card: u32) -> Option<Location> {
        let held = self.card(card)?;
        Location::of_zone(held.zone?, held.seat, &self.zones)
    }

    pub fn is_facedown(&self, card: u32) -> bool {
        self.blob
            .card_state(card)
            .is_some_and(|row| row.hidden_at.is_some())
    }

    pub fn is_pending_play(&self, card: u32) -> bool {
        self.blob
            .queue
            .iter()
            .any(|pending| pending.item.kind.card() == Some(card))
    }

    pub fn on_board(&self, card: u32) -> bool {
        self.card(card).is_some_and(|held| self.face_on_board(held))
    }

    pub fn in_play(&self, card: u32) -> bool {
        self.card(card).is_some_and(|held| self.face_in_play(held))
    }

    pub fn face_on_board(&self, held: &CardInfo) -> bool {
        held.zone.is_some_and(|zone| {
            self.zones.base == Some(zone)
                || self.zones.rune_pool == Some(zone)
                || self.zones.is_battlefield(zone)
        })
    }

    pub fn face_location(&self, held: &CardInfo) -> Option<Location> {
        Location::of_zone(held.zone?, held.seat, &self.zones)
    }

    pub fn face_in_play(&self, held: &CardInfo) -> bool {
        self.face_on_board(held)
            || (held.is_kind(KIND_LEGEND)
                && self
                    .zones
                    .legend
                    .is_some_and(|legend| held.zone == Some(legend)))
            || (held.is_kind(KIND_BATTLEFIELD)
                && held
                    .zone
                    .is_some_and(|zone| self.zones.is_battlefield(zone)))
    }

    pub fn faces_on_board(&self) -> impl Iterator<Item = &CardInfo> {
        self.table
            .cards
            .iter()
            .filter(|held| self.face_on_board(held))
    }

    pub fn units_at(&self, at: Location) -> Vec<u32> {
        self.table
            .cards
            .iter()
            .filter(|card| is_unit_face(card) && !card.is_hidden())
            .filter(|card| self.projected_move_card != Some(card.id))
            .filter(|card| self.face_location(card) == Some(at))
            .filter(|card| !self.is_pending_play(card.id) && !self.is_facedown(card.id))
            .map(|card| card.id)
            .collect()
    }

    pub fn seats_with_units(&self, zone: u16) -> Vec<u8> {
        let mut seats: Vec<u8> = self
            .units_at(Location::Battlefield(zone))
            .into_iter()
            .map(|unit| self.controller(unit))
            .collect();
        seats.sort_unstable();
        seats.dedup();
        seats
    }

    pub fn other_seats_at(&self, zone: u16, seat: u8) -> usize {
        self.seats_with_units(zone)
            .into_iter()
            .filter(|other| *other != seat)
            .count()
    }

    pub fn at_battlefield(&self, card: u32) -> bool {
        self.location(card)
            .is_some_and(|at| at.battlefield().is_some())
    }

    pub fn alone_at(&self, card: u32) -> bool {
        let Some(at) = self.location(card) else {
            return false;
        };
        let seat = self.controller(card);
        self.units_at(at)
            .into_iter()
            .filter(|unit| self.controller(*unit) == seat)
            .count()
            == 1
    }

    pub fn holds(&self, seat: u8, zone: u16) -> bool {
        self.blob.holder(zone) == Some(seat)
    }

    pub fn held_battlefields(&self, seat: u8) -> Vec<u16> {
        self.zones
            .battlefields
            .iter()
            .copied()
            .filter(|zone| self.holds(seat, *zone))
            .collect()
    }

    fn battlefield_has_static(&self, zone: u16, wanted: Static) -> bool {
        self.table.in_zone(zone).any(|card| {
            card.is_kind(KIND_BATTLEFIELD)
                && self
                    .script(card.id)
                    .is_some_and(|script| script.has_static(wanted))
        })
    }

    pub fn units_played_here(&self, zone: u16) -> bool {
        !self.battlefield_has_static(zone, Static::NoUnitsPlayedHere)
    }

    pub fn moves_to_base_from(&self, zone: u16) -> bool {
        !self.battlefield_has_static(zone, Static::NoMoveToBase) && self.units_move_to_base()
    }

    pub fn units_move_to_base(&self) -> bool {
        !self.table.cards.iter().any(|held| {
            statics::in_play(self, held.id)
                && self
                    .script(held.id)
                    .is_some_and(|script| script.has_static(Static::NoUnitsMoveToBase))
        })
    }

    pub fn unit_moves_to_base(&self, unit: u32) -> bool {
        !(statics::in_play(self, unit) && self.has_static(unit, Static::NoMoveToBase))
    }

    pub fn moves_here_from_anywhere(&self, zone: u16) -> bool {
        self.battlefield_has_static(zone, Static::MoveHereFromAnywhere)
    }

    pub fn battlefield_card_at(&self, zone: u16) -> Option<u32> {
        self.table
            .in_zone(zone)
            .find(|card| card.is_kind(KIND_BATTLEFIELD))
            .map(|card| card.id)
    }

    pub fn replaced_by(&self, token: u32) -> Option<u32> {
        let bytes = self.card(token)?.annotation(ANNOTATION_REPLACED)?;
        let bytes: [u8; 4] = bytes.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    pub fn movable_to_base(&self, unit: u32) -> bool {
        match self.location(unit) {
            Some(Location::Battlefield(zone)) => {
                self.moves_to_base_from(zone) && self.unit_moves_to_base(unit)
            }
            Some(Location::Base(_)) | None => false,
        }
    }

    pub fn units_only_to_base(&self, seat: u8) -> bool {
        statics::units_only_to_base(self, seat)
    }

    fn playable_at(&self, seat: u8, at: Location) -> bool {
        match at {
            Location::Battlefield(zone) => {
                self.units_played_here(zone) && !self.units_only_to_base(seat)
            }
            Location::Base(_) => true,
        }
    }

    pub fn play_locations(&self, seat: u8) -> Vec<Location> {
        let mut locations = vec![Location::Base(seat)];
        if self.units_only_to_base(seat) {
            return locations;
        }
        locations.extend(
            self.held_battlefields(seat)
                .into_iter()
                .filter(|zone| self.units_played_here(*zone))
                .map(Location::Battlefield),
        );
        locations
    }

    pub fn granted_play_locations(&self, seat: u8, card: u32) -> Vec<Location> {
        statics::granted_play_locations(self, seat, card)
            .into_iter()
            .filter(|at| self.playable_at(seat, *at))
            .collect()
    }

    pub fn only_play_locations(&self, seat: u8, card: u32) -> Option<Vec<Location>> {
        let only = self.script(card)?.only_play_locations()?;
        Some(
            only(self, seat, card)
                .into_iter()
                .filter(|at| self.playable_at(seat, *at))
                .collect(),
        )
    }

    pub fn play_locations_for(&self, seat: u8, card: u32) -> Vec<Location> {
        if let Some(only) = self.only_play_locations(seat, card) {
            return only;
        }
        let mut locations = self.play_locations(seat);
        for at in self.granted_play_locations(seat, card) {
            if !locations.contains(&at) {
                locations.push(at);
            }
        }
        locations
    }

    pub fn limited_play_locations(
        &self,
        seat: u8,
        card: u32,
        offered: &[Location],
    ) -> Vec<Location> {
        let only = self.only_play_locations(seat, card);
        offered
            .iter()
            .copied()
            .filter(|at| self.playable_at(seat, *at))
            .filter(|at| only.as_ref().is_none_or(|only| only.contains(at)))
            .collect()
    }

    pub fn ambush_battlefields(&self, seat: u8, card: u32) -> Vec<u16> {
        if !self.has_keyword(card, Keyword::Ambush) {
            return Vec::new();
        }
        let into_enemies = self
            .script(card)
            .is_some_and(|script| script.has_static(Static::AmbushIntoEnemies));
        self.zones
            .battlefields
            .iter()
            .copied()
            .filter(|zone| {
                let seats = self.seats_with_units(*zone);
                seats.contains(&seat) || (into_enemies && seats.iter().any(|other| *other != seat))
            })
            .collect()
    }

    pub fn ambush_locations(&self, seat: u8, card: u32) -> Vec<Location> {
        self.ambush_battlefields(seat, card)
            .into_iter()
            .map(Location::Battlefield)
            .filter(|at| self.playable_at(seat, *at))
            .filter(|at| {
                self.only_play_locations(seat, card)
                    .is_none_or(|only| only.contains(at))
            })
            .collect()
    }

    pub fn contest(&mut self, zone: u16, controller: u8) -> bool {
        if self.blob.contester(zone).is_some() || self.blob.holder(zone) == Some(controller) {
            return false;
        }
        self.blob.set_contested(zone, Some(controller));
        true
    }

    pub fn trash_of(&self, seat: u8) -> Vec<u32> {
        match self.zones.trash {
            Some(trash) => self.table.held(trash, seat).map(|card| card.id).collect(),
            None => Vec::new(),
        }
    }

    pub fn banished_of(&self, seat: u8) -> Vec<u32> {
        match self.zones.banishment {
            Some(zone) => self.table.held(zone, seat).map(|card| card.id).collect(),
            None => Vec::new(),
        }
    }

    pub fn in_trash(&self, card: u32) -> bool {
        self.zones.trash.is_some()
            && self
                .card(card)
                .is_some_and(|held| held.zone == self.zones.trash)
    }

    pub fn in_banishment(&self, card: u32) -> bool {
        self.zones.banishment.is_some()
            && self
                .card(card)
                .is_some_and(|held| held.zone == self.zones.banishment)
    }

    pub fn in_hand(&self, card: u32) -> bool {
        self.zones.hand.is_some()
            && self
                .card(card)
                .is_some_and(|held| held.zone == self.zones.hand)
    }

    pub fn in_champion_zone(&self, card: u32) -> bool {
        self.zones.champion.is_some()
            && self
                .card(card)
                .is_some_and(|held| held.zone == self.zones.champion)
    }

    fn champion_zone_unit(&self, seat: u8) -> Option<&CardInfo> {
        let zone = self.zones.champion?;
        self.table.held(zone, seat).find(|card| is_unit_face(card))
    }

    pub fn chosen_champion(&self, seat: u8) -> Option<String> {
        self.blob.seat(seat).chosen_champion.clone().or_else(|| {
            self.champion_zone_unit(seat)
                .map(|card| cards::base_name(&card.name).to_string())
        })
    }

    pub fn record_chosen_champions(&mut self) {
        for seat in 0..self.players() {
            let Some(name) = self
                .champion_zone_unit(seat)
                .map(|card| cards::base_name(&card.name).to_string())
            else {
                continue;
            };
            self.blob.seat_mut(seat).chosen_champion = Some(name);
        }
    }

    pub fn hand_of(&self, seat: u8) -> Vec<u32> {
        match self.zones.hand {
            Some(hand) => self.table.held(hand, seat).map(|card| card.id).collect(),
            None => Vec::new(),
        }
    }

    pub fn runes_of(&self, seat: u8) -> Vec<&CardInfo> {
        match self.zones.rune_pool {
            Some(pool) => self
                .table
                .held(pool, seat)
                .filter(|card| is_rune_face(card))
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn ready_runes_of(&self, seat: u8) -> Vec<&CardInfo> {
        self.runes_of(seat)
            .into_iter()
            .filter(|rune| !rune.exhausted)
            .collect()
    }

    pub fn printed_might(&self, card: u32) -> i32 {
        self.card(card)
            .and_then(|held| held.might)
            .map(i32::from)
            .unwrap_or(0)
    }

    pub fn current_might(&self, card: u32) -> i32 {
        let buffed = self
            .table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0);
        let mut total = self.printed_might(card) + buffed;
        if let Some(row) = self.blob.card_state(card) {
            let increases: i32 = row
                .might
                .iter()
                .map(|held| i32::from(held.delta))
                .filter(|delta| *delta > 0)
                .sum();
            let decreases: i32 = row
                .might
                .iter()
                .map(|held| i32::from(held.delta))
                .filter(|delta| *delta < 0)
                .sum();
            total += increases + decreases;
            if row.has(FLAG_ATTACKER) {
                total += self.keyword_bonus(card, |kw| match kw {
                    Keyword::Assault(n) => Some(n),
                    _ => None,
                });
            }
            if row.has(FLAG_DEFENDER) {
                total += self.keyword_bonus(card, |kw| match kw {
                    Keyword::Shield(n) => Some(n),
                    _ => None,
                });
            }
        }
        total += self.projected_might(card);
        total.max(0)
    }

    pub fn sync_might(&mut self) {
        if !self.blob.is_enforced() {
            return;
        }
        let values: Vec<_> = self
            .faces_on_board()
            .filter(|card| is_unit_face(card) && !card.is_hidden() && !self.is_facedown(card.id))
            .map(|card| {
                (
                    card.id,
                    self.current_might(card.id) - self.printed_might(card.id),
                )
            })
            .collect();
        for (card, value) in values {
            self.set_counter(card, COUNTER_MIGHT, value);
        }
    }

    fn keyword_bonus(&self, card: u32, pick: fn(Keyword) -> Option<u8>) -> i32 {
        let printed: i32 = self
            .script(card)
            .map(|script| {
                script
                    .keywords
                    .iter()
                    .filter_map(|kw| pick(*kw))
                    .map(i32::from)
                    .sum()
            })
            .unwrap_or(0);
        let granted: i32 = self
            .blob
            .card_state(card)
            .map(|row| {
                row.granted
                    .iter()
                    .filter_map(|(kw, _)| pick(*kw))
                    .map(i32::from)
                    .sum()
            })
            .unwrap_or(0);
        printed + granted + i32::from(self.projected_keyword_sum(card, pick))
    }

    pub fn damage_on(&self, card: u32) -> i32 {
        self.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    pub fn damage_marked_by(&self, card: u32, seat: u8) -> u8 {
        self.blob
            .card_state(card)
            .map_or(0, |row| row.marked_by(seat))
    }

    pub fn damage_marks(&self, card: u32) -> Vec<(u8, u8)> {
        self.blob
            .card_state(card)
            .map_or_else(Vec::new, |row| row.damage_marks.clone())
    }

    pub fn marker_of(&self, unit: u32, cause: Cause) -> Option<u8> {
        match cause {
            Cause::Item(item)
            | Cause::Cleanup {
                last_item: Some(item),
            } => self.live_item(item).map(|held| held.controller),
            Cause::Ability(source) => Some(self.controller(source.card)),
            Cause::Combat => self.blob.showdown.as_ref().map(|showdown| {
                if self.controller(unit) == showdown.attacker {
                    showdown.defender
                } else {
                    showdown.attacker
                }
            }),
            Cause::Replacement | Cause::Cleanup { last_item: None } | Cause::Cost | Cause::Rule => {
                None
            }
        }
    }

    pub fn points(&self, seat: u8) -> i32 {
        self.table
            .counter(Target::Seat(seat), COUNTER_POINTS)
            .unwrap_or(0)
    }

    pub fn xp(&self, seat: u8) -> i32 {
        self.table
            .counter(Target::Seat(seat), COUNTER_XP)
            .unwrap_or(0)
    }

    pub fn spend_xp(&mut self, seat: u8, amount: u8) -> bool {
        if amount == 0 || self.xp(seat) < i32::from(amount) {
            return false;
        }
        self.emit(Effect::score(seat, COUNTER_XP, -i32::from(amount)));
        self.narrate(format!("{{seat {seat}}} spends {amount} XP"));
        true
    }

    pub fn add_to_pool(&mut self, seat: u8, source: u32, adds: &Cost) {
        let added = Pool::of(adds, &self.domains_of(source));
        if added.is_empty() {
            return;
        }
        self.blob.seat_mut(seat).pool.add(&added);
        self.narrate(format!(
            "{{seat {seat}}} adds {} to their rune pool",
            added.label()
        ));
    }

    pub fn spend_pool(&mut self, seat: u8, spent: &Pool) {
        self.blob.seat_mut(seat).pool.take(spent);
        self.narrate(format!(
            "{{seat {seat}}} spends {} from their rune pool",
            spent.label()
        ));
    }

    pub fn promise(&mut self, seat: u8, kind: PromiseKind, effect: PromiseEffect, until: Expiry) {
        self.blob.seat_mut(seat).promises.push(Promise {
            kind,
            effect,
            until,
        });
    }

    pub fn promise_spell_bonus(&mut self, seat: u8, bonus: u8) {
        let row = self.blob.seat_mut(seat);
        row.next_spell_bonus = row.next_spell_bonus.saturating_add(bonus);
    }

    pub fn bind_spell_bonus(&mut self, seat: u8, item: u16, card: u32) {
        let row = self.blob.seat_mut(seat);
        let promised = std::mem::take(&mut row.next_spell_bonus);
        if promised > 0 {
            row.spell_bonus = (item, promised);
            self.narrate(format!(
                "{{card {card}}} deals {promised} Bonus Damage · the promised next spell"
            ));
        }
    }

    pub fn keep_promise(&mut self, seat: u8, index: u8) {
        let promises = &mut self.blob.seat_mut(seat).promises;
        if usize::from(index) >= promises.len() {
            return;
        }
        let kept = promises.remove(usize::from(index));
        self.narrate(format!("{{seat {seat}}}'s {} is used", kept.effect.noun()));
    }

    pub fn empty_pools(&mut self) {
        for seat in 0..self.blob.seats.len() {
            let held = std::mem::take(&mut self.blob.seats[seat].pool);
            if !held.is_empty() {
                self.narrate(format!(
                    "{{seat {seat}}}'s rune pool empties · {} lost",
                    held.label()
                ));
            }
        }
    }

    pub fn state_of(&self, card: u32) -> Option<&CardState> {
        self.blob.card_state(card)
    }

    pub fn state_mut(&mut self, card: u32) -> &mut CardState {
        self.blob.card_state_mut(card)
    }

    pub fn has_flag(&self, card: u32, flag: u16) -> bool {
        self.blob.has_flag(card, flag)
    }

    pub fn set_flag(&mut self, card: u32, flag: u16, on: bool) {
        self.blob.set_flag(card, flag, on);
    }

    pub fn named(&self, card: u32) -> Option<&str> {
        self.blob.named(card)
    }

    pub fn name(&mut self, card: u32, name: &str) {
        self.blob.set_named(card, Some(name.to_string()));
        self.narrate(format!("{{card {card}}} names {name}"));
    }

    pub fn unname(&mut self, card: u32) {
        self.blob.set_named(card, None);
    }

    pub fn bears_tag(&self, card: u32, tag: &str) -> bool {
        let tag = tag.trim();
        if tag.is_empty() {
            return false;
        }
        self.card(card).is_some_and(|held| {
            let name = cards::base_name(&held.name);
            let champion = name.split_once(" - ").map(|(champion, _)| champion);
            champion == Some(tag)
                || name
                    .split(|glyph: char| !glyph.is_alphanumeric() && glyph != '\'')
                    .any(|word| word == tag)
        })
    }

    pub fn name_options(&self, kind: NameKind) -> Vec<String> {
        match kind {
            NameKind::Tag => cards::TAGS.iter().map(|tag| tag.to_string()).collect(),
            NameKind::Spell => cards::spell_names()
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }

    pub fn enter(&mut self, action: &Action, seat: u8) -> Result<(), ApplyError> {
        self.forget_index();
        self.table.apply_entry(action, seat)?;
        self.origin.apply_entry(action, seat)
    }

    pub fn replay_table(&mut self) {
        let mut table = self.origin.clone();
        for effect in &self.effects {
            let _ = table.apply(effect, self.actor);
        }
        self.table = table;
        self.forget_index();
    }

    pub fn emit(&mut self, effect: Effect) {
        if let Effect::Despawn { card } = effect {
            if let Some(face) = self.card(card).cloned() {
                self.departed.push(face);
            }
        }
        if let Err(error) = self.table.apply(&effect, self.actor) {
            if self.fault.is_none() {
                self.fault = Some((self.effects.len(), error));
            }
        }
        if matches!(
            effect,
            Effect::Move { .. } | Effect::Spawn { .. } | Effect::Despawn { .. }
        ) {
            self.forget_index();
        }
        self.effects.push(effect);
    }

    pub fn raise(&mut self, event: Event) {
        if let Event::Played {
            card,
            controller,
            ref kind,
            ..
        } = event
        {
            if kind == KIND_GEAR
                && self
                    .script(card)
                    .is_some_and(|script| script.is_equipment())
            {
                self.blob.seat_mut(controller).equipment_played = true;
            }
        }
        self.events.push(event);
    }

    pub fn narrate(&mut self, line: impl Into<String>) {
        self.blob.narrate(line);
    }

    pub fn ask(&mut self, seat: u8, min: u8, max: u8, cancel: bool, why: PromptWhy) -> Ask {
        let id = self.blob.next_prompt_id();
        let mut prompt = Prompt::new(id, seat, min, max);
        if cancel {
            prompt = prompt.cancellable();
        }
        let ask = Ask { prompt, why };
        self.blob.open_prompt(ask.clone());
        ask
    }

    pub fn play_limited(
        &mut self,
        play: crate::engine::play::LimitedPlay,
    ) -> Result<(), crate::Refusal> {
        crate::engine::play::begin_limited(self, play)
    }

    pub fn ask_resume(&mut self, item: &ChainItem, stage: u8, min: u8, max: u8) -> Ask {
        self.ask_seat_resume(item, item.controller, stage, min, max)
    }

    pub fn ask_seat_resume(
        &mut self,
        item: &ChainItem,
        seat: u8,
        stage: u8,
        min: u8,
        max: u8,
    ) -> Ask {
        self.ask(
            seat,
            min,
            max,
            false,
            PromptWhy::Resume {
                item: item.id,
                stage,
            },
        )
    }

    pub fn ask_name(&mut self, item: &ChainItem, kind: NameKind, stage: u8) -> Ask {
        self.ask(
            item.controller,
            1,
            1,
            false,
            PromptWhy::Name {
                item: item.id,
                kind,
                stage,
            },
        )
    }

    pub fn ask_pay_or_let(
        &mut self,
        item: &ChainItem,
        payer: u8,
        cost: &crate::engine::cost::Cost,
        stage: u8,
    ) -> Ask {
        let card = item.kind.source();
        self.narrate(format!(
            "{{seat {payer}}} may pay {} to keep {{card {card}}}",
            cost.label()
        ));
        self.ask(
            payer,
            1,
            1,
            false,
            PromptWhy::PayOrLet {
                item: item.id,
                stage,
            },
        )
    }

    pub fn await_faces(&mut self, item: &ChainItem, cards: &[u32], stage: u8) -> Ask {
        for card in cards {
            self.reveal(*card);
        }
        self.awaiting = cards.to_vec();
        Ask {
            prompt: Prompt::new(0, item.controller, 0, 0),
            why: PromptWhy::Resume {
                item: item.id,
                stage,
            },
        }
    }

    pub fn picks(&self) -> &[u32] {
        &self.picked
    }

    pub fn remember(&mut self, target: TargetRef) {
        self.remembered.push(target);
    }

    pub fn chain_item(&self, item: u16) -> Option<&ChainItem> {
        self.blob.chain.iter().find(|held| held.id == item)
    }

    pub fn live_item(&self, item: u16) -> Option<&ChainItem> {
        self.chain_item(item)
            .or_else(|| self.resolving.as_ref().filter(|held| held.id == item))
    }

    pub fn is_on_chain(&self, item: u16) -> bool {
        self.chain_item(item).is_some()
    }

    pub fn delay(&mut self, when: When, source: u32, seat: u8, ability: u8, args: Vec<u32>) {
        self.blob.delayed.push(Delayed {
            when,
            source,
            seat,
            ability,
            args,
        });
    }

    pub fn top_of(&self, zone: u16, seat: u8, count: usize) -> Vec<u32> {
        let held: Vec<u32> = self.table.held(zone, seat).map(|card| card.id).collect();
        held.iter().rev().take(count).copied().collect()
    }

    pub fn deal(&mut self, seat: u8, count: usize) -> Vec<u32> {
        let (Some(deck), Some(hand)) = (self.zones.main_deck, self.zones.hand) else {
            return Vec::new();
        };
        let cards = self.top_of(deck, seat, count);
        for card in &cards {
            self.emit(Effect::Move {
                card: *card,
                zone: hand,
                seat,
                index: TOP,
            });
        }
        cards
    }

    pub fn draw(&mut self, seat: u8, count: usize) -> usize {
        let cards = self.deal(seat, count);
        for _ in &cards {
            let nth = {
                let row = self.blob.seat_mut(seat);
                row.draws = row.draws.saturating_add(1);
                row.draws
            };
            self.raise(Event::Drew { seat, nth });
        }
        cards.len()
    }

    pub fn channel(&mut self, seat: u8, count: usize) -> usize {
        let (Some(deck), Some(pool)) = (self.zones.rune_deck, self.zones.rune_pool) else {
            return 0;
        };
        let runes = self.top_of(deck, seat, count);
        for rune in &runes {
            self.emit(Effect::Move {
                card: *rune,
                zone: pool,
                seat,
                index: TOP,
            });
        }
        runes.len()
    }

    pub fn channel_exhausted(&mut self, seat: u8, count: usize) -> usize {
        let Some(pool) = self.zones.rune_pool else {
            return 0;
        };
        let before: Vec<u32> = self.table.held(pool, seat).map(|card| card.id).collect();
        let channelled = self.channel(seat, count);
        let arrived: Vec<u32> = self
            .table
            .held(pool, seat)
            .map(|card| card.id)
            .filter(|rune| !before.contains(rune))
            .collect();
        for rune in arrived {
            self.exhaust(rune);
        }
        if channelled > 0 {
            self.narrate(format!(
                "{{seat {seat}}} channels {channelled} rune{} exhausted",
                if channelled == 1 { "" } else { "s" }
            ));
        }
        channelled
    }

    pub fn burn(&mut self, seat: u8, count: usize) -> usize {
        self.burn_cards(seat, count).len()
    }

    pub fn burn_cards(&mut self, seat: u8, count: usize) -> Vec<u32> {
        let (Some(deck), Some(trash)) = (self.zones.main_deck, self.zones.trash) else {
            return Vec::new();
        };
        let cards = self.top_of(deck, seat, count);
        for card in &cards {
            self.emit(Effect::Move {
                card: *card,
                zone: trash,
                seat,
                index: TOP,
            });
            self.raise(Event::Burned { seat, card: *card });
        }
        if !cards.is_empty() {
            self.narrate(format!("{{seat {seat}}} burns {}", cards.len()));
        }
        cards
    }

    pub fn reveal_top(&mut self, seat: u8) -> Option<u32> {
        let (Some(deck), Some(chain)) = (self.zones.main_deck, self.zones.chain) else {
            return None;
        };
        let top = self.top_of(deck, seat, 1).first().copied()?;
        self.emit(Effect::Move {
            card: top,
            zone: chain,
            seat: 0,
            index: TOP,
        });
        self.set_flag(top, FLAG_REVEALING, true);
        self.emit(Effect::Reveal { card: top });
        self.narrate(format!(
            "{{seat {seat}}} reveals the top card of their deck"
        ));
        Some(top)
    }

    pub fn reveal(&mut self, card: u32) {
        if self.card(card).is_none() || self.table.is_revealed(card) {
            return;
        }
        self.emit(Effect::Reveal { card });
    }

    pub fn reveal_hand(&mut self, seat: u8) -> Vec<u32> {
        let hand = self.hand_of(seat);
        for card in &hand {
            self.reveal(*card);
        }
        self.narrate(format!("{{seat {seat}}} reveals their hand"));
        hand
    }

    pub fn peek(&mut self, card: u32, seat: u8) {
        if self.card(card).is_none() || self.table.is_revealed(card) || seat >= self.players() {
            return;
        }
        self.emit(Effect::Peek { card, seat });
    }

    pub fn peek_top(&mut self, seat: u8) -> Option<u32> {
        let deck = self.zones.main_deck?;
        let top = self.top_of(deck, seat, 1).first().copied()?;
        self.peek(top, seat);
        self.narrate(format!(
            "{{seat {seat}}} looks at the top card of their deck"
        ));
        Some(top)
    }

    pub fn look_at_facedown(&mut self, viewer: u8, of: u8) {
        if viewer >= self.players() || of >= self.players() {
            return;
        }
        self.blob.seat_mut(viewer).looks_facedown_of |= 1 << of;
        for card in hide::of_seat(self, of) {
            self.peek(card, viewer);
        }
        self.narrate(format!(
            "{{seat {viewer}}} may look at {{seat {of}}}'s facedown cards this turn"
        ));
    }

    pub fn recycle_to_bottom(&mut self, card: u32) {
        let Some(held) = self.card(card) else {
            return;
        };
        let owner = held.owner;
        let deck = if is_rune_face(held) {
            self.zones.rune_deck
        } else {
            self.zones.main_deck
        };
        let Some(deck) = deck else {
            return;
        };
        hide::reveal_before(self, card, deck);
        self.emit(Effect::Move {
            card,
            zone: deck,
            seat: owner,
            index: agni_plugin_sdk::decide::BOTTOM,
        });
    }

    pub fn exhaust(&mut self, card: u32) -> bool {
        if self.card(card).is_none_or(|held| held.exhausted) {
            return false;
        }
        self.emit(Effect::exhaust(card));
        true
    }

    fn ready_suppressed_by(&self, card: u32, pick: fn(&Static) -> Option<Suppresses>) -> bool {
        self.table.cards.iter().any(|held| {
            statics::in_play(self, held.id)
                && self.script(held.id).is_some_and(|script| {
                    script
                        .statics
                        .iter()
                        .filter_map(pick)
                        .any(|suppresses| suppresses(self, held.id, card))
                })
        })
    }

    pub fn ready_suppressed(&self, card: u32) -> bool {
        self.ready_suppressed_by(card, |held| match held {
            Static::ReadySuppressed { by_effects, .. } => Some(*by_effects),
            _ => None,
        })
    }

    pub fn awaken_suppressed(&self, card: u32) -> bool {
        self.ready_suppressed_by(card, |held| match held {
            Static::ReadySuppressed { by_awaken, .. } => Some(*by_awaken),
            _ => None,
        })
    }

    pub fn ready(&mut self, card: u32) -> bool {
        if self.card(card).is_none_or(|held| !held.exhausted) || self.ready_suppressed(card) {
            return false;
        }
        self.emit(Effect::ready(card));
        let by = self.actor;
        self.raise(Event::Readied { card, by });
        true
    }

    pub fn awaken(&mut self, card: u32, by: u8) -> bool {
        if self.card(card).is_none_or(|held| !held.exhausted) || self.awaken_suppressed(card) {
            return false;
        }
        self.emit(Effect::ready(card));
        self.raise(Event::Readied { card, by });
        true
    }

    pub fn zone_of(&self, at: Location) -> Option<(u16, u8)> {
        match at {
            Location::Base(seat) => self.zones.base.map(|zone| (zone, seat)),
            Location::Battlefield(zone) => Some((zone, 0)),
        }
    }

    pub fn destination_capped(&self, unit: u32, to: Location) -> bool {
        match to {
            Location::Battlefield(zone) => self.other_seats_at(zone, self.controller(unit)) >= 2,
            Location::Base(_) => false,
        }
    }

    pub fn arrived(&mut self, card: u32, from: Option<Location>, to: Location, cause: MoveCause) {
        let controller = self.controller(card);
        if let Location::Battlefield(zone) = to {
            self.contest(zone, controller);
        }
        let by = (!matches!(cause, MoveCause::Standard)).then_some(
            self.resolving
                .as_ref()
                .map(|item| item.controller)
                .unwrap_or(self.actor),
        );
        self.raise(Event::Moved {
            card,
            from,
            to,
            cause,
            by,
        });
    }

    pub fn move_unit(&mut self, card: u32, to: Location, cause: MoveCause) -> Moved {
        if !self.is_unit(card) {
            return Moved::NotAUnit;
        }
        let from = self.location(card);
        if self.destination_capped(card, to) {
            self.recall(card, false);
            return Moved::Recalled;
        }
        if from == Some(to) {
            return Moved::Moved;
        }
        let Some((zone, seat)) = self.zone_of(to) else {
            return Moved::NotAUnit;
        };
        self.emit(Effect::Move {
            card,
            zone,
            seat,
            index: TOP,
        });
        self.arrived(card, from, to, cause);
        Moved::Moved
    }

    pub fn recall(&mut self, card: u32, exhausted: bool) {
        let owner = self.controller(card);
        let Some(base) = self.zones.base else {
            return;
        };
        if self.location(card) != Some(Location::Base(owner)) {
            self.emit(Effect::Move {
                card,
                zone: base,
                seat: owner,
                index: TOP,
            });
        }
        if exhausted {
            self.exhaust(card);
        }
    }

    pub fn kill(&mut self, card: u32, cause: Cause) -> Killed {
        kill::kill(self, card, cause)
    }

    pub fn last_face(&self, card: u32) -> Option<&CardInfo> {
        self.card(card)
            .or_else(|| self.departed.iter().rev().find(|face| face.id == card))
            .or_else(|| self.origin.card(card))
    }

    pub fn note(&self, card: u32) -> Noted {
        Noted {
            zone: self.card(card).and_then(|held| held.zone).unwrap_or(0),
            might: self.current_might(card),
            controller: self.controller(card),
            alone: self.alone_at(card),
            buffed: self.is_buffed(card),
        }
    }

    pub fn banish(&mut self, card: u32) -> bool {
        let by = self.controller(card);
        self.banish_by(card, by)
    }

    pub fn banish_by(&mut self, card: u32, by: u8) -> bool {
        let Some(held) = self.card(card) else {
            return false;
        };
        let owner = held.owner;
        let token = self.is_token(card);
        if self.on_board(card) {
            attach::detach_all(self, card);
            if attach::is_attached(self, card) {
                attach::detach(self, card);
            }
            self.clear_designation(card);
        }
        if token {
            self.emit(Effect::Despawn { card });
        } else {
            let Some(zone) = self.zones.banishment else {
                return false;
            };
            hide::reveal_before(self, card, zone);
            self.emit(Effect::Move {
                card,
                zone,
                seat: owner,
                index: TOP,
            });
        }
        self.blob.drop_card_state(card);
        self.raise(Event::Banished {
            card,
            owner,
            by,
            token,
        });
        self.narrate(format!("{{card {card}}} is banished"));
        true
    }

    pub fn banish_instead_of_trash(&mut self, card: u32, by: u32) -> Trashed {
        self.narrate(format!(
            "{{card {card}}} would go to the trash · {{card {by}}} banishes it instead"
        ));
        self.banish(card);
        Trashed::Banished { by }
    }

    pub fn file_in_trash(&mut self, card: u32, seat: u8) -> Trashed {
        let from = self.card(card).and_then(|held| held.zone);
        if let Some(by) = self.banishes_instead_of_trash(card, from) {
            return self.banish_instead_of_trash(card, by);
        }
        if let Some(trash) = self.zones.trash {
            self.emit(Effect::Move {
                card,
                zone: trash,
                seat,
                index: TOP,
            });
        }
        Trashed::Trashed
    }

    pub fn trash(&mut self, card: u32) -> Trashed {
        let owner = self.owner(card);
        let filed = if self.is_token(card) {
            self.emit(Effect::Despawn { card });
            Trashed::Trashed
        } else {
            self.file_in_trash(card, owner)
        };
        self.blob.drop_card_state(card);
        filed
    }

    pub fn discard(&mut self, seat: u8, card: u32) -> Trashed {
        let filed = self.file_in_trash(card, seat);
        self.blob.drop_card_state(card);
        self.narrate(format!("{{seat {seat}}} discards {{card {card}}}"));
        filed
    }

    pub fn spawn(&mut self, owner: u8, token: Token, at: Location, ready: bool) -> Option<u32> {
        let (zone, seat) = self.zone_of(at)?;
        let id = self.table.next_id;
        self.emit(Effect::Spawn {
            face: token.face(),
            zone,
            seat,
            owner: Some(owner),
        });
        self.spawned += 1;
        if token == Token::Sprite {
            self.emit(Effect::Counter {
                target: Target::Card(id),
                counter: COUNTER_TEMPORARY,
                delta: 1,
            });
        }
        self.raise(Event::Played {
            card: id,
            controller: owner,
            kind: token.kind().into(),
            origin: Origin::Board,
            paid_additional: false,
        });
        self.raise(Event::Entered { card: id, at });
        if !self.enters_ready(id, owner, ready) {
            self.exhaust(id);
        }
        Some(id)
    }

    fn spawn_battlefield(&mut self, owner: u8, token: Token, zone: u16) -> u32 {
        let id = self.table.next_id;
        self.emit(Effect::Spawn {
            face: token.face(),
            zone,
            seat: 0,
            owner: Some(owner),
        });
        self.spawned += 1;
        id
    }

    fn inherit_card_state(&mut self, from: u32, to: u32) {
        let Some(row) = self.blob.card_state(from).cloned() else {
            return;
        };
        self.blob.drop_card_state(from);
        *self.blob.card_state_mut(to) = CardState { id: to, ..row };
    }

    pub fn add_battlefield_token(&mut self, owner: u8, token: Token) -> Option<u16> {
        if token.kind() != KIND_BATTLEFIELD {
            return None;
        }
        let zone = self.zones.spare_battlefield(&self.table)?;
        let id = self.spawn_battlefield(owner, token, zone);
        self.zones.battlefields.push(zone);
        self.blob.set_holder(zone, None);
        self.blob.set_contested(zone, None);
        self.narrate(format!(
            "{{seat {owner}}} adds {{card {id}}} to the board at {{zone {zone}}}"
        ));
        Some(zone)
    }

    pub fn replace_battlefield(&mut self, zone: u16, token: Token) -> Option<u32> {
        if token.kind() != KIND_BATTLEFIELD || !self.zones.is_battlefield(zone) {
            return None;
        }
        let replaced = self.battlefield_card_at(zone)?;
        let owner = self.owner(replaced);
        let original = if self.is_token(replaced) {
            self.replaced_by(replaced)
        } else {
            Some(replaced)
        };
        let id = self.spawn_battlefield(owner, token, zone);
        if let Some(original) = original {
            self.emit(Effect::Annotate {
                card: id,
                key: ANNOTATION_REPLACED.into(),
                value: Some(original.to_le_bytes().to_vec()),
            });
        }
        self.inherit_card_state(replaced, id);
        if self.is_token(replaced) {
            self.emit(Effect::Despawn { card: replaced });
        } else if let Some(banishment) = self.zones.banishment {
            self.emit(Effect::Move {
                card: replaced,
                zone: banishment,
                seat: owner,
                index: TOP,
            });
        }
        self.narrate(format!(
            "{{card {replaced}}} is replaced with {{card {id}}} at {{zone {zone}}}"
        ));
        Some(id)
    }

    pub fn swap_back(&mut self, zone: u16) -> Option<u32> {
        let token = self.battlefield_card_at(zone)?;
        if !self.is_token(token) {
            self.narrate(format!("{{card {token}}} has nothing to swap back to"));
            return None;
        }
        let original = self
            .replaced_by(token)
            .filter(|original| self.in_banishment(*original));
        let Some(original) = original else {
            self.narrate(format!("{{card {token}}} has nothing to swap back to"));
            return None;
        };
        self.inherit_card_state(token, original);
        self.emit(Effect::Despawn { card: token });
        self.emit(Effect::Move {
            card: original,
            zone,
            seat: 0,
            index: TOP,
        });
        self.narrate(format!(
            "{{card {token}}} swaps back for {{card {original}}} at {{zone {zone}}}"
        ));
        Some(original)
    }

    pub fn enters_ready(&mut self, card: u32, seat: u8, hint: bool) -> bool {
        let unit = self.is_unit(card);
        let flags = self.blob.seat(seat);
        let this_turn = unit && (flags.units_enter_ready_this_turn || flags.next_unit_enters_ready);
        if unit && flags.next_unit_enters_ready {
            self.blob.seat_mut(seat).next_unit_enters_ready = false;
        }
        hint || this_turn || statics::enters_ready(self, card)
    }
    pub fn bonus_damage(&self, unit: u32, cause: Cause) -> u8 {
        let promised = match cause {
            Cause::Item(item)
            | Cause::Cleanup {
                last_item: Some(item),
            } => self
                .live_item(item)
                .filter(|held| matches!(held.kind, ItemKind::Spell { .. }))
                .map(|held| self.blob.seat(held.controller).spell_bonus)
                .filter(|(bound, _)| *bound == item)
                .map_or(0, |(_, bonus)| bonus),
            _ => 0,
        };
        self.table
            .cards
            .iter()
            .filter(|held| self.face_in_play(held) && !held.is_hidden())
            .filter(|held| !self.is_pending_play(held.id) && !self.is_facedown(held.id))
            .flat_map(|held| {
                self.script(held.id)
                    .map_or(&[][..], |script| script.statics)
                    .iter()
                    .map(move |held_static| match held_static {
                        Static::BonusDamage(bonus) => bonus(self, held.id, unit, &cause),
                        _ => 0,
                    })
            })
            .fold(promised, u8::saturating_add)
    }

    pub fn no_damage(&self, unit: u32, cause: Cause) -> bool {
        if !statics::in_play(self, unit) {
            return false;
        }
        self.script(unit)
            .map_or(&[][..], |script| script.statics)
            .iter()
            .copied()
            .chain(self.projected_statics(unit))
            .any(|held_static| match held_static {
                Static::NoDamage(applies) => applies(self, unit, &cause),
                _ => false,
            })
    }

    pub fn any_damage_is_lethal(&self, marker: u8, unit: u32) -> bool {
        self.table
            .cards
            .iter()
            .filter(|held| self.face_in_play(held) && !held.is_hidden())
            .filter(|held| !self.is_pending_play(held.id) && !self.is_facedown(held.id))
            .filter(|held| self.controller(held.id) == marker)
            .any(|held| {
                self.script(held.id)
                    .map_or(&[][..], |script| script.statics)
                    .iter()
                    .copied()
                    .chain(self.projected_statics(held.id))
                    .any(|held_static| match held_static {
                        Static::LethalDamage(applies) => applies(self, held.id, unit),
                        _ => false,
                    })
            })
    }

    pub fn lethal_damage_marked(&self, unit: u32) -> bool {
        self.blob.card_state(unit).is_some_and(|row| {
            row.damage_marks
                .iter()
                .any(|(marker, n)| *n > 0 && self.any_damage_is_lethal(*marker, unit))
        })
    }

    pub fn damage_multiplier(&self, unit: u32) -> u8 {
        self.blob
            .card_state(unit)
            .map_or(1, |row| row.damage_multiplier_this_turn.max(1))
    }

    pub fn multiply_damage_this_turn(&mut self, unit: u32, factor: u8) -> bool {
        if factor < 2 || !self.is_unit(unit) || !self.on_board(unit) {
            return false;
        }
        let row = self.state_mut(unit);
        row.damage_multiplier_this_turn = row
            .damage_multiplier_this_turn
            .max(1)
            .saturating_mul(factor);
        true
    }

    pub fn damage(&mut self, card: u32, amount: u8, cause: Cause) -> bool {
        let marker = self.marker_of(card, cause);
        self.damage_by(card, amount, cause, marker)
    }

    pub fn damage_by(&mut self, card: u32, amount: u8, cause: Cause, marker: Option<u8>) -> bool {
        if amount == 0 || !self.is_unit(card) || !self.on_board(card) {
            return false;
        }
        if self.no_damage(card, cause) {
            self.narrate(format!("{{card {card}}}: the damage is prevented"));
            return false;
        }
        let bonus = self.bonus_damage(card, cause);
        if bonus > 0 {
            self.narrate(format!("{{card {card}}}: {bonus} Bonus Damage"));
        }
        let amount = prevent::spend(self, card, amount.saturating_add(bonus), cause);
        if amount == 0 {
            self.narrate(format!("{{card {card}}}: the damage is prevented"));
            return false;
        }
        let factor = self.damage_multiplier(card);
        let amount = if factor > 1 {
            let multiplied = amount.saturating_mul(factor);
            self.narrate(format!(
                "{{card {card}}}: {amount} damage becomes {multiplied} · trapped this turn"
            ));
            multiplied
        } else {
            amount
        };
        self.emit(Effect::Counter {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            delta: i32::from(amount),
        });
        if let Some(marker) = marker {
            self.state_mut(card).mark_damage(marker, amount);
        }
        self.raise(Event::DamageDealt {
            card,
            n: amount,
            source: cause,
        });
        true
    }

    pub fn bounce(&mut self, card: u32) -> bool {
        if !self.on_board(card) {
            return false;
        }
        let owner = self.owner(card);
        if let Some(hand) = self.zones.hand {
            hide::reveal_before(self, card, hand);
        }
        if self.is_token(card) {
            self.emit(Effect::Despawn { card });
        } else if let Some(hand) = self.zones.hand {
            self.emit(Effect::Move {
                card,
                zone: hand,
                seat: owner,
                index: TOP,
            });
        } else {
            return false;
        }
        self.blob.drop_card_state(card);
        self.narrate(format!("{{card {card}}} returns to hand"));
        true
    }

    pub fn uncounterable(&self, item: &ChainItem) -> bool {
        let own = item.kind.card().is_some_and(|card| {
            self.script(card).is_some_and(|script| {
                script.statics.iter().any(|held| match held {
                    Static::Uncounterable(applies) => applies(self, card),
                    _ => false,
                })
            })
        });
        own || self
            .statics_in_play()
            .into_iter()
            .any(|(_, held)| match held {
                Static::ProtectsFromCounter(protects) => protects(self, item),
                _ => false,
            })
    }

    pub fn refuse_counter(&mut self, item: u16) -> bool {
        let Some(held) = self.chain_item(item).cloned() else {
            return false;
        };
        if !self.uncounterable(&held) {
            return false;
        }
        match held.kind.card() {
            Some(card) => self.narrate(format!("{{card {card}}} can't be countered")),
            None => self.narrate(format!(
                "{{card {}}} ability can't be countered",
                held.kind.source()
            )),
        }
        true
    }

    pub fn counter_item(&mut self, item: u16, dest: CounterDest) -> bool {
        if self.refuse_counter(item) {
            return false;
        }
        let Some(index) = self.blob.chain.iter().position(|held| held.id == item) else {
            return false;
        };
        let countered = self.blob.chain.remove(index);
        if let Some(card) = countered.kind.card() {
            let owner = self.controller(card);
            self.set_flag(card, FLAG_NOT_PLAYED, true);
            if self.is_token(card) {
                self.emit(Effect::Despawn { card });
            } else if dest == CounterDest::Trash {
                self.file_in_trash(card, owner);
            } else if let Some(hand) = self.zones.hand {
                self.emit(Effect::Move {
                    card,
                    zone: hand,
                    seat: owner,
                    index: TOP,
                });
            }
            self.blob.drop_card_state(card);
            let where_to = match dest {
                CounterDest::Trash => "",
                CounterDest::Hand => " · back to hand",
            };
            self.narrate(format!("{{card {card}}} is countered{where_to}"));
        } else {
            self.narrate(format!(
                "{{card {}}} ability is countered",
                countered.kind.source()
            ));
        }
        true
    }

    pub fn wounded(&self) -> Vec<(u32, i32)> {
        self.table
            .counters
            .iter()
            .filter(|held| held.counter == COUNTER_DAMAGE && held.value > 0)
            .filter_map(|held| match held.target {
                Target::Card(card) => Some((card, held.value)),
                _ => None,
            })
            .collect()
    }

    pub fn heal(&mut self, card: u32) -> bool {
        if self.blob.card_state(card).is_some() {
            self.state_mut(card).damage_marks.clear();
        }
        let damage = self.damage_on(card);
        if damage <= 0 {
            return false;
        }
        self.emit(Effect::Counter {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            delta: -damage,
        });
        true
    }

    pub fn heal_all(&mut self) {
        for (card, _) in self.wounded() {
            self.heal(card);
        }
    }

    pub fn shroud(&mut self, card: u32) -> bool {
        if !self.on_board(card) || self.has_flag(card, FLAG_SHROUDED) {
            return false;
        }
        self.set_flag(card, FLAG_SHROUDED, true);
        self.narrate(format!(
            "{{card {card}}} can't be chosen by enemy spells and abilities this turn"
        ));
        true
    }

    pub fn is_shrouded(&self, card: u32) -> bool {
        self.has_flag(card, FLAG_SHROUDED)
    }

    pub fn queue_turn(&mut self, seat: u8) {
        self.blob.extra_turns.insert(0, seat);
        self.raise(Event::TurnQueued { seat });
        self.narrate(format!("{{seat {seat}}} takes a turn after this one"));
    }

    pub fn excess_damage_in_attack(&self, seat: u8, zone: u16) -> Option<u8> {
        self.blob.excess_in_attack(seat, zone)
    }

    pub fn record_excess_damage(&mut self, seat: u8, zone: u16, amount: u8) {
        self.blob.record_excess(seat, zone, amount);
        if amount > 0 {
            self.narrate(format!(
                "{{seat {seat}}} assigned {amount} excess damage at {{zone {zone}}}"
            ));
        }
    }

    pub fn prevent(&mut self, source: DamageSource, value: Amount, until: Expiry) {
        self.blob.preventions.push(Prevention {
            unit: None,
            source,
            value,
            until,
        });
    }

    pub fn prevent_on(&mut self, unit: u32, source: DamageSource, value: Amount, until: Expiry) {
        self.blob.preventions.push(Prevention {
            unit: Some(unit),
            source,
            value,
            until,
        });
    }

    pub fn score_effect(&mut self, seat: u8) {
        self.emit(Effect::score(seat, COUNTER_POINTS, 1));
        self.narrate(format!("{{seat {seat}}} scores 1 point"));
    }

    pub fn deals_combat_damage(&self, unit: u32) -> bool {
        if self.is_stunned(unit) {
            return false;
        }
        !self
            .table
            .cards
            .iter()
            .filter(|held| self.face_in_play(held) && held.id != unit && !held.is_hidden())
            .filter(|held| !self.is_pending_play(held.id) && !self.is_facedown(held.id))
            .filter(|held| !attach::is_attached(self, held.id))
            .any(|held| {
                self.script(held.id).is_some_and(|script| {
                    script.statics.iter().any(|held_static| match held_static {
                        Static::NoCombatDamageFrom(suppresses) => suppresses(self, held.id, unit),
                        _ => false,
                    })
                })
            })
    }

    pub fn grant(&mut self, card: u32, keyword: Keyword, until: Expiry) -> bool {
        match keyword {
            Keyword::Equip(_) | Keyword::Empower(_) => false,
            Keyword::Flow(_) | Keyword::Repeat(_) => {
                let Some(grant) = CostedGrant::from_keyword(keyword, until) else {
                    return false;
                };
                self.grant_costed(card, grant)
            }
            _ => {
                self.state_mut(card).granted.push((keyword, until));
                true
            }
        }
    }

    pub fn grant_costed(&mut self, card: u32, grant: CostedGrant) -> bool {
        if matches!(
            grant.kind,
            crate::state::CostedKind::Equip | crate::state::CostedKind::Empower
        ) {
            return false;
        }
        self.state_mut(card).granted_costed.push(grant);
        true
    }

    pub fn grant_costed_this_turn(&mut self, card: u32, grant: CostedGrant) -> bool {
        self.grant_costed(
            card,
            CostedGrant {
                until: Expiry::EndOfTurn(self.turn()),
                ..grant
            },
        )
    }

    pub fn grant_keyword_this_turn(&mut self, card: u32, keyword: Keyword) -> bool {
        let until = Expiry::EndOfTurn(self.turn());
        self.grant(card, keyword, until)
    }

    fn mirror_flag(&mut self, card: u32, flag: u16, key: &str, on: bool) -> bool {
        if self.has_flag(card, flag) == on {
            return false;
        }
        self.set_flag(card, flag, on);
        if self.card(card).is_none() {
            return true;
        }
        self.emit(Effect::Annotate {
            card,
            key: key.into(),
            value: on.then(|| vec![1]),
        });
        true
    }

    pub fn stun(&mut self, card: u32) -> bool {
        if !self.is_unit(card) || (!self.on_board(card) && !self.is_pending_play(card)) {
            return false;
        }
        self.mirror_flag(card, FLAG_STUNNED, ANNOTATION_STUNNED, true)
    }

    pub fn unstun(&mut self, card: u32) {
        self.mirror_flag(card, FLAG_STUNNED, ANNOTATION_STUNNED, false);
    }

    pub fn is_stunned(&self, card: u32) -> bool {
        self.has_flag(card, FLAG_STUNNED)
    }

    pub fn buff(&mut self, card: u32) -> bool {
        self.bump_counter(card, COUNTER_BUFFED)
    }

    pub fn counter_cap(&self, card: u32, counter: u16) -> Option<i32> {
        let scripted = self
            .script(card)
            .and_then(|script| {
                script.statics.iter().find_map(|held| match held {
                    Static::CounterCap { counter: held, max } if *held == counter => Some(*max),
                    _ => None,
                })
            })
            .unwrap_or(Some(1));
        let table = self
            .table
            .counter_bounds(counter)
            .and_then(|bounds| bounds.max);
        match (scripted, table) {
            (Some(cap), Some(bound)) => Some(cap.min(bound)),
            (Some(cap), None) => Some(cap),
            (None, bound) => bound,
        }
    }

    pub fn counter_room(&self, card: u32, counter: u16) -> i32 {
        let held = self.table.counter(Target::Card(card), counter).unwrap_or(0);
        self.counter_cap(card, counter)
            .map(|cap| cap.saturating_sub(held).max(0))
            .unwrap_or(i32::MAX)
    }

    fn bump_counter(&mut self, card: u32, counter: u16) -> bool {
        if !self.in_play(card) {
            return false;
        }
        if self.counter_room(card, counter) == 0 {
            return false;
        }
        self.emit(Effect::Counter {
            target: Target::Card(card),
            counter,
            delta: 1,
        });
        true
    }

    pub fn mark_temporary(&mut self, card: u32) -> bool {
        self.set_counter(card, COUNTER_TEMPORARY, 1)
    }

    pub fn disempower(&mut self, card: u32) -> bool {
        if !self.set_counter(card, COUNTER_EMPOWERED, 0) {
            return false;
        }
        self.raise(Event::Disempowered { card });
        true
    }

    pub fn empower(&mut self, card: u32) -> bool {
        let by = self.controller(card);
        self.empower_by(card, by)
    }

    pub fn empower_by(&mut self, card: u32, by: u8) -> bool {
        if !self.bump_counter(card, COUNTER_EMPOWERED) {
            return false;
        }
        self.raise(Event::Empowered { card, by });
        true
    }

    pub fn is_buffed(&self, card: u32) -> bool {
        self.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
            > 0
    }

    pub fn is_empowered(&self, card: u32) -> bool {
        self.table
            .counter(Target::Card(card), COUNTER_EMPOWERED)
            .unwrap_or(0)
            > 0
    }

    fn set_counter(&mut self, card: u32, counter: u16, wanted: i32) -> bool {
        if !self.in_play(card) {
            return false;
        }
        let current = self.table.counter(Target::Card(card), counter).unwrap_or(0);
        if current == wanted {
            return false;
        }
        self.emit(Effect::Counter {
            target: Target::Card(card),
            counter,
            delta: wanted - current,
        });
        true
    }

    pub fn lock_move(&mut self, card: u32) {
        self.set_flag(card, FLAG_NO_MOVE_BY_OWNER, true);
    }

    pub fn is_attacker(&self, card: u32) -> bool {
        self.has_flag(card, FLAG_ATTACKER)
    }

    pub fn is_defender(&self, card: u32) -> bool {
        self.has_flag(card, FLAG_DEFENDER)
    }

    pub fn in_combat(&self, card: u32) -> bool {
        self.is_attacker(card) || self.is_defender(card)
    }

    pub fn designated(&self, flag: u16) -> Vec<u32> {
        self.blob
            .cards
            .iter()
            .filter(|row| row.has(flag))
            .map(|row| row.id)
            .collect()
    }

    pub fn mark_attacker(&mut self, card: u32) -> bool {
        self.mirror_flag(card, FLAG_DEFENDER, ANNOTATION_DEFENDER, false);
        if !self.mirror_flag(card, FLAG_ATTACKER, ANNOTATION_ATTACKER, true) {
            return false;
        }
        self.raise(Event::Attacks { card });
        true
    }

    pub fn mark_defender(&mut self, card: u32) -> bool {
        self.mirror_flag(card, FLAG_ATTACKER, ANNOTATION_ATTACKER, false);
        if !self.mirror_flag(card, FLAG_DEFENDER, ANNOTATION_DEFENDER, true) {
            return false;
        }
        self.raise(Event::Defends { card });
        true
    }

    pub fn clear_designation(&mut self, card: u32) {
        self.mirror_flag(card, FLAG_ATTACKER, ANNOTATION_ATTACKER, false);
        self.mirror_flag(card, FLAG_DEFENDER, ANNOTATION_DEFENDER, false);
    }

    pub fn combat_might(&self, card: u32) -> i32 {
        if self.deals_combat_damage(card) {
            self.current_might(card)
        } else {
            0
        }
    }

    pub fn might(&mut self, card: u32, delta: i16, until: Expiry, min: Option<i32>, src: u16) {
        let current = self.current_might(card);
        let delta = match min {
            Some(floor) if delta < 0 => {
                let allowed = (current - floor).max(0);
                let wanted = i32::from(-delta).min(allowed);
                -(i16::try_from(wanted).unwrap_or(0))
            }
            _ => delta,
        };
        if delta == 0 {
            return;
        }
        self.state_mut(card)
            .might
            .push(crate::state::MightMod { delta, until, src });
        self.emit(Effect::Counter {
            target: Target::Card(card),
            counter: COUNTER_MIGHT,
            delta: i32::from(delta),
        });
    }

    pub fn expire(&mut self, until: Expiry) {
        let mut reversals = Vec::new();
        for row in &mut self.blob.cards {
            let total: i32 = row
                .might
                .iter()
                .filter(|held| held.until == until)
                .map(|held| i32::from(held.delta))
                .sum();
            if total != 0 {
                reversals.push((row.id, -total));
            }
            row.might.retain(|held| held.until != until);
            row.granted.retain(|(_, expiry)| *expiry != until);
            row.granted_costed.retain(|grant| grant.until != until);
            if matches!(until, Expiry::EndOfTurn(_)) {
                row.damage_multiplier_this_turn = 0;
            }
        }
        for seat in &mut self.blob.seats {
            seat.promises.retain(|promise| promise.until != until);
        }
        for (card, delta) in reversals {
            if self.card(card).is_some() {
                self.emit(Effect::Counter {
                    target: Target::Card(card),
                    counter: COUNTER_MIGHT,
                    delta,
                });
            }
        }
    }

    pub fn note_played(&mut self, seat: u8) {
        self.blob.seat_mut(seat).played_main = true;
    }

    pub fn scored_every_battlefield(&self, seat: u8) -> bool {
        !self.zones.battlefields.is_empty()
            && self
                .zones
                .battlefields
                .iter()
                .all(|zone| self.blob.scored(*zone, seat))
    }

    pub fn score(&mut self, seat: u8, by_hold: bool) -> bool {
        self.score_point(seat, by_hold) == Scored::Point
    }

    pub fn score_point(&mut self, seat: u8, by_hold: bool) -> Scored {
        if let Some(by) = self.score_veto(seat) {
            self.narrate(format!("{{seat {seat}}} cannot score · {{card {by}}}"));
            return Scored::Vetoed { by };
        }
        if let Some(by) = self.draws_instead_of_scoring(seat) {
            let drawn = self.draw(seat, 1);
            self.narrate(format!(
                "{{seat {seat}}} draws {drawn} instead of scoring · {{card {by}}}"
            ));
            return Scored::Drew { by };
        }
        let standing = self.points(seat);
        if standing < self.victory_score() - 1 || by_hold || self.scored_every_battlefield(seat) {
            self.emit(Effect::score(seat, COUNTER_POINTS, 1));
            Scored::Point
        } else {
            self.draw(seat, 1);
            Scored::FinalPointDrawn
        }
    }

    fn statics_in_play(&self) -> Vec<(u32, &'static Static)> {
        let mut found: Vec<(u32, &'static Static)> = self
            .table
            .cards
            .iter()
            .filter(|held| statics::in_play(self, held.id))
            .filter_map(|held| self.script(held.id).map(|script| (held.id, script)))
            .flat_map(|(card, script)| script.statics.iter().map(move |held| (card, held)))
            .collect();
        found.sort_by_key(|(card, _)| *card);
        found
    }

    pub fn score_veto(&self, seat: u8) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(card, held)| match held {
                Static::OpponentsCannotScore(applies) => {
                    self.controller(*card) != seat && applies(self, *card)
                }
                _ => false,
            })
            .map(|(card, _)| card)
    }

    pub fn draws_instead_of_scoring(&self, seat: u8) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(card, held)| match held {
                Static::DrawInsteadOfScoring(when) => when(self, *card, seat),
                _ => false,
            })
            .map(|(card, _)| card)
    }

    pub fn no_score_here(&self, zone: u16, seat: u8) -> bool {
        self.table.in_zone(zone).any(|card| {
            card.is_kind(KIND_BATTLEFIELD)
                && statics::in_play(self, card.id)
                && self.script(card.id).is_some_and(|script| {
                    script.statics.iter().any(|held| match held {
                        Static::NoScoreHere(vetoed) => vetoed(self, zone, seat),
                        _ => false,
                    })
                })
        })
    }

    pub fn skips_draw_phase(&self, seat: u8) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(card, held)| match held {
                Static::SkipsDrawPhase(applies) => {
                    self.controller(*card) == seat && applies(self, *card)
                }
                _ => false,
            })
            .map(|(card, _)| card)
    }

    pub fn channel_count(&self, seat: u8) -> usize {
        self.statics_in_play()
            .into_iter()
            .filter_map(|(_, held)| match held {
                Static::ChannelCount(count) => Some(usize::from(count(self, seat))),
                _ => None,
            })
            .fold(rules::runes_this_turn(self.blob), usize::min)
    }

    pub fn banishes_instead_of_trash(&self, card: u32, from: Option<u16>) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(_, held)| match held {
                Static::BanishesInsteadOfTrash(replaces) => replaces(self, card, from),
                _ => false,
            })
            .map(|(source, _)| source)
    }

    pub fn ignores_tank(&self, assigner: u8, zone: u16) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(card, held)| match held {
                Static::IgnoresTank(ignores) => ignores(self, *card, assigner, zone),
                _ => false,
            })
            .map(|(card, _)| card)
    }

    pub fn tie_recalls_all(&self, attacker: u8) -> Option<u32> {
        self.statics_in_play()
            .into_iter()
            .find(|(card, held)| match held {
                Static::TieRecallsAll(recalls) => recalls(self, *card, attacker),
                _ => false,
            })
            .map(|(card, _)| card)
    }

    pub fn deflect_ignored_here(&self, item: &ChainItem, extra: Option<TargetRef>) -> bool {
        self.statics_in_play()
            .into_iter()
            .any(|(card, held)| match held {
                Static::DeflectIgnoredHere(ignored) => ignored(self, item, extra, card),
                _ => false,
            })
    }

    pub fn recall_all(&mut self, zone: u16) -> Vec<u32> {
        let mut units = self.units_at(Location::Battlefield(zone));
        units.sort_unstable();
        for unit in &units {
            self.recall(*unit, false);
        }
        if !units.is_empty() {
            self.narrate(format!(
                "the tie at {{zone {zone}}} recalls every unit there · {}",
                units
                    .iter()
                    .map(|unit| format!("{{card {unit}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        units
    }

    pub fn victory_score(&self) -> i32 {
        self.statics_in_play()
            .into_iter()
            .filter_map(|(_, held)| match held {
                Static::VictoryScore(delta) => Some(*delta),
                _ => None,
            })
            .fold(self.options.victory_score, i32::saturating_add)
    }

    pub fn win(&mut self, seat: u8) {
        if self.blob.won.is_none() && seat < self.players() {
            self.blob.won = Some(seat);
        }
    }

    pub fn score_xp(&mut self, seat: u8, amount: i32) {
        if amount == 0 {
            return;
        }
        self.emit(Effect::score(seat, COUNTER_XP, amount));
    }

    pub fn item_card(&self, kind: ItemKind) -> Option<&CardInfo> {
        kind.card().and_then(|card| self.card(card))
    }

    pub fn domains_of(&self, card: u32) -> Vec<Domain> {
        self.card(card)
            .map(|held| {
                held.domain
                    .iter()
                    .filter_map(|domain| Domain::parse(domain))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn winner(&self) -> Option<u8> {
        self.blob
            .won
            .or_else(|| self.points_winner())
            .or_else(|| self.blob.conceded_winner())
    }

    pub fn points_winner(&self) -> Option<u8> {
        let victory = self.victory_score();
        (0..self.players()).find(|seat| self.points(*seat) >= victory)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scored {
    Point,
    FinalPointDrawn,
    Vetoed { by: u32 },
    Drew { by: u32 },
}

pub fn is_unit_face(card: &CardInfo) -> bool {
    match card.kind.as_deref() {
        Some(kind) => kind == KIND_UNIT,
        None => !card.is_hidden(),
    }
}

pub fn is_rune_face(card: &CardInfo) -> bool {
    card.is_kind(KIND_RUNE) || card.name.ends_with(RUNE_SUFFIX)
}

pub fn rune_domain(rune: &CardInfo) -> Option<Domain> {
    rune.domain
        .first()
        .and_then(|domain| Domain::parse(domain))
        .or_else(|| rune.name.strip_suffix(RUNE_SUFFIX).and_then(Domain::parse))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Source;
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::{ChainItem, Expiry, ItemKind, Mode, Origin};
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::table::CounterInfo;

    #[test]
    fn zones_resolve_by_name_and_battlefields_come_from_the_cards_with_a_fallback() {
        let table = fixtures::table();
        let zones = Zones::of(&table);
        assert_eq!(zones.hand, Some(fixtures::HAND));
        assert_eq!(zones.base, Some(fixtures::BASE));
        assert_eq!(zones.chain, Some(fixtures::CHAIN));
        assert_eq!(zones.battlefields, [fixtures::BF1, fixtures::BF2]);
        let mut bare = table.clone();
        bare.cards.retain(|card| !card.is_kind(KIND_BATTLEFIELD));
        assert_eq!(
            Zones::of(&bare).battlefields,
            [fixtures::BF1, fixtures::BF2]
        );
        bare.options = vec![("battlefields".into(), 3)];
        assert_eq!(
            Zones::of(&bare).battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3]
        );
        bare.options = vec![("battlefields".into(), 1)];
        assert_eq!(Zones::of(&bare).battlefields, [fixtures::BF1]);
        bare.options = vec![("battlefields".into(), 0)];
        bare.players = 3;
        assert_eq!(
            Zones::of(&bare).battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3],
            "an unusable option falls back to one per seat"
        );
        assert_eq!(
            Location::of_zone(fixtures::BASE, 1, &zones),
            Some(Location::Base(1))
        );
        assert_eq!(
            Location::of_zone(fixtures::BF2, 0, &zones),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(Location::of_zone(fixtures::HAND, 0, &zones), None);
        assert!(zones.is_deck(fixtures::MAIN_DECK));
    }

    #[test]
    fn damage_is_marked_by_the_controller_of_its_cause_and_a_heal_clears_the_marks() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert!(ctx.damage(fixtures::VI, 1, Cause::Rule));
        assert!(ctx.damage_marks(fixtures::VI).is_empty());
        ctx.blob.chain.push(ChainItem::new(
            3,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        ));
        assert_eq!(ctx.marker_of(fixtures::VI, Cause::Item(3)), Some(1));
        assert_eq!(ctx.marker_of(fixtures::VI, Cause::Item(99)), None);
        assert_eq!(ctx.marker_of(fixtures::VI, Cause::Combat), None);
        assert_eq!(
            ctx.marker_of(
                fixtures::VI,
                Cause::Ability(Source {
                    card: fixtures::THEIR_UNIT,
                    ability: 0,
                })
            ),
            Some(1)
        );
        ctx.blob.showdown = Some(crate::state::Showdown::open(fixtures::BF1, 0, 1));
        assert_eq!(ctx.marker_of(fixtures::VI, Cause::Combat), Some(1));
        assert_eq!(ctx.marker_of(fixtures::THEIR_UNIT, Cause::Combat), Some(0));
        ctx.blob.showdown = None;
        assert!(ctx.damage(fixtures::VI, 2, Cause::Item(3)));
        assert!(ctx.damage_by(fixtures::VI, 1, Cause::Item(99), Some(0)));
        assert!(ctx.damage(
            fixtures::VI,
            1,
            Cause::Ability(Source {
                card: fixtures::THEIR_UNIT,
                ability: 0,
            })
        ));
        assert_eq!(ctx.damage_marks(fixtures::VI), [(0, 1), (1, 3)]);
        assert_eq!(ctx.damage_on(fixtures::VI), 5);
        assert!(ctx.heal(fixtures::VI));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.damage_marks(fixtures::VI).is_empty());
        assert!(!ctx.heal(fixtures::VI));
        assert!(!ctx.lethal_damage_marked(fixtures::VI));
        assert!(!ctx.any_damage_is_lethal(0, fixtures::THEIR_UNIT));
    }

    #[test]
    fn a_damage_multiplier_stacks_after_prevention_and_lapses_with_the_turn() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.damage_multiplier(fixtures::VI), 1);
        assert!(!ctx.multiply_damage_this_turn(fixtures::VI, 1));
        assert!(!ctx.multiply_damage_this_turn(fixtures::HAND_UNIT, 2));
        assert!(!ctx.multiply_damage_this_turn(fixtures::GROUNDS, 2));
        assert!(ctx.multiply_damage_this_turn(fixtures::VI, 2));
        assert!(ctx.multiply_damage_this_turn(fixtures::VI, 2));
        assert_eq!(ctx.damage_multiplier(fixtures::VI), 4);
        ctx.blob
            .card_state_mut(fixtures::VI)
            .damage_multiplier_this_turn = 2;
        ctx.prevent_on(
            fixtures::VI,
            DamageSource::Any,
            Amount::N(1),
            Expiry::EndOfTurn(1),
        );
        assert!(ctx.damage(fixtures::VI, 2, Cause::Rule));
        assert_eq!(ctx.damage_on(fixtures::VI), 2);
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::DamageDealt { card, n: 2, .. } if *card == fixtures::VI)
        ));
        assert!(ctx.damage(fixtures::VI, 200, Cause::Rule));
        assert_eq!(ctx.damage_on(fixtures::VI), 2 + i32::from(u8::MAX));
        ctx.expire(Expiry::CombatEnd);
        assert_eq!(ctx.damage_multiplier(fixtures::VI), 2);
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.damage_multiplier(fixtures::VI), 1);
        assert!(ctx.blob.card_state(fixtures::VI).unwrap().is_default());
    }

    #[test]
    fn a_spawned_token_carries_its_declared_body_its_owner_and_its_mirrors() {
        assert_eq!(Token::Sprite.face().name, TOKEN_SPRITE);
        assert_eq!(Token::Sprite.face().kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(Token::Sprite.face().might, Some(3));
        assert_eq!(Token::Gold.face().name, TOKEN_GOLD);
        assert_eq!(Token::Gold.face().kind.as_deref(), Some(KIND_GEAR));
        assert_eq!(Token::Gold.face().might, None);
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        let id = ctx.table.next_id;
        let sprite = ctx
            .spawn(1, Token::Sprite, Location::Base(1), false)
            .unwrap();
        assert_eq!(sprite, id);
        assert_eq!(
            ctx.effects,
            [
                Effect::Spawn {
                    face: Token::Sprite.face(),
                    zone: fixtures::BASE,
                    seat: 1,
                    owner: Some(1)
                },
                Effect::Counter {
                    target: Target::Card(sprite),
                    counter: COUNTER_TEMPORARY,
                    delta: 1
                },
                Effect::exhaust(sprite),
            ]
        );
        assert_eq!(ctx.controller(sprite), 1);
        assert!(ctx.is_temporary(sprite));
        let gold = ctx.spawn(0, Token::Gold, Location::Base(0), true).unwrap();
        assert!(ctx.is_gear(gold) && !ctx.is_temporary(gold));
        assert!(!ctx.card(gold).unwrap().exhausted);
    }

    #[test]
    fn a_buff_is_a_mirrored_counter_that_reads_into_might_and_leaves_with_the_card() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.buff(fixtures::VI));
        assert!(!ctx.buff(fixtures::VI), "the buff is idempotent");
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            ctx.effects,
            [Effect::Counter {
                target: Target::Card(fixtures::VI),
                counter: COUNTER_BUFFED,
                delta: 1
            }]
        );
        assert!(ctx.empower(fixtures::VI));
        assert!(ctx.is_empowered(fixtures::VI));
        assert!(ctx.disempower(fixtures::VI));
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(ctx.bounce(fixtures::VI));
        assert!(
            !ctx.is_buffed(fixtures::VI),
            "the mirror sheds with the card"
        );
        assert_eq!(ctx.printed_might(fixtures::VI), 3);
        assert!(!ctx.buff(fixtures::VI), "a card in hand cannot be buffed");
    }

    #[test]
    fn the_entry_is_applied_before_rules_run_and_primitives_mirror_their_effects() {
        let mut fixture = Fixture::enforced();
        let action = fixtures::move_action(fixtures::VI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.entry.unwrap().from, Some(fixtures::BASE));
        assert!(ctx.fault.is_none());
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [fixtures::VI]
        );
        assert_eq!(ctx.seats_with_units(fixtures::BF2), [1]);
        assert!(ctx.exhaust(fixtures::VI));
        assert!(!ctx.exhaust(fixtures::VI));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.ready(fixtures::VI));
        assert_eq!(
            ctx.events,
            [Event::Readied {
                card: fixtures::VI,
                by: 0
            }]
        );
        assert!(!ctx.awaken(fixtures::VI, 0));
        assert_eq!(ctx.draw(0, 2), 2);
        assert_eq!(ctx.hand_of(0).len(), 6);
        assert_eq!(ctx.blob.seat(0).draws, 2);
        assert_eq!(ctx.channel(0, 1), 1);
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(ctx.table.held(fixtures::RUNE_POOL, 0).count(), 5);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(fixtures::VI),
                Effect::ready(fixtures::VI),
                Effect::Move {
                    card: 23,
                    zone: fixtures::HAND,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 22,
                    zone: fixtures::HAND,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 32,
                    zone: fixtures::RUNE_POOL,
                    seat: 0,
                    index: TOP
                },
            ]
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.printed_might(fixtures::SPRITE), 3);
        assert!(ctx.is_temporary(fixtures::SPRITE));
        assert!(ctx.is_token(fixtures::SPRITE));
        assert!(!ctx.is_temporary(fixtures::VI));
        assert!(ctx.at_battlefield(fixtures::VI));
        assert!(ctx.alone_at(fixtures::VI));
        assert!(ctx.script(fixtures::VI).is_some());
        assert_eq!(ctx.domains_of(fixtures::VI), [Domain::Fury]);
        assert!(!ctx.units_played_here(fixtures::BF2));
        assert!(ctx.units_played_here(fixtures::BF1));
    }

    #[test]
    fn moves_mark_contest_and_the_cap_recalls_kills_trash_or_despawn_and_spawns_own_tokens() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 2, "Jinx", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(91, fixtures::BF1, 3, "Vex", 2));
        fixture.table.players = 4;
        fixture.blob = GameBlob::start(4, 0, Mode::Enforced);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Recalled
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.effects.is_empty());
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF2),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(
            ctx.events.last(),
            Some(&Event::Moved {
                card: fixtures::VI,
                from: Some(Location::Base(0)),
                to: Location::Battlefield(fixtures::BF2),
                cause: MoveCause::Effect,
                by: Some(0)
            })
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF2),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.effects.len(), 1);
        assert_eq!(
            ctx.move_unit(fixtures::GROUNDS, Location::Base(0), MoveCause::Effect),
            Moved::NotAUnit
        );
        ctx.recall(fixtures::VI, true);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::NotOnBoard);
        assert_eq!(ctx.kill(fixtures::SPRITE, Cause::Rule), Killed::Yes);
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert_eq!(
            ctx.kill(fixtures::HAND_UNIT, Cause::Rule),
            Killed::NotOnBoard
        );
        let next = ctx.table.next_id;
        let sprite = ctx
            .spawn(1, Token::Sprite, Location::Base(1), false)
            .unwrap();
        assert_eq!(sprite, next);
        assert!(ctx.is_token(sprite));
        assert!(ctx.is_temporary(sprite));
        assert_eq!(ctx.controller(sprite), 1);
        assert!(ctx.card(sprite).unwrap().exhausted);
        assert_eq!(ctx.location(sprite), Some(Location::Base(1)));
        let gold = ctx.spawn(0, Token::Gold, Location::Base(0), true).unwrap();
        assert!(ctx.is_gear(gold));
        assert!(!ctx.card(gold).unwrap().exhausted);
        assert!(matches!(
            ctx.effects.last(),
            Some(Effect::Spawn { owner: Some(0), .. })
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn effect_move_records_the_resolving_item_controller() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        ctx.resolving = Some(ChainItem::new(
            9,
            ItemKind::Ability {
                source: fixtures::THEIR_UNIT,
                index: 0,
            },
            1,
            Origin::Board,
        ));
        ctx.arrived(
            fixtures::VI,
            Some(Location::Base(0)),
            Location::Battlefield(fixtures::BF2),
            MoveCause::Effect,
        );
        assert!(matches!(
            ctx.events.last(),
            Some(Event::Moved {
                by: Some(1),
                cause: MoveCause::Effect,
                ..
            })
        ));
    }

    #[test]
    fn might_mods_expire_in_reverse_and_scoring_follows_the_final_point_rule() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        ctx.might(fixtures::VI, 2, Expiry::EndOfTurn(1), None, 1);
        ctx.might(fixtures::VI, -4, Expiry::EndOfTurn(1), Some(1), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 1);
        ctx.might(fixtures::VI, 1, Expiry::Permanent, None, 3);
        assert_eq!(ctx.current_might(fixtures::VI), 2);
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            ctx.effects,
            [
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: COUNTER_MIGHT,
                    delta: 2
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: COUNTER_MIGHT,
                    delta: -4
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: COUNTER_MIGHT,
                    delta: 1
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: COUNTER_MIGHT,
                    delta: 2
                },
            ]
        );
        ctx.stun(fixtures::VI);
        ctx.stun(fixtures::VI);
        assert!(ctx.has_flag(fixtures::VI, FLAG_STUNNED));
        assert_eq!(
            ctx.card(fixtures::VI)
                .unwrap()
                .annotation(ANNOTATION_STUNNED),
            Some(&[1u8][..])
        );
        ctx.unstun(fixtures::VI);
        assert!(!ctx.has_flag(fixtures::VI, FLAG_STUNNED));
        assert_eq!(ctx.effects.len(), 6);
        ctx.emit(Effect::Counter {
            target: Target::Card(fixtures::VI),
            counter: COUNTER_DAMAGE,
            delta: 2,
        });
        assert_eq!(ctx.damage_on(fixtures::VI), 2);
        ctx.heal_all();
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.score(1, false));
        assert_eq!(ctx.points(1), 1);
        let mut ending = Fixture::enforced();
        ending.set_points(1, DEFAULT_VICTORY_SCORE - 1);
        let mut ctx = ending.ctx();
        assert!(!ctx.score(1, false));
        assert!(
            matches!(ctx.effects.last(), Some(Effect::Move { zone, .. }) if *zone == fixtures::HAND)
        );
        assert!(ctx.score(1, true));
        assert_eq!(ctx.winner(), Some(1));
        ctx.blob.mark_scored(fixtures::BF1, 0);
        assert!(!ctx.scored_every_battlefield(0));
        ctx.blob.mark_scored(fixtures::BF2, 0);
        assert!(ctx.scored_every_battlefield(0));
        ctx.recycle_to_bottom(fixtures::HAND_UNIT);
        ctx.recycle_to_bottom(fixtures::RUNE_A);
        assert!(
            matches!(ctx.effects[ctx.effects.len() - 2], Effect::Move { zone, index, .. } if zone == fixtures::MAIN_DECK && index == BOTTOM)
        );
        assert!(
            matches!(ctx.effects[ctx.effects.len() - 1], Effect::Move { zone, index, .. } if zone == fixtures::RUNE_DECK && index == BOTTOM)
        );
    }

    #[test]
    fn a_bad_effect_is_recorded_as_a_fault_instead_of_a_panic() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        ctx.emit(Effect::Despawn { card: 424242 });
        assert_eq!(ctx.fault, Some((0, ApplyError::UnknownCard)));
        assert_eq!(
            rune_domain(&fixtures::rune(1, 0, "Mind", false)),
            Some(Domain::Mind)
        );
        let mut nameless = fixtures::rune(2, 0, "Chaos", false);
        nameless.domain.clear();
        assert_eq!(rune_domain(&nameless), Some(Domain::Chaos));
        assert_eq!(Token::Sprite.face().might, Some(3));
        assert_eq!(Token::Gold.face().kind.as_deref(), Some(KIND_GEAR));
    }

    #[test]
    fn the_three_new_tokens_carry_their_bodies_and_spawn_as_units() {
        assert_eq!(Token::SandSoldier.face().name, TOKEN_SAND_SOLDIER);
        assert_eq!(Token::SandSoldier.face().might, Some(2));
        assert_eq!(Token::ShadowClone.face().name, TOKEN_SHADOW_CLONE);
        assert_eq!(Token::ShadowClone.face().might, Some(0));
        assert_eq!(Token::Tentacle.face().name, TOKEN_TENTACLE);
        assert_eq!(Token::Tentacle.face().might, Some(1));
        for token in [Token::SandSoldier, Token::ShadowClone, Token::Tentacle] {
            assert_eq!(token.face().kind.as_deref(), Some(KIND_UNIT));
            assert_eq!(token.kind(), KIND_UNIT);
        }
        assert_eq!(Token::Gold.kind(), KIND_GEAR);
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        let tentacle = ctx
            .spawn(1, Token::Tentacle, Location::Base(1), false)
            .unwrap();
        assert!(ctx.is_unit(tentacle));
        assert!(ctx.is_token(tentacle));
        assert!(!ctx.is_temporary(tentacle));
        assert_eq!(ctx.current_might(tentacle), 1);
        assert!(matches!(
            ctx.events.iter().rev().nth(1),
            Some(Event::Played { card, controller: 1, kind, .. }) if *card == tentacle && kind == KIND_UNIT
        ));
        assert_eq!(
            ctx.events.last(),
            Some(&Event::Entered {
                card: tentacle,
                at: Location::Base(1)
            })
        );
        assert!(ctx
            .script(tentacle)
            .is_some_and(|script| script.name == TOKEN_TENTACLE));
    }

    #[test]
    fn banish_moves_a_card_face_up_to_its_owners_pile_and_despawns_a_token() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::gear(90, fixtures::BASE, 0, "Brutalizer", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.zones.banishment, Some(fixtures::BANISHMENT));
        assert_eq!(
            crate::engine::attach::attach(&mut ctx, 90, fixtures::VI),
            crate::engine::attach::Attached::Yes
        );
        ctx.buff(fixtures::VI);
        ctx.stun(fixtures::VI);
        ctx.mark_attacker(fixtures::VI);
        let events = ctx.events.len();
        assert!(ctx.banish(fixtures::VI));
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::BANISHMENT)
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().seat, 0);
        assert!(ctx.in_banishment(fixtures::VI));
        assert_eq!(ctx.banished_of(0), [fixtures::VI]);
        assert!(
            !crate::engine::attach::is_attached(&ctx, 90),
            "719.5 · the gear is detached"
        );
        assert_eq!(ctx.state_of(fixtures::VI), None, "its state row is dropped");
        assert!(!ctx.is_buffed(fixtures::VI), "counters are shed on entry");
        assert!(
            ctx.deaths.is_empty()
                && !ctx
                    .events
                    .iter()
                    .any(|event| matches!(event, Event::Died { .. })),
            "427.2.a · a banish is not a kill"
        );
        assert_eq!(
            &ctx.events[events..],
            [Event::Banished {
                card: fixtures::VI,
                owner: 0,
                by: 0,
                token: false
            }]
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 50} is banished");
        assert!(ctx.banish(fixtures::SPRITE));
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "186.1 · a token ceases to exist"
        );
        assert_eq!(
            ctx.events.last(),
            Some(&Event::Banished {
                card: fixtures::SPRITE,
                owner: 1,
                by: 1,
                token: true
            }),
            "185 · the event says it was a token, not a card"
        );
        assert!(!ctx.banish(999));
        assert!(!ctx.in_trash(fixtures::VI));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn burn_moves_the_top_cards_to_the_trash_and_raises_one_event_each() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.burn(0, 1), 1);
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.events, [Event::Burned { seat: 0, card: 23 }]);
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} burns 1");
        assert!(ctx.in_trash(23));
        assert_eq!(ctx.trash_of(0), [23]);
        assert_eq!(ctx.burn(1, 5), 2, "a short deck burns what it has");
        assert_eq!(ctx.burn(1, 1), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empower_raises_one_event_per_real_flip_and_disempower_none_when_never_set() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert!(!ctx.disempower(fixtures::VI));
        assert!(ctx.events.is_empty());
        assert!(ctx.empower(fixtures::VI));
        assert!(!ctx.empower(fixtures::VI));
        assert_eq!(
            ctx.events,
            [Event::Empowered {
                card: fixtures::VI,
                by: 0
            }]
        );
        assert!(ctx.disempower(fixtures::VI));
        assert_eq!(
            ctx.events,
            [
                Event::Empowered {
                    card: fixtures::VI,
                    by: 0
                },
                Event::Disempowered { card: fixtures::VI }
            ]
        );
        assert!(ctx.empower_by(fixtures::THEIR_UNIT, 0));
        assert_eq!(
            ctx.events.last(),
            Some(&Event::Empowered {
                card: fixtures::THEIR_UNIT,
                by: 0
            }),
            "the empowering item's controller, not the card's"
        );
        assert!(ctx.disempower(fixtures::THEIR_UNIT));
        assert!(ctx.empower(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.events.last(),
            Some(&Event::Empowered {
                card: fixtures::THEIR_UNIT,
                by: 1
            }),
            "a rule empower is the card's controller's"
        );
    }

    #[test]
    fn empower_lands_on_a_legend_in_its_zone_and_not_in_hand_or_trash() {
        let mut fixture = Fixture::enforced();
        {
            let mut ctx = fixture.ctx();
            assert!(!ctx.on_board(fixtures::LEGEND_CARD));
            assert!(ctx.in_play(fixtures::LEGEND_CARD));
            assert!(ctx.empower(fixtures::LEGEND_CARD));
            assert!(ctx.is_empowered(fixtures::LEGEND_CARD));
            assert_eq!(
                ctx.events,
                [Event::Empowered {
                    card: fixtures::LEGEND_CARD,
                    by: 0
                }]
            );
            assert!(ctx.disempower(fixtures::LEGEND_CARD));
            assert!(!ctx.is_empowered(fixtures::LEGEND_CARD));
            assert!(!ctx.in_play(fixtures::HAND_UNIT));
            assert!(!ctx.empower(fixtures::HAND_UNIT));
            assert!(!ctx.is_empowered(fixtures::HAND_UNIT));
            assert_eq!(ctx.events.len(), 2);
        }
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().zone = Some(fixtures::HAND);
        {
            let mut ctx = fixture.ctx();
            assert!(!ctx.in_play(fixtures::LEGEND_CARD));
            assert!(!ctx.empower(fixtures::LEGEND_CARD));
        }
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().zone = Some(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert!(!ctx.in_play(fixtures::LEGEND_CARD));
        assert!(!ctx.empower(fixtures::LEGEND_CARD));
    }

    #[test]
    fn an_empowered_legend_cannot_be_disempowered_off_zone_or_without_a_legend_zone() {
        let mut fixture = Fixture::enforced();
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::LEGEND_CARD),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().zone = Some(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(fixtures::LEGEND_CARD));
        assert!(!ctx.in_play(fixtures::LEGEND_CARD));
        assert!(!ctx.disempower(fixtures::LEGEND_CARD));
        assert!(ctx.is_empowered(fixtures::LEGEND_CARD));
        drop(ctx);
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().zone = Some(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(fixtures::LEGEND_CARD));
        assert!(!ctx.in_play(fixtures::LEGEND_CARD));
        assert!(!ctx.disempower(fixtures::LEGEND_CARD));
        assert!(ctx.is_empowered(fixtures::LEGEND_CARD));

        let mut missing = Fixture::enforced();
        missing.table.card_mut(fixtures::LEGEND_CARD).unwrap().zone = None;
        missing
            .table
            .zones
            .retain(|zone| zone.id != fixtures::LEGEND);
        let mut ctx = missing.ctx();
        assert!(!ctx.in_play(fixtures::LEGEND_CARD));
        assert!(!ctx.empower(fixtures::LEGEND_CARD));
    }

    #[test]
    fn xp_is_the_seat_counter_spent_only_while_the_seat_has_that_much() {
        let mut fixture = Fixture::enforced();
        fixture.set_xp(0, 2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(ctx.xp(1), 0);
        assert!(!ctx.spend_xp(0, 3));
        assert!(!ctx.spend_xp(1, 1));
        assert!(ctx.effects.is_empty());
        assert!(ctx.spend_xp(0, 2));
        assert_eq!(ctx.xp(0), 0);
        assert_eq!(
            ctx.effects,
            [Effect::score(0, crate::rules::COUNTER_XP, -2)]
        );
        ctx.score_xp(1, 3);
        assert_eq!(ctx.xp(1), 3);
        ctx.score_effect(1);
        assert_eq!(ctx.points(1), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_shrouded_unit_is_not_a_candidate_for_the_enemy_until_the_ending_step() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert!(ctx.shroud(fixtures::VI));
        assert!(!ctx.shroud(fixtures::VI));
        assert!(ctx.is_shrouded(fixtures::VI));
        let theirs = ChainItem::new(
            3,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        let mine = ChainItem::new(
            4,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let spec = crate::cards::prelude::a_unit("a unit");
        let cards =
            |ctx: &Ctx, item: &ChainItem| crate::engine::targets::candidates(ctx, item, &spec);
        assert!(!cards(&ctx, &theirs).contains(&crate::state::TargetRef::Card(fixtures::VI)));
        assert!(cards(&ctx, &mine).contains(&crate::state::TargetRef::Card(fixtures::VI)));
        crate::engine::expiry::clear_stuns(&mut ctx);
        assert!(!ctx.is_shrouded(fixtures::VI));
        assert!(cards(&ctx, &theirs).contains(&crate::state::TargetRef::Card(fixtures::VI)));
    }

    #[test]
    fn await_faces_reveals_the_cards_and_parks_the_item_without_a_prompt() {
        static PEEKER: Card = crate::cards::prelude::spell(
            "Peeker",
            &[],
            &[crate::cards::prelude::play(&[], |ctx, item, stage| {
                if stage.0 == 0 {
                    let cards = ctx.hand_of(1);
                    return crate::cards::Flow::Ask(ctx.await_faces(item, &cards, 1));
                }
                crate::cards::Flow::Done
            })],
        );
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &PEEKER);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the spell parks");
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, crate::state::ItemStatus::Resolving);
        assert_eq!(parked.stage, 1);
        assert_eq!(parked.awaiting, [fixtures::THEIR_HAND_CARD]);
        assert!(
            ctx.blob.prompt.is_none(),
            "no prompt: the host's Reveal is awaited"
        );
        assert!(ctx.effects.contains(&Effect::Reveal {
            card: fixtures::THEIR_HAND_CARD
        }));
        assert!(ctx.awaiting.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn contest_marks_a_battlefield_once_and_never_for_its_holder() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert!(
            !ctx.contest(fixtures::BF2, 1),
            "the holder never contests its own"
        );
        assert!(ctx.contest(fixtures::BF2, 0));
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert!(!ctx.contest(fixtures::BF2, 0), "marked once");
        assert!(ctx.events.is_empty(), "no Moved is raised by a contest");
    }

    #[test]
    fn ambush_locations_are_the_battlefields_with_friendly_units_or_enemies_for_rengar() {
        static AMBUSHER: Card = crate::cards::prelude::unit("Ambusher", &[Keyword::Ambush], &[]);
        static RENGAR: Card = crate::cards::prelude::with_statics(
            crate::cards::prelude::unit("Rengar", &[Keyword::Ambush], &[]),
            &[Static::AmbushIntoEnemies],
        );
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_UNIT, &AMBUSHER)
            .with_script(fixtures::CHAMPION_CARD, &RENGAR);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, fixtures::HAND_UNIT),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            ctx.ambush_locations(0, fixtures::CHAMPION_CARD),
            [Location::Battlefield(fixtures::BF1)],
            "054.1 · Rockfall Path at BF2 forbids even Rengar"
        );
        assert_eq!(
            ctx.ambush_locations(1, fixtures::CHAMPION_CARD),
            [Location::Battlefield(fixtures::BF1)],
            "the enemy-held battlefield with only enemy units"
        );
        assert!(ctx.ambush_locations(1, fixtures::HAND_UNIT).is_empty());
        assert!(
            ctx.ambush_locations(0, fixtures::HAND_SPELL).is_empty(),
            "no Ambush, no locations"
        );
    }

    #[test]
    fn channel_exhausted_and_queue_turn_and_prevent_write_what_they_say() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.channel_exhausted(0, 1), 1);
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the new rune enters exhausted"
        );
        assert!(ctx.card(32).unwrap().exhausted);
        ctx.queue_turn(0);
        assert_eq!(ctx.blob.extra_turns, [0]);
        assert!(ctx.events.contains(&Event::TurnQueued { seat: 0 }));
        ctx.queue_turn(1);
        assert_eq!(
            ctx.blob.extra_turns,
            [1, 0],
            "738 · a later additional turn slots in right after the current one"
        );
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(1),
        );
        assert_eq!(ctx.blob.preventions.len(), 1);
        assert!(GameBlob::decode(&ctx.blob.encode()).is_some());
    }

    #[test]
    fn a_control_change_is_read_before_the_owner_and_moves_the_card_home_to_the_thief() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::gear(90, fixtures::BASE, 1, "Trinket", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(90), 1);
        assert!(ctx.set_controller(90, 0, fixtures::VI));
        assert_eq!(ctx.controller(90), 0);
        assert_eq!(ctx.owner(90), 1);
        assert_eq!(ctx.location(90), Some(Location::Base(0)));
        let row = ctx.state_of(90).unwrap();
        assert_eq!(row.controlled_by, Some(0));
        assert_eq!(row.control_source, Some(fixtures::VI));
        assert!(!row.is_default());
        assert!(!ctx.set_controller(90, 9, fixtures::VI));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_control_change_in_place_leaves_a_unit_at_its_battlefield_and_contests_it_for_the_thief() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture
            .table
            .cards
            .push(fixtures::gear(90, fixtures::BASE, 1, "Trinket", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller_in_place(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { card, .. } if *card == fixtures::THEIR_UNIT
            )),
            "taken where it stands"
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(ctx.blob.holder(fixtures::BF2), Some(1));
        assert!(
            ctx.events.is_empty(),
            "190.3.a · a control change is not a move"
        );
        assert!(ctx.set_controller_in_place(90, 0, fixtures::VI));
        assert_eq!(
            ctx.location(90),
            Some(Location::Base(0)),
            "off a battlefield in place means the thief's base"
        );
        drop(ctx);

        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller_in_place(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            None,
            "the holder never contests its own battlefield"
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn heal_zeroes_one_unit_and_combat_damage_is_suppressed_by_a_static() {
        static VILE: Card = crate::cards::prelude::with_statics(
            crate::cards::prelude::unit("Vile", &[], &[]),
            &[Static::NoCombatDamageFrom(|ctx, source, unit| {
                ctx.controller(unit) != ctx.controller(source)
                    && ctx.current_might(unit) < ctx.current_might(source)
            })],
        );
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 1, "Vile", 8));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(90, &VILE);
        let mut ctx = fixture.ctx();
        ctx.emit(Effect::Counter {
            target: Target::Card(fixtures::VI),
            counter: COUNTER_DAMAGE,
            delta: 2,
        });
        assert!(ctx.heal(fixtures::VI));
        assert!(!ctx.heal(fixtures::VI));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(
            !ctx.deals_combat_damage(fixtures::VI),
            "3 Might beside an 8-Might Vile"
        );
        assert_eq!(ctx.combat_might(fixtures::VI), 0);
        assert!(ctx.deals_combat_damage(90));
        assert!(
            ctx.deals_combat_damage(fixtures::THEIR_UNIT),
            "Vile's own side is untouched"
        );
        ctx.might(fixtures::VI, 5, Expiry::Permanent, None, 0);
        assert!(
            ctx.deals_combat_damage(fixtures::VI),
            "8 is not less than 8"
        );
        ctx.stun(fixtures::VI);
        assert!(!ctx.deals_combat_damage(fixtures::VI));
    }

    #[test]
    fn grants_refuse_unread_costed_keywords_and_a_swept_card_unstuns_without_an_effect() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        let equip = Keyword::Equip(cards::Cost {
            energy: 1,
            power: &[],
        });
        assert!(!ctx.grant(fixtures::VI, equip, Expiry::Permanent));
        assert!(!ctx.grant(
            fixtures::VI,
            Keyword::Empower(cards::Cost::FREE),
            Expiry::Permanent
        ));
        let repeat = Keyword::Repeat(cards::Cost {
            energy: 2,
            power: &[cards::Power::Domain(cards::Domain::Chaos)],
        });
        assert!(ctx.grant(fixtures::HAND_SPELL, repeat, Expiry::Permanent));
        assert!(ctx.has_keyword(fixtures::HAND_SPELL, Keyword::Repeat(cards::Cost::FREE)));
        assert_eq!(
            ctx.keyword_instances(fixtures::HAND_SPELL, Keyword::Repeat(cards::Cost::FREE)),
            1
        );
        assert_eq!(
            ctx.granted_cost(fixtures::HAND_SPELL, Keyword::Repeat(cards::Cost::FREE)),
            Some(crate::engine::cost::Cost {
                energy: 2,
                power: vec![crate::engine::cost::Need::Domain(cards::Domain::Chaos)],
                ..crate::engine::cost::Cost::default()
            })
        );
        assert_eq!(
            ctx.granted_cost(fixtures::HAND_SPELL, Keyword::Flow(cards::Cost::FREE)),
            None
        );
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(ctx.grant(fixtures::VI, Keyword::Ganking, Expiry::EndOfTurn(1)));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(ctx.grant_keyword_this_turn(fixtures::HAND_SPELL, Keyword::Flow(cards::Cost::FREE)));
        assert_eq!(
            ctx.flow_of(fixtures::HAND_SPELL),
            Some(crate::engine::cost::Cost::free())
        );
        let decoded = GameBlob::decode(&ctx.blob.encode()).unwrap();
        assert_eq!(
            decoded
                .card_state(fixtures::HAND_SPELL)
                .unwrap()
                .granted_costed,
            ctx.blob
                .card_state(fixtures::HAND_SPELL)
                .unwrap()
                .granted_costed,
            "a granted Flow or Repeat rides the wire with its cost"
        );
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(ctx.flow_of(fixtures::HAND_SPELL), None, "this turn only");
        assert!(
            ctx.has_keyword(fixtures::HAND_SPELL, Keyword::Repeat(cards::Cost::FREE)),
            "the permanent grant stays"
        );
        ctx.blob.set_flag(999, FLAG_STUNNED, true);
        ctx.unstun(999);
        assert!(!ctx.has_flag(999, FLAG_STUNNED));
        assert!(ctx.effects.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_battlefield_token_is_added_to_a_spare_shared_zone_that_the_next_ctx_reads_as_a_battlefield(
    ) {
        assert_eq!(Token::Brush.face().kind.as_deref(), Some(KIND_BATTLEFIELD));
        assert_eq!(
            Token::BaronPit.face().kind.as_deref(),
            Some(KIND_BATTLEFIELD)
        );
        assert_eq!(Token::Brush.kind(), KIND_BATTLEFIELD);
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.zones.spare_battlefield(&ctx.table), Some(fixtures::BF3));
        assert_eq!(
            ctx.add_battlefield_token(1, Token::Sprite),
            None,
            "only a battlefield face stands in a battlefield zone"
        );
        let pit = ctx.add_battlefield_token(1, Token::BaronPit).unwrap();
        assert_eq!(pit, fixtures::BF3);
        assert_eq!(
            ctx.zones.battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3]
        );
        assert_eq!(ctx.zones.spare_battlefield(&ctx.table), None);
        assert_eq!(ctx.add_battlefield_token(1, Token::Brush), None);
        let token = ctx.battlefield_card_at(pit).unwrap();
        assert!(ctx.is_token(token));
        assert_eq!(ctx.owner(token), 1);
        assert!(ctx.face_in_play(ctx.card(token).unwrap()));
        assert_eq!(ctx.blob.holder(pit), None);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Played { .. })));
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        assert_eq!(
            Zones::of(&table).battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3],
            "the zone list follows the battlefield faces on the table"
        );
    }

    #[test]
    fn replacing_a_battlefield_banishes_the_card_keeps_its_statuses_and_swaps_back_through_a_chain_of_tokens(
    ) {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.mark_scored(fixtures::BF1, 0);
        let mut ctx = fixture.ctx();
        ctx.set_flag(fixtures::GROUNDS, FLAG_STUNNED, true);
        assert_eq!(ctx.replace_battlefield(fixtures::BASE, Token::Brush), None);
        assert_eq!(ctx.replace_battlefield(fixtures::BF1, Token::Sprite), None);
        let first = ctx
            .replace_battlefield(fixtures::BF1, Token::Brush)
            .unwrap();
        assert!(ctx.is_token(first));
        assert!(ctx.is_battlefield_card(first));
        assert_eq!(ctx.battlefield_card_at(fixtures::BF1), Some(first));
        assert_eq!(ctx.replaced_by(first), Some(fixtures::GROUNDS));
        assert!(ctx.in_banishment(fixtures::GROUNDS));
        assert!(!ctx.on_board(fixtures::GROUNDS));
        assert_eq!(
            ctx.owner(first),
            0,
            "the token is the replaced card's owner's"
        );
        assert!(
            ctx.has_flag(first, FLAG_STUNNED),
            "438.1 · statuses carry over"
        );
        assert!(!ctx.has_flag(fixtures::GROUNDS, FLAG_STUNNED));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.blob.scored(fixtures::BF1, 0));
        assert!(ctx.zones.is_battlefield(fixtures::BF1));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Banished { .. })));
        let second = ctx
            .replace_battlefield(fixtures::BF1, Token::Brush)
            .unwrap();
        assert!(
            ctx.card(first).is_none(),
            "438.6 · a replaced token stops existing"
        );
        assert_eq!(
            ctx.replaced_by(second),
            Some(fixtures::GROUNDS),
            "438.7.b.1 · the card the earlier token replaced is what swaps back"
        );
        assert_eq!(
            ctx.swap_back(fixtures::BF2),
            None,
            "Rockfall Path replaced nothing"
        );
        assert_eq!(ctx.swap_back(fixtures::BF1), Some(fixtures::GROUNDS));
        assert!(ctx.card(second).is_none());
        assert_eq!(
            ctx.battlefield_card_at(fixtures::BF1),
            Some(fixtures::GROUNDS)
        );
        assert!(ctx.on_board(fixtures::GROUNDS));
        assert!(
            ctx.has_flag(fixtures::GROUNDS, FLAG_STUNNED),
            "438.7.b · and back"
        );
        assert_eq!(ctx.swap_back(fixtures::BF1), None);
        assert!(ctx.fault.is_none());
    }
}

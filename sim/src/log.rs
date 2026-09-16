use crate::abi::{decode, encode};
use crate::wire::{
    sheds_state, zone_kind, zone_owner, zone_visibility, CounterDecl, CounterTarget, CounterValue,
    ZoneDecl, ZoneKind, ZoneOwner, ZoneVisibility,
};
use agni_core::{Card, CardFace, CardId, Intent, PlayerId, Table, Zone};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableConfig {
    pub engine: Option<String>,
    pub plugin: Option<String>,
    pub zones: Vec<ZoneDecl>,
    pub options: Option<ByteBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<CounterDecl>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub despawn_any: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogAction {
    Genesis {
        name: String,
        config: TableConfig,
    },
    Join {
        name: String,
    },
    Deal {
        cards: Vec<u32>,
        to: Zone,
    },
    Move {
        card: u32,
        to: Zone,
        seat: u8,
        index: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        hidden: bool,
    },
    Reveal {
        card: u32,
        face: CardFace,
    },
    Annotate {
        card: u32,
        key: String,
        value: Option<ByteBuf>,
    },
    Game {
        data: ByteBuf,
    },
    Counter {
        target: CounterTarget,
        counter: u16,
        delta: i32,
    },
    Spawn {
        face: CardFace,
        to: Zone,
        seat: u8,
    },
    Reset,
    Clear {
        seat: u8,
    },
}

impl LogAction {
    pub fn genesis(name: impl Into<String>) -> Self {
        LogAction::Genesis {
            name: name.into(),
            config: TableConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub seat: u8,
    pub action: LogAction,
}

impl LogEntry {
    pub fn new(seq: u64, seat: u8, action: LogAction) -> Self {
        Self { seq, seat, action }
    }
}

pub fn encode_entry(entry: &LogEntry) -> Vec<u8> {
    encode(entry)
}

pub fn decode_entry(bytes: &[u8]) -> Option<LogEntry> {
    decode(bytes)
}

pub fn encode_log(entries: &[LogEntry]) -> Vec<u8> {
    encode(&entries)
}

pub fn decode_log(bytes: &[u8]) -> Option<Vec<LogEntry>> {
    decode(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoldError {
    BadSeq,
    MissingGenesis,
    DuplicateGenesis,
    BadZoneTable,
    NotTheHost,
    SeatTaken,
    UnknownSeat,
    UnknownCard,
    UnknownZone,
    DuplicateCard,
    ForeignHand,
    ForeignHandTarget,
    HiddenZone,
    FaceConflict,
    UnknownCounter,
    CounterScope,
    NoOp,
    Rejected { reason: Option<String> },
    BadEffect { index: usize },
}

impl FoldError {
    pub fn rejected() -> Self {
        Self::Rejected { reason: None }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Rejected { reason } => reason.as_deref(),
            _ => None,
        }
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }
}

impl fmt::Display for FoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rejected {
                reason: Some(reason),
            } => return f.write_str(reason),
            Self::BadEffect { index } => {
                return write!(
                    f,
                    "effect {index}: the game rules asked the table for something it cannot do"
                )
            }
            Self::BadSeq => "the entry is out of sequence",
            Self::MissingGenesis => "the table has no genesis yet",
            Self::DuplicateGenesis => "the table already has a genesis",
            Self::BadZoneTable => "the zone table repeats an id",
            Self::NotTheHost => "only the host may do that",
            Self::SeatTaken => "that seat is already taken",
            Self::UnknownSeat => "that seat is not at the table",
            Self::UnknownCard => "that card is not on the table",
            Self::UnknownZone => "that zone is not declared",
            Self::DuplicateCard => "that card id is already dealt",
            Self::ForeignHand => "that card is in another player's private zone",
            Self::ForeignHandTarget => "cards cannot be put into another player's private zone",
            Self::HiddenZone => "cards in a face-down zone cannot be revealed",
            Self::FaceConflict => "the card already shows a different face",
            Self::NoOp => "that changes nothing",
            Self::UnknownCounter => "no counter with that id is declared",
            Self::CounterScope => "that counter does not apply to that target",
            Self::Rejected { reason: None } => "the game rules refused it",
        })
    }
}

impl std::error::Error for FoldError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    pub seat: u8,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LogState {
    pub table: Table,
    pub seats: Vec<Seat>,
    pub revealed: BTreeSet<u32>,
    pub zones: Vec<ZoneDecl>,
    pub annotations: BTreeMap<u32, BTreeMap<String, ByteBuf>>,
    pub plugin_state: ByteBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counter_table: Vec<CounterDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<CounterValue>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tokens: BTreeSet<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub despawn_any: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub owed_reveals: BTreeSet<u32>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub peeks: BTreeSet<(u32, u8)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub shown: BTreeSet<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ByteBuf>,
    pub next_seq: u64,
}

impl LogState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn annotation(&self, card: u32, key: &str) -> Option<&[u8]> {
        self.annotations
            .get(&card)
            .and_then(|entries| entries.get(key))
            .map(|value| value.as_slice())
    }

    pub fn has_zone_table(&self) -> bool {
        !self.zones.is_empty()
    }

    pub fn counter_decl(&self, counter: u16) -> Option<&CounterDecl> {
        crate::wire::counter_decl(&self.counter_table, counter)
    }

    pub fn counter(&self, target: CounterTarget, counter: u16) -> Option<i32> {
        let decl = self.counter_decl(counter)?;
        Some(
            self.counters
                .iter()
                .find(|held| held.target == target && held.counter == counter)
                .map(|held| held.value)
                .unwrap_or(decl.start),
        )
    }

    fn set_counter(&mut self, target: CounterTarget, counter: u16, value: i32) {
        match self
            .counters
            .iter_mut()
            .find(|held| held.target == target && held.counter == counter)
        {
            Some(held) => held.value = value,
            None => {
                self.counters.push(CounterValue {
                    target,
                    counter,
                    value,
                });
                self.counters.sort();
            }
        }
    }

    pub fn seated(&self, seat: u8) -> bool {
        self.seats.iter().any(|taken| taken.seat == seat)
    }

    fn visibility(&self, zone: Zone) -> Result<ZoneVisibility, FoldError> {
        zone_visibility(&self.zones, zone).ok_or(FoldError::UnknownZone)
    }

    fn owner(&self, zone: Zone) -> Result<ZoneOwner, FoldError> {
        zone_owner(&self.zones, zone).ok_or(FoldError::UnknownZone)
    }

    fn kind(&self, zone: Zone) -> Result<ZoneKind, FoldError> {
        zone_kind(&self.zones, zone).ok_or(FoldError::UnknownZone)
    }

    fn shed(&mut self, card: u32) {
        self.annotations.remove(&card);
        self.counters
            .retain(|held| held.target != CounterTarget::Card(card));
        if self.tokens.remove(&card) {
            self.table.retain(|held| held.id.0 != card);
            self.revealed.remove(&card);
            self.forget_faces(card);
        }
    }

    fn forget_faces(&mut self, card: u32) {
        self.owed_reveals.remove(&card);
        self.peeks.retain(|(peeked, _)| *peeked != card);
        self.shown.remove(&card);
    }

    fn show_in_place(&mut self, card: u32) -> Result<(), FoldError> {
        let Some(current) = self.table.get(CardId(card)) else {
            return Ok(());
        };
        if self.visibility(current.zone)? != ZoneVisibility::All {
            self.shown.insert(card);
        }
        Ok(())
    }

    pub fn shown_in_place(&self, card: u32) -> bool {
        self.shown.contains(&card)
    }

    pub fn peeked_by(&self, card: u32, seat: u8) -> bool {
        self.peeks.contains(&(card, seat))
    }

    pub fn peeks_of(&self, seat: u8) -> impl Iterator<Item = u32> + '_ {
        self.peeks
            .iter()
            .filter(move |(_, viewer)| *viewer == seat)
            .map(|(card, _)| *card)
    }

    pub fn is_token(&self, card: u32) -> bool {
        self.tokens.contains(&card)
    }

    fn effective_seat(&self, zone: Zone, seat: u8) -> Result<u8, FoldError> {
        Ok(match self.owner(zone)? {
            ZoneOwner::PerSeat => seat,
            ZoneOwner::Shared => 0,
        })
    }

    fn guards_card(&self, card: &Card, actor: u8) -> Result<bool, FoldError> {
        let private = self.visibility(card.zone)? != ZoneVisibility::All;
        Ok(private && self.owner(card.zone)? == ZoneOwner::PerSeat && card.seat.0 != actor)
    }
}

pub fn validate(state: &LogState, entry: &LogEntry) -> Result<(), FoldError> {
    if entry.seq != state.next_seq {
        return Err(FoldError::BadSeq);
    }
    validate_action(state, entry)
}

fn validate_genesis(
    state: &LogState,
    entry: &LogEntry,
    config: &TableConfig,
) -> Result<(), FoldError> {
    if !state.seats.is_empty() {
        return Err(FoldError::DuplicateGenesis);
    }
    if entry.seat != 0 {
        return Err(FoldError::NotTheHost);
    }
    let ids: BTreeSet<u16> = config.zones.iter().map(|decl| decl.id).collect();
    if ids.len() != config.zones.len() {
        return Err(FoldError::BadZoneTable);
    }
    Ok(())
}

fn validate_action(state: &LogState, entry: &LogEntry) -> Result<(), FoldError> {
    if let LogAction::Genesis { config, .. } = &entry.action {
        return validate_genesis(state, entry, config);
    }
    if state.seats.is_empty() {
        return Err(FoldError::MissingGenesis);
    }
    match &entry.action {
        LogAction::Genesis { .. } => Err(FoldError::DuplicateGenesis),
        LogAction::Join { .. } => {
            if state.seated(entry.seat) {
                return Err(FoldError::SeatTaken);
            }
            Ok(())
        }
        LogAction::Deal { cards, to } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            state.visibility(*to)?;
            if cards.is_empty() {
                return Err(FoldError::NoOp);
            }
            let unique: BTreeSet<u32> = cards.iter().copied().collect();
            if unique.len() != cards.len() {
                return Err(FoldError::DuplicateCard);
            }
            if cards
                .iter()
                .any(|id| state.table.get(CardId(*id)).is_some())
            {
                return Err(FoldError::DuplicateCard);
            }
            Ok(())
        }
        LogAction::Move {
            card,
            to,
            seat,
            index,
            hidden,
        } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            let Some(current) = state.table.get(CardId(*card)) else {
                return Err(FoldError::UnknownCard);
            };
            if state.guards_card(current, entry.seat)? {
                return Err(FoldError::ForeignHand);
            }
            if *hidden && current.owner.0 != entry.seat && current.seat.0 != entry.seat {
                return Err(FoldError::ForeignHand);
            }
            let target_vis = state.visibility(*to)?;
            let target_seat = state.effective_seat(*to, *seat)?;
            if target_vis != ZoneVisibility::All
                && state.owner(*to)? == ZoneOwner::PerSeat
                && target_seat != entry.seat
            {
                return Err(FoldError::ForeignHandTarget);
            }
            let intent = Intent::MoveCard {
                card: CardId(*card),
                to: *to,
                seat: PlayerId(target_seat),
                index: *index as usize,
            };
            if !state.table.changes(intent) {
                return Err(FoldError::NoOp);
            }
            Ok(())
        }
        LogAction::Reveal { card, face } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            let Some(current) = state.table.get(CardId(*card)) else {
                return Err(FoldError::UnknownCard);
            };
            if state.revealed.contains(card) {
                if current.face != *face {
                    return Err(FoldError::FaceConflict);
                }
                return Ok(());
            }
            match state.visibility(current.zone)? {
                ZoneVisibility::None if !state.owed_reveals.contains(card) => {
                    Err(FoldError::HiddenZone)
                }
                ZoneVisibility::None if current.owner.0 != entry.seat => {
                    Err(FoldError::ForeignHand)
                }
                ZoneVisibility::None => Ok(()),
                ZoneVisibility::Owner => {
                    let foreign = state.owner(current.zone)? == ZoneOwner::PerSeat
                        && current.seat.0 != entry.seat
                        && current.owner.0 != entry.seat;
                    if foreign {
                        Err(FoldError::ForeignHand)
                    } else {
                        Ok(())
                    }
                }
                ZoneVisibility::All => Ok(()),
            }
        }
        LogAction::Annotate { card, key, value } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            let Some(current) = state.table.get(CardId(*card)) else {
                return Err(FoldError::UnknownCard);
            };
            if state.guards_card(current, entry.seat)? {
                return Err(FoldError::ForeignHand);
            }
            let stored = state
                .annotations
                .get(card)
                .and_then(|entries| entries.get(key));
            if stored == value.as_ref() {
                return Err(FoldError::NoOp);
            }
            Ok(())
        }
        LogAction::Game { .. } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            Ok(())
        }
        LogAction::Counter {
            target,
            counter,
            delta,
        } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            let Some(decl) = state.counter_decl(*counter) else {
                return Err(FoldError::UnknownCounter);
            };
            if !decl.accepts(target) {
                return Err(FoldError::CounterScope);
            }
            match target {
                CounterTarget::Seat(seat) => {
                    if !state.seated(*seat) {
                        return Err(FoldError::UnknownSeat);
                    }
                }
                CounterTarget::Card(card) => {
                    if state.table.get(CardId(*card)).is_none() {
                        return Err(FoldError::UnknownCard);
                    }
                }
                CounterTarget::Table => {}
            }
            let current = state.counter(*target, *counter).unwrap_or(decl.start);
            if decl.clamp(current.saturating_add(*delta)) == current {
                return Err(FoldError::NoOp);
            }
            Ok(())
        }
        LogAction::Spawn { face, to, seat } => {
            if !state.seated(entry.seat) {
                return Err(FoldError::UnknownSeat);
            }
            if face.is_hidden() {
                return Err(FoldError::NoOp);
            }
            let target_seat = state.effective_seat(*to, *seat)?;
            if state.visibility(*to)? != ZoneVisibility::All
                && state.owner(*to)? == ZoneOwner::PerSeat
                && target_seat != entry.seat
            {
                return Err(FoldError::ForeignHandTarget);
            }
            Ok(())
        }
        LogAction::Reset => {
            if entry.seat != 0 {
                return Err(FoldError::NotTheHost);
            }
            Ok(())
        }
        LogAction::Clear { seat } => {
            if !state.seated(entry.seat) || !state.seated(*seat) {
                return Err(FoldError::UnknownSeat);
            }
            if entry.seat != 0 && entry.seat != *seat {
                return Err(FoldError::ForeignHand);
            }
            if !state
                .table
                .cards()
                .iter()
                .any(|card| card.owner == PlayerId(*seat))
            {
                return Err(FoldError::NoOp);
            }
            Ok(())
        }
    }
}

fn apply_action(state: &mut LogState, entry: &LogEntry) -> Result<(), FoldError> {
    match &entry.action {
        LogAction::Genesis { name, config } => {
            state.seats.push(Seat {
                seat: entry.seat,
                name: name.clone(),
            });
            state.zones = config.zones.clone();
            state.counter_table = config.counters.clone();
            state.despawn_any = config.despawn_any;
            state.options = config.options.clone();
        }
        LogAction::Join { name } => {
            state.seats.push(Seat {
                seat: entry.seat,
                name: name.clone(),
            });
        }
        LogAction::Deal { cards, to } => {
            let seat = state.effective_seat(*to, entry.seat)?;
            for id in cards {
                state.table.insert_card(Card {
                    id: CardId(*id),
                    owner: PlayerId(entry.seat),
                    seat: PlayerId(seat),
                    zone: *to,
                    face: CardFace::hidden(),
                });
            }
        }
        LogAction::Move {
            card,
            to,
            seat,
            index,
            hidden,
        } => {
            let target_seat = state.effective_seat(*to, *seat)?;
            state.table.apply(Intent::MoveCard {
                card: CardId(*card),
                to: *to,
                seat: PlayerId(target_seat),
                index: *index as usize,
            });
            state.shown.remove(card);
            let sheds = *hidden || state.visibility(*to)? == ZoneVisibility::None;
            if sheds && state.revealed.remove(card) {
                if let Some(current) = state.table.get_mut(CardId(*card)) {
                    current.face = CardFace::hidden();
                }
            }
            if *hidden {
                state.forget_faces(*card);
            }
            if sheds_state(state.kind(*to)?) {
                state.shed(*card);
            }
        }
        LogAction::Spawn { face, to, seat } => {
            let target_seat = state.effective_seat(*to, *seat)?;
            let id = state
                .table
                .add_face(PlayerId(entry.seat), *to, face.clone());
            if let Some(card) = state.table.get_mut(id) {
                card.seat = PlayerId(target_seat);
            }
            if state.visibility(*to)? != ZoneVisibility::None {
                state.revealed.insert(id.0);
            }
            state.tokens.insert(id.0);
        }
        LogAction::Reveal { card, face } => {
            if let Some(current) = state.table.get_mut(CardId(*card)) {
                current.face = face.clone();
            }
            state.revealed.insert(*card);
            state.forget_faces(*card);
            state.show_in_place(*card)?;
        }
        LogAction::Annotate { card, key, value } => match value {
            Some(value) => {
                state
                    .annotations
                    .entry(*card)
                    .or_default()
                    .insert(key.clone(), value.clone());
            }
            None => {
                if let Some(entries) = state.annotations.get_mut(card) {
                    entries.remove(key);
                    if entries.is_empty() {
                        state.annotations.remove(card);
                    }
                }
            }
        },
        LogAction::Game { .. } => {}
        LogAction::Counter {
            target,
            counter,
            delta,
        } => {
            let Some(decl) = state.counter_decl(*counter).cloned() else {
                return Err(FoldError::UnknownCounter);
            };
            let current = state.counter(*target, *counter).unwrap_or(decl.start);
            let next = decl.clamp(current.saturating_add(*delta));
            state.set_counter(*target, *counter, next);
        }
        LogAction::Reset => {
            state.table = Table::new();
            state.revealed.clear();
            state.annotations.clear();
            state.counters.clear();
            state.tokens.clear();
            state.owed_reveals.clear();
            state.peeks.clear();
            state.shown.clear();
        }
        LogAction::Clear { seat } => {
            let owner = PlayerId(*seat);
            let swept: Vec<u32> = state
                .table
                .cards()
                .iter()
                .filter(|card| card.owner == owner)
                .map(|card| card.id.0)
                .collect();
            state.table.retain(|card| card.owner != owner);
            for card in swept {
                state.revealed.remove(&card);
                state.annotations.remove(&card);
                state.tokens.remove(&card);
                state.forget_faces(card);
                state
                    .counters
                    .retain(|held| held.target != CounterTarget::Card(card));
            }
            state
                .counters
                .retain(|held| held.target != CounterTarget::Seat(*seat));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Move {
        card: u32,
        to: Zone,
        seat: u8,
        index: u32,
    },
    Annotate {
        card: u32,
        key: String,
        value: Option<ByteBuf>,
    },
    Counter {
        target: CounterTarget,
        counter: u16,
        delta: i32,
    },
    Spawn {
        face: CardFace,
        to: Zone,
        seat: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<u8>,
    },
    Despawn {
        card: u32,
    },
    Reveal {
        card: u32,
    },
    Peek {
        card: u32,
        seat: u8,
    },
    Conceal {
        card: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub accept: bool,
    pub plugin_state: Option<ByteBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Verdict {
    pub fn accept() -> Self {
        Self {
            accept: true,
            plugin_state: None,
            effects: Vec::new(),
            reason: None,
        }
    }

    pub fn reject() -> Self {
        Self {
            accept: false,
            plugin_state: None,
            effects: Vec::new(),
            reason: None,
        }
    }

    pub fn refuse(reason: impl Into<String>) -> Self {
        Self {
            accept: false,
            plugin_state: None,
            effects: Vec::new(),
            reason: Some(reason.into()),
        }
    }

    pub fn refusal(&self) -> FoldError {
        FoldError::Rejected {
            reason: self.reason.clone(),
        }
    }

    pub fn with_effects(mut self, effects: Vec<Effect>) -> Self {
        self.effects = effects;
        self
    }
}

fn apply_effect(state: &mut LogState, actor: u8, effect: &Effect) -> Result<(), FoldError> {
    match effect {
        Effect::Move {
            card,
            to,
            seat,
            index,
        } => {
            if state.table.get(CardId(*card)).is_none() {
                return Err(FoldError::UnknownCard);
            }
            let target_seat = state.effective_seat(*to, *seat)?;
            let moved = state.table.apply(Intent::MoveCard {
                card: CardId(*card),
                to: *to,
                seat: PlayerId(target_seat),
                index: *index as usize,
            });
            if !moved {
                return Ok(());
            }
            state.shown.remove(card);
            if state.visibility(*to)? == ZoneVisibility::None && state.revealed.remove(card) {
                if let Some(current) = state.table.get_mut(CardId(*card)) {
                    current.face = CardFace::hidden();
                }
            }
            if sheds_state(state.kind(*to)?) {
                state.shed(*card);
            }
            Ok(())
        }
        Effect::Annotate { card, key, value } => {
            if state.table.get(CardId(*card)).is_none() {
                return Err(FoldError::UnknownCard);
            }
            match value {
                Some(value) => {
                    state
                        .annotations
                        .entry(*card)
                        .or_default()
                        .insert(key.clone(), value.clone());
                }
                None => {
                    if let Some(entries) = state.annotations.get_mut(card) {
                        entries.remove(key);
                        if entries.is_empty() {
                            state.annotations.remove(card);
                        }
                    }
                }
            }
            Ok(())
        }
        Effect::Counter {
            target,
            counter,
            delta,
        } => {
            let Some(decl) = state.counter_decl(*counter).cloned() else {
                return Err(FoldError::UnknownCounter);
            };
            if !decl.accepts(target) {
                return Err(FoldError::CounterScope);
            }
            match target {
                CounterTarget::Seat(seat) if !state.seated(*seat) => {
                    return Err(FoldError::UnknownSeat)
                }
                CounterTarget::Card(card) if state.table.get(CardId(*card)).is_none() => {
                    return Err(FoldError::UnknownCard)
                }
                _ => {}
            }
            let current = state.counter(*target, *counter).unwrap_or(decl.start);
            let next = decl.clamp(current.saturating_add(*delta));
            state.set_counter(*target, *counter, next);
            Ok(())
        }
        Effect::Spawn {
            face,
            to,
            seat,
            owner,
        } => {
            if face.is_hidden() {
                return Err(FoldError::NoOp);
            }
            let target_seat = state.effective_seat(*to, *seat)?;
            let owner = owner.unwrap_or(actor);
            let id = state.table.add_face(PlayerId(owner), *to, face.clone());
            if let Some(card) = state.table.get_mut(id) {
                card.seat = PlayerId(target_seat);
            }
            if state.visibility(*to)? != ZoneVisibility::None {
                state.revealed.insert(id.0);
            }
            state.tokens.insert(id.0);
            Ok(())
        }
        Effect::Despawn { card } => {
            if state.table.get(CardId(*card)).is_none() {
                return Err(FoldError::UnknownCard);
            }
            if !state.despawn_any && !state.tokens.contains(card) {
                return Err(FoldError::BadEffect { index: 0 });
            }
            state.table.retain(|held| held.id.0 != *card);
            state.revealed.remove(card);
            state.tokens.remove(card);
            state.shed(*card);
            state.forget_faces(*card);
            Ok(())
        }
        Effect::Reveal { card } => {
            if state.table.get(CardId(*card)).is_none() {
                return Err(FoldError::UnknownCard);
            }
            if !state.revealed.contains(card) {
                state.owed_reveals.insert(*card);
            }
            Ok(())
        }
        Effect::Peek { card, seat } => {
            if state.table.get(CardId(*card)).is_none() {
                return Err(FoldError::UnknownCard);
            }
            if !state.seated(*seat) {
                return Err(FoldError::UnknownSeat);
            }
            if !state.revealed.contains(card) {
                state.peeks.insert((*card, *seat));
            }
            Ok(())
        }
        Effect::Conceal { card } => {
            let current = state
                .table
                .get_mut(CardId(*card))
                .ok_or(FoldError::UnknownCard)?;
            current.face = CardFace::hidden();
            state.revealed.remove(card);
            state.forget_faces(*card);
            Ok(())
        }
    }
}

pub trait Decider {
    fn decide(&mut self, plugin_state: &[u8], state: &LogState, entry: &LogEntry) -> Verdict;
}

pub struct AcceptAll;

impl Decider for AcceptAll {
    fn decide(&mut self, _plugin_state: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
        Verdict::accept()
    }
}

pub fn fold_begin(state: &mut LogState, entry: &LogEntry) -> Result<(), FoldError> {
    if entry.seq != state.next_seq {
        return Err(FoldError::BadSeq);
    }
    let checked = validate_action(state, entry);
    state.next_seq += 1;
    checked
}

pub fn fold_finish(
    state: &mut LogState,
    entry: &LogEntry,
    verdict: Verdict,
) -> Result<(), FoldError> {
    if !verdict.accept {
        return Err(verdict.refusal());
    }
    if verdict.effects.is_empty() {
        apply_action(state, entry)?;
    } else {
        let mut next = state.clone();
        apply_action(&mut next, entry)?;
        for (index, effect) in verdict.effects.iter().enumerate() {
            apply_effect(&mut next, entry.seat, effect)
                .map_err(|_| FoldError::BadEffect { index })?;
        }
        *state = next;
    }
    if let Some(next) = verdict.plugin_state {
        state.plugin_state = next;
    }
    Ok(())
}

pub fn fold_entry_with(
    state: &mut LogState,
    entry: &LogEntry,
    decider: &mut dyn Decider,
) -> Result<(), FoldError> {
    fold_begin(state, entry)?;
    let plugin_state = std::mem::take(&mut state.plugin_state);
    let verdict = decider.decide(&plugin_state, state, entry);
    state.plugin_state = plugin_state;
    fold_finish(state, entry, verdict)
}

pub fn encode_state(state: &LogState) -> Vec<u8> {
    encode(state)
}

pub fn decode_state(bytes: &[u8]) -> Option<LogState> {
    decode(bytes)
}

pub fn fold_entry(state: &mut LogState, entry: &LogEntry) -> Result<(), FoldError> {
    fold_entry_with(state, entry, &mut AcceptAll)
}

pub fn fold_with(entries: &[LogEntry], decider: &mut dyn Decider) -> LogState {
    let mut state = LogState::new();
    for entry in entries {
        let _ = fold_entry_with(&mut state, entry, decider);
    }
    state
}

pub fn fold(entries: &[LogEntry]) -> LogState {
    fold_with(entries, &mut AcceptAll)
}

#[cfg(test)]
pub fn render_table(state: &LogState, faces: &BTreeMap<u32, CardFace>, viewer: u8) -> Table {
    let mut table = state.table.clone();
    let zones = &state.zones;
    for (id, face) in table.faces_mut() {
        let Some(known) = faces.get(&id.0) else {
            continue;
        };
        let Some(card) = state.table.get(id) else {
            continue;
        };
        let visible = match zone_visibility(zones, card.zone) {
            Some(ZoneVisibility::All) => true,
            Some(ZoneVisibility::Owner) => card.seat.0 == viewer,
            Some(ZoneVisibility::None) | None => false,
        };
        if visible || state.peeked_by(id.0, viewer) || state.shown_in_place(id.0) {
            *face = known.clone();
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{ZoneLayout, ZonePlace};

    fn decl(id: u16, name: &str, owner: ZoneOwner, visibility: ZoneVisibility) -> ZoneDecl {
        ZoneDecl {
            id,
            name: name.into(),
            kind: ZoneKind::Aux,
            owner,
            visibility,
            layout: ZoneLayout::Pile,
            place: ZonePlace::Outer,
            span: 1,
            label: name.into(),
        }
    }

    fn zoned_state(zones: Vec<ZoneDecl>) -> LogState {
        let mut state = LogState::new();
        let entry = LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: None,
                    plugin: None,
                    zones,
                    options: None,
                    counters: Vec::new(),
                    despawn_any: false,
                },
            },
        );
        fold_entry(&mut state, &entry).expect("genesis folds");
        state
    }

    fn step(state: &mut LogState, seat: u8, action: LogAction) -> Result<(), FoldError> {
        let entry = LogEntry::new(state.next_seq, seat, action);
        fold_entry(state, &entry)
    }

    fn counter(
        id: u16,
        name: &str,
        scope: crate::wire::CounterScope,
        start: i32,
        min: Option<i32>,
        max: Option<i32>,
    ) -> CounterDecl {
        CounterDecl {
            id,
            name: name.into(),
            label: name.into(),
            scope,
            start,
            min,
            max,
            step: 1,
            place: crate::wire::CounterPlace::SeatPlate,
            color: crate::wire::DEFAULT_COUNTER_COLOR,
        }
    }

    fn counting_state(counters: Vec<CounterDecl>) -> LogState {
        let mut state = LogState::new();
        let entry = LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: None,
                    plugin: None,
                    zones: vec![decl(1, "board", ZoneOwner::Shared, ZoneVisibility::All)],
                    options: None,
                    counters,
                    despawn_any: false,
                },
            },
        );
        fold_entry(&mut state, &entry).expect("genesis folds");
        state
    }

    #[test]
    fn a_counter_starts_at_its_declared_value_and_moves_by_deltas() {
        use crate::wire::CounterScope;
        let mut state =
            counting_state(vec![counter(1, "life", CounterScope::Seat, 20, None, None)]);
        assert_eq!(state.counter(CounterTarget::Seat(0), 1), Some(20));

        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: -3,
            },
        )
        .unwrap();
        assert_eq!(state.counter(CounterTarget::Seat(0), 1), Some(17));

        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: 5,
            },
        )
        .unwrap();
        assert_eq!(state.counter(CounterTarget::Seat(0), 1), Some(22));
    }

    #[test]
    fn a_counter_clamps_to_its_declared_range() {
        use crate::wire::CounterScope;
        let mut state = counting_state(vec![counter(
            2,
            "points",
            CounterScope::Seat,
            0,
            Some(0),
            Some(8),
        )]);
        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 2,
                delta: 100,
            },
        )
        .unwrap();
        assert_eq!(state.counter(CounterTarget::Seat(0), 2), Some(8));

        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Counter {
                    target: CounterTarget::Seat(0),
                    counter: 2,
                    delta: 1,
                },
            ),
            Err(FoldError::NoOp)
        );

        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 2,
                delta: -100,
            },
        )
        .unwrap();
        assert_eq!(state.counter(CounterTarget::Seat(0), 2), Some(0));
    }

    #[test]
    fn an_undeclared_counter_is_refused() {
        let mut state = counting_state(Vec::new());
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Counter {
                    target: CounterTarget::Seat(0),
                    counter: 9,
                    delta: 1,
                },
            ),
            Err(FoldError::UnknownCounter)
        );
    }

    #[test]
    fn a_counter_refuses_a_target_outside_its_scope() {
        use crate::wire::CounterScope;
        let mut state =
            counting_state(vec![counter(1, "life", CounterScope::Seat, 20, None, None)]);
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Counter {
                    target: CounterTarget::Table,
                    counter: 1,
                    delta: 1,
                },
            ),
            Err(FoldError::CounterScope)
        );
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Counter {
                    target: CounterTarget::Card(1),
                    counter: 1,
                    delta: 1,
                },
            ),
            Err(FoldError::CounterScope)
        );
    }

    #[test]
    fn a_counter_on_a_card_that_is_not_there_is_refused() {
        use crate::wire::CounterScope;
        let mut state = counting_state(vec![counter(
            3,
            "plus-one",
            CounterScope::Card,
            0,
            None,
            None,
        )]);
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Counter {
                    target: CounterTarget::Card(77),
                    counter: 3,
                    delta: 1,
                },
            ),
            Err(FoldError::UnknownCard)
        );
    }

    #[test]
    fn clearing_a_seat_drops_its_counters_and_its_cards_counters() {
        use crate::wire::CounterScope;
        let mut state = counting_state(vec![
            counter(1, "life", CounterScope::Seat, 20, None, None),
            counter(3, "plus-one", CounterScope::Card, 0, None, None),
        ]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Card(1),
                counter: 3,
                delta: 2,
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: -5,
            },
        )
        .unwrap();
        assert_eq!(state.counter(CounterTarget::Card(1), 3), Some(2));

        step(&mut state, 0, LogAction::Clear { seat: 0 }).unwrap();
        assert!(state.counters.is_empty());
        assert_eq!(state.counter(CounterTarget::Seat(0), 1), Some(20));
    }

    #[test]
    fn a_reset_clears_counter_values_but_keeps_the_counter_table() {
        use crate::wire::CounterScope;
        let mut state =
            counting_state(vec![counter(1, "life", CounterScope::Seat, 20, None, None)]);
        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: -7,
            },
        )
        .unwrap();
        step(&mut state, 0, LogAction::Reset).unwrap();
        assert!(state.counters.is_empty());
        assert_eq!(state.counter_table.len(), 1);
        assert_eq!(state.counter(CounterTarget::Seat(0), 1), Some(20));
    }

    #[test]
    fn the_genesis_options_land_in_the_state_and_survive_a_reset_and_a_round_trip() {
        let mut state = LogState::new();
        let options = ByteBuf::from(vec![0xa1, 0x61, 0x6b, 0x06]);
        step(
            &mut state,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    options: Some(options.clone()),
                    ..TableConfig::default()
                },
            },
        )
        .unwrap();
        assert_eq!(state.options.as_ref(), Some(&options));
        step(&mut state, 0, LogAction::Reset).unwrap();
        assert_eq!(state.options.as_ref(), Some(&options));
        let decoded = decode_state(&encode_state(&state)).unwrap();
        assert_eq!(decoded.options, state.options);
        let bare = fold(&[LogEntry::new(0, 0, LogAction::genesis("rae"))]);
        assert_eq!(bare.options, None);
        assert!(!String::from_utf8_lossy(&encode_state(&bare)).contains("options"));
    }

    #[test]
    fn a_counter_log_survives_its_round_trip() {
        use crate::wire::CounterScope;
        let mut state =
            counting_state(vec![counter(1, "life", CounterScope::Seat, 20, None, None)]);
        step(
            &mut state,
            0,
            LogAction::Counter {
                target: CounterTarget::Seat(0),
                counter: 1,
                delta: -3,
            },
        )
        .unwrap();
        let bytes = encode_state(&state);
        let decoded = decode_state(&bytes).unwrap();
        assert_eq!(decoded.counters, state.counters);
        assert_eq!(decoded.counter_table, state.counter_table);
    }

    #[test]
    fn nothing_folds_before_genesis() {
        let mut state = LogState::new();
        let entry = LogEntry::new(0, 0, LogAction::Join { name: "ada".into() });
        assert_eq!(
            fold_entry(&mut state, &entry),
            Err(FoldError::MissingGenesis)
        );
        assert!(state.table.is_empty());
        assert!(state.seats.is_empty());
    }

    #[test]
    fn a_genesis_with_duplicate_zone_ids_is_refused() {
        let mut state = LogState::new();
        let entry = LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: None,
                    plugin: None,
                    zones: vec![
                        decl(3, "a", ZoneOwner::PerSeat, ZoneVisibility::All),
                        decl(3, "b", ZoneOwner::Shared, ZoneVisibility::All),
                    ],
                    options: None,
                    counters: Vec::new(),
                    despawn_any: false,
                },
            },
        );
        assert_eq!(fold_entry(&mut state, &entry), Err(FoldError::BadZoneTable));
    }

    #[test]
    fn a_second_genesis_is_a_duplicate() {
        let mut state = zoned_state(Vec::new());
        assert_eq!(
            step(&mut state, 0, LogAction::genesis("again")),
            Err(FoldError::DuplicateGenesis)
        );
        assert_eq!(state.seats.len(), 1);
    }

    #[test]
    fn genesis_pins_the_zone_table_and_moves_reach_declared_zones() {
        let mut state = zoned_state(vec![
            decl(0, "deck", ZoneOwner::PerSeat, ZoneVisibility::None),
            decl(1, "field", ZoneOwner::Shared, ZoneVisibility::All),
        ]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![7],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        assert_eq!(state.table.get(CardId(7)).unwrap().zone, Zone::Plugin(0));
        step(
            &mut state,
            0,
            LogAction::Move {
                card: 7,
                to: Zone::Plugin(1),
                seat: 5,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        let card = state.table.get(CardId(7)).unwrap();
        assert_eq!(card.zone, Zone::Plugin(1));
        assert_eq!(card.seat, PlayerId(0));
    }

    #[test]
    fn a_move_to_an_undeclared_zone_is_refused() {
        let mut state = zoned_state(vec![decl(
            0,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Move {
                    card: 1,
                    to: Zone::Plugin(9),
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            ),
            Err(FoldError::UnknownZone)
        );
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Deal {
                    cards: vec![2],
                    to: Zone::Plugin(9),
                },
            ),
            Err(FoldError::UnknownZone)
        );
    }

    #[test]
    fn a_face_never_survives_entry_into_a_hidden_zone() {
        let mut state = zoned_state(vec![decl(
            0,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Reveal {
                card: 1,
                face: CardFace::named("public"),
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Move {
                card: 1,
                to: Zone::Plugin(0),
                seat: 0,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        let card = state.table.get(CardId(1)).unwrap();
        assert_eq!(card.face, CardFace::hidden());
        assert!(!state.revealed.contains(&1));
    }

    #[test]
    fn a_reveal_inside_a_hidden_zone_is_refused() {
        let mut state = zoned_state(vec![decl(
            0,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Reveal {
                    card: 1,
                    face: CardFace::hidden(),
                },
            ),
            Err(FoldError::HiddenZone)
        );
    }

    #[test]
    fn a_public_face_survives_a_return_to_an_owner_zone() {
        let mut state = zoned_state(vec![decl(
            0,
            "pocket",
            ZoneOwner::PerSeat,
            ZoneVisibility::Owner,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        let face = CardFace::named("public");
        step(
            &mut state,
            0,
            LogAction::Reveal {
                card: 1,
                face: face.clone(),
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Move {
                card: 1,
                to: Zone::Plugin(0),
                seat: 0,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        assert_eq!(state.table.get(CardId(1)).unwrap().face, face);
        assert!(state.revealed.contains(&1));
    }

    #[test]
    fn a_card_in_a_public_zone_can_be_revealed_by_any_seat() {
        let mut state = zoned_state(vec![
            decl(0, "deck", ZoneOwner::PerSeat, ZoneVisibility::None),
            decl(1, "field", ZoneOwner::PerSeat, ZoneVisibility::All),
        ]);
        step(&mut state, 1, LogAction::Join { name: "ada".into() }).unwrap();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![4],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        step(
            &mut state,
            1,
            LogAction::Move {
                card: 4,
                to: Zone::Plugin(1),
                seat: 0,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        let landed = state.table.get(CardId(4)).unwrap();
        assert_eq!(landed.seat, PlayerId(0));
        assert_eq!(landed.owner, PlayerId(1));
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 4,
                face: CardFace::named("played across"),
            },
        )
        .unwrap();
        assert!(state.revealed.contains(&4));
        assert_eq!(
            state.table.get(CardId(4)).unwrap().face.name,
            "played across"
        );
    }

    #[test]
    fn an_owner_may_reveal_their_card_from_another_seats_private_zone_but_a_stranger_may_not() {
        let mut state = zoned_state(vec![decl(
            0,
            "pocket",
            ZoneOwner::PerSeat,
            ZoneVisibility::Owner,
        )]);
        step(&mut state, 1, LogAction::Join { name: "ada".into() }).unwrap();
        step(&mut state, 2, LogAction::Join { name: "lin".into() }).unwrap();
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        assert_eq!(
            step(
                &mut state,
                2,
                LogAction::Reveal {
                    card: 1,
                    face: CardFace::named("peeked"),
                },
            ),
            Err(FoldError::ForeignHand)
        );
        step(
            &mut state,
            0,
            LogAction::Reveal {
                card: 1,
                face: CardFace::named("mine"),
            },
        )
        .unwrap();
    }

    #[test]
    fn annotations_fold_toggle_and_clear() {
        let mut state = zoned_state(Vec::new());
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        let on = ByteBuf::from(vec![0xf5]);
        step(
            &mut state,
            0,
            LogAction::Annotate {
                card: 1,
                key: "exhausted".into(),
                value: Some(on.clone()),
            },
        )
        .unwrap();
        assert_eq!(state.annotation(1, "exhausted"), Some(&[0xf5][..]));
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Annotate {
                    card: 1,
                    key: "exhausted".into(),
                    value: Some(on),
                },
            ),
            Err(FoldError::NoOp)
        );
        step(
            &mut state,
            0,
            LogAction::Annotate {
                card: 1,
                key: "exhausted".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(state.annotation(1, "exhausted"), None);
        assert!(state.annotations.is_empty());
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Annotate {
                    card: 1,
                    key: "exhausted".into(),
                    value: None,
                },
            ),
            Err(FoldError::NoOp)
        );
    }

    #[test]
    fn a_game_entry_folds_without_touching_the_table_or_plugin_state() {
        let mut state = zoned_state(Vec::new());
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        let table = state.table.clone();
        step(
            &mut state,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![1, 2, 3]),
            },
        )
        .unwrap();
        assert_eq!(state.table, table);
        assert!(state.plugin_state.is_empty());
    }

    #[test]
    fn a_rejecting_decider_burns_the_seq_slot_without_mutating() {
        struct RejectGames;
        impl Decider for RejectGames {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, entry: &LogEntry) -> Verdict {
                Verdict {
                    accept: !matches!(entry.action, LogAction::Game { .. }),
                    plugin_state: None,
                    effects: Vec::new(),
                    reason: None,
                }
            }
        }
        let mut state = zoned_state(Vec::new());
        let before = state.clone();
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![9]),
            },
        );
        assert_eq!(
            fold_entry_with(&mut state, &entry, &mut RejectGames),
            Err(FoldError::rejected())
        );
        assert_eq!(state.next_seq, before.next_seq + 1);
        assert_eq!(state.table, before.table);
        assert_eq!(state.plugin_state, before.plugin_state);
    }

    #[test]
    fn a_deciders_plugin_state_threads_through_the_fold_untouched_by_the_engine() {
        struct Counter;
        impl Decider for Counter {
            fn decide(&mut self, blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                let count = if blob.is_empty() { 0 } else { blob[0] };
                Verdict {
                    accept: true,
                    plugin_state: Some(ByteBuf::from(vec![count + 1])),
                    effects: Vec::new(),
                    reason: None,
                }
            }
        }
        let mut state = LogState::new();
        let entries = vec![
            LogEntry::new(0, 0, LogAction::genesis("rae")),
            LogEntry::new(
                1,
                0,
                LogAction::Deal {
                    cards: vec![1],
                    to: Zone::Hand,
                },
            ),
            LogEntry::new(
                2,
                0,
                LogAction::Game {
                    data: ByteBuf::from(vec![7]),
                },
            ),
        ];
        for entry in &entries {
            fold_entry_with(&mut state, entry, &mut Counter).unwrap();
        }
        assert_eq!(state.plugin_state.as_slice(), &[3]);
        let refolded = fold_with(&entries, &mut Counter);
        assert_eq!(refolded, state);
        assert!(fold(&entries).plugin_state.is_empty());
    }

    #[test]
    fn shared_zone_moves_normalize_the_seat() {
        let mut state = zoned_state(vec![decl(
            0,
            "field",
            ZoneOwner::Shared,
            ZoneVisibility::All,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1, 2],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        for id in [1u32, 2] {
            assert_eq!(state.table.get(CardId(id)).unwrap().seat, PlayerId(0));
        }
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Move {
                    card: 1,
                    to: Zone::Plugin(0),
                    seat: 3,
                    index: 0,
                    hidden: false,
                },
            ),
            Err(FoldError::NoOp)
        );
    }

    #[test]
    fn foreign_private_declared_zones_are_guarded_like_hands() {
        let mut state = zoned_state(vec![decl(
            0,
            "pocket",
            ZoneOwner::PerSeat,
            ZoneVisibility::Owner,
        )]);
        step(&mut state, 1, LogAction::Join { name: "ada".into() }).unwrap();
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        assert_eq!(
            step(
                &mut state,
                1,
                LogAction::Move {
                    card: 1,
                    to: Zone::Board,
                    seat: 1,
                    index: 0,
                    hidden: false,
                },
            ),
            Err(FoldError::ForeignHand)
        );
        assert_eq!(
            step(
                &mut state,
                1,
                LogAction::Annotate {
                    card: 1,
                    key: "exhausted".into(),
                    value: Some(ByteBuf::from(vec![0xf5])),
                },
            ),
            Err(FoldError::ForeignHand)
        );
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![2],
                to: Zone::Hand,
            },
        )
        .unwrap();
        assert_eq!(
            step(
                &mut state,
                1,
                LogAction::Move {
                    card: 2,
                    to: Zone::Plugin(0),
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            ),
            Err(FoldError::ForeignHandTarget)
        );
    }

    #[test]
    fn a_hidden_zone_face_never_reaches_a_rendered_view() {
        let mut state = zoned_state(vec![decl(
            0,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        let mut faces = BTreeMap::new();
        faces.insert(1u32, CardFace::named("secret"));
        let owner_view = render_table(&state, &faces, 0);
        assert_eq!(owner_view.get(CardId(1)).unwrap().face, CardFace::hidden());
        let other_view = render_table(&state, &faces, 1);
        assert_eq!(other_view.get(CardId(1)).unwrap().face, CardFace::hidden());
    }

    #[test]
    fn a_reset_clears_annotations_but_keeps_zones_and_plugin_state() {
        let mut state = zoned_state(vec![decl(
            0,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        state.plugin_state = ByteBuf::from(vec![42]);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1],
                to: Zone::Hand,
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Annotate {
                card: 1,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![0xf5])),
            },
        )
        .unwrap();
        step(&mut state, 0, LogAction::Reset).unwrap();
        assert!(state.table.is_empty());
        assert!(state.annotations.is_empty());
        assert_eq!(state.zones.len(), 1);
        assert_eq!(state.plugin_state.as_slice(), &[42]);
    }

    #[test]
    fn a_clear_sweeps_only_the_cards_that_seat_brought() {
        let mut state = zoned_state(vec![
            decl(0, "deck", ZoneOwner::PerSeat, ZoneVisibility::None),
            decl(1, "field", ZoneOwner::Shared, ZoneVisibility::All),
        ]);
        step(&mut state, 1, LogAction::Join { name: "ada".into() }).unwrap();
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![1, 2],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![3],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![4],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Annotate {
                card: 3,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![0xf5])),
            },
        )
        .unwrap();
        assert_eq!(
            step(&mut state, 1, LogAction::Clear { seat: 0 }),
            Err(FoldError::ForeignHand)
        );
        assert_eq!(
            step(&mut state, 0, LogAction::Clear { seat: 9 }),
            Err(FoldError::UnknownSeat)
        );
        step(&mut state, 0, LogAction::Clear { seat: 0 }).unwrap();
        let left: Vec<u32> = state.table.cards().iter().map(|card| card.id.0).collect();
        assert_eq!(left, vec![4]);
        assert!(state.annotations.is_empty());
        assert_eq!(state.zones.len(), 2);
        assert_eq!(
            step(&mut state, 0, LogAction::Clear { seat: 0 }),
            Err(FoldError::NoOp)
        );
        step(&mut state, 1, LogAction::Clear { seat: 1 }).unwrap();
        assert!(state.table.is_empty());
    }

    #[test]
    fn every_fold_error_explains_itself() {
        let all = [
            FoldError::BadSeq,
            FoldError::MissingGenesis,
            FoldError::DuplicateGenesis,
            FoldError::BadZoneTable,
            FoldError::NotTheHost,
            FoldError::SeatTaken,
            FoldError::UnknownSeat,
            FoldError::UnknownCard,
            FoldError::UnknownZone,
            FoldError::DuplicateCard,
            FoldError::ForeignHand,
            FoldError::ForeignHandTarget,
            FoldError::HiddenZone,
            FoldError::FaceConflict,
            FoldError::NoOp,
            FoldError::rejected(),
            FoldError::BadEffect { index: 2 },
        ];
        for error in all {
            assert!(!error.to_string().is_empty());
        }
        assert_eq!(
            FoldError::Rejected {
                reason: Some("not your turn".into())
            }
            .to_string(),
            "not your turn"
        );
        assert_eq!(
            FoldError::rejected().to_string(),
            "the game rules refused it"
        );
        assert!(FoldError::BadEffect { index: 2 }
            .to_string()
            .starts_with("effect 2:"));
    }

    #[test]
    fn a_refusal_carries_the_deciders_reason_and_nothing_else_changes() {
        struct Because;
        impl Decider for Because {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                Verdict::refuse("runes are paid for you")
            }
        }
        let mut state = zoned_state(Vec::new());
        let before = state.clone();
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![1]),
            },
        );
        let refused = fold_entry_with(&mut state, &entry, &mut Because).unwrap_err();
        assert_eq!(refused.reason(), Some("runes are paid for you"));
        assert!(refused.is_rejected());
        assert_eq!(state.table, before.table);
        assert_eq!(state.next_seq, before.next_seq + 1);
        let bytes = encode(&Verdict::refuse("why"));
        assert!(bytes.windows(6).any(|window| window == b"reason"));
        assert!(!encode(&Verdict::reject())
            .windows(6)
            .any(|window| window == b"reason"));
        assert_eq!(
            decode::<Verdict>(&bytes).unwrap().reason.as_deref(),
            Some("why")
        );
    }
    fn kinded(id: u16, name: &str, kind: ZoneKind, owner: ZoneOwner) -> ZoneDecl {
        ZoneDecl {
            kind,
            ..decl(id, name, owner, ZoneVisibility::All)
        }
    }

    fn play_table() -> LogState {
        let mut state = zoned_state(vec![
            kinded(0, "hand", ZoneKind::Hand, ZoneOwner::PerSeat),
            kinded(1, "deck", ZoneKind::Deck, ZoneOwner::PerSeat),
            kinded(2, "base", ZoneKind::Battlefield, ZoneOwner::PerSeat),
            kinded(3, "field", ZoneKind::Battlefield, ZoneOwner::Shared),
        ]);
        state.counter_table = vec![
            crate::wire::CounterDecl {
                id: 0,
                name: "points".into(),
                label: "points".into(),
                scope: crate::wire::CounterScope::Seat,
                start: 0,
                min: Some(0),
                max: None,
                step: 1,
                place: crate::wire::CounterPlace::SeatPlate,
                color: [0; 3],
            },
            crate::wire::CounterDecl {
                id: 1,
                name: "damage".into(),
                label: "damage".into(),
                scope: crate::wire::CounterScope::Card,
                start: 0,
                min: Some(0),
                max: None,
                step: 1,
                place: crate::wire::CounterPlace::CardBadge,
                color: [0; 3],
            },
        ];
        step(&mut state, 1, LogAction::Join { name: "b".into() }).unwrap();
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![7, 8],
                to: Zone::Plugin(2),
            },
        )
        .unwrap();
        state
    }

    fn exhausted(state: &mut LogState, card: u32) {
        step(
            state,
            0,
            LogAction::Annotate {
                card,
                key: "exhausted".into(),
                value: Some(ByteBuf::from(vec![1])),
            },
        )
        .unwrap();
        step(
            state,
            0,
            LogAction::Counter {
                target: CounterTarget::Card(card),
                counter: 1,
                delta: 2,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_card_leaving_play_for_a_deck_hand_or_trash_sheds_its_marks() {
        for (zone, sheds) in [(1u16, true), (0, true), (3, false)] {
            let mut state = play_table();
            exhausted(&mut state, 7);
            step(
                &mut state,
                0,
                LogAction::Move {
                    card: 7,
                    to: Zone::Plugin(zone),
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            )
            .unwrap();
            assert_eq!(
                state.annotation(7, "exhausted").is_none(),
                sheds,
                "zone {zone}"
            );
            assert_eq!(
                state.counter(CounterTarget::Card(7), 1) == Some(0),
                sheds,
                "zone {zone}"
            );
        }
    }

    #[test]
    fn a_spawn_puts_a_fresh_revealed_card_on_the_table() {
        let mut state = play_table();
        step(
            &mut state,
            1,
            LogAction::Spawn {
                face: CardFace::named("Sprite")
                    .with_kind("Unit")
                    .with_might(Some(3)),
                to: Zone::Plugin(3),
                seat: 1,
            },
        )
        .unwrap();
        let token = state
            .table
            .cards()
            .iter()
            .find(|card| card.face.name == "Sprite")
            .expect("the token is on the table");
        assert_eq!(token.owner, PlayerId(1));
        assert_eq!(token.seat, PlayerId(0));
        assert_eq!(token.zone, Zone::Plugin(3));
        assert_eq!(token.face.might, Some(3));
        assert!(state.revealed.contains(&token.id.0));
        assert!(state.is_token(token.id.0));
        let id = token.id.0;
        step(
            &mut state,
            1,
            LogAction::Move {
                card: id,
                to: Zone::Plugin(1),
                seat: 1,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        assert!(state.table.get(CardId(id)).is_none());
        assert!(!state.is_token(id));
        assert!(state.table.get(CardId(7)).is_some());
        assert_eq!(
            step(
                &mut state,
                1,
                LogAction::Spawn {
                    face: CardFace::hidden(),
                    to: Zone::Plugin(3),
                    seat: 1,
                },
            ),
            Err(FoldError::NoOp)
        );
        assert_eq!(
            step(
                &mut state,
                1,
                LogAction::Spawn {
                    face: CardFace::named("Sprite"),
                    to: Zone::Plugin(9),
                    seat: 1,
                },
            ),
            Err(FoldError::UnknownZone)
        );
    }

    #[test]
    fn verdict_effects_apply_after_the_entry_and_a_bad_one_refuses_it_whole() {
        struct Choreography(Vec<Effect>);
        impl Decider for Choreography {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                Verdict::accept().with_effects(self.0.clone())
            }
        }
        let mut state = play_table();
        exhausted(&mut state, 7);
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![1]),
            },
        );
        let mut decider = Choreography(vec![
            Effect::Annotate {
                card: 7,
                key: "exhausted".into(),
                value: None,
            },
            Effect::Move {
                card: 8,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
            },
            Effect::Counter {
                target: CounterTarget::Seat(0),
                counter: 0,
                delta: 1,
            },
            Effect::Spawn {
                face: CardFace::named("Sprite"),
                to: Zone::Plugin(3),
                seat: 0,
                owner: None,
            },
        ]);
        fold_entry_with(&mut state, &entry, &mut decider).unwrap();
        assert!(state.annotation(7, "exhausted").is_none());
        assert_eq!(state.table.get(CardId(8)).unwrap().zone, Zone::Plugin(3));
        assert_eq!(state.counter(CounterTarget::Seat(0), 0), Some(1));
        assert_eq!(state.table.len(), 3);
        let token = state
            .table
            .cards()
            .iter()
            .find(|card| card.face.name == "Sprite")
            .map(|card| card.id.0)
            .unwrap();
        assert_eq!(state.table.get(CardId(token)).unwrap().owner, PlayerId(0));
        let before = state.clone();
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![2]),
            },
        );
        let mut bad = Choreography(vec![
            Effect::Counter {
                target: CounterTarget::Seat(0),
                counter: 0,
                delta: 1,
            },
            Effect::Despawn { card: 99 },
        ]);
        assert_eq!(
            fold_entry_with(&mut state, &entry, &mut bad),
            Err(FoldError::BadEffect { index: 1 })
        );
        assert_eq!(state.table, before.table);
        assert_eq!(state.counters, before.counters);
        assert_eq!(state.next_seq, before.next_seq + 1);
        let mut dealt = Choreography(vec![Effect::Despawn { card: 8 }]);
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![3]),
            },
        );
        assert_eq!(
            fold_entry_with(&mut state, &entry, &mut dealt),
            Err(FoldError::BadEffect { index: 0 })
        );
        assert!(state.table.get(CardId(8)).is_some());
        let mut despawn = Choreography(vec![Effect::Despawn { card: token }]);
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![4]),
            },
        );
        fold_entry_with(&mut state, &entry, &mut despawn).unwrap();
        assert!(state.table.get(CardId(token)).is_none());
        assert!(!state.is_token(token));
    }

    #[test]
    fn a_clear_forgets_the_swept_seats_tokens() {
        let mut state = play_table();
        step(
            &mut state,
            0,
            LogAction::Spawn {
                face: CardFace::named("Sprite"),
                to: Zone::Plugin(3),
                seat: 0,
            },
        )
        .unwrap();
        let token = state
            .table
            .cards()
            .iter()
            .find(|card| card.face.name == "Sprite")
            .map(|card| card.id.0)
            .unwrap();
        assert!(state.is_token(token));
        step(&mut state, 0, LogAction::Clear { seat: 0 }).unwrap();
        assert!(state.table.get(CardId(token)).is_none());
        assert!(!state.is_token(token));
        assert!(state.tokens.is_empty());
    }

    #[test]
    fn a_manifest_that_allows_it_lets_a_plugin_despawn_dealt_cards() {
        struct Choreography(Vec<Effect>);
        impl Decider for Choreography {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                Verdict::accept().with_effects(self.0.clone())
            }
        }
        let mut state = LogState::new();
        let genesis = LogEntry::new(
            0,
            0,
            LogAction::Genesis {
                name: "rae".into(),
                config: TableConfig {
                    engine: None,
                    plugin: None,
                    zones: vec![kinded(2, "base", ZoneKind::Battlefield, ZoneOwner::PerSeat)],
                    options: None,
                    counters: Vec::new(),
                    despawn_any: true,
                },
            },
        );
        fold_entry(&mut state, &genesis).unwrap();
        assert!(state.despawn_any);
        step(
            &mut state,
            0,
            LogAction::Deal {
                cards: vec![7, 8],
                to: Zone::Plugin(2),
            },
        )
        .unwrap();
        let entry = LogEntry::new(
            state.next_seq,
            0,
            LogAction::Game {
                data: ByteBuf::from(vec![1]),
            },
        );
        let mut dealt = Choreography(vec![Effect::Despawn { card: 8 }]);
        fold_entry_with(&mut state, &entry, &mut dealt).unwrap();
        assert!(state.table.get(CardId(8)).is_none());
        assert!(state.table.get(CardId(7)).is_some());
        let round_trip = decode_state(&encode_state(&state)).unwrap();
        assert!(round_trip.despawn_any);
        let quiet = encode_state(&play_table());
        assert!(!quiet.windows(11).any(|window| window == b"despawn_any"));
    }

    #[test]
    fn a_spawn_effect_names_its_owner_or_falls_back_to_the_actor() {
        struct Mint(Option<u8>);
        impl Decider for Mint {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                Verdict::accept().with_effects(vec![Effect::Spawn {
                    face: CardFace::named("Gold").with_kind("Gear"),
                    to: Zone::Plugin(3),
                    seat: 1,
                    owner: self.0,
                }])
            }
        }
        let mut state = play_table();
        for (owner, expected) in [(Some(1), 1), (None, 0)] {
            let entry = LogEntry::new(
                state.next_seq,
                0,
                LogAction::Game {
                    data: ByteBuf::from(vec![1]),
                },
            );
            fold_entry_with(&mut state, &entry, &mut Mint(owner)).unwrap();
            let minted = state.table.cards().last().unwrap();
            assert_eq!(minted.owner, PlayerId(expected));
            assert_eq!(minted.seat, PlayerId(0));
            assert!(state.is_token(minted.id.0));
        }
        let bytes = encode(&Effect::Spawn {
            face: CardFace::named("Gold"),
            to: Zone::Plugin(3),
            seat: 1,
            owner: None,
        });
        assert!(!bytes.windows(5).any(|window| window == b"owner"));
        assert_eq!(
            decode::<Effect>(&bytes).unwrap(),
            Effect::Spawn {
                face: CardFace::named("Gold"),
                to: Zone::Plugin(3),
                seat: 1,
                owner: None,
            }
        );
    }

    fn choreographed(
        state: &mut LogState,
        seat: u8,
        effects: Vec<Effect>,
    ) -> Result<(), FoldError> {
        struct Choreography(Vec<Effect>);
        impl Decider for Choreography {
            fn decide(&mut self, _blob: &[u8], _state: &LogState, _entry: &LogEntry) -> Verdict {
                Verdict::accept().with_effects(self.0.clone())
            }
        }
        let entry = LogEntry::new(
            state.next_seq,
            seat,
            LogAction::Game {
                data: ByteBuf::from(vec![1]),
            },
        );
        fold_entry_with(state, &entry, &mut Choreography(effects))
    }

    #[test]
    fn a_reveal_effect_owes_the_face_to_the_table_until_the_reveal_entry_pays_it() {
        let mut state = play_table();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        choreographed(&mut state, 0, vec![Effect::Reveal { card: 9 }]).unwrap();
        assert_eq!(state.owed_reveals, BTreeSet::from([9]));
        assert!(
            !state.revealed.contains(&9),
            "the fold knows no face; the host pays the debt"
        );
        let round_trip = decode_state(&encode_state(&state)).unwrap();
        assert_eq!(round_trip.owed_reveals, BTreeSet::from([9]));
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        assert!(state.owed_reveals.is_empty());
        assert!(state.revealed.contains(&9));
        assert_eq!(state.table.get(CardId(9)).unwrap().face.name, "Seer");
        choreographed(&mut state, 0, vec![Effect::Reveal { card: 9 }]).unwrap();
        assert!(
            state.owed_reveals.is_empty(),
            "a card already public owes nothing"
        );
        assert_eq!(
            choreographed(&mut state, 0, vec![Effect::Reveal { card: 99 }]),
            Err(FoldError::BadEffect { index: 0 })
        );
        let quiet = encode_state(&play_table());
        assert!(!quiet.windows(12).any(|window| window == b"owed_reveals"));
        assert!(!quiet.windows(5).any(|window| window == b"peeks"));
    }

    #[test]
    fn a_reveal_effect_can_show_a_card_in_place_in_a_hidden_deck() {
        let mut state = zoned_state(vec![decl(
            1,
            "deck",
            ZoneOwner::PerSeat,
            ZoneVisibility::None,
        )]);
        step(&mut state, 1, LogAction::Join { name: "b".into() }).unwrap();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        choreographed(&mut state, 1, vec![Effect::Reveal { card: 9 }]).unwrap();
        assert_eq!(
            step(
                &mut state,
                0,
                LogAction::Reveal {
                    card: 9,
                    face: CardFace::named("Seer"),
                },
            ),
            Err(FoldError::ForeignHand)
        );
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        assert!(state.shown_in_place(9));
        assert!(
            crate::view::table_view(&state, 0)
                .card(9)
                .unwrap()
                .face_visible
        );
        assert_eq!(state.table.get(CardId(9)).unwrap().zone, Zone::Plugin(1));
    }

    #[test]
    fn a_card_revealed_in_its_hand_stays_shown_there_until_it_moves() {
        let mut state = zoned_state(vec![
            decl(0, "hand", ZoneOwner::PerSeat, ZoneVisibility::Owner),
            kinded(3, "field", ZoneKind::Battlefield, ZoneOwner::Shared),
        ]);
        step(&mut state, 1, LogAction::Join { name: "b".into() }).unwrap();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9, 10],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        assert!(state.shown_in_place(9));
        assert!(!state.shown_in_place(10));
        let round_trip = decode_state(&encode_state(&state)).unwrap();
        assert_eq!(round_trip.shown, BTreeSet::from([9]));
        let seen_by = |state: &LogState, card: u32, viewer: u8| {
            crate::view::table_view(state, viewer)
                .card(card)
                .unwrap()
                .face_visible
        };
        assert!(seen_by(&state, 9, 0));
        assert!(!seen_by(&state, 10, 0));
        step(
            &mut state,
            1,
            LogAction::Move {
                card: 9,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
                hidden: false,
            },
        )
        .unwrap();
        assert!(!state.shown_in_place(9));
        assert!(state.revealed.contains(&9));
        choreographed(
            &mut state,
            1,
            vec![Effect::Move {
                card: 9,
                to: Zone::Plugin(0),
                seat: 1,
                index: 0,
            }],
        )
        .unwrap();
        assert!(
            !state.shown_in_place(9),
            "a card back in hand is private again"
        );
        assert!(!seen_by(&state, 9, 0));
        assert!(seen_by(&state, 9, 1));
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        assert!(
            state.shown_in_place(9),
            "showing it again in hand shows it again"
        );
        step(&mut state, 1, LogAction::Clear { seat: 1 }).unwrap();
        assert!(state.shown.is_empty());
        let quiet = encode_state(&play_table());
        assert!(!quiet.windows(5).any(|window| window == b"shown"));
    }

    #[test]
    fn a_hidden_move_strips_a_face_the_table_had_seen_and_forgets_its_peeks() {
        let mut state = zoned_state(vec![
            decl(0, "hand", ZoneOwner::PerSeat, ZoneVisibility::Owner),
            kinded(3, "field", ZoneKind::Battlefield, ZoneOwner::Shared),
        ]);
        step(&mut state, 1, LogAction::Join { name: "b".into() }).unwrap();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9, 10],
                to: Zone::Plugin(0),
            },
        )
        .unwrap();
        step(
            &mut state,
            1,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        choreographed(&mut state, 1, vec![Effect::Peek { card: 10, seat: 0 }]).unwrap();
        let plain = LogEntry::new(
            state.next_seq,
            1,
            LogAction::Move {
                card: 9,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
                hidden: false,
            },
        );
        let hide = LogEntry::new(
            state.next_seq,
            1,
            LogAction::Move {
                card: 9,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
                hidden: true,
            },
        );
        assert!(
            !encode_entry(&plain)
                .windows(6)
                .any(|window| window == b"hidden"),
            "a plain move encodes as it always did"
        );
        assert_eq!(decode_entry(&encode_entry(&hide)), Some(hide.clone()));
        assert_eq!(
            validate(
                &state,
                &LogEntry::new(state.next_seq, 0, hide.action.clone())
            ),
            Err(FoldError::ForeignHand),
            "only the card's own seat may hide it"
        );
        fold_entry(&mut state, &hide).unwrap();
        assert!(!state.revealed.contains(&9));
        assert!(!state.shown_in_place(9));
        assert!(state.table.get(CardId(9)).unwrap().face.is_hidden());
        assert_eq!(state.table.get(CardId(9)).unwrap().zone, Zone::Plugin(3));
        assert!(
            !crate::view::table_view(&state, 0).revealed.contains(&9),
            "no viewer is told the face is public"
        );
        assert!(state.peeked_by(10, 0));
        step(
            &mut state,
            1,
            LogAction::Move {
                card: 10,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
                hidden: true,
            },
        )
        .unwrap();
        assert!(!state.peeked_by(10, 0), "a hide forgets the peeks too");
        assert!(state.owed_reveals.is_empty());
    }

    #[test]
    fn a_peek_effect_owes_the_face_to_one_seat_only() {
        let mut state = play_table();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9, 10],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        choreographed(&mut state, 1, vec![Effect::Peek { card: 9, seat: 0 }]).unwrap();
        assert_eq!(state.peeks, BTreeSet::from([(9, 0)]));
        assert!(state.peeked_by(9, 0));
        assert!(!state.peeked_by(9, 1));
        assert_eq!(state.peeks_of(0).collect::<Vec<u32>>(), [9]);
        assert!(state.peeks_of(1).next().is_none());
        let round_trip = decode_state(&encode_state(&state)).unwrap();
        assert_eq!(round_trip.peeks, BTreeSet::from([(9, 0)]));
        assert_eq!(
            choreographed(&mut state, 1, vec![Effect::Peek { card: 9, seat: 5 }]),
            Err(FoldError::BadEffect { index: 0 })
        );
        assert_eq!(
            choreographed(&mut state, 1, vec![Effect::Peek { card: 77, seat: 0 }]),
            Err(FoldError::BadEffect { index: 0 })
        );
        choreographed(
            &mut state,
            1,
            vec![Effect::Move {
                card: 9,
                to: Zone::Plugin(0),
                seat: 1,
                index: 0,
            }],
        )
        .unwrap();
        assert!(
            state.peeked_by(9, 0),
            "a face once seen stays seen while the card is on the table"
        );
        choreographed(
            &mut state,
            1,
            vec![Effect::Move {
                card: 9,
                to: Zone::Plugin(3),
                seat: 0,
                index: 0,
            }],
        )
        .unwrap();
        step(
            &mut state,
            0,
            LogAction::Reveal {
                card: 9,
                face: CardFace::named("Seer"),
            },
        )
        .unwrap();
        assert!(state.peeks.is_empty(), "a public face needs no peek");
        choreographed(&mut state, 1, vec![Effect::Peek { card: 9, seat: 1 }]).unwrap();
        assert!(state.peeks.is_empty());
        choreographed(&mut state, 1, vec![Effect::Peek { card: 10, seat: 0 }]).unwrap();
        step(&mut state, 1, LogAction::Clear { seat: 1 }).unwrap();
        assert!(
            state.peeks.is_empty(),
            "a swept card takes its peeks with it"
        );
    }

    #[test]
    fn the_information_effects_round_trip_and_skip_unknown_keys() {
        for effect in [
            Effect::Reveal { card: 4 },
            Effect::Peek { card: 4, seat: 1 },
            Effect::Conceal { card: 4 },
        ] {
            assert_eq!(decode::<Effect>(&encode(&effect)).unwrap(), effect);
        }
        let padded = ciborium::Value::Map(vec![(
            ciborium::Value::Text("Peek".into()),
            ciborium::Value::Map(vec![
                (
                    ciborium::Value::Text("card".into()),
                    ciborium::Value::Integer(4.into()),
                ),
                (
                    ciborium::Value::Text("seat".into()),
                    ciborium::Value::Integer(1.into()),
                ),
                (
                    ciborium::Value::Text("later".into()),
                    ciborium::Value::Text("ignored".into()),
                ),
            ]),
        )]);
        assert_eq!(
            decode::<Effect>(&encode(&padded)).unwrap(),
            Effect::Peek { card: 4, seat: 1 }
        );
        let padded = ciborium::Value::Map(vec![(
            ciborium::Value::Text("Reveal".into()),
            ciborium::Value::Map(vec![
                (
                    ciborium::Value::Text("card".into()),
                    ciborium::Value::Integer(4.into()),
                ),
                (
                    ciborium::Value::Text("to".into()),
                    ciborium::Value::Integer(2.into()),
                ),
            ]),
        )]);
        assert_eq!(
            decode::<Effect>(&encode(&padded)).unwrap(),
            Effect::Reveal { card: 4 }
        );
        let verdict = Verdict::accept().with_effects(vec![Effect::Peek { card: 4, seat: 1 }]);
        assert_eq!(decode::<Verdict>(&encode(&verdict)).unwrap(), verdict);
    }

    #[test]
    fn concealing_a_card_revokes_peeks_without_moving_or_removing_it() {
        let mut state = play_table();
        step(
            &mut state,
            1,
            LogAction::Deal {
                cards: vec![9, 10],
                to: Zone::Plugin(1),
            },
        )
        .unwrap();
        choreographed(&mut state, 1, vec![Effect::Peek { card: 9, seat: 0 }]).unwrap();
        state.owed_reveals.insert(9);
        let before = state.table.clone();
        choreographed(&mut state, 1, vec![Effect::Conceal { card: 9 }]).unwrap();
        assert_eq!(state.table, before);
        assert!(state.peeks.is_empty());
        assert!(state.owed_reveals.is_empty());
        assert!(!state.revealed.contains(&9));
        assert_eq!(decode_state(&encode_state(&state)).unwrap(), state);
    }
}

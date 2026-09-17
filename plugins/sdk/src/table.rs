use crate::cbor::{Item, Reader, Writer};
use crate::decide::{Action, Effect};

pub const BOARD: u16 = u16::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Face {
    pub name: String,
    pub kind: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
}

impl Face {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn with_might(mut self, might: Option<u8>) -> Self {
        self.might = might;
        self
    }

    pub fn with_cost(mut self, energy: Option<u8>, power: Option<u8>) -> Self {
        self.energy = energy;
        self.power = power;
        self
    }

    pub fn with_domain(mut self, domain: Vec<String>) -> Self {
        self.domain = domain;
        self
    }

    pub fn is_hidden(&self) -> bool {
        self.name.is_empty()
    }

    pub(crate) fn read(reader: &mut Reader) -> Option<Self> {
        let mut face = Face::default();
        for _ in 0..reader.map_len()? {
            match reader.key()? {
                "name" => face.name = text(reader)?,
                "kind" => face.kind = optional_text(reader)?,
                "energy" => face.energy = optional_small(reader)?,
                "power" => face.power = optional_small(reader)?,
                "might" => face.might = optional_small(reader)?,
                "domain" => {
                    for _ in 0..reader.array_len()? {
                        face.domain.push(text(reader)?);
                    }
                }
                _ => reader.skip()?,
            }
        }
        Some(face)
    }

    pub(crate) fn write(&self, writer: &mut Writer) {
        let extras = self.kind.is_some() as usize
            + self.energy.is_some() as usize
            + self.power.is_some() as usize
            + self.might.is_some() as usize
            + (!self.domain.is_empty()) as usize;
        writer.map(3 + extras);
        writer.text("name");
        writer.text(&self.name);
        writer.text("tint");
        writer.array(3);
        for _ in 0..3 {
            writer.unsigned(128);
        }
        writer.text("foil");
        writer.bool(false);
        if let Some(kind) = &self.kind {
            writer.text("kind");
            writer.text(kind);
        }
        if let Some(energy) = self.energy {
            writer.text("energy");
            writer.unsigned(u64::from(energy));
        }
        if let Some(power) = self.power {
            writer.text("power");
            writer.unsigned(u64::from(power));
        }
        if let Some(might) = self.might {
            writer.text("might");
            writer.unsigned(u64::from(might));
        }
        if !self.domain.is_empty() {
            writer.text("domain");
            writer.array(self.domain.len());
            for domain in &self.domain {
                writer.text(domain);
            }
        }
    }
}

pub(crate) fn write_zone(writer: &mut Writer, zone: u16) {
    if zone == BOARD {
        writer.text("Board");
        return;
    }
    writer.map(1);
    writer.text("Plugin");
    writer.unsigned(u64::from(zone));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneKind {
    Hand,
    Deck,
    Discard,
    Stack,
    Battlefield,
    #[default]
    Aux,
}

impl ZoneKind {
    fn parse(text: &str) -> Self {
        match text {
            "Hand" => ZoneKind::Hand,
            "Deck" => ZoneKind::Deck,
            "Discard" => ZoneKind::Discard,
            "Stack" => ZoneKind::Stack,
            "Battlefield" => ZoneKind::Battlefield,
            _ => ZoneKind::Aux,
        }
    }

    pub fn sheds_state(self) -> bool {
        matches!(self, ZoneKind::Hand | ZoneKind::Deck | ZoneKind::Discard)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoneVisibility {
    #[default]
    All,
    Owner,
    None,
}

impl ZoneVisibility {
    fn parse(text: &str) -> Self {
        match text {
            "Owner" => ZoneVisibility::Owner,
            "None" => ZoneVisibility::None,
            _ => ZoneVisibility::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ZoneSummary {
    pub id: u16,
    pub name: String,
    pub kind: ZoneKind,
    pub shared: bool,
    pub battlefield: bool,
    pub label: String,
    pub visibility: ZoneVisibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardInfo {
    pub id: u32,
    pub zone: Option<u16>,
    pub seat: u8,
    pub owner: u8,
    pub name: String,
    pub kind: Option<String>,
    pub energy: Option<u8>,
    pub power: Option<u8>,
    pub might: Option<u8>,
    pub domain: Vec<String>,
    pub exhausted: bool,
    pub annotations: Vec<(String, Vec<u8>)>,
}

impl CardInfo {
    pub fn is_hidden(&self) -> bool {
        self.name.is_empty()
    }

    pub fn is_kind(&self, kind: &str) -> bool {
        self.kind.as_deref() == Some(kind)
    }

    pub fn annotation(&self, key: &str) -> Option<&[u8]> {
        self.annotations
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, value)| value.as_slice())
    }

    fn annotate(&mut self, key: &str, value: Option<&[u8]>) {
        match value {
            Some(value) => match self
                .annotations
                .binary_search_by(|(held, _)| held.as_str().cmp(key))
            {
                Ok(at) => self.annotations[at].1 = value.to_vec(),
                Err(at) => self
                    .annotations
                    .insert(at, (key.to_string(), value.to_vec())),
            },
            None => self.annotations.retain(|(held, _)| held != key),
        }
        self.exhausted = self.annotation("exhausted").is_some();
    }

    pub fn face(&self) -> Face {
        Face {
            name: self.name.clone(),
            kind: self.kind.clone(),
            energy: self.energy,
            power: self.power,
            might: self.might,
            domain: self.domain.clone(),
        }
    }

    fn set_face(&mut self, face: &Face) {
        self.name = face.name.clone();
        self.kind = face.kind.clone();
        self.energy = face.energy;
        self.power = face.power;
        self.might = face.might;
        self.domain = face.domain.clone();
    }

    fn blank(&mut self) {
        self.set_face(&Face::default());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Table,
    Seat(u8),
    Card(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CounterInfo {
    pub target: Target,
    pub counter: u16,
    pub value: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CounterScope {
    #[default]
    Seat,
    Card,
    Table,
}

impl CounterScope {
    fn parse(text: &str) -> Self {
        match text {
            "Card" => CounterScope::Card,
            "Table" => CounterScope::Table,
            _ => CounterScope::Seat,
        }
    }

    pub fn accepts(self, target: Target) -> bool {
        matches!(
            (self, target),
            (CounterScope::Seat, Target::Seat(_))
                | (CounterScope::Card, Target::Card(_))
                | (CounterScope::Table, Target::Table)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CounterBounds {
    pub id: u16,
    pub scope: CounterScope,
    pub start: i32,
    pub min: Option<i32>,
    pub max: Option<i32>,
}

impl CounterBounds {
    pub fn clamp(&self, value: i32) -> i32 {
        let value = match self.min {
            Some(min) => value.max(min),
            None => value,
        };
        match self.max {
            Some(max) => value.min(max),
            None => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    UnknownCard,
    UnknownZone,
    UnknownSeat,
    UnknownCounter,
    CounterScope,
    HiddenFace,
    NotAToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub players: u8,
    pub zones: Vec<ZoneSummary>,
    pub cards: Vec<CardInfo>,
    pub counters: Vec<CounterInfo>,
    pub next_id: u32,
    pub revealed: Vec<u32>,
    pub tokens: Vec<u32>,
    pub counter_table: Vec<CounterBounds>,
    pub options: Vec<(String, i64)>,
}

struct ZoneFacts {
    shared: bool,
    visibility: ZoneVisibility,
    kind: ZoneKind,
}

impl Snapshot {
    pub fn zone(&self, id: u16) -> Option<&ZoneSummary> {
        self.zones.iter().find(|zone| zone.id == id)
    }

    pub fn zone_named(&self, name: &str) -> Option<&ZoneSummary> {
        self.zones.iter().find(|zone| zone.name == name)
    }

    pub fn in_zone(&self, zone: u16) -> impl Iterator<Item = &CardInfo> {
        self.cards
            .iter()
            .filter(move |card| card.zone == Some(zone))
    }

    pub fn held(&self, zone: u16, seat: u8) -> impl Iterator<Item = &CardInfo> {
        self.in_zone(zone).filter(move |card| card.seat == seat)
    }

    pub fn owned_at(&self, zone: u16, owner: u8) -> impl Iterator<Item = &CardInfo> {
        self.in_zone(zone).filter(move |card| card.owner == owner)
    }

    pub fn card(&self, id: u32) -> Option<&CardInfo> {
        self.cards.iter().find(|card| card.id == id)
    }

    pub fn card_mut(&mut self, id: u32) -> Option<&mut CardInfo> {
        self.cards.iter_mut().find(|card| card.id == id)
    }

    pub fn counter(&self, target: Target, counter: u16) -> Option<i32> {
        self.counters
            .iter()
            .find(|held| held.target == target && held.counter == counter)
            .map(|held| held.value)
    }

    pub fn counter_bounds(&self, counter: u16) -> Option<&CounterBounds> {
        self.counter_table.iter().find(|decl| decl.id == counter)
    }

    pub fn option(&self, key: &str) -> Option<i64> {
        self.options
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, value)| *value)
    }

    pub fn is_token(&self, card: u32) -> bool {
        self.tokens.contains(&card)
    }

    pub fn is_revealed(&self, card: u32) -> bool {
        self.revealed.contains(&card)
    }

    fn facts(&self, zone: Option<u16>) -> Result<ZoneFacts, ApplyError> {
        match zone {
            Some(BOARD) => Ok(ZoneFacts {
                shared: false,
                visibility: ZoneVisibility::All,
                kind: ZoneKind::Aux,
            }),
            Some(id) => {
                let zone = self.zone(id).ok_or(ApplyError::UnknownZone)?;
                Ok(ZoneFacts {
                    shared: zone.shared,
                    visibility: zone.visibility,
                    kind: zone.kind,
                })
            }
            None => Ok(ZoneFacts {
                shared: false,
                visibility: ZoneVisibility::Owner,
                kind: ZoneKind::Hand,
            }),
        }
    }

    fn insertion_point(&self, seat: u8, zone: Option<u16>, index: usize) -> usize {
        let mut ordinal = 0;
        for (at, card) in self.cards.iter().enumerate() {
            if card.seat == seat && card.zone == zone {
                if ordinal == index {
                    return at;
                }
                ordinal += 1;
            }
        }
        self.cards.len()
    }

    fn insertion_point_without(
        &self,
        without: usize,
        seat: u8,
        zone: Option<u16>,
        index: usize,
    ) -> usize {
        let mut ordinal = 0;
        for (at, card) in self.cards.iter().enumerate() {
            if at == without {
                continue;
            }
            if card.seat == seat && card.zone == zone {
                if ordinal == index {
                    return if at > without { at - 1 } else { at };
                }
                ordinal += 1;
            }
        }
        self.cards.len() - 1
    }

    fn relocate(
        &mut self,
        card: u32,
        zone: Option<u16>,
        seat: u8,
        index: u32,
    ) -> Result<bool, ApplyError> {
        let from = self
            .cards
            .iter()
            .position(|held| held.id == card)
            .ok_or(ApplyError::UnknownCard)?;
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let held = &self.cards[from];
        let changes = held.zone != zone
            || held.seat != seat
            || self.insertion_point_without(from, seat, zone, index) != from;
        if !changes {
            return Ok(false);
        }
        let mut moving = self.cards.remove(from);
        moving.zone = zone;
        moving.seat = seat;
        let at = self.insertion_point(seat, zone, index);
        self.cards.insert(at, moving);
        Ok(true)
    }

    fn set_sorted(list: &mut Vec<u32>, value: u32) {
        if let Err(at) = list.binary_search(&value) {
            list.insert(at, value);
        }
    }

    fn unset_sorted(list: &mut Vec<u32>, value: u32) -> bool {
        match list.binary_search(&value) {
            Ok(at) => {
                list.remove(at);
                true
            }
            Err(_) => false,
        }
    }

    fn shed(&mut self, card: u32) {
        if let Some(held) = self.card_mut(card) {
            held.annotations.clear();
            held.exhausted = false;
        }
        self.counters
            .retain(|held| held.target != Target::Card(card));
        if Self::unset_sorted(&mut self.tokens, card) {
            self.cards.retain(|held| held.id != card);
            Self::unset_sorted(&mut self.revealed, card);
        }
    }

    fn arrive(&mut self, card: u32, zone: Option<u16>) -> Result<(), ApplyError> {
        let facts = self.facts(zone)?;
        if facts.visibility == ZoneVisibility::None && Self::unset_sorted(&mut self.revealed, card)
        {
            if let Some(held) = self.card_mut(card) {
                held.blank();
            }
        }
        if facts.kind.sheds_state() {
            self.shed(card);
        }
        Ok(())
    }

    fn move_card(
        &mut self,
        card: u32,
        zone: Option<u16>,
        seat: u8,
        index: u32,
        always: bool,
    ) -> Result<(), ApplyError> {
        if self.card(card).is_none() {
            return Err(ApplyError::UnknownCard);
        }
        let facts = self.facts(zone)?;
        let target_seat = if facts.shared { 0 } else { seat };
        let moved = self.relocate(card, zone, target_seat, index)?;
        if !moved && !always {
            return Ok(());
        }
        self.arrive(card, zone)
    }

    fn annotate(&mut self, card: u32, key: &str, value: Option<&[u8]>) -> Result<(), ApplyError> {
        let held = self.card_mut(card).ok_or(ApplyError::UnknownCard)?;
        held.annotate(key, value);
        Ok(())
    }

    fn count(&mut self, target: Target, counter: u16, delta: i32) -> Result<(), ApplyError> {
        let decl = *self
            .counter_bounds(counter)
            .ok_or(ApplyError::UnknownCounter)?;
        if !decl.scope.accepts(target) {
            return Err(ApplyError::CounterScope);
        }
        match target {
            Target::Seat(seat) if seat >= self.players => return Err(ApplyError::UnknownSeat),
            Target::Card(card) if self.card(card).is_none() => return Err(ApplyError::UnknownCard),
            _ => {}
        }
        let current = self.counter(target, counter).unwrap_or(decl.start);
        let next = decl.clamp(current.saturating_add(delta));
        match self
            .counters
            .iter_mut()
            .find(|held| held.target == target && held.counter == counter)
        {
            Some(held) => held.value = next,
            None => {
                self.counters.push(CounterInfo {
                    target,
                    counter,
                    value: next,
                });
                self.counters.sort();
            }
        }
        Ok(())
    }

    fn spawn(
        &mut self,
        face: &Face,
        zone: Option<u16>,
        seat: u8,
        owner: u8,
    ) -> Result<u32, ApplyError> {
        if face.is_hidden() {
            return Err(ApplyError::HiddenFace);
        }
        let facts = self.facts(zone)?;
        let target_seat = if facts.shared { 0 } else { seat };
        let id = self.next_id;
        self.next_id += 1;
        let mut minted = CardInfo {
            id,
            zone,
            seat: target_seat,
            owner,
            ..CardInfo::default()
        };
        minted.set_face(face);
        self.cards.push(minted);
        if facts.visibility != ZoneVisibility::None {
            Self::set_sorted(&mut self.revealed, id);
        }
        Self::set_sorted(&mut self.tokens, id);
        Ok(id)
    }

    fn despawn(&mut self, card: u32) -> Result<(), ApplyError> {
        if self.card(card).is_none() {
            return Err(ApplyError::UnknownCard);
        }
        if !self.is_token(card) {
            return Err(ApplyError::NotAToken);
        }
        self.cards.retain(|held| held.id != card);
        Self::unset_sorted(&mut self.revealed, card);
        Self::unset_sorted(&mut self.tokens, card);
        self.shed(card);
        Ok(())
    }

    pub fn apply_entry(&mut self, action: &Action, seat: u8) -> Result<(), ApplyError> {
        match action {
            Action::Move {
                card,
                to,
                seat: to_seat,
                index,
                hidden,
            } => {
                self.move_card(*card, *to, *to_seat, *index, true)?;
                if *hidden {
                    Self::unset_sorted(&mut self.revealed, *card);
                    if let Some(held) = self.card_mut(*card) {
                        held.blank();
                    }
                }
                Ok(())
            }
            Action::Spawn {
                face,
                zone,
                seat: to_seat,
            } => self.spawn(face, *zone, *to_seat, seat).map(|_| ()),
            Action::Reveal { card, face } => {
                let held = self.card_mut(*card).ok_or(ApplyError::UnknownCard)?;
                held.set_face(face);
                Self::set_sorted(&mut self.revealed, *card);
                Ok(())
            }
            Action::Annotate { card, key, value } => self.annotate(*card, key, value.as_deref()),
            Action::Counter {
                target,
                counter,
                delta,
            } => self.count(*target, *counter, *delta),
            Action::Reset => {
                self.cards.clear();
                self.next_id = 0;
                self.revealed.clear();
                self.tokens.clear();
                self.counters.clear();
                Ok(())
            }
            Action::Clear { seat: swept } => {
                let gone: Vec<u32> = self
                    .cards
                    .iter()
                    .filter(|card| card.owner == *swept)
                    .map(|card| card.id)
                    .collect();
                self.cards.retain(|card| card.owner != *swept);
                for card in gone {
                    Self::unset_sorted(&mut self.revealed, card);
                    Self::unset_sorted(&mut self.tokens, card);
                    self.counters
                        .retain(|held| held.target != Target::Card(card));
                }
                self.counters
                    .retain(|held| held.target != Target::Seat(*swept));
                Ok(())
            }
            Action::Game(_) | Action::Deal { .. } | Action::Join | Action::Other => Ok(()),
        }
    }

    pub fn apply(&mut self, effect: &Effect, actor: u8) -> Result<(), ApplyError> {
        match effect {
            Effect::Transform { card, face } => {
                let held = self.card_mut(*card).ok_or(ApplyError::UnknownCard)?;
                if held.is_hidden() || face.is_hidden() {
                    return Err(ApplyError::HiddenFace);
                }
                held.set_face(face);
                Ok(())
            }
            Effect::Move {
                card,
                zone,
                seat,
                index,
            } => self.move_card(*card, Some(*zone), *seat, *index, false),
            Effect::Annotate { card, key, value } => self.annotate(*card, key, value.as_deref()),
            Effect::Counter {
                target,
                counter,
                delta,
            } => self.count(*target, *counter, *delta),
            Effect::Spawn {
                face,
                zone,
                seat,
                owner,
            } => self
                .spawn(face, Some(*zone), *seat, owner.unwrap_or(actor))
                .map(|_| ()),
            Effect::Despawn { card } => self.despawn(*card),
            Effect::Reveal { card } => self.card(*card).map(|_| ()).ok_or(ApplyError::UnknownCard),
            Effect::Peek { card, seat } => {
                if self.card(*card).is_none() {
                    return Err(ApplyError::UnknownCard);
                }
                if *seat >= self.players {
                    return Err(ApplyError::UnknownSeat);
                }
                Ok(())
            }
            Effect::Conceal { card } => {
                self.card_mut(*card).ok_or(ApplyError::UnknownCard)?.blank();
                self.revealed.retain(|held| held != card);
                Ok(())
            }
        }
    }

    pub fn apply_all(&mut self, effects: &[Effect], actor: u8) -> Result<(), (usize, ApplyError)> {
        for (index, effect) in effects.iter().enumerate() {
            self.apply(effect, actor).map_err(|error| (index, error))?;
        }
        Ok(())
    }
}

pub(crate) fn parse_state(reader: &mut Reader) -> Option<Snapshot> {
    let mut snapshot = Snapshot::default();
    let mut annotations = Vec::new();
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "seats" => snapshot.players = u8::try_from(crate::decide::skip_array(reader)?).ok()?,
            "zones" => {
                for _ in 0..reader.array_len()? {
                    snapshot.zones.push(zone(reader)?);
                }
            }
            "table" => {
                let (cards, next_id) = table(reader)?;
                snapshot.cards = cards;
                snapshot.next_id = next_id;
            }
            "annotations" => annotations = annotated(reader)?,
            "counters" => {
                for _ in 0..reader.array_len()? {
                    snapshot.counters.push(counter(reader)?);
                }
            }
            "counter_table" => {
                for _ in 0..reader.array_len()? {
                    snapshot.counter_table.push(bounds(reader)?);
                }
            }
            "revealed" => snapshot.revealed = ids(reader)?,
            "tokens" => snapshot.tokens = ids(reader)?,
            "options" => snapshot.options = options(reader)?,
            _ => reader.skip()?,
        }
    }
    for (card, keys) in annotations {
        if let Some(held) = snapshot.card_mut(card) {
            held.annotations = keys;
            held.exhausted = held.annotation("exhausted").is_some();
        }
    }
    snapshot.revealed.sort_unstable();
    snapshot.tokens.sort_unstable();
    Some(snapshot)
}

fn options(reader: &mut Reader) -> Option<Vec<(String, i64)>> {
    let bytes = match reader.item()? {
        Item::Bytes(bytes) => bytes,
        Item::Simple(22) => return Some(Vec::new()),
        _ => return None,
    };
    let mut inner = Reader::new(bytes);
    let mut options = Vec::new();
    for _ in 0..inner.map_len()? {
        let key = inner.key()?.to_string();
        match inner.item()? {
            Item::Unsigned(value) => options.push((key, i64::try_from(value).ok()?)),
            Item::Negative(value) => options.push((key, -1 - i64::try_from(value).ok()?)),
            _ => return None,
        }
    }
    Some(options)
}

fn ids(reader: &mut Reader) -> Option<Vec<u32>> {
    let mut ids = Vec::new();
    for _ in 0..reader.array_len()? {
        ids.push(u32::try_from(reader.unsigned()?).ok()?);
    }
    Some(ids)
}

fn zone(reader: &mut Reader) -> Option<ZoneSummary> {
    let mut summary = ZoneSummary::default();
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "id" => summary.id = u16::try_from(reader.unsigned()?).ok()?,
            "name" => summary.name = text(reader)?,
            "kind" => {
                summary.kind = ZoneKind::parse(&text(reader)?);
                summary.battlefield = summary.kind == ZoneKind::Battlefield;
            }
            "owner" => summary.shared = matches!(reader.item()?, Item::Text("Shared")),
            "visibility" => summary.visibility = ZoneVisibility::parse(&text(reader)?),
            "label" => summary.label = text(reader)?,
            _ => reader.skip()?,
        }
    }
    Some(summary)
}

fn table(reader: &mut Reader) -> Option<(Vec<CardInfo>, u32)> {
    let mut cards = Vec::new();
    let mut next_id = 0;
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "cards" => {
                for _ in 0..reader.array_len()? {
                    cards.push(card(reader)?);
                }
            }
            "next_id" => next_id = u32::try_from(reader.unsigned()?).ok()?,
            _ => reader.skip()?,
        }
    }
    Some((cards, next_id))
}

fn card(reader: &mut Reader) -> Option<CardInfo> {
    let mut info = CardInfo::default();
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "id" => info.id = u32::try_from(reader.unsigned()?).ok()?,
            "owner" => info.owner = u8::try_from(reader.unsigned()?).ok()?,
            "seat" => info.seat = u8::try_from(reader.unsigned()?).ok()?,
            "zone" => info.zone = zone_id(reader)?,
            "face" => face(reader, &mut info)?,
            _ => reader.skip()?,
        }
    }
    Some(info)
}

pub(crate) fn zone_id(reader: &mut Reader) -> Option<Option<u16>> {
    match reader.item()? {
        Item::Text("Hand") => Some(None),
        Item::Text("Board") => Some(Some(BOARD)),
        Item::Map(1) => {
            let key = reader.key()?;
            let id = reader.unsigned()?;
            Some((key == "Plugin").then(|| u16::try_from(id).ok()).flatten())
        }
        _ => None,
    }
}

pub(crate) fn face(reader: &mut Reader, info: &mut CardInfo) -> Option<()> {
    info.set_face(&Face::read(reader)?);
    Some(())
}

pub(crate) fn private_faces(reader: &mut Reader) -> Option<Vec<(u32, Face)>> {
    let mut faces = Vec::new();
    for _ in 0..reader.map_len()? {
        let Item::Unsigned(card) = reader.item()? else {
            return None;
        };
        faces.push((u32::try_from(card).ok()?, Face::read(reader)?));
    }
    Some(faces)
}

pub(crate) fn unveil(snapshot: &mut Snapshot, faces: &[(u32, Face)]) {
    for (card, face) in faces {
        if face.is_hidden() {
            continue;
        }
        if let Some(held) = snapshot.card_mut(*card) {
            if held.is_hidden() {
                held.set_face(face);
            }
        }
    }
}

type Annotated = Vec<(u32, Vec<(String, Vec<u8>)>)>;

fn annotated(reader: &mut Reader) -> Option<Annotated> {
    let mut marked = Vec::new();
    for _ in 0..reader.map_len()? {
        let Item::Unsigned(card) = reader.item()? else {
            return None;
        };
        let mut keys = Vec::new();
        for _ in 0..reader.map_len()? {
            let key = reader.key()?.to_string();
            let value = reader.bytes()?.to_vec();
            keys.push((key, value));
        }
        keys.sort();
        marked.push((u32::try_from(card).ok()?, keys));
    }
    Some(marked)
}

pub(crate) fn target(reader: &mut Reader) -> Option<Target> {
    match reader.item()? {
        Item::Text(_) => Some(Target::Table),
        Item::Map(1) => match reader.key()? {
            "Seat" => Some(Target::Seat(u8::try_from(reader.unsigned()?).ok()?)),
            "Card" => Some(Target::Card(u32::try_from(reader.unsigned()?).ok()?)),
            _ => None,
        },
        _ => None,
    }
}

fn counter(reader: &mut Reader) -> Option<CounterInfo> {
    let mut held = Target::Table;
    let mut id = 0;
    let mut value = 0;
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "target" => held = target(reader)?,
            "counter" => id = u16::try_from(reader.unsigned()?).ok()?,
            "value" => value = signed(reader)?,
            _ => reader.skip()?,
        }
    }
    Some(CounterInfo {
        target: held,
        counter: id,
        value,
    })
}

fn bounds(reader: &mut Reader) -> Option<CounterBounds> {
    let mut decl = CounterBounds::default();
    for _ in 0..reader.map_len()? {
        match reader.key()? {
            "id" => decl.id = u16::try_from(reader.unsigned()?).ok()?,
            "scope" => decl.scope = CounterScope::parse(&text(reader)?),
            "start" => decl.start = signed(reader)?,
            "min" => decl.min = optional_signed(reader)?,
            "max" => decl.max = optional_signed(reader)?,
            _ => reader.skip()?,
        }
    }
    Some(decl)
}

pub(crate) fn signed(reader: &mut Reader) -> Option<i32> {
    match reader.item()? {
        Item::Unsigned(value) => i32::try_from(value).ok(),
        Item::Negative(value) => i32::try_from(-1 - i64::try_from(value).ok()?).ok(),
        _ => None,
    }
}

fn optional_signed(reader: &mut Reader) -> Option<Option<i32>> {
    match reader.item()? {
        Item::Unsigned(value) => Some(i32::try_from(value).ok()),
        Item::Negative(value) => Some(i32::try_from(-1 - i64::try_from(value).ok()?).ok()),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

pub(crate) fn text(reader: &mut Reader) -> Option<String> {
    match reader.item()? {
        Item::Text(text) => Some(text.to_string()),
        _ => None,
    }
}

fn optional_text(reader: &mut Reader) -> Option<Option<String>> {
    match reader.item()? {
        Item::Text(text) => Some(Some(text.to_string())),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

fn optional_small(reader: &mut Reader) -> Option<Option<u8>> {
    match reader.item()? {
        Item::Unsigned(value) => Some(u8::try_from(value).ok()),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::cbor::Writer;

    pub struct Placed<'a> {
        pub id: u64,
        pub zone: u64,
        pub seat: u64,
        pub name: &'a str,
        pub kind: Option<&'a str>,
        pub energy: Option<u64>,
        pub power: Option<u64>,
        pub domain: &'a [&'a str],
    }

    pub fn placed(id: u64, zone: u64, seat: u64, name: &str) -> Placed<'_> {
        Placed {
            id,
            zone,
            seat,
            name,
            kind: None,
            energy: None,
            power: None,
            domain: &[],
        }
    }

    pub fn cards(writer: &mut Writer, cards: &[Placed]) {
        writer.text("table");
        writer.map(2);
        writer.text("cards");
        writer.array(cards.len());
        for card in cards {
            writer.map(5);
            writer.text("id");
            writer.unsigned(card.id);
            writer.text("owner");
            writer.unsigned(card.seat);
            writer.text("seat");
            writer.unsigned(card.seat);
            writer.text("zone");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(card.zone);
            writer.text("face");
            let extras = card.kind.is_some() as usize
                + card.energy.is_some() as usize
                + card.power.is_some() as usize
                + (!card.domain.is_empty()) as usize;
            writer.map(3 + extras);
            writer.text("name");
            writer.text(card.name);
            writer.text("tint");
            writer.array(3);
            for _ in 0..3 {
                writer.unsigned(128);
            }
            writer.text("foil");
            writer.bool(false);
            if let Some(kind) = card.kind {
                writer.text("kind");
                writer.text(kind);
            }
            if let Some(energy) = card.energy {
                writer.text("energy");
                writer.unsigned(energy);
            }
            if let Some(power) = card.power {
                writer.text("power");
                writer.unsigned(power);
            }
            if !card.domain.is_empty() {
                writer.text("domain");
                writer.array(card.domain.len());
                for domain in card.domain {
                    writer.text(domain);
                }
            }
        }
        writer.text("next_id");
        writer.unsigned(cards.iter().map(|card| card.id + 1).max().unwrap_or(0));
    }

    pub fn exhausted(writer: &mut Writer, cards: &[u64]) {
        writer.text("annotations");
        writer.map(cards.len());
        for card in cards {
            writer.unsigned(*card);
            writer.map(1);
            writer.text("exhausted");
            writer.bytes(&[1]);
        }
    }

    pub fn seat_counter(writer: &mut Writer, seat: u64, counter: u64, value: i64) {
        writer.map(3);
        writer.text("target");
        writer.map(1);
        writer.text("Seat");
        writer.unsigned(seat);
        writer.text("counter");
        writer.unsigned(counter);
        writer.text("value");
        writer.signed(value);
    }

    pub fn counter_decl(writer: &mut Writer, id: u64, scope: &str, start: i64, max: Option<i64>) {
        writer.map(5);
        writer.text("id");
        writer.unsigned(id);
        writer.text("scope");
        writer.text(scope);
        writer.text("start");
        writer.signed(start);
        writer.text("min");
        writer.signed(0);
        writer.text("max");
        match max {
            Some(max) => writer.signed(max),
            None => writer.null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::cbor::Writer;
    use crate::decide::fixtures::seats;
    use crate::decide::{BOTTOM, TOP};

    #[test]
    fn a_state_yields_its_cards_marks_and_counters() {
        let mut writer = Writer::new();
        writer.map(8);
        seats(&mut writer, 2);
        writer.text("zones");
        writer.array(1);
        crate::view::fixtures::zone_seen(&mut writer, 7, "Deck", "PerSeat", "None", "Rune deck");
        cards(
            &mut writer,
            &[
                placed(3, 7, 0, "Fury Rune"),
                Placed {
                    kind: Some("Unit"),
                    energy: Some(4),
                    power: Some(1),
                    domain: &["Fury"],
                    ..placed(4, 9, 1, "Noxus Hopeful")
                },
                placed(5, 9, 1, ""),
            ],
        );
        writer.text("annotations");
        writer.map(1);
        writer.unsigned(3);
        writer.map(2);
        writer.text("hidden");
        writer.bytes(&[9, 0]);
        writer.text("exhausted");
        writer.bytes(&[1]);
        writer.text("counters");
        writer.array(1);
        seat_counter(&mut writer, 1, 0, -2);
        writer.text("counter_table");
        writer.array(1);
        counter_decl(&mut writer, 0, "Seat", 0, Some(8));
        writer.text("revealed");
        writer.array(2);
        writer.unsigned(4);
        writer.unsigned(3);
        writer.text("tokens");
        writer.array(1);
        writer.unsigned(5);
        let bytes = writer.finish();
        let snapshot = parse_state(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(snapshot.players, 2);
        assert_eq!(snapshot.next_id, 6);
        assert_eq!(snapshot.zone(7).unwrap().kind, ZoneKind::Deck);
        assert_eq!(snapshot.zone(7).unwrap().visibility, ZoneVisibility::None);
        assert_eq!(snapshot.zone_named("n").unwrap().id, 7);
        let rune = snapshot.card(3).unwrap();
        assert!(rune.exhausted);
        assert_eq!(rune.zone, Some(7));
        assert_eq!(rune.annotation("hidden"), Some(&[9u8, 0][..]));
        assert_eq!(rune.annotations.len(), 2);
        assert_eq!(rune.annotations[0].0, "exhausted");
        let unit = snapshot.card(4).unwrap();
        assert!(!unit.exhausted);
        assert!(unit.annotations.is_empty());
        assert!(unit.is_kind("Unit"));
        assert_eq!(
            (unit.energy, unit.power, unit.might),
            (Some(4), Some(1), None)
        );
        assert_eq!(unit.domain, ["Fury"]);
        assert!(snapshot.card(5).unwrap().is_hidden());
        assert_eq!(snapshot.held(9, 1).count(), 2);
        assert_eq!(snapshot.owned_at(9, 0).count(), 0);
        assert_eq!(snapshot.counter(Target::Seat(1), 0), Some(-2));
        assert_eq!(snapshot.counter(Target::Seat(0), 0), None);
        assert_eq!(snapshot.revealed, [3, 4]);
        assert_eq!(snapshot.tokens, [5]);
        assert!(snapshot.is_token(5));
        assert!(snapshot.is_revealed(3));
        let bounds = snapshot.counter_bounds(0).unwrap();
        assert_eq!(
            (bounds.scope, bounds.start, bounds.min, bounds.max),
            (CounterScope::Seat, 0, Some(0), Some(8))
        );
        assert_eq!(bounds.clamp(100), 8);
        assert_eq!(bounds.clamp(-3), 0);
        assert!(snapshot.options.is_empty());
        assert_eq!(snapshot.option("victory_score"), None);
    }

    #[test]
    fn the_table_options_are_read_from_the_genesis_bytes_with_unknown_keys_kept() {
        let mut inner = Writer::new();
        inner.map(3);
        inner.text("victory_score");
        inner.signed(6);
        inner.text("handicap");
        inner.signed(-2);
        inner.text("battlefields");
        inner.signed(3);
        let inner = inner.finish();
        let mut writer = Writer::new();
        writer.map(2);
        seats(&mut writer, 2);
        writer.text("options");
        writer.bytes(&inner);
        let bytes = writer.finish();
        let snapshot = parse_state(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(snapshot.option("victory_score"), Some(6));
        assert_eq!(snapshot.option("battlefields"), Some(3));
        assert_eq!(snapshot.option("handicap"), Some(-2));
        assert_eq!(snapshot.option("missing"), None);
        assert_eq!(snapshot.options.len(), 3);
        let mut absent = Writer::new();
        absent.map(2);
        seats(&mut absent, 2);
        absent.text("options");
        absent.null();
        let bytes = absent.finish();
        let snapshot = parse_state(&mut Reader::new(&bytes)).unwrap();
        assert!(snapshot.options.is_empty());
        let mut broken = Writer::new();
        broken.map(2);
        seats(&mut broken, 2);
        broken.text("options");
        broken.bytes(&[0xa1, 0x61, 0x6b, 0x61, 0x76]);
        let bytes = broken.finish();
        assert!(parse_state(&mut Reader::new(&bytes)).is_none());
    }

    #[test]
    fn signed_values_read_both_cbor_majors() {
        let mut writer = Writer::new();
        writer.signed(-1);
        writer.signed(0);
        writer.signed(7);
        writer.signed(-300);
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        assert_eq!(signed(&mut reader), Some(-1));
        assert_eq!(signed(&mut reader), Some(0));
        assert_eq!(signed(&mut reader), Some(7));
        assert_eq!(signed(&mut reader), Some(-300));
    }

    fn zone_of(id: u16, kind: ZoneKind, shared: bool, visibility: ZoneVisibility) -> ZoneSummary {
        ZoneSummary {
            id,
            name: format!("z{id}"),
            kind,
            shared,
            battlefield: kind == ZoneKind::Battlefield,
            label: format!("Z{id}"),
            visibility,
        }
    }

    fn card_at(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            id,
            zone: Some(zone),
            seat,
            owner: seat,
            name: format!("c{id}"),
            ..CardInfo::default()
        }
    }

    fn board() -> Snapshot {
        Snapshot {
            players: 2,
            zones: vec![
                zone_of(1, ZoneKind::Deck, false, ZoneVisibility::None),
                zone_of(2, ZoneKind::Battlefield, false, ZoneVisibility::All),
                zone_of(3, ZoneKind::Battlefield, true, ZoneVisibility::All),
                zone_of(5, ZoneKind::Discard, false, ZoneVisibility::All),
            ],
            cards: vec![card_at(7, 2, 0), card_at(8, 2, 0), card_at(9, 2, 1)],
            counters: Vec::new(),
            next_id: 10,
            revealed: vec![7, 8, 9],
            tokens: Vec::new(),
            options: Vec::new(),
            counter_table: vec![
                CounterBounds {
                    id: 0,
                    scope: CounterScope::Seat,
                    start: 0,
                    min: Some(0),
                    max: Some(8),
                },
                CounterBounds {
                    id: 1,
                    scope: CounterScope::Card,
                    start: 0,
                    min: Some(0),
                    max: None,
                },
            ],
        }
    }

    fn order(snapshot: &Snapshot) -> Vec<u32> {
        snapshot.cards.iter().map(|card| card.id).collect()
    }

    #[test]
    fn a_move_lands_at_its_ordinal_with_the_seat_normalised_in_shared_zones() {
        let mut table = board();
        table
            .apply(
                &Effect::Move {
                    card: 7,
                    zone: 3,
                    seat: 1,
                    index: BOTTOM,
                },
                0,
            )
            .unwrap();
        assert_eq!(table.card(7).unwrap().seat, 0);
        assert_eq!(table.card(7).unwrap().zone, Some(3));
        assert_eq!(order(&table), [8, 9, 7]);
        table
            .apply(
                &Effect::Move {
                    card: 9,
                    zone: 3,
                    seat: 1,
                    index: BOTTOM,
                },
                0,
            )
            .unwrap();
        assert_eq!(order(&table), [8, 9, 7]);
        table
            .apply(
                &Effect::Move {
                    card: 8,
                    zone: 3,
                    seat: 0,
                    index: TOP,
                },
                0,
            )
            .unwrap();
        assert_eq!(order(&table), [9, 7, 8]);
        table
            .apply(
                &Effect::Move {
                    card: 8,
                    zone: 3,
                    seat: 0,
                    index: 1,
                },
                0,
            )
            .unwrap();
        assert_eq!(order(&table), [9, 8, 7]);
        assert_eq!(
            table.apply(
                &Effect::Move {
                    card: 99,
                    zone: 3,
                    seat: 0,
                    index: 0,
                },
                0,
            ),
            Err(ApplyError::UnknownCard)
        );
        assert_eq!(
            table.apply(
                &Effect::Move {
                    card: 7,
                    zone: 42,
                    seat: 0,
                    index: 0,
                },
                0,
            ),
            Err(ApplyError::UnknownZone)
        );
    }

    #[test]
    fn entering_a_hidden_or_shedding_zone_blanks_the_face_and_drops_the_marks() {
        let mut table = board();
        table.apply(&Effect::exhaust(7), 0).unwrap();
        table
            .apply(
                &Effect::Counter {
                    target: Target::Card(7),
                    counter: 1,
                    delta: 2,
                },
                0,
            )
            .unwrap();
        assert!(table.card(7).unwrap().exhausted);
        assert_eq!(table.counter(Target::Card(7), 1), Some(2));
        table
            .apply(
                &Effect::Move {
                    card: 7,
                    zone: 1,
                    seat: 0,
                    index: BOTTOM,
                },
                0,
            )
            .unwrap();
        let hidden = table.card(7).unwrap();
        assert!(hidden.is_hidden());
        assert!(!hidden.exhausted);
        assert!(hidden.annotations.is_empty());
        assert!(!table.is_revealed(7));
        assert_eq!(table.counter(Target::Card(7), 1), None);
        table.apply(&Effect::exhaust(8), 0).unwrap();
        table
            .apply(
                &Effect::Move {
                    card: 8,
                    zone: 5,
                    seat: 0,
                    index: BOTTOM,
                },
                0,
            )
            .unwrap();
        let trashed = table.card(8).unwrap();
        assert!(!trashed.is_hidden());
        assert!(!trashed.exhausted);
        assert!(table.is_revealed(8));
    }

    #[test]
    fn a_spawn_mints_the_next_id_for_its_owner_and_a_token_vanishes_when_shed() {
        let mut table = board();
        table
            .apply(
                &Effect::Spawn {
                    face: Face::named("Sprite")
                        .with_kind("Unit")
                        .with_might(Some(3))
                        .with_cost(Some(2), Some(1))
                        .with_domain(vec!["Calm".into()]),
                    zone: 3,
                    seat: 1,
                    owner: Some(1),
                },
                0,
            )
            .unwrap();
        let sprite = table.card(10).unwrap();
        assert_eq!((sprite.owner, sprite.seat, sprite.zone), (1, 0, Some(3)));
        assert_eq!(sprite.might, Some(3));
        assert_eq!((sprite.energy, sprite.power), (Some(2), Some(1)));
        assert_eq!(sprite.domain, ["Calm"]);
        assert_eq!(sprite.face(), table.card(10).unwrap().face());
        assert!(table.is_token(10));
        assert!(table.is_revealed(10));
        assert_eq!(table.next_id, 11);
        table
            .apply(
                &Effect::Spawn {
                    face: Face::named("Gold").with_kind("Gear"),
                    zone: 1,
                    seat: 1,
                    owner: None,
                },
                0,
            )
            .unwrap();
        let gold = table.card(11).unwrap();
        assert_eq!((gold.owner, gold.seat), (0, 1));
        assert!(!table.is_revealed(11));
        assert_eq!(
            table.apply(
                &Effect::Spawn {
                    face: Face::default(),
                    zone: 3,
                    seat: 0,
                    owner: None,
                },
                0,
            ),
            Err(ApplyError::HiddenFace)
        );
        table
            .apply(
                &Effect::Move {
                    card: 10,
                    zone: 5,
                    seat: 1,
                    index: BOTTOM,
                },
                0,
            )
            .unwrap();
        assert!(table.card(10).is_none());
        assert!(!table.is_token(10));
        assert_eq!(
            table.apply(&Effect::Despawn { card: 7 }, 0),
            Err(ApplyError::NotAToken)
        );
        table.apply(&Effect::Despawn { card: 11 }, 0).unwrap();
        assert!(table.card(11).is_none());
        assert_eq!(
            table.apply(&Effect::Despawn { card: 11 }, 0),
            Err(ApplyError::UnknownCard)
        );
    }

    #[test]
    fn the_information_effects_leave_the_projection_alone_but_refuse_bad_ids() {
        let mut table = board();
        let before = table.clone();
        table.apply(&Effect::Reveal { card: 7 }, 0).unwrap();
        table.apply(&Effect::Peek { card: 7, seat: 1 }, 0).unwrap();
        assert_eq!(table, before);
        assert_eq!(
            table.apply(&Effect::Reveal { card: 99 }, 0),
            Err(ApplyError::UnknownCard)
        );
        assert_eq!(
            table.apply(&Effect::Peek { card: 99, seat: 0 }, 0),
            Err(ApplyError::UnknownCard)
        );
        assert_eq!(
            table.apply(&Effect::Peek { card: 7, seat: 9 }, 0),
            Err(ApplyError::UnknownSeat)
        );
    }

    #[test]
    fn counters_clamp_to_their_bounds_and_refuse_bad_targets() {
        let mut table = board();
        table.apply(&Effect::score(1, 0, 20), 0).unwrap();
        assert_eq!(table.counter(Target::Seat(1), 0), Some(8));
        table.apply(&Effect::score(1, 0, -100), 0).unwrap();
        assert_eq!(table.counter(Target::Seat(1), 0), Some(0));
        assert_eq!(
            table.apply(&Effect::score(2, 0, 1), 0),
            Err(ApplyError::UnknownSeat)
        );
        assert_eq!(
            table.apply(&Effect::score(0, 9, 1), 0),
            Err(ApplyError::UnknownCounter)
        );
        assert_eq!(
            table.apply(&Effect::score(0, 1, 1), 0),
            Err(ApplyError::CounterScope)
        );
        assert_eq!(
            table.apply(
                &Effect::Counter {
                    target: Target::Card(99),
                    counter: 1,
                    delta: 1,
                },
                0,
            ),
            Err(ApplyError::UnknownCard)
        );
        table.apply(&Effect::score(0, 0, 3), 0).unwrap();
        assert_eq!(table.counters[0].target, Target::Seat(0));
        assert_eq!(
            table.apply_all(&[Effect::score(0, 0, 1), Effect::score(5, 0, 1)], 0),
            Err((1, ApplyError::UnknownSeat))
        );
    }

    #[test]
    fn an_entry_is_projected_before_the_effects_that_follow_it() {
        let mut table = board();
        table
            .apply_entry(
                &Action::Spawn {
                    face: Face::named("Sprite").with_kind("Unit").with_might(Some(3)),
                    zone: Some(2),
                    seat: 1,
                },
                1,
            )
            .unwrap();
        assert_eq!(table.card(10).unwrap().owner, 1);
        table
            .apply_entry(
                &Action::Move {
                    card: 9,
                    to: Some(3),
                    seat: 1,
                    index: TOP,
                    hidden: false,
                },
                1,
            )
            .unwrap();
        assert_eq!(table.card(9).unwrap().seat, 0);
        table
            .apply_entry(
                &Action::Annotate {
                    card: 9,
                    key: "exhausted".into(),
                    value: Some(vec![1]),
                },
                1,
            )
            .unwrap();
        assert!(table.card(9).unwrap().exhausted);
        table
            .apply_entry(
                &Action::Counter {
                    target: Target::Seat(1),
                    counter: 0,
                    delta: 2,
                },
                1,
            )
            .unwrap();
        assert_eq!(table.counter(Target::Seat(1), 0), Some(2));
        table
            .apply_entry(
                &Action::Reveal {
                    card: 7,
                    face: Face::named("Vi").with_kind("Unit").with_might(Some(5)),
                },
                0,
            )
            .unwrap();
        assert_eq!(table.card(7).unwrap().name, "Vi");
        assert_eq!(table.card(7).unwrap().might, Some(5));
        assert!(table.is_revealed(7));
        assert_eq!(
            table.apply_entry(
                &Action::Reveal {
                    card: 70,
                    face: Face::named("Vi")
                },
                0
            ),
            Err(ApplyError::UnknownCard)
        );
        table.apply_entry(&Action::Game(vec![1]), 0).unwrap();
        table.apply_entry(&Action::Clear { seat: 1 }, 1).unwrap();
        assert!(table.card(9).is_none());
        assert!(table.card(10).is_none());
        assert_eq!(table.counter(Target::Seat(1), 0), None);
        assert!(table.card(7).is_some());
        table.apply_entry(&Action::Reset, 0).unwrap();
        assert!(table.cards.is_empty());
        assert_eq!(table.next_id, 0);
        table
            .apply_entry(
                &Action::Spawn {
                    face: Face::named("Sprite"),
                    zone: None,
                    seat: 1,
                },
                0,
            )
            .unwrap();
        let in_hand = table.card(0).unwrap();
        assert_eq!((in_hand.zone, in_hand.seat, in_hand.owner), (None, 1, 0));
        assert!(table.is_revealed(0));
        assert!(table.is_token(0));
        assert_eq!(
            table.apply_entry(
                &Action::Spawn {
                    face: Face::named("Sprite"),
                    zone: Some(42),
                    seat: 0,
                },
                0,
            ),
            Err(ApplyError::UnknownZone)
        );
    }

    #[test]
    fn the_core_hand_and_board_zones_project_like_the_engine_folds_them() {
        let mut table = board();
        table.apply(&Effect::exhaust(7), 0).unwrap();
        table
            .apply_entry(
                &Action::Move {
                    card: 7,
                    to: Some(BOARD),
                    seat: 1,
                    index: BOTTOM,
                    hidden: false,
                },
                0,
            )
            .unwrap();
        let on_board = table.card(7).unwrap();
        assert_eq!((on_board.zone, on_board.seat), (Some(BOARD), 1));
        assert!(on_board.exhausted);
        assert!(!on_board.is_hidden());
        assert!(table.is_revealed(7));
        assert_eq!(table.in_zone(BOARD).count(), 1);
        table
            .apply_entry(
                &Action::Move {
                    card: 8,
                    to: None,
                    seat: 0,
                    index: TOP,
                    hidden: false,
                },
                0,
            )
            .unwrap();
        let in_hand = table.card(8).unwrap();
        assert_eq!(in_hand.zone, None);
        assert!(!in_hand.exhausted);
        assert!(!in_hand.is_hidden());
        assert!(table.is_revealed(8));
        table
            .apply(
                &Effect::Spawn {
                    face: Face::named("Sprite"),
                    zone: BOARD,
                    seat: 1,
                    owner: None,
                },
                0,
            )
            .unwrap();
        assert!(table.is_revealed(10));
        assert_eq!(table.card(10).unwrap().seat, 1);
    }
}

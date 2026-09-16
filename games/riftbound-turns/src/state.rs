use crate::cards::{Cost as ScriptCost, Domain, Keyword, Power, KIND_GEAR, KIND_SPELL};
use agni_plugin_sdk::blob::{discriminator, MapReader, MapWriter};
use agni_plugin_sdk::cbor::{Item, Reader, Writer};
use agni_plugin_sdk::dice::Roll;
use agni_plugin_sdk::prompt::Prompt;
use agni_plugin_sdk::turns::{PassWindow, TurnOrder};
use std::sync::OnceLock;

pub const BLOB_VERSION: u64 = 14;
pub const NARRATION_LINES: usize = 12;
pub const DIE_SIDES: u8 = 6;

pub const FLAG_STUNNED: u16 = 1;
pub const FLAG_NO_MOVE_BY_OWNER: u16 = 1 << 1;
pub const FLAG_ATTACKER: u16 = 1 << 2;
pub const FLAG_DEFENDER: u16 = 1 << 3;
pub const FLAG_ENTERED_THIS_TURN: u16 = 1 << 4;
pub const FLAG_NOT_PLAYED: u16 = 1 << 5;
pub const FLAG_ONCE_USED: u16 = 1 << 6;
pub const FLAG_FROM_FACEDOWN: u16 = 1 << 7;
pub const FLAG_PAYING: u16 = 1 << 8;
pub const FLAG_DISCARDED: u16 = 1 << 9;
pub const FLAG_SHROUDED: u16 = 1 << 10;
pub const FLAG_ONCE_BY_SEAT: u16 = 0b1111 << 11;
pub const FLAG_REVEALING: u16 = 1 << 15;

pub fn once_by_seat(seat: u8) -> u16 {
    1 << (11 + (seat & 3))
}

pub const UNANSWERED: u8 = u8::MAX;
pub const SLOT_ACCELERATE: usize = 0;
pub const SLOT_TRIGGER_COST: usize = 1;
pub const SLOT_REPEAT: usize = 2;
pub const SLOT_ADDITIONAL: usize = 3;
pub const SLOTS: usize = 4;
pub const SLOT_ORDINAL: usize = 5;
pub const SLOT_PROMISED_REPEAT: usize = 6;
pub const SLOT_MODE: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Free,
    Enforced,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Free => "free table",
            Mode::Enforced => "rules enforced",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Mode::Free => Mode::Enforced,
            Mode::Enforced => Mode::Free,
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Mode::Free => 0,
            Mode::Enforced => 1,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Mode::Free),
            1 => Some(Mode::Enforced),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Setup,
    Awaken,
    Beginning,
    Channel,
    Draw,
    Action,
    Ending,
    Cleanup,
    Expiration,
}

impl Phase {
    pub const ALL: [Phase; 9] = [
        Phase::Setup,
        Phase::Awaken,
        Phase::Beginning,
        Phase::Channel,
        Phase::Draw,
        Phase::Action,
        Phase::Ending,
        Phase::Cleanup,
        Phase::Expiration,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Phase::Setup => "setup",
            Phase::Awaken => "awaken step",
            Phase::Beginning => "beginning phase",
            Phase::Channel => "channel step",
            Phase::Draw => "draw step",
            Phase::Action => "action phase",
            Phase::Ending => "ending step",
            Phase::Cleanup => "cleanup",
            Phase::Expiration => "expiration step",
        }
    }

    pub fn code(self) -> u8 {
        Phase::ALL
            .iter()
            .position(|phase| *phase == self)
            .unwrap_or(0) as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Phase::ALL.get(usize::from(code)).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnCore {
    pub players: u8,
    pub first: u8,
    pub turn: u16,
    pub player: u8,
    pub phase: Phase,
}

impl TurnCore {
    pub fn start(players: u8, first: u8) -> Self {
        Self {
            players: players.max(1),
            first,
            turn: 1,
            player: first,
            phase: Phase::Action,
        }
    }

    pub fn order(&self) -> TurnOrder {
        TurnOrder {
            players: self.players.max(1),
            turn: self.turn,
            turn_player: self.player,
        }
    }

    pub fn advance(&mut self) {
        let mut order = self.order();
        order.advance();
        self.turn = order.turn;
        self.player = order.turn_player;
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(5);
        writer.unsigned(u64::from(self.players));
        writer.unsigned(u64::from(self.first));
        writer.unsigned(u64::from(self.turn));
        writer.unsigned(u64::from(self.player));
        writer.unsigned(u64::from(self.phase.code()));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 5)?;
        Some(Self {
            players: u8_of(reader)?.max(1),
            first: u8_of(reader)?,
            turn: u16_of(reader)?,
            player: u8_of(reader)?,
            phase: Phase::from_code(u8_of(reader)?)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    pub zone: u16,
    pub holder: Option<u8>,
    pub scored: u8,
    pub contested: Option<u8>,
}

impl Control {
    pub fn new(zone: u16) -> Self {
        Self {
            zone,
            holder: None,
            scored: 0,
            contested: None,
        }
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(4);
        writer.unsigned(u64::from(self.zone));
        seat_or_null(writer, self.holder);
        writer.unsigned(u64::from(self.scored));
        seat_or_null(writer, self.contested);
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 4)?;
        Some(Self {
            zone: u16_of(reader)?,
            holder: seat_of(reader)?,
            scored: u8_of(reader)?,
            contested: seat_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Excess {
    pub seat: u8,
    pub zone: u16,
    pub amount: u8,
}

impl Excess {
    fn write(&self, writer: &mut Writer) {
        writer.array(3);
        writer.unsigned(u64::from(self.seat));
        writer.unsigned(u64::from(self.zone));
        writer.unsigned(u64::from(self.amount));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        Some(Self {
            seat: u8_of(reader)?,
            zone: u16_of(reader)?,
            amount: u8_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ShowdownStage {
    #[default]
    Open,
    Damage {
        assigner: u8,
        remaining: u8,
        assigned: Vec<(u32, u8)>,
    },
    Resolution,
}

impl ShowdownStage {
    fn write(&self, writer: &mut Writer) {
        match self {
            ShowdownStage::Open => writer.unsigned(0),
            ShowdownStage::Damage {
                assigner,
                remaining,
                assigned,
            } => {
                writer.array(4);
                writer.unsigned(1);
                writer.unsigned(u64::from(*assigner));
                writer.unsigned(u64::from(*remaining));
                writer.array(assigned.len());
                for (card, amount) in assigned {
                    writer.array(2);
                    writer.unsigned(u64::from(*card));
                    writer.unsigned(u64::from(*amount));
                }
            }
            ShowdownStage::Resolution => writer.unsigned(2),
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        match reader.item()? {
            Item::Unsigned(0) => Some(ShowdownStage::Open),
            Item::Unsigned(2) => Some(ShowdownStage::Resolution),
            Item::Array(4) => {
                (reader.unsigned()? == 1).then_some(())?;
                let assigner = u8_of(reader)?;
                let remaining = u8_of(reader)?;
                let mut assigned = Vec::new();
                for _ in 0..reader.array_len()? {
                    fixed(reader, 2)?;
                    assigned.push((u32_of(reader)?, u8_of(reader)?));
                }
                Some(ShowdownStage::Damage {
                    assigner,
                    remaining,
                    assigned,
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Showdown {
    pub zone: u16,
    pub attacker: u8,
    pub defender: u8,
    pub window: PassWindow,
    pub combat: bool,
    pub initial_chain: bool,
    pub stage: ShowdownStage,
}

impl Showdown {
    pub fn open(zone: u16, attacker: u8, defender: u8) -> Self {
        Self {
            zone,
            attacker,
            defender,
            window: PassWindow::open(attacker),
            combat: false,
            initial_chain: false,
            stage: ShowdownStage::Open,
        }
    }

    pub fn focus(&self) -> u8 {
        self.window.focus
    }

    pub fn passes(&self) -> u8 {
        self.window.passes
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(8);
        writer.unsigned(u64::from(self.zone));
        writer.unsigned(u64::from(self.attacker));
        writer.unsigned(u64::from(self.defender));
        writer.unsigned(u64::from(self.window.focus));
        writer.unsigned(u64::from(self.window.passes));
        writer.bool(self.combat);
        writer.bool(self.initial_chain);
        self.stage.write(writer);
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 8)?;
        Some(Self {
            zone: u16_of(reader)?,
            attacker: u8_of(reader)?,
            defender: u8_of(reader)?,
            window: PassWindow {
                focus: u8_of(reader)?,
                passes: u8_of(reader)?,
            },
            combat: bool_of(reader)?,
            initial_chain: bool_of(reader)?,
            stage: ShowdownStage::read(reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetupStage {
    #[default]
    Waiting,
    Drawn,
    Done,
}

impl SetupStage {
    fn code(self) -> u8 {
        match self {
            SetupStage::Waiting => 0,
            SetupStage::Drawn => 1,
            SetupStage::Done => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(SetupStage::Waiting),
            1 => Some(SetupStage::Drawn),
            2 => Some(SetupStage::Done),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooled {
    Rainbow,
    Domain(Domain),
}

impl Pooled {
    pub fn of(power: Power, domains: &[Domain]) -> Self {
        match power {
            Power::Domain(domain) => Self::Domain(domain),
            Power::Rainbow => Self::Rainbow,
            Power::Own => match domains {
                [domain] => Self::Domain(*domain),
                _ => Self::Rainbow,
            },
        }
    }

    pub fn domain(self) -> Option<Domain> {
        match self {
            Self::Rainbow => None,
            Self::Domain(domain) => Some(domain),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Rainbow => "rainbow".to_string(),
            Self::Domain(domain) => format!("{} power", domain.label()),
        }
    }

    fn code(self) -> u8 {
        match self {
            Self::Rainbow => 0,
            Self::Domain(domain) => domain.code().saturating_add(1),
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Rainbow),
            code => Domain::from_code(code - 1).map(Self::Domain),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pool {
    pub energy: u8,
    pub power: Vec<Pooled>,
}

impl Pool {
    pub fn of(cost: &ScriptCost, domains: &[Domain]) -> Self {
        Self {
            energy: cost.energy,
            power: cost
                .power
                .iter()
                .map(|power| Pooled::of(*power, domains))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.energy == 0 && self.power.is_empty()
    }

    pub fn add(&mut self, other: &Self) {
        self.energy = self.energy.saturating_add(other.energy);
        self.power.extend(other.power.iter().copied());
    }

    pub fn take(&mut self, spent: &Self) {
        self.energy = self.energy.saturating_sub(spent.energy);
        for held in &spent.power {
            if let Some(index) = self.power.iter().position(|mine| mine == held) {
                self.power.remove(index);
            }
        }
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.energy > 0 {
            parts.push(format!("{} energy", self.energy));
        }
        let mut grouped: Vec<(String, usize)> = Vec::new();
        for held in &self.power {
            let label = held.label();
            match grouped.iter_mut().find(|(known, _)| *known == label) {
                Some((_, count)) => *count += 1,
                None => grouped.push((label, 1)),
            }
        }
        for (label, count) in grouped {
            parts.push(format!("{count} {label}"));
        }
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(" and ")
        }
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(2);
        writer.unsigned(u64::from(self.energy));
        writer.array(self.power.len());
        for power in &self.power {
            writer.unsigned(u64::from(power.code()));
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 2)?;
        let energy = u8_of(reader)?;
        let mut power = Vec::new();
        for _ in 0..reader.array_len()? {
            power.push(Pooled::from_code(u8_of(reader)?)?);
        }
        Some(Self { energy, power })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayLock(u8);

impl PlayLock {
    pub const NONE: Self = Self(0);
    pub const SPELLS: Self = Self(1);
    pub const UNITS: Self = Self(2);
    pub const GEAR: Self = Self(4);
    pub const CARDS: Self = Self(7);

    pub fn of_kind(kind: &str) -> Self {
        match kind {
            KIND_SPELL => Self::SPELLS,
            KIND_GEAR => Self::GEAR,
            _ => Self::UNITS,
        }
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn from_bits(bits: u8) -> Option<Self> {
        (bits & !Self::CARDS.0 == 0).then_some(Self(bits))
    }
}

impl std::ops::BitOr for PlayLock {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOrAssign for PlayLock {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeatState {
    pub setup: SetupStage,
    pub draws: u8,
    pub played_main: bool,
    pub play_lock: PlayLock,
    pub looks_facedown_of: u8,
    pub cards_played: u8,
    pub spells_played: u8,
    pub gear_played: u8,
    pub gear_abilities_activated: u8,
    pub promises: Vec<Promise>,
    pub pool: Pool,
    pub chosen_champion: Option<String>,
    pub units_enter_ready_this_turn: bool,
    pub next_unit_enters_ready: bool,
    pub next_spell_bonus: u8,
    pub spell_bonus: (u16, u8),
}

fn fresh_seat() -> &'static SeatState {
    static FRESH: OnceLock<SeatState> = OnceLock::new();
    FRESH.get_or_init(SeatState::default)
}

impl SeatState {
    pub fn reset_turn(&mut self) {
        self.draws = 0;
        self.played_main = false;
        self.play_lock = PlayLock::NONE;
        self.looks_facedown_of = 0;
        self.cards_played = 0;
        self.spells_played = 0;
        self.units_enter_ready_this_turn = false;
        self.next_unit_enters_ready = false;
        self.next_spell_bonus = 0;
        self.spell_bonus = (0, 0);
        self.gear_played = 0;
        self.gear_abilities_activated = 0;
        self.pool = Pool::default();
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(16);
        writer.unsigned(u64::from(self.setup.code()));
        writer.unsigned(u64::from(self.draws));
        writer.bool(self.played_main);
        writer.unsigned(u64::from(self.play_lock.0));
        writer.unsigned(u64::from(self.looks_facedown_of));
        writer.unsigned(u64::from(self.cards_played));
        writer.unsigned(u64::from(self.spells_played));
        writer.array(self.promises.len());
        for promise in &self.promises {
            promise.write(writer);
        }
        writer.bool(self.units_enter_ready_this_turn);
        writer.bool(self.next_unit_enters_ready);
        match &self.chosen_champion {
            Some(name) => writer.text(name),
            None => writer.null(),
        }
        writer.unsigned(u64::from(self.gear_played));
        writer.unsigned(u64::from(self.gear_abilities_activated));
        self.pool.write(writer);
        writer.unsigned(u64::from(self.next_spell_bonus));
        writer.array(2);
        writer.unsigned(u64::from(self.spell_bonus.0));
        writer.unsigned(u64::from(self.spell_bonus.1));
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        let len = reader.array_len()?;
        if version == 7 && len != 8 && len != 9
            || version == 9 && len != 10
            || version == 10 && len != 11
            || (11..=12).contains(&version) && len != 14
            || version >= 13 && len != 16
        {
            return None;
        }
        let setup = SetupStage::from_code(u8_of(reader)?)?;
        let draws = u8_of(reader)?;
        let played_main = bool_of(reader)?;
        let lock_or_no_spells = reader.item()?;
        let looks_facedown_of = u8_of(reader)?;
        let cards_played = u8_of(reader)?;
        let spells_played = u8_of(reader)?;
        let play_lock = match lock_or_no_spells {
            Item::Unsigned(bits) => PlayLock::from_bits(u8::try_from(bits).ok()?)?,
            Item::Simple(21) => PlayLock::SPELLS,
            Item::Simple(20) => PlayLock::NONE,
            _ => return None,
        };
        let (
            promises,
            units_enter_ready_this_turn,
            next_unit_enters_ready,
            chosen_champion,
            gear_played,
            gear_abilities_activated,
            pool,
            next_spell_bonus,
            spell_bonus,
        ) = if version >= 11 {
            let mut promises = Vec::new();
            for _ in 0..reader.array_len()? {
                promises.push(Promise::read(reader)?);
            }
            let units_enter_ready_this_turn = bool_of(reader)?;
            let next_unit_enters_ready = bool_of(reader)?;
            let chosen_champion = text_or_none(reader)?;
            let gear_played = u8_of(reader)?;
            let gear_abilities_activated = u8_of(reader)?;
            let pool = Pool::read(reader)?;
            let (next_spell_bonus, spell_bonus) = if version >= 13 {
                let bonus = u8_of(reader)?;
                fixed(reader, 2)?;
                (bonus, (u16_of(reader)?, u8_of(reader)?))
            } else {
                (0, (0, 0))
            };
            (
                promises,
                units_enter_ready_this_turn,
                next_unit_enters_ready,
                chosen_champion,
                gear_played,
                gear_abilities_activated,
                pool,
                next_spell_bonus,
                spell_bonus,
            )
        } else {
            fixed(reader, 2)?;
            let next_discount = (u8_of(reader)?, u8_of(reader)?);
            let promises = if next_discount == (0, 0) {
                Vec::new()
            } else {
                vec![Promise {
                    kind: PromiseKind::Any,
                    effect: PromiseEffect::Discount(Pool {
                        energy: next_discount.0,
                        power: (0..next_discount.1).map(|_| Pooled::Rainbow).collect(),
                    }),
                    until: Expiry::Permanent,
                }]
            };
            match version {
                7 => (
                    promises,
                    false,
                    false,
                    if len == 9 {
                        text_or_none(reader)?
                    } else {
                        None
                    },
                    0,
                    0,
                    Pool::default(),
                    0,
                    (0, 0),
                ),
                9 => (
                    promises,
                    bool_of(reader)?,
                    bool_of(reader)?,
                    None,
                    0,
                    0,
                    Pool::default(),
                    0,
                    (0, 0),
                ),
                _ => (
                    promises,
                    bool_of(reader)?,
                    bool_of(reader)?,
                    text_or_none(reader)?,
                    0,
                    0,
                    Pool::default(),
                    0,
                    (0, 0),
                ),
            }
        };
        Some(Self {
            setup,
            draws,
            played_main,
            play_lock,
            looks_facedown_of,
            cards_played,
            spells_played,
            promises,
            pool,
            units_enter_ready_this_turn,
            next_unit_enters_ready,
            chosen_champion,
            gear_played,
            gear_abilities_activated,
            next_spell_bonus,
            spell_bonus,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Expiry {
    #[default]
    Permanent,
    EndOfTurn(u16),
    CombatEnd,
    WhileAttached(u32),
}

impl Expiry {
    fn write(self, writer: &mut Writer) {
        match self {
            Expiry::Permanent => writer.unsigned(0),
            Expiry::EndOfTurn(turn) => {
                writer.array(2);
                writer.unsigned(1);
                writer.unsigned(u64::from(turn));
            }
            Expiry::CombatEnd => writer.unsigned(2),
            Expiry::WhileAttached(unit) => {
                writer.array(2);
                writer.unsigned(3);
                writer.unsigned(u64::from(unit));
            }
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        match reader.item()? {
            Item::Unsigned(0) => Some(Expiry::Permanent),
            Item::Unsigned(2) => Some(Expiry::CombatEnd),
            Item::Array(2) => match reader.unsigned()? {
                1 => Some(Expiry::EndOfTurn(u16_of(reader)?)),
                3 => Some(Expiry::WhileAttached(u32_of(reader)?)),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromiseKind {
    Spell,
    Gear,
    Unit,
    Any,
}

impl PromiseKind {
    fn code(self) -> u8 {
        match self {
            PromiseKind::Spell => 0,
            PromiseKind::Gear => 1,
            PromiseKind::Unit => 2,
            PromiseKind::Any => 3,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(PromiseKind::Spell),
            1 => Some(PromiseKind::Gear),
            2 => Some(PromiseKind::Unit),
            3 => Some(PromiseKind::Any),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PromiseKind::Spell => "spell",
            PromiseKind::Gear => "gear",
            PromiseKind::Unit => "unit",
            PromiseKind::Any => "card",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromiseEffect {
    Discount(Pool),
    RepeatForCost,
    FreeForPower { max_energy: u8 },
}

impl PromiseEffect {
    pub fn label(&self) -> String {
        match self {
            PromiseEffect::Discount(pool) => format!("discount of {}", pool.label()),
            PromiseEffect::RepeatForCost => "Repeat equal to its cost".to_string(),
            PromiseEffect::FreeForPower { max_energy } => {
                format!("play for its Power alone at {max_energy} energy or less")
            }
        }
    }

    pub fn noun(&self) -> &'static str {
        match self {
            PromiseEffect::Discount(_) => "discount",
            PromiseEffect::RepeatForCost => "Repeat",
            PromiseEffect::FreeForPower { .. } => "energy-free play",
        }
    }

    fn write(&self, writer: &mut Writer) {
        match self {
            PromiseEffect::Discount(pool) => {
                writer.array(2);
                writer.unsigned(0);
                pool.write(writer);
            }
            PromiseEffect::RepeatForCost => writer.unsigned(1),
            PromiseEffect::FreeForPower { max_energy } => {
                writer.array(2);
                writer.unsigned(2);
                writer.unsigned(u64::from(*max_energy));
            }
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        match reader.item()? {
            Item::Unsigned(1) => Some(PromiseEffect::RepeatForCost),
            Item::Array(2) => match reader.unsigned()? {
                0 => Some(PromiseEffect::Discount(Pool::read(reader)?)),
                2 => Some(PromiseEffect::FreeForPower {
                    max_energy: u8_of(reader)?,
                }),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promise {
    pub kind: PromiseKind,
    pub effect: PromiseEffect,
    pub until: Expiry,
}

impl Promise {
    fn write(&self, writer: &mut Writer) {
        writer.array(3);
        writer.unsigned(u64::from(self.kind.code()));
        self.effect.write(writer);
        self.until.write(writer);
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        Some(Self {
            kind: PromiseKind::from_code(u8_of(reader)?)?,
            effect: PromiseEffect::read(reader)?,
            until: Expiry::read(reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MightMod {
    pub delta: i16,
    pub until: Expiry,
    pub src: u16,
}

impl MightMod {
    fn write(&self, writer: &mut Writer) {
        writer.array(3);
        writer.signed(i64::from(self.delta));
        self.until.write(writer);
        writer.unsigned(u64::from(self.src));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        Some(Self {
            delta: i16::try_from(signed_of(reader)?).ok()?,
            until: Expiry::read(reader)?,
            src: u16_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostedGrant {
    pub kind: CostedKind,
    pub energy: u8,
    pub power: Vec<Power>,
    pub until: Expiry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostedKind {
    Equip,
    Repeat,
    Empower,
    Flow,
}

impl CostedKind {
    pub fn code(self) -> u8 {
        match self {
            Self::Equip => 14,
            Self::Repeat => 16,
            Self::Empower => 19,
            Self::Flow => 20,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            14 => Self::Equip,
            16 => Self::Repeat,
            19 => Self::Empower,
            20 => Self::Flow,
            _ => return None,
        })
    }

    pub fn matches(self, keyword: Keyword) -> bool {
        keyword
            .cost()
            .is_some_and(|_| self.code() == keyword.codes().0)
    }
}

impl CostedGrant {
    pub fn from_keyword(keyword: Keyword, until: Expiry) -> Option<Self> {
        let cost = keyword.cost()?;
        Some(Self {
            kind: CostedKind::from_code(keyword.codes().0)?,
            energy: cost.energy,
            power: cost.power.to_vec(),
            until,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardState {
    pub id: u32,
    pub flags: u16,
    pub might: Vec<MightMod>,
    pub granted: Vec<(Keyword, Expiry)>,
    pub granted_costed: Vec<CostedGrant>,
    pub attached_to: Option<u32>,
    pub hidden_at: Option<u16>,
    pub hidden_since: u16,
    pub entered: u16,
    pub attached_turn: u16,
    pub controlled_by: Option<u8>,
    pub control_source: Option<u32>,
    pub named: Option<String>,
    pub damage_multiplier_this_turn: u8,
    pub damage_marks: Vec<(u8, u8)>,
}

impl CardState {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    pub fn is_default(&self) -> bool {
        self.flags == 0
            && self.might.is_empty()
            && self.granted.is_empty()
            && self.granted_costed.is_empty()
            && self.attached_to.is_none()
            && self.hidden_at.is_none()
            && self.hidden_since == 0
            && self.entered == 0
            && self.attached_turn == 0
            && self.controlled_by.is_none()
            && self.control_source.is_none()
            && self.named.is_none()
            && self.damage_multiplier_this_turn == 0
            && self.damage_marks.is_empty()
    }

    pub fn marked_by(&self, seat: u8) -> u8 {
        self.damage_marks
            .iter()
            .find(|(marker, _)| *marker == seat)
            .map_or(0, |(_, n)| *n)
    }

    pub fn mark_damage(&mut self, seat: u8, n: u8) {
        match self
            .damage_marks
            .binary_search_by_key(&seat, |(marker, _)| *marker)
        {
            Ok(index) => {
                self.damage_marks[index].1 = self.damage_marks[index].1.saturating_add(n);
            }
            Err(index) => self.damage_marks.insert(index, (seat, n)),
        }
    }

    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    pub fn set(&mut self, flag: u16, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(15);
        writer.unsigned(u64::from(self.id));
        writer.unsigned(u64::from(self.flags));
        writer.array(self.might.len());
        for held in &self.might {
            held.write(writer);
        }
        writer.array(self.granted.len());
        for (keyword, until) in &self.granted {
            let (code, arg) = keyword.codes();
            writer.array(3);
            writer.unsigned(u64::from(code));
            writer.unsigned(u64::from(arg));
            until.write(writer);
        }
        writer.array(self.granted_costed.len());
        for grant in &self.granted_costed {
            writer.array(4);
            writer.unsigned(u64::from(grant.kind.code()));
            writer.unsigned(u64::from(grant.energy));
            writer.array(grant.power.len());
            for power in &grant.power {
                writer.unsigned(u64::from(power.code()));
            }
            grant.until.write(writer);
        }
        card_or_null(writer, self.attached_to);
        zone_or_null(writer, self.hidden_at);
        writer.unsigned(u64::from(self.hidden_since));
        writer.unsigned(u64::from(self.entered));
        writer.unsigned(u64::from(self.attached_turn));
        seat_or_null(writer, self.controlled_by);
        card_or_null(writer, self.control_source);
        text_or_null(writer, self.named.as_deref());
        writer.unsigned(u64::from(self.damage_multiplier_this_turn));
        writer.array(self.damage_marks.len());
        for (seat, n) in &self.damage_marks {
            writer.array(2);
            writer.unsigned(u64::from(*seat));
            writer.unsigned(u64::from(*n));
        }
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        fixed(
            reader,
            if version >= 13 {
                15
            } else if version >= 10 {
                13
            } else {
                12
            },
        )?;
        let id = u32_of(reader)?;
        let flags = u16_of(reader)?;
        let mut might = Vec::new();
        for _ in 0..reader.array_len()? {
            might.push(MightMod::read(reader)?);
        }
        let mut granted = Vec::new();
        for _ in 0..reader.array_len()? {
            fixed(reader, 3)?;
            let code = u8_of(reader)?;
            let arg = u8_of(reader)?;
            let until = Expiry::read(reader)?;
            granted.push((Keyword::from_codes(code, arg)?, until));
        }
        let mut granted_costed = Vec::new();
        if version >= 9 {
            for _ in 0..reader.array_len()? {
                fixed(reader, 4)?;
                let code = u8_of(reader)?;
                let energy = u8_of(reader)?;
                let mut power = Vec::new();
                for _ in 0..reader.array_len()? {
                    power.push(Power::from_code(u8_of(reader)?)?);
                }
                let until = Expiry::read(reader)?;
                granted_costed.push(CostedGrant {
                    kind: CostedKind::from_code(code)?,
                    energy,
                    power,
                    until,
                });
            }
        }
        let attached_to = card_of(reader)?;
        let hidden_at = zone_of(reader)?;
        let hidden_since = u16_of(reader)?;
        let entered = u16_of(reader)?;
        let attached_turn = u16_of(reader)?;
        let controlled_by = seat_of(reader)?;
        let control_source = card_of(reader)?;
        let named = if version == 7 || version >= 10 {
            text_or_none(reader)?
        } else {
            None
        };
        let (damage_multiplier_this_turn, damage_marks) = if version >= 13 {
            (u8_of(reader)?, marks_of(reader)?)
        } else {
            (0, Vec::new())
        };
        Some(Self {
            id,
            flags,
            might,
            granted,
            granted_costed,
            attached_to,
            hidden_at,
            hidden_since,
            entered,
            attached_turn,
            controlled_by,
            control_source,
            named,
            damage_multiplier_this_turn,
            damage_marks,
        })
    }
}

fn marks_of(reader: &mut Reader) -> Option<Vec<(u8, u8)>> {
    let mut marks = Vec::new();
    let mut previous = None;
    for _ in 0..reader.array_len()? {
        fixed(reader, 2)?;
        let seat = u8_of(reader)?;
        if previous.is_some_and(|held| held >= seat) {
            return None;
        }
        previous = Some(seat);
        marks.push((seat, u8_of(reader)?));
    }
    Some(marks)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Spell { card: u32 },
    Permanent { card: u32 },
    Ability { source: u32, index: u8 },
    Trigger { source: u32, index: u8 },
    Granted { holder: u32, lender: u32, index: u8 },
    Lent { holder: u32, lender: u32, index: u8 },
}

impl ItemKind {
    pub fn card(self) -> Option<u32> {
        match self {
            ItemKind::Spell { card } | ItemKind::Permanent { card } => Some(card),
            ItemKind::Ability { .. }
            | ItemKind::Trigger { .. }
            | ItemKind::Granted { .. }
            | ItemKind::Lent { .. } => None,
        }
    }

    pub fn source(self) -> u32 {
        match self {
            ItemKind::Spell { card } | ItemKind::Permanent { card } => card,
            ItemKind::Ability { source, .. } | ItemKind::Trigger { source, .. } => source,
            ItemKind::Granted { holder, .. } | ItemKind::Lent { holder, .. } => holder,
        }
    }

    pub fn lender(self) -> Option<u32> {
        match self {
            ItemKind::Granted { lender, .. } | ItemKind::Lent { lender, .. } => Some(lender),
            _ => None,
        }
    }

    pub fn ability_index(self) -> Option<u8> {
        match self {
            ItemKind::Ability { index, .. }
            | ItemKind::Trigger { index, .. }
            | ItemKind::Granted { index, .. }
            | ItemKind::Lent { index, .. } => Some(index),
            ItemKind::Spell { .. } | ItemKind::Permanent { .. } => None,
        }
    }

    fn write(self, writer: &mut Writer) {
        match self {
            ItemKind::Spell { card } => {
                writer.array(3);
                writer.unsigned(0);
                writer.unsigned(u64::from(card));
                writer.unsigned(0);
            }
            ItemKind::Permanent { card } => {
                writer.array(3);
                writer.unsigned(1);
                writer.unsigned(u64::from(card));
                writer.unsigned(0);
            }
            ItemKind::Ability { source, index } => {
                writer.array(3);
                writer.unsigned(2);
                writer.unsigned(u64::from(source));
                writer.unsigned(u64::from(index));
            }
            ItemKind::Trigger { source, index } => {
                writer.array(3);
                writer.unsigned(3);
                writer.unsigned(u64::from(source));
                writer.unsigned(u64::from(index));
            }
            ItemKind::Granted {
                holder,
                lender,
                index,
            } => {
                writer.array(4);
                writer.unsigned(4);
                writer.unsigned(u64::from(holder));
                writer.unsigned(u64::from(lender));
                writer.unsigned(u64::from(index));
            }
            ItemKind::Lent {
                holder,
                lender,
                index,
            } => {
                writer.array(4);
                writer.unsigned(5);
                writer.unsigned(u64::from(holder));
                writer.unsigned(u64::from(lender));
                writer.unsigned(u64::from(index));
            }
        }
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        let len = reader.array_len()?;
        let tag = u8_of(reader)?;
        let source = u32_of(reader)?;
        if tag == 4 || tag == 5 {
            if version < 14 {
                return None;
            }
            if len != 4 {
                return None;
            }
            let lender = u32_of(reader)?;
            let index = u8_of(reader)?;
            let holder = source;
            return Some(if tag == 4 {
                ItemKind::Granted {
                    holder,
                    lender,
                    index,
                }
            } else {
                ItemKind::Lent {
                    holder,
                    lender,
                    index,
                }
            });
        }
        if len != 3 {
            return None;
        }
        let index = u8_of(reader)?;
        Some(match tag {
            0 => ItemKind::Spell { card: source },
            1 => ItemKind::Permanent { card: source },
            2 => ItemKind::Ability { source, index },
            3 => ItemKind::Trigger { source, index },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemStatus {
    #[default]
    Pending,
    Finalized,
    Resolving,
}

impl ItemStatus {
    fn code(self) -> u8 {
        match self {
            ItemStatus::Pending => 0,
            ItemStatus::Finalized => 1,
            ItemStatus::Resolving => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(ItemStatus::Pending),
            1 => Some(ItemStatus::Finalized),
            2 => Some(ItemStatus::Resolving),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Banish,
    Recycle,
}

impl Leave {
    fn code(self) -> u8 {
        match self {
            Leave::Banish => 0,
            Leave::Recycle => 1,
        }
    }

    fn from_code(code: u16) -> Option<Self> {
        match code {
            0 => Some(Leave::Banish),
            1 => Some(Leave::Recycle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealedFrom {
    Deck,
}

impl RevealedFrom {
    fn code(self) -> u8 {
        match self {
            RevealedFrom::Deck => 0,
        }
    }

    fn from_code(code: u16) -> Option<Self> {
        match code {
            0 => Some(RevealedFrom::Deck),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    #[default]
    Hand,
    Champion,
    Facedown {
        zone: u16,
    },
    Board,
    Trash {
        leave: Leave,
    },
    Revealed {
        from: RevealedFrom,
    },
    Banishment,
}

impl Origin {
    fn write(self, writer: &mut Writer) {
        writer.array(2);
        match self {
            Origin::Hand => {
                writer.unsigned(0);
                writer.unsigned(0);
            }
            Origin::Champion => {
                writer.unsigned(1);
                writer.unsigned(0);
            }
            Origin::Facedown { zone } => {
                writer.unsigned(2);
                writer.unsigned(u64::from(zone));
            }
            Origin::Board => {
                writer.unsigned(3);
                writer.unsigned(0);
            }
            Origin::Trash { leave } => {
                writer.unsigned(4);
                writer.unsigned(u64::from(leave.code()));
            }
            Origin::Revealed { from } => {
                writer.unsigned(5);
                writer.unsigned(u64::from(from.code()));
            }
            Origin::Banishment => {
                writer.unsigned(6);
                writer.unsigned(0);
            }
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 2)?;
        let tag = u8_of(reader)?;
        let zone = u16_of(reader)?;
        Some(match tag {
            0 => Origin::Hand,
            1 => Origin::Champion,
            2 => Origin::Facedown { zone },
            3 => Origin::Board,
            4 => Origin::Trash {
                leave: Leave::from_code(zone)?,
            },
            5 => Origin::Revealed {
                from: RevealedFrom::from_code(zone)?,
            },
            6 => Origin::Banishment,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRef {
    Card(u32),
    Seat(u8),
    Zone(u16),
    Item(u16),
}

impl TargetRef {
    fn write(self, writer: &mut Writer) {
        writer.array(2);
        match self {
            TargetRef::Card(card) => {
                writer.unsigned(0);
                writer.unsigned(u64::from(card));
            }
            TargetRef::Seat(seat) => {
                writer.unsigned(1);
                writer.unsigned(u64::from(seat));
            }
            TargetRef::Zone(zone) => {
                writer.unsigned(2);
                writer.unsigned(u64::from(zone));
            }
            TargetRef::Item(item) => {
                writer.unsigned(3);
                writer.unsigned(u64::from(item));
            }
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 2)?;
        Self::read_body(reader)
    }

    fn read_body(reader: &mut Reader) -> Option<Self> {
        let tag = u8_of(reader)?;
        let value = reader.unsigned()?;
        Some(match tag {
            0 => TargetRef::Card(u32::try_from(value).ok()?),
            1 => TargetRef::Seat(u8::try_from(value).ok()?),
            2 => TargetRef::Zone(u16::try_from(value).ok()?),
            3 => TargetRef::Item(u16::try_from(value).ok()?),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Noted {
    pub zone: u16,
    pub might: i32,
    pub controller: u8,
    pub alone: bool,
    pub buffed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Price {
    #[default]
    Printed,
    PowerOnly,
    LessEnergy(u8),
    Free,
    Ignored,
}

impl Price {
    fn write(self, writer: &mut Writer) {
        writer.array(2);
        match self {
            Price::Printed => {
                writer.unsigned(0);
                writer.unsigned(0);
            }
            Price::PowerOnly => {
                writer.unsigned(1);
                writer.unsigned(0);
            }
            Price::LessEnergy(energy) => {
                writer.unsigned(2);
                writer.unsigned(u64::from(energy));
            }
            Price::Free => {
                writer.unsigned(3);
                writer.unsigned(0);
            }
            Price::Ignored => {
                writer.unsigned(4);
                writer.unsigned(0);
            }
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 2)?;
        let tag = u8_of(reader)?;
        let value = u8_of(reader)?;
        Some(match tag {
            0 => Price::Printed,
            1 => Price::PowerOnly,
            2 => Price::LessEnergy(value),
            3 => Price::Free,
            4 => Price::Ignored,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limited {
    pub zones: Vec<u16>,
    pub price: Price,
}

impl Limited {
    fn write(&self, writer: &mut Writer) {
        writer.array(2);
        writer.array(self.zones.len());
        for zone in &self.zones {
            writer.unsigned(u64::from(*zone));
        }
        self.price.write(writer);
    }

    fn read_body(reader: &mut Reader) -> Option<Self> {
        let mut zones = Vec::new();
        for _ in 0..reader.array_len()? {
            zones.push(u16_of(reader)?);
        }
        Some(Self {
            zones,
            price: Price::read(reader)?,
        })
    }
}

impl Noted {
    fn write(&self, writer: &mut Writer) {
        writer.array(5);
        writer.unsigned(u64::from(self.zone));
        writer.signed(i64::from(self.might));
        writer.unsigned(u64::from(self.controller));
        writer.bool(self.alone);
        writer.bool(self.buffed);
    }

    fn read_body_version(reader: &mut Reader, version: u64) -> Option<Self> {
        Some(Self {
            zone: u16_of(reader)?,
            might: signed_of(reader)?,
            controller: u8_of(reader)?,
            alone: bool_of(reader)?,
            buffed: if version >= 14 {
                bool_of(reader)?
            } else {
                false
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainItem {
    pub id: u16,
    pub kind: ItemKind,
    pub controller: u8,
    pub status: ItemStatus,
    pub origin: Origin,
    pub targets: Vec<TargetRef>,
    pub spec_counts: Vec<u8>,
    pub picks: Vec<u8>,
    pub stage: u8,
    pub noted: Option<Noted>,
    pub subject: Option<TargetRef>,
    pub execution: u8,
    pub awaiting: Vec<u32>,
    pub limited: Option<Limited>,
}

impl ChainItem {
    pub fn new(id: u16, kind: ItemKind, controller: u8, origin: Origin) -> Self {
        Self {
            id,
            kind,
            controller,
            status: ItemStatus::Pending,
            origin,
            targets: Vec::new(),
            spec_counts: Vec::new(),
            picks: vec![UNANSWERED; SLOTS],
            stage: 0,
            noted: None,
            subject: None,
            execution: 0,
            awaiting: Vec::new(),
            limited: None,
        }
    }

    pub fn slot(&self, slot: usize) -> Option<u8> {
        self.picks
            .get(slot)
            .copied()
            .filter(|held| *held != UNANSWERED)
    }

    pub fn set_slot(&mut self, slot: usize, value: u8) {
        if self.picks.len() <= slot {
            self.picks.resize(slot + 1, UNANSWERED);
        }
        self.picks[slot] = value;
    }

    pub fn accelerated(&self) -> bool {
        self.slot(SLOT_ACCELERATE) == Some(1)
    }

    pub fn repeats(&self) -> u8 {
        u8::from(self.slot(SLOT_REPEAT) == Some(1))
            + u8::from(self.slot(SLOT_PROMISED_REPEAT) == Some(1))
    }

    pub fn repeated(&self) -> bool {
        self.repeats() > 0
    }

    pub fn paid_additional(&self) -> bool {
        self.slot(SLOT_ADDITIONAL) == Some(1)
    }

    pub fn mode_at(&self, execution: u8) -> Option<u8> {
        self.slot(SLOT_MODE + usize::from(execution))
    }

    pub fn mode(&self) -> Option<u8> {
        self.mode_at(self.execution)
    }

    pub fn set_mode(&mut self, execution: u8, mode: u8) {
        self.set_slot(SLOT_MODE + usize::from(execution), mode);
    }

    pub fn subject_card(&self) -> Option<u32> {
        match self.subject {
            Some(TargetRef::Card(card)) => Some(card),
            _ => None,
        }
    }

    pub fn zone_target(&self) -> Option<u16> {
        self.targets.iter().find_map(|target| match target {
            TargetRef::Zone(zone) => Some(*zone),
            _ => None,
        })
    }

    fn write(&self, writer: &mut Writer) {
        writer.array(14);
        writer.unsigned(u64::from(self.id));
        self.kind.write(writer);
        writer.unsigned(u64::from(self.controller));
        writer.unsigned(u64::from(self.status.code()));
        self.origin.write(writer);
        writer.array(self.targets.len());
        for target in &self.targets {
            target.write(writer);
        }
        writer.array(self.spec_counts.len());
        for count in &self.spec_counts {
            writer.unsigned(u64::from(*count));
        }
        writer.array(self.picks.len());
        for pick in &self.picks {
            writer.unsigned(u64::from(*pick));
        }
        writer.unsigned(u64::from(self.stage));
        match &self.noted {
            Some(noted) => noted.write(writer),
            None => writer.null(),
        }
        match self.subject {
            Some(subject) => subject.write(writer),
            None => writer.null(),
        }
        writer.unsigned(u64::from(self.execution));
        writer.array(self.awaiting.len());
        for card in &self.awaiting {
            writer.unsigned(u64::from(*card));
        }
        match &self.limited {
            Some(limited) => limited.write(writer),
            None => writer.null(),
        }
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        fixed(reader, if version >= 12 { 14 } else { 13 })?;
        let id = u16_of(reader)?;
        let kind = ItemKind::read_version(reader, version)?;
        let controller = u8_of(reader)?;
        let status = ItemStatus::from_code(u8_of(reader)?)?;
        let origin = Origin::read(reader)?;
        let mut targets = Vec::new();
        for _ in 0..reader.array_len()? {
            targets.push(TargetRef::read(reader)?);
        }
        let mut spec_counts = Vec::new();
        for _ in 0..reader.array_len()? {
            spec_counts.push(u8_of(reader)?);
        }
        let mut picks = Vec::new();
        for _ in 0..reader.array_len()? {
            picks.push(u8_of(reader)?);
        }
        if version < 11 && picks.len() > SLOT_PROMISED_REPEAT {
            picks.insert(SLOT_PROMISED_REPEAT, UNANSWERED);
        }
        let stage = u8_of(reader)?;
        let noted = match reader.item()? {
            Item::Simple(22) => None,
            Item::Array(length) if length == if version >= 14 { 5 } else { 4 } => {
                Some(Noted::read_body_version(reader, version)?)
            }
            _ => return None,
        };
        let subject = match reader.item()? {
            Item::Simple(22) => None,
            Item::Array(2) => Some(TargetRef::read_body(reader)?),
            _ => return None,
        };
        let execution = u8_of(reader)?;
        let mut awaiting = Vec::new();
        for _ in 0..reader.array_len()? {
            awaiting.push(u32_of(reader)?);
        }
        let limited = if version >= 12 {
            match reader.item()? {
                Item::Simple(22) => None,
                Item::Array(2) => Some(Limited::read_body(reader)?),
                _ => return None,
            }
        } else {
            None
        };
        Some(Self {
            id,
            kind,
            controller,
            status,
            origin,
            targets,
            spec_counts,
            picks,
            stage,
            noted,
            subject,
            execution,
            awaiting,
            limited,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Order,
    Choices,
    OptionalCost,
    Legality,
}

impl Needs {
    fn code(self) -> u8 {
        match self {
            Needs::Order => 0,
            Needs::Choices => 1,
            Needs::OptionalCost => 2,
            Needs::Legality => 3,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Needs::Order),
            1 => Some(Needs::Choices),
            2 => Some(Needs::OptionalCost),
            3 => Some(Needs::Legality),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub item: ChainItem,
    pub needs: Needs,
}

impl Pending {
    fn write(&self, writer: &mut Writer) {
        writer.array(2);
        self.item.write(writer);
        writer.unsigned(u64::from(self.needs.code()));
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        fixed(reader, 2)?;
        Some(Self {
            item: ChainItem::read_version(reader, version)?,
            needs: Needs::from_code(u8_of(reader)?)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    EndOfTurn(u16),
    BeginningOf(u8),
    AfterKillsBy(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delayed {
    pub when: When,
    pub source: u32,
    pub seat: u8,
    pub ability: u8,
    pub args: Vec<u32>,
}

impl Delayed {
    fn write(&self, writer: &mut Writer) {
        writer.array(6);
        match self.when {
            When::EndOfTurn(turn) => {
                writer.unsigned(0);
                writer.unsigned(u64::from(turn));
            }
            When::BeginningOf(seat) => {
                writer.unsigned(1);
                writer.unsigned(u64::from(seat));
            }
            When::AfterKillsBy(item) => {
                writer.unsigned(2);
                writer.unsigned(u64::from(item));
            }
        }
        writer.unsigned(u64::from(self.source));
        writer.unsigned(u64::from(self.seat));
        writer.unsigned(u64::from(self.ability));
        writer.array(self.args.len());
        for arg in &self.args {
            writer.unsigned(u64::from(*arg));
        }
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 6)?;
        let tag = u8_of(reader)?;
        let value = reader.unsigned()?;
        let when = match tag {
            0 => When::EndOfTurn(u16::try_from(value).ok()?),
            1 => When::BeginningOf(u8::try_from(value).ok()?),
            2 => When::AfterKillsBy(u16::try_from(value).ok()?),
            _ => return None,
        };
        let source = u32_of(reader)?;
        let seat = u8_of(reader)?;
        let ability = u8_of(reader)?;
        let mut args = Vec::new();
        for _ in 0..reader.array_len()? {
            args.push(u32_of(reader)?);
        }
        Some(Self {
            when,
            source,
            seat,
            ability,
            args,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority {
    pub active: u8,
    pub passes: u8,
}

impl Priority {
    fn write(&self, writer: &mut Writer) {
        writer.array(2);
        writer.unsigned(u64::from(self.active));
        writer.unsigned(u64::from(self.passes));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 2)?;
        Some(Self {
            active: u8_of(reader)?,
            passes: u8_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Staged {
    pub zone: u16,
    pub combat: bool,
    pub contester: u8,
}

impl Staged {
    fn write(&self, writer: &mut Writer) {
        writer.array(3);
        writer.unsigned(u64::from(self.zone));
        writer.bool(self.combat);
        writer.unsigned(u64::from(self.contester));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        Some(Self {
            zone: u16_of(reader)?,
            combat: bool_of(reader)?,
            contester: u8_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InGameRoll {
    pub id: u32,
    pub roll: Roll,
    pub why: u8,
}

impl InGameRoll {
    fn write(&self, writer: &mut Writer) {
        writer.array(3);
        writer.unsigned(u64::from(self.id));
        writer.bytes(&self.roll.encode());
        writer.unsigned(u64::from(self.why));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        Some(Self {
            id: u32_of(reader)?,
            roll: Roll::decode(reader.bytes()?)?,
            why: u8_of(reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Cards,
    Options,
    Order,
    Confirm,
    Assign,
    Roll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Spell,
    Tag,
}

impl NameKind {
    fn code(self) -> u64 {
        match self {
            NameKind::Spell => 0,
            NameKind::Tag => 1,
        }
    }

    fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(NameKind::Spell),
            1 => Some(NameKind::Tag),
            _ => None,
        }
    }
}

pub const PROMPT_NAME_TAG: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptWhy {
    Mulligan,
    Target {
        item: u16,
        spec: u8,
    },
    PlayLocation {
        item: u16,
    },
    OptionalCost {
        item: u16,
        cost: u8,
    },
    OrderTriggers {
        seat: u8,
    },
    PickStaged,
    GroupMove {
        unit: u32,
        to: u16,
    },
    Assign,
    Resume {
        item: u16,
        stage: u8,
    },
    PayWith {
        item: u16,
    },
    Discard {
        item: u16,
        stage: u8,
    },
    Shuffle {
        why: u8,
    },
    PayOrLet {
        item: u16,
        stage: u8,
    },
    Mode {
        item: u16,
        execution: u8,
    },
    Name {
        item: u16,
        kind: NameKind,
        stage: u8,
    },
}

impl PromptWhy {
    pub fn kind(self) -> PromptKind {
        match self {
            PromptWhy::Mulligan | PromptWhy::GroupMove { .. } | PromptWhy::Discard { .. } => {
                PromptKind::Cards
            }
            PromptWhy::Target { .. }
            | PromptWhy::PlayLocation { .. }
            | PromptWhy::PickStaged
            | PromptWhy::Resume { .. }
            | PromptWhy::Mode { .. }
            | PromptWhy::Name { .. }
            | PromptWhy::PayWith { .. } => PromptKind::Options,
            PromptWhy::OrderTriggers { .. } => PromptKind::Order,
            PromptWhy::OptionalCost { .. } | PromptWhy::PayOrLet { .. } => PromptKind::Confirm,
            PromptWhy::Assign => PromptKind::Assign,
            PromptWhy::Shuffle { .. } => PromptKind::Roll,
        }
    }

    pub fn item(self) -> Option<u16> {
        match self {
            PromptWhy::Target { item, .. }
            | PromptWhy::PlayLocation { item }
            | PromptWhy::OptionalCost { item, .. }
            | PromptWhy::Resume { item, .. }
            | PromptWhy::PayWith { item }
            | PromptWhy::Discard { item, .. }
            | PromptWhy::PayOrLet { item, .. }
            | PromptWhy::Mode { item, .. }
            | PromptWhy::Name { item, .. } => Some(item),
            _ => None,
        }
    }

    fn write(self, writer: &mut Writer) {
        let (tag, a, b): (u64, u64, u64) = match self {
            PromptWhy::Mulligan => (0, 0, 0),
            PromptWhy::Target { item, spec } => (1, u64::from(item), u64::from(spec)),
            PromptWhy::PlayLocation { item } => (2, u64::from(item), 0),
            PromptWhy::OptionalCost { item, cost } => (3, u64::from(item), u64::from(cost)),
            PromptWhy::OrderTriggers { seat } => (4, u64::from(seat), 0),
            PromptWhy::PickStaged => (5, 0, 0),
            PromptWhy::GroupMove { unit, to } => (6, u64::from(unit), u64::from(to)),
            PromptWhy::Assign => (7, 0, 0),
            PromptWhy::Resume { item, stage } => (8, u64::from(item), u64::from(stage)),
            PromptWhy::PayWith { item } => (10, u64::from(item), 0),
            PromptWhy::Discard { item, stage } => (11, u64::from(item), u64::from(stage)),
            PromptWhy::Shuffle { why } => (12, u64::from(why), 0),
            PromptWhy::PayOrLet { item, stage } => (13, u64::from(item), u64::from(stage)),
            PromptWhy::Mode { item, execution } => (14, u64::from(item), u64::from(execution)),
            PromptWhy::Name { item, kind, stage } => (
                PROMPT_NAME_TAG + kind.code(),
                u64::from(item),
                u64::from(stage),
            ),
        };
        writer.array(3);
        writer.unsigned(tag);
        writer.unsigned(a);
        writer.unsigned(b);
    }

    pub fn each() -> Vec<Self> {
        (0..64u64)
            .filter_map(|tag| Self::of_parts(tag, 1, 1))
            .collect()
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 3)?;
        let tag = reader.unsigned()?;
        let a = reader.unsigned()?;
        let b = reader.unsigned()?;
        Self::of_parts(tag, a, b)
    }

    fn of_parts(tag: u64, a: u64, b: u64) -> Option<Self> {
        let item = || u16::try_from(a).ok();
        let small = || u8::try_from(b).ok();
        Some(match tag {
            0 => PromptWhy::Mulligan,
            1 => PromptWhy::Target {
                item: item()?,
                spec: small()?,
            },
            2 => PromptWhy::PlayLocation { item: item()? },
            3 => PromptWhy::OptionalCost {
                item: item()?,
                cost: small()?,
            },
            4 => PromptWhy::OrderTriggers {
                seat: u8::try_from(a).ok()?,
            },
            5 => PromptWhy::PickStaged,
            6 => PromptWhy::GroupMove {
                unit: u32::try_from(a).ok()?,
                to: u16::try_from(b).ok()?,
            },
            7 => PromptWhy::Assign,
            8 => PromptWhy::Resume {
                item: item()?,
                stage: small()?,
            },
            10 => PromptWhy::PayWith { item: item()? },
            11 => PromptWhy::Discard {
                item: item()?,
                stage: small()?,
            },
            12 => PromptWhy::Shuffle {
                why: u8::try_from(a).ok()?,
            },
            13 => PromptWhy::PayOrLet {
                item: item()?,
                stage: small()?,
            },
            14 => PromptWhy::Mode {
                item: item()?,
                execution: small()?,
            },
            PROMPT_NAME_TAG..=16 => PromptWhy::Name {
                item: item()?,
                kind: NameKind::from_code(tag - PROMPT_NAME_TAG)?,
                stage: small()?,
            },
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub prompt: Prompt,
    pub why: PromptWhy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSource {
    SpellOrAbility,
    Combat,
    Any,
}

impl DamageSource {
    fn code(self) -> u8 {
        match self {
            DamageSource::SpellOrAbility => 0,
            DamageSource::Combat => 1,
            DamageSource::Any => 2,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(DamageSource::SpellOrAbility),
            1 => Some(DamageSource::Combat),
            2 => Some(DamageSource::Any),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amount {
    All,
    Next,
    N(u8),
}

impl Amount {
    pub fn spent(self) -> bool {
        matches!(self, Amount::N(0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prevention {
    pub source: DamageSource,
    pub value: Amount,
    pub until: Expiry,
    pub unit: Option<u32>,
}

impl Prevention {
    fn write(&self, writer: &mut Writer) {
        writer.array(4);
        card_or_null(writer, self.unit);
        writer.unsigned(u64::from(self.source.code()));
        match self.value {
            Amount::All => writer.null(),
            Amount::Next => writer.bool(true),
            Amount::N(n) => writer.unsigned(u64::from(n)),
        }
        self.until.write(writer);
    }

    fn read_version(reader: &mut Reader, version: u64) -> Option<Self> {
        fixed(reader, if version >= 13 { 4 } else { 3 })?;
        let unit = if version >= 13 {
            card_of(reader)?
        } else {
            None
        };
        let source = DamageSource::from_code(u8_of(reader)?)?;
        let value = match reader.item()? {
            Item::Simple(22) => Amount::All,
            Item::Simple(21) if version >= 13 => Amount::Next,
            Item::Unsigned(n) => Amount::N(u8::try_from(n).ok()?),
            _ => return None,
        };
        Some(Self {
            source,
            value,
            until: Expiry::read(reader)?,
            unit,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Death {
    pub card: u32,
    pub controller: u8,
    pub unit: bool,
    pub phase: Phase,
}

impl Death {
    fn write(&self, writer: &mut Writer) {
        writer.array(4);
        writer.unsigned(u64::from(self.card));
        writer.unsigned(u64::from(self.controller));
        writer.bool(self.unit);
        writer.unsigned(u64::from(self.phase.code()));
    }

    fn read(reader: &mut Reader) -> Option<Self> {
        fixed(reader, 4)?;
        Some(Self {
            card: u32_of(reader)?,
            controller: u8_of(reader)?,
            unit: bool_of(reader)?,
            phase: Phase::from_code(u8_of(reader)?)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameBlob {
    pub mode: Mode,
    pub manual: bool,
    pub lobby: Option<Roll>,
    pub roll: Option<InGameRoll>,
    pub turn: Option<TurnCore>,
    pub seats: Vec<SeatState>,
    pub cards: Vec<CardState>,
    pub control: Vec<Control>,
    pub chain: Vec<ChainItem>,
    pub queue: Vec<Pending>,
    pub delayed: Vec<Delayed>,
    pub prompt: Option<Prompt>,
    pub why: Option<PromptWhy>,
    pub priority: Option<Priority>,
    pub showdown: Option<Showdown>,
    pub staged: Vec<Staged>,
    pub free_table: Option<u8>,
    pub log: Vec<String>,
    pub next_prompt: u16,
    pub next_item: u16,
    pub extra_turns: Vec<u8>,
    pub preventions: Vec<Prevention>,
    pub conceded: Vec<u8>,
    pub won: Option<u8>,
    pub deaths_this_turn: Vec<Death>,
    pub excess: Vec<Excess>,
}

impl GameBlob {
    pub fn lobby(players: u8) -> Self {
        Self::lobby_in(players, Mode::Free)
    }

    pub fn lobby_in(players: u8, mode: Mode) -> Self {
        Self {
            mode,
            lobby: Some(Roll::new(players.max(1), DIE_SIDES)),
            ..Self::default()
        }
    }

    pub fn start(players: u8, first: u8, mode: Mode) -> Self {
        Self {
            mode,
            turn: Some(TurnCore::start(players, first)),
            ..Self::default()
        }
    }

    pub fn is_playing(&self) -> bool {
        self.turn.is_some()
    }

    pub fn is_enforced(&self) -> bool {
        self.mode == Mode::Enforced
    }

    pub fn has_conceded(&self, seat: u8) -> bool {
        self.conceded.contains(&seat)
    }

    pub fn concede(&mut self, seat: u8) -> bool {
        if self.has_conceded(seat) {
            return false;
        }
        self.conceded.push(seat);
        self.conceded.sort_unstable();
        true
    }

    pub fn conceded_winner(&self) -> Option<u8> {
        if self.conceded.is_empty() {
            return None;
        }
        let mut standing = (0..self.players()).filter(|seat| !self.has_conceded(*seat));
        let winner = standing.next()?;
        standing.next().is_none().then_some(winner)
    }

    pub fn roll(&self) -> Option<&Roll> {
        self.lobby.as_ref()
    }

    pub fn core(&self) -> Option<&TurnCore> {
        self.turn.as_ref()
    }

    pub fn core_mut(&mut self) -> Option<&mut TurnCore> {
        self.turn.as_mut()
    }

    pub fn players(&self) -> u8 {
        match (&self.turn, &self.lobby) {
            (Some(core), _) => core.players.max(1),
            (None, Some(roll)) => roll.players().max(1),
            (None, None) => 1,
        }
    }

    pub fn turn(&self) -> u16 {
        self.turn.map(|core| core.turn).unwrap_or(0)
    }

    pub fn turn_player(&self) -> u8 {
        self.turn.map(|core| core.player).unwrap_or(0)
    }

    pub fn phase(&self) -> Option<Phase> {
        self.turn.map(|core| core.phase)
    }

    pub fn set_phase(&mut self, phase: Phase) {
        if let Some(core) = self.turn.as_mut() {
            core.phase = phase;
        }
    }

    pub fn order(&self) -> TurnOrder {
        self.turn
            .map(|core| core.order())
            .unwrap_or_else(|| TurnOrder::start(self.players(), 0))
    }

    pub fn is_turn_player(&self, seat: u8) -> bool {
        self.turn.is_some_and(|core| core.player == seat)
    }

    pub fn has_focus(&self, seat: u8) -> bool {
        self.showdown
            .as_ref()
            .is_some_and(|showdown| showdown.window.has_focus(seat))
    }

    pub fn is_neutral_open(&self) -> bool {
        self.phase() == Some(Phase::Action)
            && self.showdown.is_none()
            && self.priority.is_none()
            && self.chain.is_empty()
            && self.prompt.is_none()
    }

    pub fn seat(&self, seat: u8) -> &SeatState {
        match self.seats.get(usize::from(seat)) {
            Some(seat) => seat,
            None => fresh_seat(),
        }
    }

    pub fn seat_mut(&mut self, seat: u8) -> &mut SeatState {
        let wanted = usize::from(seat) + 1;
        if self.seats.len() < wanted {
            self.seats.resize(wanted, SeatState::default());
        }
        &mut self.seats[usize::from(seat)]
    }

    pub fn card_state(&self, card: u32) -> Option<&CardState> {
        self.cards
            .binary_search_by_key(&card, |held| held.id)
            .ok()
            .map(|index| &self.cards[index])
    }

    pub fn card_state_mut(&mut self, card: u32) -> &mut CardState {
        let index = match self.cards.binary_search_by_key(&card, |held| held.id) {
            Ok(index) => index,
            Err(index) => {
                self.cards.insert(index, CardState::new(card));
                index
            }
        };
        &mut self.cards[index]
    }

    pub fn drop_card_state(&mut self, card: u32) {
        self.cards.retain(|held| held.id != card);
    }

    pub fn has_flag(&self, card: u32, flag: u16) -> bool {
        self.card_state(card).is_some_and(|held| held.has(flag))
    }

    pub fn set_flag(&mut self, card: u32, flag: u16, on: bool) {
        if !on && self.card_state(card).is_none() {
            return;
        }
        self.card_state_mut(card).set(flag, on);
    }

    pub fn named(&self, card: u32) -> Option<&str> {
        self.card_state(card).and_then(|held| held.named.as_deref())
    }

    pub fn set_named(&mut self, card: u32, name: Option<String>) {
        if name.is_none() && self.card_state(card).is_none() {
            return;
        }
        self.card_state_mut(card).named = name;
        if self.card_state(card).is_some_and(CardState::is_default) {
            self.drop_card_state(card);
        }
    }

    pub fn holder(&self, zone: u16) -> Option<u8> {
        self.control
            .iter()
            .find(|control| control.zone == zone)
            .and_then(|control| control.holder)
    }

    pub fn contester(&self, zone: u16) -> Option<u8> {
        self.control
            .iter()
            .find(|control| control.zone == zone)
            .and_then(|control| control.contested)
    }

    pub fn scored(&self, zone: u16, seat: u8) -> bool {
        self.control
            .iter()
            .find(|control| control.zone == zone)
            .is_some_and(|control| control.scored & (1 << (seat & 7)) != 0)
    }

    pub fn slot(&mut self, zone: u16) -> &mut Control {
        let index = match self
            .control
            .binary_search_by_key(&zone, |control| control.zone)
        {
            Ok(index) => index,
            Err(index) => {
                self.control.insert(index, Control::new(zone));
                index
            }
        };
        &mut self.control[index]
    }

    pub fn set_holder(&mut self, zone: u16, holder: Option<u8>) {
        let known = self.control.iter().any(|control| control.zone == zone);
        if holder.is_some() || known {
            self.slot(zone).holder = holder;
        }
    }

    pub fn set_contested(&mut self, zone: u16, contester: Option<u8>) {
        let known = self.control.iter().any(|control| control.zone == zone);
        if contester.is_some() || known {
            self.slot(zone).contested = contester;
        }
    }

    pub fn mark_scored(&mut self, zone: u16, seat: u8) {
        self.slot(zone).scored |= 1 << (seat & 7);
    }

    pub fn clear_scored(&mut self) {
        for control in &mut self.control {
            control.scored = 0;
        }
    }

    pub fn excess_in_attack(&self, seat: u8, zone: u16) -> Option<u8> {
        self.excess
            .iter()
            .find(|row| row.seat == seat && row.zone == zone)
            .map(|row| row.amount)
    }

    pub fn record_excess(&mut self, seat: u8, zone: u16, amount: u8) {
        match self
            .excess
            .iter_mut()
            .find(|row| row.seat == seat && row.zone == zone)
        {
            Some(row) => row.amount = amount,
            None => {
                self.excess.push(Excess { seat, zone, amount });
                self.excess.sort_by_key(|row| (row.seat, row.zone));
            }
        }
    }

    pub fn clear_excess(&mut self) {
        self.excess.clear();
    }

    pub fn held_zones(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        self.control
            .iter()
            .filter_map(|control| control.holder.map(|holder| (control.zone, holder)))
    }

    pub fn contested_zones(&self) -> impl Iterator<Item = (u16, u8)> + '_ {
        self.control
            .iter()
            .filter_map(|control| control.contested.map(|seat| (control.zone, seat)))
    }

    pub fn note_play(&mut self, seat: u8) -> bool {
        let Some(mut showdown) = self.showdown.clone() else {
            return false;
        };
        if !showdown.window.has_focus(seat) {
            return false;
        }
        showdown.window.play(&self.order());
        self.showdown = Some(showdown);
        true
    }

    pub fn narrate(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        let overflow = self.log.len().saturating_sub(NARRATION_LINES);
        self.log.drain(..overflow);
    }

    pub fn next_prompt_id(&mut self) -> u16 {
        self.next_prompt = self.next_prompt.wrapping_add(1);
        self.next_prompt
    }

    pub fn next_item_id(&mut self) -> u16 {
        self.next_item = self.next_item.wrapping_add(1);
        self.next_item
    }

    pub fn pending(&self, item: u16) -> Option<&Pending> {
        self.queue.iter().find(|pending| pending.item.id == item)
    }

    pub fn pending_mut(&mut self, item: u16) -> Option<&mut Pending> {
        self.queue
            .iter_mut()
            .find(|pending| pending.item.id == item)
    }

    pub fn take_pending(&mut self, item: u16) -> Option<Pending> {
        let index = self
            .queue
            .iter()
            .position(|pending| pending.item.id == item)?;
        Some(self.queue.remove(index))
    }

    pub fn open_prompt(&mut self, ask: Ask) {
        self.prompt = Some(ask.prompt);
        self.why = Some(ask.why);
    }

    pub fn close_prompt(&mut self) -> Option<(Prompt, PromptWhy)> {
        let prompt = self.prompt.take()?;
        let why = self.why.take();
        Some((prompt, why?))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut map = MapWriter::new();
        map.field("v").unsigned(BLOB_VERSION);
        if self.manual {
            map.field("manual").bool(true);
        }
        if self.mode != Mode::Free {
            map.field("m").unsigned(u64::from(self.mode.code()));
        }
        if let Some(roll) = &self.lobby {
            map.field("l").bytes(&roll.encode());
        }
        if let Some(roll) = &self.roll {
            roll.write(map.field("r"));
        }
        if let Some(core) = &self.turn {
            core.write(map.field("t"));
        }
        if self.seats.iter().any(|seat| *seat != SeatState::default()) {
            let writer = map.field("s");
            writer.array(self.seats.len());
            for seat in &self.seats {
                seat.write(writer);
            }
        }
        let rows: Vec<&CardState> = self.cards.iter().filter(|row| !row.is_default()).collect();
        if !rows.is_empty() {
            let writer = map.field("c");
            writer.array(rows.len());
            for row in rows {
                row.write(writer);
            }
        }
        if !self.control.is_empty() {
            let writer = map.field("k");
            writer.array(self.control.len());
            for control in &self.control {
                control.write(writer);
            }
        }
        if !self.chain.is_empty() {
            let writer = map.field("ch");
            writer.array(self.chain.len());
            for item in &self.chain {
                item.write(writer);
            }
        }
        if !self.queue.is_empty() {
            let writer = map.field("q");
            writer.array(self.queue.len());
            for pending in &self.queue {
                pending.write(writer);
            }
        }
        if !self.delayed.is_empty() {
            let writer = map.field("d");
            writer.array(self.delayed.len());
            for delayed in &self.delayed {
                delayed.write(writer);
            }
        }
        if let Some(prompt) = &self.prompt {
            prompt.write(map.field("p"));
        }
        if let Some(why) = self.why {
            why.write(map.field("pw"));
        }
        if let Some(priority) = &self.priority {
            priority.write(map.field("pr"));
        }
        if let Some(showdown) = &self.showdown {
            showdown.write(map.field("sd"));
        }
        if !self.staged.is_empty() {
            let writer = map.field("st");
            writer.array(self.staged.len());
            for staged in &self.staged {
                staged.write(writer);
            }
        }
        if let Some(seat) = self.free_table {
            map.field("ft").unsigned(u64::from(seat));
        }
        if !self.log.is_empty() {
            let writer = map.field("lg");
            writer.array(self.log.len());
            for line in &self.log {
                writer.text(line);
            }
        }
        if self.next_prompt != 0 {
            map.field("np").unsigned(u64::from(self.next_prompt));
        }
        if self.next_item != 0 {
            map.field("ni").unsigned(u64::from(self.next_item));
        }
        if !self.extra_turns.is_empty() {
            let writer = map.field("xt");
            writer.array(self.extra_turns.len());
            for seat in &self.extra_turns {
                writer.unsigned(u64::from(*seat));
            }
        }
        if !self.preventions.is_empty() {
            let writer = map.field("pv");
            writer.array(self.preventions.len());
            for prevention in &self.preventions {
                prevention.write(writer);
            }
        }
        if !self.conceded.is_empty() {
            let writer = map.field("cc");
            writer.array(self.conceded.len());
            for seat in &self.conceded {
                writer.unsigned(u64::from(*seat));
            }
        }
        if let Some(seat) = self.won {
            map.field("wn").unsigned(u64::from(seat));
        }
        if !self.deaths_this_turn.is_empty() {
            let writer = map.field("dt");
            writer.array(self.deaths_this_turn.len());
            for death in &self.deaths_this_turn {
                death.write(writer);
            }
        }
        if !self.excess.is_empty() {
            let writer = map.field("xd");
            writer.array(self.excess.len());
            for excess in &self.excess {
                excess.write(writer);
            }
        }
        map.finish()
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let version = match discriminator(bytes) {
            Some(("v", version @ (7 | 9 | 10 | 11 | 12 | 13 | 14))) => version,
            _ => return None,
        };
        let mut reader = Reader::new(bytes);
        let mut map = MapReader::open(&mut reader)?;
        let mut blob = GameBlob::default();
        while let Some(key) = map.field() {
            match key {
                "v" => {
                    map.value().unsigned()?;
                }
                "m" => blob.mode = Mode::from_code(u8_of(map.value())?)?,
                "l" => blob.lobby = Some(Roll::decode(map.value().bytes()?)?),
                "r" => blob.roll = Some(InGameRoll::read(map.value())?),
                "t" => blob.turn = Some(TurnCore::read(map.value())?),
                "s" => {
                    for _ in 0..map.value().array_len()? {
                        blob.seats
                            .push(SeatState::read_version(map.value(), version)?);
                    }
                }
                "c" => {
                    for _ in 0..map.value().array_len()? {
                        blob.cards
                            .push(CardState::read_version(map.value(), version)?);
                    }
                }
                "k" => {
                    for _ in 0..map.value().array_len()? {
                        blob.control.push(Control::read(map.value())?);
                    }
                }
                "ch" => {
                    for _ in 0..map.value().array_len()? {
                        blob.chain
                            .push(ChainItem::read_version(map.value(), version)?);
                    }
                }
                "q" => {
                    for _ in 0..map.value().array_len()? {
                        blob.queue
                            .push(Pending::read_version(map.value(), version)?);
                    }
                }
                "d" => {
                    for _ in 0..map.value().array_len()? {
                        blob.delayed.push(Delayed::read(map.value())?);
                    }
                }
                "p" => blob.prompt = Some(Prompt::read(map.value())?),
                "pw" => blob.why = Some(PromptWhy::read(map.value())?),
                "pr" => blob.priority = Some(Priority::read(map.value())?),
                "sd" => blob.showdown = Some(Showdown::read(map.value())?),
                "st" => {
                    for _ in 0..map.value().array_len()? {
                        blob.staged.push(Staged::read(map.value())?);
                    }
                }
                "ft" => blob.free_table = Some(u8_of(map.value())?),
                "manual" => blob.manual = bool_of(map.value())?,
                "lg" => {
                    for _ in 0..map.value().array_len()? {
                        blob.log.push(text_of(map.value())?);
                    }
                }
                "np" => blob.next_prompt = u16_of(map.value())?,
                "ni" => blob.next_item = u16_of(map.value())?,
                "xt" => {
                    for _ in 0..map.value().array_len()? {
                        blob.extra_turns.push(u8_of(map.value())?);
                    }
                }
                "pv" => {
                    for _ in 0..map.value().array_len()? {
                        blob.preventions
                            .push(Prevention::read_version(map.value(), version)?);
                    }
                }
                "cc" => {
                    for _ in 0..map.value().array_len()? {
                        blob.conceded.push(u8_of(map.value())?);
                    }
                }
                "wn" => blob.won = Some(u8_of(map.value())?),
                "dt" => {
                    for _ in 0..map.value().array_len()? {
                        blob.deaths_this_turn.push(Death::read(map.value())?);
                    }
                }
                "xd" => {
                    if version < 14 {
                        map.skip_unknown()?;
                        continue;
                    }
                    let mut rows = Vec::new();
                    for _ in 0..map.value().array_len()? {
                        rows.push(Excess::read(map.value())?);
                    }
                    rows.sort_by_key(|row| (row.seat, row.zone));
                    if rows
                        .windows(2)
                        .any(|pair| (pair[0].seat, pair[0].zone) == (pair[1].seat, pair[1].zone))
                    {
                        return None;
                    }
                    blob.excess = rows;
                }
                _ => map.skip_unknown()?,
            }
        }
        map.finish()?;
        blob.cards.sort_by_key(|row| row.id);
        blob.control.sort_by_key(|control| control.zone);
        Some(blob)
    }
}

fn seat_or_null(writer: &mut Writer, seat: Option<u8>) {
    match seat {
        Some(seat) => writer.unsigned(u64::from(seat)),
        None => writer.null(),
    }
}

fn card_or_null(writer: &mut Writer, card: Option<u32>) {
    match card {
        Some(card) => writer.unsigned(u64::from(card)),
        None => writer.null(),
    }
}

fn zone_or_null(writer: &mut Writer, zone: Option<u16>) {
    match zone {
        Some(zone) => writer.unsigned(u64::from(zone)),
        None => writer.null(),
    }
}

fn fixed(reader: &mut Reader, len: usize) -> Option<()> {
    (reader.array_len()? == len).then_some(())
}

fn u8_of(reader: &mut Reader) -> Option<u8> {
    u8::try_from(reader.unsigned()?).ok()
}

fn u16_of(reader: &mut Reader) -> Option<u16> {
    u16::try_from(reader.unsigned()?).ok()
}

fn u32_of(reader: &mut Reader) -> Option<u32> {
    u32::try_from(reader.unsigned()?).ok()
}

fn signed_of(reader: &mut Reader) -> Option<i32> {
    match reader.item()? {
        Item::Unsigned(value) => i32::try_from(value).ok(),
        Item::Negative(value) => i32::try_from(-1 - i64::try_from(value).ok()?).ok(),
        _ => None,
    }
}

fn seat_of(reader: &mut Reader) -> Option<Option<u8>> {
    match reader.item()? {
        Item::Unsigned(seat) => Some(Some(u8::try_from(seat).ok()?)),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

fn card_of(reader: &mut Reader) -> Option<Option<u32>> {
    match reader.item()? {
        Item::Unsigned(card) => Some(Some(u32::try_from(card).ok()?)),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

fn zone_of(reader: &mut Reader) -> Option<Option<u16>> {
    match reader.item()? {
        Item::Unsigned(zone) => Some(Some(u16::try_from(zone).ok()?)),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

fn bool_of(reader: &mut Reader) -> Option<bool> {
    match reader.item()? {
        Item::Simple(21) => Some(true),
        Item::Simple(20) => Some(false),
        _ => None,
    }
}

fn text_of(reader: &mut Reader) -> Option<String> {
    match reader.item()? {
        Item::Text(text) => Some(text.to_string()),
        _ => None,
    }
}

fn text_or_null(writer: &mut Writer, text: Option<&str>) {
    match text {
        Some(text) => writer.text(text),
        None => writer.null(),
    }
}

fn text_or_none(reader: &mut Reader) -> Option<Option<String>> {
    match reader.item()? {
        Item::Text(text) => Some(Some(text.to_string())),
        Item::Simple(22) => Some(None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_plugin_sdk::dice::commitment;

    #[test]
    fn a_fresh_lobby_and_a_started_game_round_trip_through_the_blob() {
        let lobby = GameBlob::lobby(2);
        let bytes = lobby.encode();
        assert_eq!(discriminator(&bytes), Some(("v", BLOB_VERSION)));
        assert_eq!(GameBlob::decode(&bytes), Some(lobby.clone()));
        assert!(!lobby.is_playing());
        assert_eq!(lobby.players(), 2);
        let mut committed = lobby;
        committed
            .lobby
            .as_mut()
            .unwrap()
            .commit(1, commitment(&[3; 8]))
            .unwrap();
        assert_eq!(GameBlob::decode(&committed.encode()), Some(committed));
        let started = GameBlob::start(2, 1, Mode::Enforced);
        let bytes = started.encode();
        assert_eq!(GameBlob::decode(&bytes), Some(started.clone()));
        assert!(started.is_playing());
        assert!(started.is_enforced());
        assert!(started.is_neutral_open());
        assert_eq!(
            (started.turn(), started.turn_player(), started.phase()),
            (1, 1, Some(Phase::Action))
        );
        assert_eq!(started.core().unwrap().first, 1);
        assert!(bytes.len() < 40, "a quiet blob is small: {}", bytes.len());
        assert_eq!(GameBlob::default().encode(), [0xa1, 0x61, b'v', 0x0e]);
    }

    #[test]
    fn every_field_survives_the_round_trip_and_defaults_are_omitted() {
        let mut busy = GameBlob::start(2, 0, Mode::Free);
        busy.core_mut().unwrap().turn = 7;
        busy.core_mut().unwrap().phase = Phase::Cleanup;
        busy.showdown = Some(Showdown {
            window: PassWindow {
                focus: 1,
                passes: 1,
            },
            combat: true,
            ..Showdown::open(10, 0, 1)
        });
        busy.set_holder(10, Some(1));
        busy.mark_scored(10, 1);
        busy.mark_scored(9, 0);
        busy.slot(11).contested = Some(0);
        busy.prompt = Some(Prompt::new(3, 1, 0, 2).cancellable());
        busy.free_table = Some(1);
        busy.next_prompt = 3;
        for line in [
            "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
            "eleven", "twelve", "thirteen",
        ] {
            busy.narrate(line);
        }
        assert_eq!(
            busy.log,
            [
                "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
                "twelve", "thirteen"
            ],
            "the window holds NARRATION_LINES lines"
        );
        let bytes = busy.encode();
        assert_eq!(GameBlob::decode(&bytes), Some(busy.clone()));
        assert_eq!(busy.holder(10), Some(1));
        assert!(busy.scored(10, 1));
        assert!(!busy.scored(10, 0));
        assert!(busy.scored(9, 0));
        assert_eq!(busy.held_zones().collect::<Vec<_>>(), [(10, 1)]);
        assert_eq!(busy.contested_zones().collect::<Vec<_>>(), [(11, 0)]);
        assert_eq!(
            busy.control.iter().map(|c| c.zone).collect::<Vec<_>>(),
            [9, 10, 11]
        );
        busy.clear_scored();
        assert!(!busy.scored(9, 0));
        assert_eq!(busy.holder(10), Some(1));
        let mut untouched = GameBlob::start(2, 0, Mode::Free);
        untouched.set_holder(9, None);
        untouched.set_contested(9, None);
        assert!(untouched.control.is_empty());
        untouched.set_flag(5, FLAG_STUNNED, false);
        assert!(untouched.cards.is_empty());
        let quiet = untouched.encode();
        for key in [
            "m", "k", "sd", "p", "pw", "ft", "lg", "np", "l", "r", "s", "c", "ch", "q", "d", "pr",
            "st", "ni", "xt", "pv",
        ] {
            let needle = [&[0x60 | key.len() as u8], key.as_bytes()].concat();
            assert!(
                !quiet.windows(needle.len()).any(|window| window == needle),
                "{key} is written for a quiet blob"
            );
        }
    }

    #[test]
    fn the_engine_fields_round_trip_and_default_rows_are_dropped() {
        let mut busy = GameBlob::start(2, 0, Mode::Enforced);
        busy.set_phase(Phase::Setup);
        busy.seat_mut(1).setup = SetupStage::Drawn;
        busy.seat_mut(1).draws = 2;
        busy.seat_mut(0).played_main = true;
        busy.seat_mut(0).play_lock = PlayLock::CARDS;
        busy.seat_mut(0).looks_facedown_of = 2;
        busy.seat_mut(0).cards_played = 2;
        busy.seat_mut(0).spells_played = 1;
        busy.seat_mut(0).gear_played = 1;
        busy.seat_mut(0).gear_abilities_activated = 1;
        busy.seat_mut(1).promises = vec![Promise {
            kind: PromiseKind::Any,
            effect: PromiseEffect::Discount(Pool {
                energy: 2,
                power: vec![Pooled::Rainbow; 2],
            }),
            until: Expiry::Permanent,
        }];
        busy.seat_mut(1).pool = Pool {
            energy: 1,
            power: vec![Pooled::Rainbow, Pooled::Domain(Domain::Fury)],
        };
        busy.seat_mut(1).chosen_champion = Some("Jinx - Rebel".into());
        busy.seat_mut(0).units_enter_ready_this_turn = true;
        busy.seat_mut(1).next_unit_enters_ready = true;
        busy.deaths_this_turn.push(Death {
            card: 30,
            controller: 1,
            unit: true,
            phase: Phase::Beginning,
        });
        busy.deaths_this_turn.push(Death {
            card: 31,
            controller: 0,
            unit: false,
            phase: Phase::Action,
        });
        busy.roll = Some(InGameRoll {
            id: 4,
            roll: Roll::new(2, DIE_SIDES),
            why: 1,
        });
        {
            let row = busy.card_state_mut(30);
            row.set(FLAG_STUNNED, true);
            row.set(FLAG_ENTERED_THIS_TURN, true);
            row.might.push(MightMod {
                delta: -2,
                until: Expiry::EndOfTurn(3),
                src: 9,
            });
            row.might.push(MightMod {
                delta: 1,
                until: Expiry::WhileAttached(31),
                src: 0,
            });
            row.granted
                .push((Keyword::Ganking, Expiry::WhileAttached(31)));
            row.granted_costed.push(CostedGrant {
                kind: CostedKind::Flow,
                energy: 2,
                power: vec![
                    Power::Own,
                    Power::Domain(crate::cards::Domain::Chaos),
                    Power::Rainbow,
                ],
                until: Expiry::EndOfTurn(3),
            });
            row.attached_to = Some(31);
            row.hidden_at = Some(10);
            row.hidden_since = 2;
            row.entered = 3;
            row.attached_turn = 3;
        }
        busy.card_state_mut(10);
        busy.card_state_mut(20).set(FLAG_DEFENDER, true);
        {
            let stolen = busy.card_state_mut(25);
            stolen.controlled_by = Some(1);
            stolen.control_source = Some(30);
        }
        busy.card_state_mut(27).named = Some("Defy".to_string());
        busy.card_state_mut(26)
            .set(FLAG_SHROUDED | once_by_seat(1), true);
        let mut item = ChainItem::new(
            2,
            ItemKind::Spell { card: 40 },
            1,
            Origin::Facedown { zone: 9 },
        );
        item.status = ItemStatus::Finalized;
        item.targets = vec![
            TargetRef::Card(3),
            TargetRef::Seat(1),
            TargetRef::Zone(9),
            TargetRef::Item(1),
        ];
        item.spec_counts = vec![1, 0, 1];
        item.set_slot(SLOT_ACCELERATE, 1);
        item.set_slot(SLOT_TRIGGER_COST, 0);
        item.set_slot(SLOT_REPEAT, 1);
        item.stage = 2;
        item.noted = Some(Noted {
            zone: 9,
            might: -1,
            controller: 1,
            alone: true,
            buffed: true,
        });
        item.execution = 1;
        item.awaiting = vec![41, 42];
        busy.chain.push(item.clone());
        busy.chain.push(ChainItem::new(
            9,
            ItemKind::Spell { card: 44 },
            0,
            Origin::Trash {
                leave: Leave::Recycle,
            },
        ));
        busy.chain.push(ChainItem::new(
            10,
            ItemKind::Permanent { card: 45 },
            1,
            Origin::Banishment,
        ));
        busy.chain.push(ChainItem::new(
            11,
            ItemKind::Granted {
                holder: 46,
                lender: 47,
                index: 129,
            },
            0,
            Origin::Board,
        ));
        busy.chain.push(ChainItem::new(
            12,
            ItemKind::Lent {
                holder: 48,
                lender: 49,
                index: 2,
            },
            1,
            Origin::Board,
        ));
        busy.queue.push(Pending {
            item: ChainItem::new(
                3,
                ItemKind::Trigger {
                    source: 5,
                    index: 1,
                },
                0,
                Origin::Board,
            ),
            needs: Needs::OptionalCost,
        });
        busy.queue.push(Pending {
            item: ChainItem::new(
                4,
                ItemKind::Ability {
                    source: 6,
                    index: 0,
                },
                0,
                Origin::Champion,
            ),
            needs: Needs::Legality,
        });
        busy.queue.push(Pending {
            item: ChainItem::new(5, ItemKind::Permanent { card: 7 }, 0, Origin::Hand),
            needs: Needs::Choices,
        });
        busy.delayed.push(Delayed {
            when: When::EndOfTurn(3),
            source: 8,
            seat: 1,
            ability: 0,
            args: vec![1, 2],
        });
        busy.delayed.push(Delayed {
            when: When::BeginningOf(1),
            source: 8,
            seat: 1,
            ability: 1,
            args: Vec::new(),
        });
        busy.delayed.push(Delayed {
            when: When::AfterKillsBy(2),
            source: 40,
            seat: 1,
            ability: 0,
            args: Vec::new(),
        });
        busy.extra_turns = vec![0, 1];
        busy.conceded = vec![1];
        busy.won = Some(0);
        busy.record_excess(0, 9, 3);
        busy.record_excess(0, 9, 2);
        busy.record_excess(1, 10, 1);
        busy.preventions.push(Prevention {
            unit: None,
            source: DamageSource::SpellOrAbility,
            value: Amount::All,
            until: Expiry::EndOfTurn(7),
        });
        busy.preventions.push(Prevention {
            unit: None,
            source: DamageSource::Combat,
            value: Amount::N(3),
            until: Expiry::CombatEnd,
        });
        busy.prompt = Some(Prompt::new(3, 1, 0, 2));
        busy.why = Some(PromptWhy::GroupMove { unit: 30, to: 10 });
        busy.priority = Some(Priority {
            active: 1,
            passes: 1,
        });
        busy.showdown = Some(Showdown {
            combat: true,
            initial_chain: true,
            stage: ShowdownStage::Damage {
                assigner: 1,
                remaining: 3,
                assigned: vec![(30, 2)],
            },
            ..Showdown::open(9, 0, 1)
        });
        busy.staged.push(Staged {
            zone: 10,
            combat: false,
            contester: 0,
        });
        busy.next_item = 5;
        let bytes = busy.encode();
        let decoded = GameBlob::decode(&bytes).unwrap();
        let mut expected = busy.clone();
        expected.cards.retain(|row| !row.is_default());
        assert_eq!(decoded, expected);
        assert_eq!(
            decoded.cards.iter().map(|row| row.id).collect::<Vec<_>>(),
            [20, 25, 26, 27, 30],
            "a row whose only fact is a control change, a seat's once or a name survives"
        );
        assert_eq!(decoded.card_state(25).unwrap().controlled_by, Some(1));
        assert_eq!(decoded.card_state(25).unwrap().control_source, Some(30));
        assert_eq!(
            decoded.card_state(27).unwrap().named.as_deref(),
            Some("Defy")
        );
        assert_eq!(decoded.card_state(30).unwrap().attached_turn, 3);
        assert!(decoded.has_flag(26, FLAG_SHROUDED));
        assert!(decoded.has_flag(26, once_by_seat(1)));
        assert!(!decoded.has_flag(26, once_by_seat(0)));
        assert_eq!(once_by_seat(3), 1 << 14);
        assert_eq!(once_by_seat(4), once_by_seat(0));
        assert_eq!(
            FLAG_ONCE_BY_SEAT,
            once_by_seat(0) | once_by_seat(1) | once_by_seat(2) | once_by_seat(3)
        );
        assert_eq!(decoded.seat(0).cards_played, 2);
        assert_eq!(decoded.seat(0).spells_played, 1);
        assert_eq!(decoded.seat(0).gear_played, 1);
        assert_eq!(decoded.seat(0).gear_abilities_activated, 1);
        assert_eq!(decoded.seat(1).promises, busy.seat(1).promises);
        assert_eq!(decoded.seat(1).promises.len(), 1);
        assert_eq!(
            decoded.seat(1).pool,
            Pool {
                energy: 1,
                power: vec![Pooled::Rainbow, Pooled::Domain(Domain::Fury)],
            }
        );
        assert_eq!(decoded.chain[0].picks, [1, 0, 1, UNANSWERED]);
        assert!(decoded.chain[0].accelerated());
        assert!(decoded.chain[0].repeated());
        assert!(!decoded.chain[0].paid_additional());
        assert_eq!(decoded.chain[0].slot(SLOT_ADDITIONAL), None);
        assert_eq!(decoded.chain[0].slot(SLOT_TRIGGER_COST), Some(0));
        assert_eq!(decoded.chain[0].execution, 1);
        assert_eq!(decoded.chain[0].awaiting, [41, 42]);
        assert!(decoded.chain[0].noted.unwrap().alone);
        assert!(decoded.chain[0].noted.unwrap().buffed);
        assert_eq!(
            decoded.chain[1].origin,
            Origin::Trash {
                leave: Leave::Recycle
            }
        );
        assert_eq!(decoded.chain[2].origin, Origin::Banishment);
        assert_eq!(decoded.chain[2].picks, [UNANSWERED; SLOTS]);
        assert_eq!(
            decoded.chain[3].kind,
            ItemKind::Granted {
                holder: 46,
                lender: 47,
                index: 129
            }
        );
        assert_eq!(decoded.chain[3].kind.source(), 46);
        assert_eq!(decoded.chain[3].kind.lender(), Some(47));
        assert_eq!(decoded.chain[3].kind.card(), None);
        assert_eq!(
            decoded.chain[4].kind,
            ItemKind::Lent {
                holder: 48,
                lender: 49,
                index: 2
            }
        );
        assert_eq!(decoded.chain[4].kind.source(), 48);
        assert_eq!(decoded.chain[4].kind.lender(), Some(49));
        assert_eq!(decoded.chain[4].kind.ability_index(), Some(2));
        assert_eq!(decoded.chain[4].controller, 1);
        assert_eq!(decoded.delayed[2].when, When::AfterKillsBy(2));
        assert_eq!(decoded.extra_turns, [0, 1]);
        assert_eq!(decoded.preventions, busy.preventions);
        assert_eq!(decoded.conceded, [1]);
        assert_eq!(decoded.won, Some(0));
        assert_eq!(decoded.excess, busy.excess);
        assert_eq!(
            decoded.excess_in_attack(0, 9),
            Some(2),
            "the later attack at a battlefield replaces the earlier one"
        );
        assert_eq!(decoded.excess_in_attack(1, 10), Some(1));
        assert_eq!(decoded.excess_in_attack(1, 9), None);
        busy.clear_excess();
        assert_eq!(busy.excess_in_attack(0, 9), None);
        assert!(decoded.has_flag(30, FLAG_STUNNED));
        assert!(!decoded.has_flag(30, FLAG_ATTACKER));
        assert!(!decoded.has_flag(99, FLAG_STUNNED));
        assert_eq!(decoded.seat(1).draws, 2);
        assert_eq!(
            decoded.seat(1).chosen_champion.as_deref(),
            Some("Jinx - Rebel")
        );
        assert_eq!(decoded.seat(0).chosen_champion, None);
        assert_eq!(*decoded.seat(5), SeatState::default());
        assert_eq!(decoded.pending(4).unwrap().needs, Needs::Legality);
        assert_eq!(decoded.chain[0].zone_target(), Some(9));
        assert_eq!(item.kind.card(), Some(40));
        assert_eq!(
            ItemKind::Ability {
                source: 6,
                index: 0
            }
            .card(),
            None
        );
        assert_eq!(
            ItemKind::Trigger {
                source: 6,
                index: 0
            }
            .source(),
            6
        );
        let mut showdown = Showdown::open(9, 0, 1);
        showdown.stage = ShowdownStage::Resolution;
        busy.showdown = Some(showdown);
        assert_eq!(
            GameBlob::decode(&busy.encode()).unwrap().showdown,
            busy.showdown
        );
        let mut seat = busy.seat(0).clone();
        seat.reset_turn();
        assert_eq!(seat, SeatState::default());
        let mut discounted = busy.seat(1).clone();
        discounted.reset_turn();
        assert_eq!(
            discounted.promises,
            busy.seat(1).promises,
            "a promise persists until it is used or expires"
        );
        assert_eq!(
            discounted.chosen_champion.as_deref(),
            Some("Jinx - Rebel"),
            "the Chosen Champion is chosen for the game"
        );
        assert!(!busy.is_neutral_open());
        let mut taken = busy.clone();
        assert_eq!(taken.take_pending(3).unwrap().needs, Needs::OptionalCost);
        assert_eq!(taken.queue.len(), 2);
        assert_eq!(taken.take_pending(3), None);
        assert_eq!(
            taken.close_prompt(),
            Some((
                Prompt::new(3, 1, 0, 2),
                PromptWhy::GroupMove { unit: 30, to: 10 }
            ))
        );
        assert_eq!(taken.close_prompt(), None);
        taken.drop_card_state(30);
        assert_eq!(taken.card_state(30), None);
    }

    #[test]
    fn every_prompt_why_round_trips_with_its_kind() {
        let whys = [
            PromptWhy::Mulligan,
            PromptWhy::Target { item: 300, spec: 1 },
            PromptWhy::PlayLocation { item: 2 },
            PromptWhy::OptionalCost { item: 2, cost: 1 },
            PromptWhy::OrderTriggers { seat: 1 },
            PromptWhy::PickStaged,
            PromptWhy::GroupMove { unit: 70000, to: 9 },
            PromptWhy::Assign,
            PromptWhy::Resume { item: 3, stage: 2 },
            PromptWhy::PayWith { item: 4 },
            PromptWhy::Discard { item: 4, stage: 1 },
            PromptWhy::Shuffle { why: 2 },
            PromptWhy::PayOrLet { item: 5, stage: 1 },
            PromptWhy::Mode {
                item: 6,
                execution: 1,
            },
            PromptWhy::Name {
                item: 7,
                kind: NameKind::Spell,
                stage: 1,
            },
            PromptWhy::Name {
                item: 7,
                kind: NameKind::Tag,
                stage: 0,
            },
        ];
        for why in whys {
            let mut writer = Writer::new();
            why.write(&mut writer);
            let bytes = writer.finish();
            assert_eq!(PromptWhy::read(&mut Reader::new(&bytes)), Some(why));
            let _ = why.kind();
        }
        assert_eq!(PromptWhy::Mulligan.kind(), PromptKind::Cards);
        assert_eq!(
            PromptWhy::OptionalCost { item: 1, cost: 0 }.kind(),
            PromptKind::Confirm
        );
        assert_eq!(PromptWhy::Shuffle { why: 0 }.kind(), PromptKind::Roll);
        assert_eq!(
            PromptWhy::PayOrLet { item: 5, stage: 1 }.kind(),
            PromptKind::Confirm
        );
        assert_eq!(PromptWhy::PayOrLet { item: 5, stage: 1 }.item(), Some(5));
        assert_eq!(PromptWhy::PlayLocation { item: 7 }.item(), Some(7));
        assert_eq!(PromptWhy::PickStaged.item(), None);
        let mut writer = Writer::new();
        writer.array(3);
        writer.unsigned(99);
        writer.unsigned(0);
        writer.unsigned(0);
        let bytes = writer.finish();
        assert_eq!(PromptWhy::read(&mut Reader::new(&bytes)), None);
        let each = PromptWhy::each();
        assert!(each
            .iter()
            .any(|why| matches!(why, PromptWhy::PayOrLet { .. })));
        assert_eq!(
            each.len(),
            whys.len(),
            "each() decodes one sample of every tag the writer knows"
        );
        for why in &whys {
            assert!(
                each.iter()
                    .any(|sample| std::mem::discriminant(sample) == std::mem::discriminant(why)),
                "{why:?} is enumerated"
            );
        }
        let mut writer = Writer::new();
        writer.array(3);
        writer.unsigned(9);
        writer.unsigned(5);
        writer.unsigned(0);
        assert_eq!(
            PromptWhy::read(&mut Reader::new(&writer.finish())),
            None,
            "tag 9 was the Replacement prompt no code ever raised; it stays retired"
        );
    }

    #[test]
    fn a_seat_row_with_the_old_spell_lock_bool_still_reads() {
        let mut spells = Writer::new();
        spells.array(8);
        spells.unsigned(2);
        spells.unsigned(0);
        spells.bool(false);
        spells.bool(true);
        spells.unsigned(0);
        spells.unsigned(0);
        spells.unsigned(0);
        spells.array(2);
        spells.unsigned(0);
        spells.unsigned(0);
        let row = SeatState::read_version(&mut Reader::new(&spells.finish()), 7).unwrap();
        assert_eq!(row.play_lock, PlayLock::SPELLS);
        let cards = SeatState {
            play_lock: PlayLock::CARDS,
            ..SeatState::default()
        };
        let mut writer = Writer::new();
        cards.write(&mut writer);
        assert_eq!(
            SeatState::read_version(&mut Reader::new(&writer.finish()), BLOB_VERSION),
            Some(cards)
        );
        let mut wide = Writer::new();
        wide.array(9);
        wide.unsigned(2);
        wide.unsigned(0);
        wide.bool(false);
        wide.unsigned(8);
        wide.unsigned(0);
        wide.unsigned(0);
        wide.unsigned(0);
        wide.array(2);
        wide.unsigned(0);
        wide.unsigned(0);
        wide.null();
        assert_eq!(
            SeatState::read_version(&mut Reader::new(&wide.finish()), 7),
            None
        );
        assert!(PlayLock::CARDS.contains(PlayLock::UNITS));
        assert!(!PlayLock::SPELLS.contains(PlayLock::GEAR));
        assert_eq!(
            PlayLock::SPELLS | PlayLock::UNITS | PlayLock::GEAR,
            PlayLock::CARDS
        );
        assert!(PlayLock::NONE.is_empty());
    }

    #[test]
    fn v10_seat_rows_migrate_and_reject_ambiguous_layouts() {
        let mut economy = Writer::new();
        economy.array(11);
        economy.unsigned(2);
        economy.unsigned(1);
        economy.bool(false);
        economy.bool(true);
        economy.unsigned(3);
        economy.unsigned(4);
        economy.unsigned(5);
        economy.unsigned(6);
        economy.unsigned(7);
        economy.array(0);
        Pool::default().write(&mut economy);
        assert!(SeatState::read_version(&mut Reader::new(&economy.finish()), 10).is_none());

        let mut legacy = Writer::new();
        legacy.array(11);
        legacy.unsigned(2);
        legacy.unsigned(1);
        legacy.bool(false);
        legacy.unsigned(0);
        legacy.unsigned(3);
        legacy.unsigned(4);
        legacy.unsigned(5);
        legacy.array(2);
        legacy.unsigned(2);
        legacy.unsigned(1);
        legacy.bool(false);
        legacy.bool(false);
        legacy.null();
        let legacy = SeatState::read_version(&mut Reader::new(&legacy.finish()), 10).unwrap();
        assert_eq!(legacy.play_lock, PlayLock::NONE);
        assert_eq!(legacy.promises.len(), 1);
        assert_eq!(legacy.promises[0].kind, PromiseKind::Any);
        assert_eq!(
            legacy.promises[0].effect,
            PromiseEffect::Discount(Pool {
                energy: 2,
                power: vec![Pooled::Rainbow],
            })
        );
        assert_eq!(legacy.promises[0].until, Expiry::Permanent);
    }

    #[test]
    fn v12_damage_rows_migrate_to_v13_defaults_and_v13_damage_round_trips() {
        let mut writer = Writer::new();
        writer.map(4);
        writer.text("v");
        writer.unsigned(12);
        writer.text("s");
        writer.array(1);
        writer.array(14);
        writer.unsigned(1);
        writer.unsigned(2);
        writer.bool(true);
        writer.unsigned(1);
        writer.unsigned(2);
        writer.unsigned(3);
        writer.unsigned(1);
        writer.array(1);
        writer.array(3);
        writer.unsigned(0);
        writer.array(2);
        writer.unsigned(0);
        writer.array(2);
        writer.unsigned(1);
        writer.array(1);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.bool(true);
        writer.bool(false);
        writer.text("Legacy Champion");
        writer.unsigned(1);
        writer.unsigned(2);
        writer.array(2);
        writer.unsigned(1);
        writer.array(1);
        writer.unsigned(0);
        writer.text("c");
        writer.array(1);
        writer.array(13);
        writer.unsigned(77);
        writer.unsigned(0);
        writer.array(0);
        writer.array(0);
        writer.array(0);
        writer.null();
        writer.null();
        writer.unsigned(0);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.null();
        writer.null();
        writer.null();
        writer.text("pv");
        writer.array(1);
        writer.array(3);
        writer.unsigned(1);
        writer.unsigned(2);
        writer.array(2);
        writer.unsigned(1);
        writer.unsigned(4);
        let old = writer.finish();
        let decoded = GameBlob::decode(&old).expect("independent v12 fixture");
        assert_eq!(decoded.seat(0).draws, 2);
        assert_eq!(decoded.seat(0).promises.len(), 1);
        assert_eq!(decoded.seat(0).pool.energy, 1);
        assert_eq!(
            decoded.seat(0).chosen_champion.as_deref(),
            Some("Legacy Champion")
        );
        assert_eq!(
            decoded.card_state(77).unwrap().damage_multiplier_this_turn,
            0
        );
        assert!(decoded.card_state(77).unwrap().damage_marks.is_empty());
        assert_eq!(
            decoded.preventions,
            [Prevention {
                source: DamageSource::Combat,
                value: Amount::N(2),
                until: Expiry::EndOfTurn(4),
                unit: None,
            }]
        );

        let mut current = decoded;
        current.seat_mut(0).next_spell_bonus = 2;
        current.seat_mut(0).spell_bonus = (9, 2);
        current.card_state_mut(77).damage_multiplier_this_turn = 2;
        current.card_state_mut(77).mark_damage(1, 3);
        current.preventions.push(Prevention {
            unit: Some(77),
            source: DamageSource::Any,
            value: Amount::Next,
            until: Expiry::EndOfTurn(1),
        });
        assert_eq!(GameBlob::decode(&current.encode()), Some(current));
    }

    #[test]
    fn malformed_old_seat_and_unsorted_damage_marks_are_rejected() {
        let mut writer = Writer::new();
        writer.map(2);
        writer.text("v");
        writer.unsigned(11);
        writer.text("s");
        writer.array(1);
        writer.array(13);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.bool(false);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.unsigned(0);
        writer.array(0);
        writer.bool(false);
        writer.bool(false);
        writer.null();
        writer.unsigned(0);
        writer.unsigned(0);
        writer.array(2);
        writer.unsigned(0);
        writer.array(0);
        assert!(GameBlob::decode(&writer.finish()).is_none());

        let mut marks = Writer::new();
        marks.array(2);
        marks.array(2);
        marks.unsigned(1);
        marks.unsigned(1);
        marks.array(2);
        marks.unsigned(0);
        marks.unsigned(1);
        let marks = marks.finish();
        assert!(marks_of(&mut Reader::new(&marks)).is_none());

        let mut duplicate_marks = Writer::new();
        duplicate_marks.array(2);
        duplicate_marks.array(2);
        duplicate_marks.unsigned(1);
        duplicate_marks.unsigned(1);
        duplicate_marks.array(2);
        duplicate_marks.unsigned(1);
        duplicate_marks.unsigned(2);
        assert!(marks_of(&mut Reader::new(&duplicate_marks.finish())).is_none());

        let mut prevention = Writer::new();
        prevention.array(3);
        prevention.unsigned(0);
        prevention.bool(true);
        prevention.unsigned(0);
        assert!(Prevention::read_version(&mut Reader::new(&prevention.finish()), 12).is_none());
    }

    #[test]
    fn v10_chain_and_pending_modes_move_after_promised_repeat_slot() {
        let mut item = ChainItem::new(1, ItemKind::Spell { card: 2 }, 0, Origin::Hand);
        item.picks = vec![0, 1, UNANSWERED, UNANSWERED, UNANSWERED, UNANSWERED, 3];
        let mut map = MapWriter::new();
        map.field("v").unsigned(10);
        let chain = map.field("ch");
        chain.array(1);
        fn write_v10_item(item: &ChainItem, writer: &mut Writer) {
            writer.array(13);
            writer.unsigned(u64::from(item.id));
            item.kind.write(writer);
            writer.unsigned(u64::from(item.controller));
            writer.unsigned(u64::from(item.status.code()));
            item.origin.write(writer);
            writer.array(item.targets.len());
            for target in &item.targets {
                target.write(writer);
            }
            writer.array(item.spec_counts.len());
            for count in &item.spec_counts {
                writer.unsigned(u64::from(*count));
            }
            writer.array(item.picks.len());
            for pick in &item.picks {
                writer.unsigned(u64::from(*pick));
            }
            writer.unsigned(u64::from(item.stage));
            match &item.noted {
                Some(noted) => noted.write(writer),
                None => writer.null(),
            }
            match item.subject {
                Some(subject) => subject.write(writer),
                None => writer.null(),
            }
            writer.unsigned(u64::from(item.execution));
            writer.array(item.awaiting.len());
            for card in &item.awaiting {
                writer.unsigned(u64::from(*card));
            }
        }
        write_v10_item(&item, chain);
        let queue = map.field("q");
        queue.array(1);
        queue.array(2);
        write_v10_item(&item, queue);
        queue.unsigned(u64::from(Needs::Choices.code()));
        let decoded = GameBlob::decode(&map.finish()).unwrap();
        assert_eq!(decoded.chain[0].slot(SLOT_PROMISED_REPEAT), None);
        assert_eq!(decoded.chain[0].mode_at(0), Some(3));
        assert_eq!(decoded.pending(1).unwrap().item.mode_at(0), Some(3));
        assert_eq!(
            decoded.pending(1).unwrap().item.picks[SLOT_PROMISED_REPEAT],
            UNANSWERED
        );
    }

    #[test]
    fn v13_noted_rows_keep_the_old_four_fields_and_reject_new_item_kinds() {
        fn blob_with_item_kind(kind: &[u64]) -> Vec<u8> {
            let mut map = MapWriter::new();
            map.field("v").unsigned(13);
            let chain = map.field("ch");
            chain.array(1);
            chain.array(14);
            chain.unsigned(1);
            chain.array(kind.len());
            for value in kind {
                chain.unsigned(*value);
            }
            chain.unsigned(0);
            chain.unsigned(0);
            Origin::Hand.write(chain);
            chain.array(0);
            chain.array(0);
            chain.array(SLOTS);
            for _ in 0..SLOTS {
                chain.unsigned(u64::from(UNANSWERED));
            }
            chain.unsigned(0);
            chain.array(4);
            chain.unsigned(3);
            chain.signed(2);
            chain.unsigned(0);
            chain.bool(false);
            chain.null();
            chain.unsigned(0);
            chain.array(0);
            chain.null();
            map.finish()
        }

        let decoded = GameBlob::decode(&blob_with_item_kind(&[0, 7, 0])).unwrap();
        assert_eq!(decoded.chain[0].noted.unwrap().might, 2);
        assert!(!decoded.chain[0].noted.unwrap().buffed);
        assert!(GameBlob::decode(&blob_with_item_kind(&[4, 7, 8, 1])).is_none());
    }

    #[test]
    fn v14_granted_lent_noted_and_excess_have_an_independent_wire_vector() {
        let bytes = [
            0xa4, 0x61, 0x76, 0x0e, 0x62, 0x63, 0x68, 0x81, 0x8e, 0x01, 0x84, 0x04, 0x02, 0x03,
            0x04, 0x00, 0x00, 0x82, 0x03, 0x00, 0x80, 0x80, 0x84, 0x00, 0x01, 0x02, 0x03, 0x00,
            0x85, 0x09, 0x21, 0x01, 0xf5, 0xf5, 0xf6, 0x00, 0x81, 0x18, 0x4d, 0xf6, 0x61, 0x71,
            0x81, 0x82, 0x8e, 0x02, 0x84, 0x05, 0x06, 0x07, 0x08, 0x01, 0x00, 0x82, 0x03, 0x00,
            0x80, 0x80, 0x84, 0x00, 0x01, 0x02, 0x03, 0x01, 0x85, 0x09, 0x21, 0x01, 0xf5, 0xf5,
            0xf6, 0x00, 0x81, 0x18, 0x4d, 0xf6, 0x02, 0x62, 0x78, 0x64, 0x82, 0x83, 0x00, 0x09,
            0x00, 0x83, 0x01, 0x0a, 0x02,
        ];
        let decoded = GameBlob::decode(&bytes).expect("independent v14 vector");
        assert_eq!(
            decoded.chain[0].kind,
            ItemKind::Granted {
                holder: 2,
                lender: 3,
                index: 4
            }
        );
        assert_eq!(
            decoded.pending(2).unwrap().item.kind,
            ItemKind::Lent {
                holder: 6,
                lender: 7,
                index: 8
            }
        );
        assert!(decoded.chain[0].noted.unwrap().buffed);
        assert_eq!(
            decoded.excess,
            [
                Excess {
                    seat: 0,
                    zone: 9,
                    amount: 0
                },
                Excess {
                    seat: 1,
                    zone: 10,
                    amount: 2
                }
            ]
        );
        assert_eq!(decoded.encode(), bytes);

        let xd = bytes.windows(2).position(|window| window == b"xd").unwrap() - 1;
        let mut reordered = bytes[..xd + 4].to_vec();
        reordered.extend_from_slice(&[0x83, 0x01, 0x0a, 0x02, 0x83, 0x00, 0x09, 0x00]);
        assert_eq!(GameBlob::decode(&reordered).unwrap().encode(), bytes);
        let mut duplicate = bytes[..xd + 4].to_vec();
        let header = duplicate.len() - 1;
        duplicate[header] = 0x83;
        duplicate.extend_from_slice(&[
            0x83, 0x00, 0x09, 0x00, 0x83, 0x01, 0x0a, 0x02, 0x83, 0x01, 0x0a, 0x03,
        ]);
        assert!(GameBlob::decode(&duplicate).is_none());
    }

    #[test]
    fn unknown_keys_are_skipped_and_other_versions_are_a_fresh_lobby() {
        let mut map = MapWriter::new();
        map.field("v").unsigned(BLOB_VERSION);
        map.field("zz").text("later");
        map.field("m").unsigned(1);
        let extra = map.field("qq");
        extra.array(1);
        extra.map(1);
        extra.text("k");
        extra.unsigned(2);
        let decoded = GameBlob::decode(&map.finish()).unwrap();
        assert_eq!(decoded.mode, Mode::Enforced);
        assert!(!decoded.is_playing());
        let mut old = MapWriter::new();
        old.field("v").unsigned(3);
        assert_eq!(GameBlob::decode(&old.finish()), None);
        assert_eq!(GameBlob::decode(&[]), None);
        assert_eq!(GameBlob::decode(&[3, 2, 1, 0]), None);
        let mut wrong = MapWriter::new();
        wrong.field("m").unsigned(0);
        wrong.field("v").unsigned(BLOB_VERSION);
        assert_eq!(GameBlob::decode(&wrong.finish()), None);
        let mut bad = MapWriter::new();
        bad.field("v").unsigned(BLOB_VERSION);
        bad.field("m").unsigned(9);
        assert_eq!(GameBlob::decode(&bad.finish()), None);
    }

    #[test]
    fn modes_and_phases_map_to_their_codes() {
        for mode in [Mode::Free, Mode::Enforced] {
            assert_eq!(Mode::from_code(mode.code()), Some(mode));
            assert_eq!(mode.other().other(), mode);
        }
        assert_eq!(Mode::from_code(2), None);
        for phase in Phase::ALL {
            assert_eq!(Phase::from_code(phase.code()), Some(phase));
            assert!(!phase.label().is_empty());
        }
        assert_eq!(Phase::from_code(9), None);
        let mut core = TurnCore::start(3, 2);
        core.advance();
        assert_eq!((core.turn, core.player, core.first), (2, 0, 2));
    }

    #[test]
    fn a_concession_names_the_last_seat_standing() {
        let mut blob = GameBlob::start(3, 0, Mode::Enforced);
        assert_eq!(blob.conceded_winner(), None);
        assert!(blob.concede(2));
        assert!(!blob.concede(2), "a seat concedes once");
        assert_eq!(blob.conceded, [2]);
        assert_eq!(blob.conceded_winner(), None, "two seats still play");
        assert!(blob.concede(0));
        assert_eq!(blob.conceded, [0, 2]);
        assert_eq!(blob.conceded_winner(), Some(1));
        assert!(blob.has_conceded(0) && !blob.has_conceded(1));
        let decoded = GameBlob::decode(&blob.encode()).unwrap();
        assert_eq!(decoded.conceded_winner(), Some(1));
    }
}

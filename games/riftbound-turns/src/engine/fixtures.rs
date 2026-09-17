use crate::cards::Resolved;
use crate::engine::ctx::Ctx;
use crate::engine::{legal, play, priority, prompts, resume, settle};
use crate::rules::{COUNTER_POINTS, COUNTER_XP};
use crate::state::{ChainItem, GameBlob, ItemKind, Mode, Origin, Phase, PromptWhy};
use crate::Refusal;
use agni_plugin_sdk::decide::Action;
use agni_plugin_sdk::prompt::Pick;
use agni_plugin_sdk::table::{
    CardInfo, CounterBounds, CounterInfo, CounterScope, Snapshot, Target, ZoneKind, ZoneSummary,
    ZoneVisibility,
};

pub const HAND: u16 = 0;
pub const MAIN_DECK: u16 = 1;
pub const RUNE_DECK: u16 = 2;
pub const LEGEND: u16 = 3;
pub const CHAMPION: u16 = 4;
pub const TRASH: u16 = 5;
pub const SIDEBOARD: u16 = 6;
pub const RUNE_POOL: u16 = 7;
pub const BASE: u16 = 8;
pub const BF1: u16 = 9;
pub const BF2: u16 = 10;
pub const BF3: u16 = 11;
pub const CHAIN: u16 = 12;
pub const BANISHMENT: u16 = 13;

pub const VI: u32 = 50;
pub const GROUNDS: u32 = 51;
pub const ROCKFALL: u32 = 52;
pub const SPRITE: u32 = 60;
pub const HAND_UNIT: u32 = 70;
pub const HAND_SPELL: u32 = 71;
pub const HAND_GEAR: u32 = 72;
pub const HAND_HIDDEN: u32 = 73;
pub const CHAMPION_CARD: u32 = 74;
pub const LEGEND_CARD: u32 = 75;
pub const RUNE_A: u32 = 40;
pub const THEIR_HAND_CARD: u32 = 80;
pub const THEIR_UNIT: u32 = 81;

pub fn zone(id: u16, name: &str, kind: ZoneKind, shared: bool) -> ZoneSummary {
    ZoneSummary {
        id,
        name: name.into(),
        kind,
        shared,
        battlefield: kind == ZoneKind::Battlefield,
        label: name.into(),
        visibility: match kind {
            ZoneKind::Deck => ZoneVisibility::None,
            ZoneKind::Hand => ZoneVisibility::Owner,
            _ => ZoneVisibility::All,
        },
    }
}

pub fn zones() -> Vec<ZoneSummary> {
    vec![
        zone(HAND, "hand", ZoneKind::Hand, false),
        zone(MAIN_DECK, "main-deck", ZoneKind::Deck, false),
        zone(RUNE_DECK, "rune-deck", ZoneKind::Deck, false),
        zone(LEGEND, "legend", ZoneKind::Aux, false),
        zone(CHAMPION, "champion", ZoneKind::Aux, false),
        zone(TRASH, "trash", ZoneKind::Discard, false),
        zone(SIDEBOARD, "sideboard", ZoneKind::Aux, false),
        zone(RUNE_POOL, "rune-pool", ZoneKind::Aux, false),
        zone(BASE, "base", ZoneKind::Battlefield, false),
        zone(BF1, "battlefield-1", ZoneKind::Battlefield, true),
        zone(BF2, "battlefield-2", ZoneKind::Battlefield, true),
        zone(BF3, "battlefield-3", ZoneKind::Battlefield, true),
        zone(CHAIN, "chain", ZoneKind::Stack, true),
        zone(BANISHMENT, "banishment", ZoneKind::Discard, false),
    ]
}

pub fn card(id: u32, zone: u16, seat: u8, name: &str, kind: &str) -> CardInfo {
    CardInfo {
        id,
        zone: Some(zone),
        seat: if (BF1..=CHAIN).contains(&zone) {
            0
        } else {
            seat
        },
        owner: seat,
        name: name.into(),
        kind: (!kind.is_empty()).then(|| kind.into()),
        ..Default::default()
    }
}

pub fn unit(id: u32, zone: u16, seat: u8, name: &str, might: u8) -> CardInfo {
    CardInfo {
        might: Some(might),
        energy: Some(2),
        domain: vec!["Fury".into()],
        ..card(id, zone, seat, name, "Unit")
    }
}

pub fn spell(id: u32, zone: u16, seat: u8, name: &str, energy: u8, power: u8) -> CardInfo {
    CardInfo {
        energy: Some(energy),
        power: Some(power),
        domain: vec!["Fury".into()],
        ..card(id, zone, seat, name, "Spell")
    }
}

pub fn gear(id: u32, zone: u16, seat: u8, name: &str, energy: u8) -> CardInfo {
    CardInfo {
        energy: Some(energy),
        domain: vec!["Fury".into()],
        ..card(id, zone, seat, name, "Gear")
    }
}

pub fn rune(id: u32, seat: u8, domain: &str, exhausted: bool) -> CardInfo {
    CardInfo {
        exhausted,
        domain: vec![domain.into()],
        ..card(id, RUNE_POOL, seat, &format!("{domain} Rune"), "Rune")
    }
}

pub fn hidden(id: u32, zone: u16, seat: u8) -> CardInfo {
    card(id, zone, seat, "", "")
}

pub fn gold(id: u32, seat: u8, exhausted: bool) -> CardInfo {
    CardInfo {
        exhausted,
        ..card(id, BASE, seat, "Gold", "Gear")
    }
}

pub fn table() -> Snapshot {
    Snapshot {
        players: 2,
        zones: zones(),
        cards: vec![
            hidden(20, MAIN_DECK, 0),
            hidden(21, MAIN_DECK, 0),
            hidden(22, MAIN_DECK, 0),
            hidden(23, MAIN_DECK, 0),
            hidden(24, MAIN_DECK, 1),
            hidden(25, MAIN_DECK, 1),
            hidden(30, RUNE_DECK, 0),
            hidden(31, RUNE_DECK, 0),
            hidden(32, RUNE_DECK, 0),
            hidden(33, RUNE_DECK, 1),
            hidden(34, RUNE_DECK, 1),
            hidden(35, RUNE_DECK, 1),
            rune(RUNE_A, 0, "Fury", true),
            rune(41, 0, "Fury", false),
            rune(42, 0, "Calm", false),
            rune(43, 0, "Fury", false),
            rune(44, 1, "Mind", false),
            rune(45, 1, "Mind", false),
            unit(VI, BASE, 0, "Vi", 3),
            card(GROUNDS, BF1, 0, "Proving Grounds", "Battlefield"),
            card(ROCKFALL, BF2, 1, "Rockfall Path", "Battlefield"),
            CardInfo {
                might: Some(3),
                ..card(SPRITE, BF2, 1, "Sprite", "Unit")
            },
            unit(HAND_UNIT, HAND, 0, "Shadow Order Disciple", 2),
            spell(HAND_SPELL, HAND, 0, "Spark", 2, 1),
            gear(HAND_GEAR, HAND, 0, "Boots of Swiftness", 2),
            hidden(HAND_HIDDEN, HAND, 0),
            CardInfo {
                energy: Some(3),
                domain: vec!["Mind".into()],
                ..unit(CHAMPION_CARD, CHAMPION, 0, "Lillia - Fae Fawn", 3)
            },
            card(LEGEND_CARD, LEGEND, 0, "Lillia - Bashful Bloom", "Legend"),
            hidden(THEIR_HAND_CARD, HAND, 1),
            unit(THEIR_UNIT, BASE, 1, "Jinx", 2),
        ],
        counters: Vec::new(),
        next_id: 200,
        revealed: Vec::new(),
        tokens: vec![SPRITE],
        counter_table: vec![
            CounterBounds {
                id: COUNTER_POINTS,
                scope: CounterScope::Seat,
                start: 0,
                min: Some(0),
                max: None,
            },
            CounterBounds {
                id: COUNTER_XP,
                scope: CounterScope::Seat,
                start: 0,
                min: Some(0),
                max: None,
            },
            CounterBounds {
                id: 2,
                scope: CounterScope::Card,
                start: 0,
                min: None,
                max: None,
            },
            CounterBounds {
                id: 3,
                scope: CounterScope::Card,
                start: 0,
                min: Some(0),
                max: None,
            },
            CounterBounds {
                id: 4,
                scope: CounterScope::Card,
                start: 0,
                min: Some(0),
                max: Some(1),
            },
            CounterBounds {
                id: 5,
                scope: CounterScope::Card,
                start: 0,
                min: Some(0),
                max: None,
            },
            CounterBounds {
                id: 6,
                scope: CounterScope::Card,
                start: 0,
                min: Some(0),
                max: None,
            },
        ],
        options: Vec::new(),
    }
}

pub fn move_action(card: u32, to: u16, seat: u8) -> Action {
    Action::Move {
        card,
        to: Some(to),
        seat,
        index: agni_plugin_sdk::decide::TOP,
        hidden: false,
    }
}

pub fn move_to_bottom(card: u32, to: u16, seat: u8) -> Action {
    Action::Move {
        card,
        to: Some(to),
        seat,
        index: agni_plugin_sdk::decide::BOTTOM,
        hidden: false,
    }
}

pub fn effect_of(seat: u8) -> ChainItem {
    let card = if seat == 0 {
        HAND_SPELL
    } else {
        THEIR_HAND_CARD
    };
    ChainItem::new(u16::MAX, ItemKind::Spell { card }, seat, Origin::Hand)
}

pub struct Fixture {
    pub table: Snapshot,
    pub blob: GameBlob,
    pub scripts: Resolved,
}

impl Fixture {
    pub fn enforced() -> Self {
        let table = table();
        let mut blob = GameBlob::start(2, 0, Mode::Enforced);
        blob.set_holder(BF2, Some(1));
        blob.set_phase(Phase::Action);
        blob.seats = vec![Default::default(); 2];
        let scripts = Resolved::of(&table);
        Ctx::fresh(&table, &mut blob, &scripts, 0).record_chosen_champions();
        Self {
            scripts,
            table,
            blob,
        }
    }

    pub fn resolve(&mut self) {
        self.scripts = Resolved::of(&self.table);
    }

    pub fn set_points(&mut self, seat: u8, value: i32) {
        self.table.counters.push(CounterInfo {
            target: Target::Seat(seat),
            counter: COUNTER_POINTS,
            value,
        });
        self.table.counters.sort();
    }

    pub fn set_xp(&mut self, seat: u8, value: i32) {
        self.table.counters.push(CounterInfo {
            target: Target::Seat(seat),
            counter: COUNTER_XP,
            value,
        });
        self.table.counters.sort();
    }

    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx::fresh(&self.table, &mut self.blob, &self.scripts, 0)
    }

    pub fn ctx_for(&mut self, seat: u8, action: &Action) -> Ctx<'_> {
        Ctx::new(&self.table, &mut self.blob, &self.scripts, seat, action)
    }

    pub fn commit(&mut self, ctx_table: Snapshot) {
        self.table = ctx_table;
        self.resolve();
    }
}

pub fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    let entry = crate::engine::ctx::EntryMove {
        card,
        from: ctx.zones.hand,
        from_seat: seat,
        to: ctx.zones.chain,
        to_seat: 0,
        index: agni_plugin_sdk::decide::TOP,
        hidden: false,
    };
    legal::classify(ctx, seat, &entry)?;
    let chain = ctx.zones.chain.unwrap_or(0);
    ctx.enter(&move_action(card, chain, 0), seat).unwrap();
    play::begin(ctx, seat, card, Origin::Hand, None)?;
    settle(ctx)?;
    settle_rune_payments(ctx, seat)
}

pub fn labels(ctx: &Ctx) -> Vec<String> {
    prompts::offered(ctx)
        .iter()
        .map(|opt| opt.label.clone())
        .collect()
}

pub fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
    let option = labels(ctx)
        .iter()
        .position(|held| held == label)
        .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
        as u16;
    let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
    if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
        resume(ctx, &answered)?;
    }
    settle(ctx)?;
    settle_rune_payments(ctx, seat)
}

pub fn settle_rune_payments(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
    while matches!(ctx.blob.why, Some(PromptWhy::PayWith { .. }))
        && ctx
            .blob
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.seat == seat)
        && ctx
            .blob
            .why
            .and_then(PromptWhy::item)
            .and_then(|item| ctx.blob.pending(item))
            .is_some_and(|pending| pending.item.stage == play::STAGE_PAY)
    {
        let prompt = ctx.blob.prompt.as_ref().map(|held| held.id).unwrap_or(0);
        let pick = Pick { prompt, option: 0 };
        if let Some(answered) = prompts::answer(ctx, seat, pick)? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
    }
    Ok(())
}

pub fn pass_until_open(ctx: &mut Ctx) {
    for _ in 0..16 {
        if ctx.blob.chain.is_empty() || ctx.blob.prompt.is_some() {
            return;
        }
        let Some(holder) = priority::holder(ctx) else {
            return;
        };
        priority::pass(ctx, holder).unwrap();
    }
}

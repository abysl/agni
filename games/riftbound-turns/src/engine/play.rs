use crate::cards::{
    Keyword, ModeTiming, NameKind, Paying, SelfCost, Static, KIND_GEAR, KIND_SPELL, KIND_UNIT,
};
use crate::engine::ctx::{Ctx, Event, Location};
use crate::engine::legal::{self, Reason};
use crate::engine::{activate, attach, cleanup, combat, cost, hide, pay, targets, triggers};
use crate::state::{
    ChainItem, ItemKind, ItemStatus, Leave, Limited, Needs, Origin, Pending, Price, Priority,
    PromptWhy, RevealedFrom, TargetRef, FLAG_ENTERED_THIS_TURN, SLOT_ORDINAL,
};
use crate::Refusal;
use agni_plugin_sdk::decide::{Effect, TOP};

pub use crate::state::{
    SLOT_ACCELERATE, SLOT_ADDITIONAL, SLOT_PROMISED_REPEAT, SLOT_REPEAT, SLOT_TRIGGER_COST,
};

pub const STAGE_LOCATION: u8 = 0;
pub const STAGE_ACCELERATE: u8 = 1;
pub const STAGE_PAY_WITH: u8 = 2;
pub const STAGE_PAY: u8 = 3;
pub const STAGE_REPEAT: u8 = 4;
pub const STAGE_ADDITIONAL: u8 = 5;
pub const STAGE_PROMISED_REPEAT: u8 = 6;
pub const STAGE_TARGET: u8 = 16;

pub fn slot_of_stage(stage: u8) -> Option<usize> {
    match stage {
        STAGE_ACCELERATE => Some(SLOT_ACCELERATE),
        STAGE_PAY => Some(SLOT_TRIGGER_COST),
        STAGE_REPEAT => Some(SLOT_REPEAT),
        STAGE_PROMISED_REPEAT => Some(SLOT_PROMISED_REPEAT),
        STAGE_ADDITIONAL => Some(SLOT_ADDITIONAL),
        _ => None,
    }
}

pub fn stage_after_additional(item: &ChainItem) -> u8 {
    match item.kind {
        ItemKind::Spell { .. } => STAGE_TARGET,
        _ => STAGE_PAY_WITH,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    Play,
    Activation,
    Trigger,
}

pub fn path_of(item: &ChainItem) -> Path {
    match item.kind {
        ItemKind::Spell { .. } | ItemKind::Permanent { .. } => Path::Play,
        ItemKind::Ability { .. } | ItemKind::Lent { .. } => Path::Activation,
        ItemKind::Trigger { .. } | ItemKind::Granted { .. } => Path::Trigger,
    }
}

pub fn begin(
    ctx: &mut Ctx,
    seat: u8,
    card: u32,
    origin: Origin,
    location: Option<Location>,
) -> Result<(), Refusal> {
    begin_declining(ctx, seat, card, origin, location, &[], None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitedPlay {
    pub card: u32,
    pub by: u8,
    pub origin: Origin,
    pub locations: Vec<Location>,
    pub price: Price,
}

pub fn begin_limited(ctx: &mut Ctx, play: LimitedPlay) -> Result<(), Refusal> {
    let zones = ctx
        .limited_play_locations(play.by, play.card, &play.locations)
        .into_iter()
        .filter_map(|at| ctx.zone_of(at).map(|(zone, _)| zone))
        .collect();
    let limited = Limited {
        zones,
        price: play.price,
    };
    begin_declining(
        ctx,
        play.by,
        play.card,
        play.origin,
        None,
        &[],
        Some((limited, ctx.defer_limited)),
    )
}

pub fn limited_locations(ctx: &Ctx, item: &ChainItem) -> Vec<Location> {
    let Some(limited) = &item.limited else {
        return Vec::new();
    };
    let offered: Vec<Location> = limited
        .zones
        .iter()
        .filter_map(|zone| Location::of_zone(*zone, item.controller, &ctx.zones))
        .collect();
    ctx.limited_play_locations(item.controller, item.kind.source(), &offered)
}

fn begin_declining(
    ctx: &mut Ctx,
    seat: u8,
    card: u32,
    origin: Origin,
    location: Option<Location>,
    declined: &[usize],
    limited: Option<(Limited, bool)>,
) -> Result<(), Refusal> {
    let (limited, defer) = limited
        .map(|(limited, defer)| (Some(limited), defer))
        .unwrap_or((None, false));
    let kind = ctx.kind_of(card).unwrap_or(KIND_UNIT).to_string();
    let id = ctx.blob.next_item_id();
    let item_kind = if kind == KIND_SPELL {
        ItemKind::Spell { card }
    } else {
        ItemKind::Permanent { card }
    };
    let mut item = ChainItem::new(id, item_kind, seat, origin);
    item.limited = limited;
    for slot in declined.iter().copied() {
        item.set_slot(slot, 0);
    }
    let base = ctx
        .zones
        .base
        .ok_or(Refusal::Illegal(Reason::UnknownDestination))?;
    onto_the_chain(ctx, card, origin);
    match (kind.as_str(), location) {
        (KIND_SPELL, _) => item.stage = STAGE_REPEAT,
        (KIND_GEAR, _) => {
            item.targets.push(TargetRef::Zone(base));
            item.stage = STAGE_ACCELERATE;
        }
        (_, Some(location)) => {
            let (zone, _) = ctx
                .zone_of(location)
                .ok_or(Refusal::Illegal(Reason::UnknownDestination))?;
            item.targets.push(TargetRef::Zone(zone));
            item.stage = STAGE_ACCELERATE;
        }
        (_, None) => item.stage = STAGE_LOCATION,
    }
    ctx.blob.queue.push(Pending {
        item,
        needs: Needs::Choices,
    });
    if defer {
        return Ok(());
    }
    advance(ctx, id)
}

fn onto_the_chain(ctx: &mut Ctx, card: u32, origin: Origin) {
    let Some(chain) = ctx.zones.chain else {
        return;
    };
    let Some(zone) = ctx.card(card).and_then(|held| held.zone) else {
        return;
    };
    let waiting = match origin {
        Origin::Trash { .. } | Origin::Banishment | Origin::Revealed { .. } => zone != chain,
        Origin::Hand => ctx.zones.hand == Some(zone),
        Origin::Champion | Origin::Facedown { .. } | Origin::Board => false,
    };
    if !waiting {
        return;
    }
    ctx.emit(Effect::Move {
        card,
        zone: chain,
        seat: 0,
        index: TOP,
    });
}

pub fn cancellable(item: &ChainItem) -> bool {
    path_of(item) == Path::Play
        && item.limited.is_none()
        && !matches!(
            item.origin,
            Origin::Trash {
                leave: Leave::Recycle
            } | Origin::Banishment
                | Origin::Revealed { .. }
        )
}

fn additional_cost(ctx: &Ctx, card: u32) -> Option<cost::Cost> {
    let printed = ctx.script(card)?.additional?;
    Some(cost::of_script(&printed, &ctx.domains_of(card)))
}

fn trigger_cost_asks(ctx: &Ctx, item: &ChainItem, total: &cost::Cost) -> bool {
    if !total.is_free() {
        return true;
    }
    let Some(ability) = targets::ability_of(ctx, item) else {
        return false;
    };
    if !ability.optional {
        return false;
    }
    let self_paid = !matches!(
        activate::self_cost_of_item(ctx, item),
        None | Some((_, SelfCost::Free | SelfCost::Auto))
    );
    self_paid || ability.xp > 0 || ability.burn > 0
}

fn trigger_cost_payable(ctx: &Ctx, item: &ChainItem) -> bool {
    let Some(ability) = targets::ability_of(ctx, item) else {
        return true;
    };
    let seat = item.controller;
    if ability.xp > 0 && ctx.xp(seat) < i32::from(ability.xp) {
        return false;
    }
    if ability.burn > 0 {
        let deck = ctx
            .zones
            .main_deck
            .map(|deck| ctx.table.held(deck, seat).count())
            .unwrap_or(0);
        if deck < usize::from(ability.burn) {
            return false;
        }
    }
    true
}

pub fn locations_of(ctx: &Ctx, item: &ChainItem) -> Vec<Location> {
    if item.limited.is_some() {
        return limited_locations(ctx, item);
    }
    legal::locations_for(ctx, item.controller, item.kind.source())
}

fn location_still_open(ctx: &Ctx, item: &ChainItem) -> bool {
    if item.limited.is_some() {
        let Some(location) = item
            .zone_target()
            .and_then(|zone| Location::of_zone(zone, item.controller, &ctx.zones))
        else {
            return true;
        };
        return limited_locations(ctx, item).contains(&location);
    }
    if !matches!(item.kind, ItemKind::Permanent { .. })
        || !matches!(item.origin, Origin::Hand | Origin::Champion)
    {
        return true;
    }
    let Some(card) = item.kind.card() else {
        return true;
    };
    let seat = item.controller;
    match item.zone_target() {
        Some(zone) if ctx.zones.is_battlefield(zone) => {
            legal::locations_for(ctx, seat, card).contains(&Location::Battlefield(zone))
        }
        _ => true,
    }
}

pub fn is_play(item: &ChainItem) -> bool {
    matches!(
        item.kind,
        ItemKind::Spell { .. } | ItemKind::Permanent { .. }
    )
}

fn drop_trigger(ctx: &mut Ctx, item: u16, why: &str) {
    if let Some(dropped) = ctx.blob.take_pending(item) {
        ctx.narrate(format!(
            "{{card {}}} trigger fizzles · {why}",
            dropped.item.kind.source()
        ));
    }
}

fn decline_trigger(ctx: &mut Ctx, item: u16, why: &str) {
    if let Some(dropped) = ctx.blob.take_pending(item) {
        ctx.narrate(format!(
            "{{card {}}} trigger is removed · {why}",
            dropped.item.kind.source()
        ));
    }
}

pub fn advance(ctx: &mut Ctx, item: u16) -> Result<(), Refusal> {
    loop {
        let Some(pending) = ctx.blob.pending(item).cloned() else {
            return Ok(());
        };
        let seat = pending.item.controller;
        let path = path_of(&pending.item);
        let play = path == Path::Play;
        let chosen = path != Path::Trigger;
        let cancellable = cancellable(&pending.item);
        let stage = pending.item.stage;
        if !chosen && stage < STAGE_TARGET && stage != STAGE_PAY && stage != STAGE_PAY_WITH {
            if let Some(held) = ctx.blob.pending_mut(item) {
                held.item.stage = STAGE_TARGET;
            }
            continue;
        }
        match stage {
            STAGE_LOCATION => {
                let locations = locations_of(ctx, &pending.item);
                if locations.is_empty() {
                    cancel(ctx, item);
                    return Ok(());
                }
                if locations.len() == 1 {
                    let zone = ctx
                        .zone_of(locations[0])
                        .map(|(zone, _)| zone)
                        .unwrap_or(ctx.zones.base.unwrap_or(0));
                    if let Some(held) = ctx.blob.pending_mut(item) {
                        held.item.targets.push(TargetRef::Zone(zone));
                        held.item.stage = STAGE_ACCELERATE;
                    }
                    continue;
                }
                ctx.ask(seat, 1, 1, cancellable, PromptWhy::PlayLocation { item });
                return Ok(());
            }
            STAGE_ACCELERATE => {
                if cost::can_accelerate_item(ctx, &pending.item)
                    && pending.item.slot(SLOT_ACCELERATE).is_none()
                {
                    let mut accelerated = pending.item.clone();
                    accelerated.set_slot(SLOT_ACCELERATE, 1);
                    let total = cost::of_item(ctx, &accelerated, None);
                    if pay::affordable_for(ctx, seat, &total, Paying::Item(&accelerated)) {
                        ctx.ask(
                            seat,
                            1,
                            1,
                            cancellable,
                            PromptWhy::OptionalCost {
                                item,
                                cost: SLOT_ACCELERATE as u8,
                            },
                        );
                        return Ok(());
                    }
                }
                if let Some(held) = ctx.blob.pending_mut(item) {
                    held.item.stage = STAGE_ADDITIONAL;
                }
                continue;
            }
            STAGE_REPEAT | STAGE_PROMISED_REPEAT => {
                let (slot, offered, next) = if stage == STAGE_REPEAT {
                    (
                        SLOT_REPEAT,
                        cost::repeat_of(ctx, &pending.item),
                        STAGE_PROMISED_REPEAT,
                    )
                } else {
                    (
                        SLOT_PROMISED_REPEAT,
                        cost::promised_repeat_of(ctx, &pending.item),
                        STAGE_ADDITIONAL,
                    )
                };
                if offered.is_some() && pending.item.slot(slot).is_none() {
                    let with = paying_slot(&pending.item, slot);
                    let total = cost::of_item(ctx, &with, None);
                    if pay::affordable_for(ctx, seat, &total, Paying::Item(&with)) {
                        ctx.ask(
                            seat,
                            1,
                            1,
                            cancellable,
                            PromptWhy::OptionalCost {
                                item,
                                cost: slot as u8,
                            },
                        );
                        return Ok(());
                    }
                }
                if let Some(held) = ctx.blob.pending_mut(item) {
                    held.item.stage = next;
                }
                continue;
            }
            STAGE_ADDITIONAL => {
                let card = pending.item.kind.source();
                if additional_cost(ctx, card).is_some()
                    && pending.item.slot(SLOT_ADDITIONAL).is_none()
                {
                    let mut with_additional = pending.item.clone();
                    with_additional.set_slot(SLOT_ADDITIONAL, 1);
                    let total = cost::of_item(ctx, &with_additional, None);
                    if pay::affordable_for(ctx, seat, &total, Paying::Item(&with_additional)) {
                        ctx.ask(
                            seat,
                            1,
                            1,
                            cancellable,
                            PromptWhy::OptionalCost {
                                item,
                                cost: SLOT_ADDITIONAL as u8,
                            },
                        );
                        return Ok(());
                    }
                }
                let next = stage_after_additional(&pending.item);
                if let Some(held) = ctx.blob.pending_mut(item) {
                    held.item.stage = next;
                }
                continue;
            }
            STAGE_PAY_WITH => {
                if let Some(kind) = unnamed(ctx, &pending.item) {
                    ctx.ask(
                        seat,
                        1,
                        1,
                        cancellable,
                        PromptWhy::Name {
                            item,
                            kind,
                            stage: 0,
                        },
                    );
                    return Ok(());
                }
                let total = cost::of_item(ctx, &pending.item, None);
                if chosen && pay::source_is_a_choice(ctx, seat, &total, Paying::Item(&pending.item))
                {
                    ctx.ask(seat, 1, 1, cancellable, PromptWhy::PayWith { item });
                    return Ok(());
                }
                if let Some(held) = ctx.blob.pending_mut(item) {
                    held.item.stage = STAGE_PAY;
                }
                continue;
            }
            STAGE_PAY => {
                let total = cost::of_item(ctx, &pending.item, None);
                if play && !location_still_open(ctx, &pending.item) {
                    let card = pending.item.kind.source();
                    ctx.narrate(format!(
                        "{{card {card}}} can no longer be played there · the play is taken back"
                    ));
                    cancel(ctx, item);
                    return Ok(());
                }
                if !chosen && !activate::self_cost_payable(ctx, &pending.item) {
                    decline_trigger(ctx, item, "its source is exhausted");
                    return Ok(());
                }
                if !chosen && trigger_cost_asks(ctx, &pending.item, &total) {
                    match pending.item.slot(SLOT_TRIGGER_COST) {
                        None if !pay::affordable_for(
                            ctx,
                            seat,
                            &total,
                            Paying::Item(&pending.item),
                        ) || !trigger_cost_payable(ctx, &pending.item) =>
                        {
                            decline_trigger(ctx, item, "its cost can't be paid");
                            return Ok(());
                        }
                        None => {
                            ctx.ask(
                                seat,
                                1,
                                1,
                                false,
                                PromptWhy::OptionalCost {
                                    item,
                                    cost: SLOT_TRIGGER_COST as u8,
                                },
                            );
                            return Ok(());
                        }
                        Some(0) => {
                            decline_trigger(ctx, item, "its cost is declined");
                            return Ok(());
                        }
                        Some(_) => {}
                    }
                }
                let plan = match pay::plan(ctx, seat, &total) {
                    Ok(plan) => plan,
                    Err(_) if pending.item.limited.is_some() => {
                        let card = pending.item.kind.source();
                        ctx.narrate(format!(
                            "{{card {card}}} can't be played · its cost can't be paid"
                        ));
                        cancel(ctx, item);
                        return Ok(());
                    }
                    Err(refusal) if chosen => {
                        pay::clear_pins(ctx, seat);
                        return Err(refusal);
                    }
                    Err(_) => {
                        decline_trigger(ctx, item, "its cost can't be paid");
                        return Ok(());
                    }
                };
                if path != Path::Play && !activate::pay_self(ctx, &pending.item) {
                    if path == Path::Activation {
                        cancel(ctx, item);
                    } else {
                        decline_trigger(ctx, item, "its cost can't be paid");
                    }
                    return Ok(());
                }
                pay::pay(ctx, seat, &plan);
                let Some(pending) = ctx.blob.take_pending(item) else {
                    return Ok(());
                };
                finalize(ctx, pending.item);
                return Ok(());
            }
            _ => {
                let spec_index = usize::from(stage - STAGE_TARGET);
                if let Some(kind) = unnamed(ctx, &pending.item) {
                    ctx.ask(
                        seat,
                        1,
                        1,
                        cancellable,
                        PromptWhy::Name {
                            item,
                            kind,
                            stage: 0,
                        },
                    );
                    return Ok(());
                }
                if let Some(execution) = unchosen_mode(ctx, &pending.item) {
                    ctx.ask(
                        seat,
                        1,
                        1,
                        chosen && (cancellable || !play),
                        PromptWhy::Mode { item, execution },
                    );
                    return Ok(());
                }
                let specs = targets::specs_of(ctx, &pending.item);
                let Some(spec) = specs.get(spec_index) else {
                    let next = if chosen { STAGE_PAY_WITH } else { STAGE_PAY };
                    if let Some(held) = ctx.blob.pending_mut(item) {
                        held.item.stage = next;
                    }
                    continue;
                };
                let Some(min) = targets::min_of(ctx, &pending.item, spec) else {
                    if let Some(held) = ctx.blob.pending_mut(item) {
                        held.item.spec_counts.push(0);
                        held.item.stage = stage + 1;
                    }
                    continue;
                };
                let candidates = targets::candidates_at(ctx, &pending.item, spec_index, spec);
                if candidates.is_empty() && min == 0 {
                    if let Some(held) = ctx.blob.pending_mut(item) {
                        held.item.spec_counts.push(0);
                        held.item.stage = stage + 1;
                    }
                    continue;
                }
                if candidates.len() < usize::from(min) {
                    let why = if candidates.is_empty() {
                        "no legal target"
                    } else {
                        "not enough legal targets"
                    };
                    if !chosen {
                        drop_trigger(ctx, item, why);
                        return Ok(());
                    }
                    if play && !cancellable {
                        let card = pending.item.kind.source();
                        ctx.narrate(format!("{{card {card}}} can't be played · {why}"));
                        cancel(ctx, item);
                        return Ok(());
                    }
                }
                if play && !cancellable && !any_affordable(ctx, &pending.item, &candidates) {
                    let card = pending.item.kind.source();
                    ctx.narrate(format!(
                        "{{card {card}}} can't be played · its cost can't be paid"
                    ));
                    cancel(ctx, item);
                    return Ok(());
                }
                let max = spec.max.max(1);
                ctx.ask(
                    seat,
                    min,
                    max,
                    chosen && (cancellable || !play),
                    PromptWhy::Target {
                        item,
                        spec: spec_index as u8,
                    },
                );
                return Ok(());
            }
        }
    }
}

pub fn modes_of(ctx: &Ctx, item: &ChainItem) -> &'static [crate::cards::ModeSpec] {
    targets::ability_of(ctx, item)
        .filter(|ability| ability.mode_timing == ModeTiming::AtPlay)
        .map(|ability| ability.modes)
        .unwrap_or(&[])
}

pub fn names_at_play(ctx: &Ctx, item: &ChainItem) -> Option<NameKind> {
    if !is_play(item) {
        return None;
    }
    ctx.script(item.kind.source())?.names
}

fn unnamed(ctx: &Ctx, item: &ChainItem) -> Option<NameKind> {
    let kind = names_at_play(ctx, item)?;
    (ctx.named(item.kind.source()).is_none() && !ctx.name_options(kind).is_empty()).then_some(kind)
}

pub fn choose_name(ctx: &mut Ctx, item: u16, kind: NameKind, index: u16) -> Result<(), Refusal> {
    let Some(pending) = ctx.blob.pending(item).cloned() else {
        return Ok(());
    };
    let Some(name) = ctx.name_options(kind).get(usize::from(index)).cloned() else {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    };
    ctx.name(pending.item.kind.source(), &name);
    advance(ctx, item)
}

fn unchosen_mode(ctx: &Ctx, item: &ChainItem) -> Option<u8> {
    if modes_of(ctx, item).is_empty() {
        return None;
    }
    let executions = 1 + usize::from(item.repeats());
    (0..executions)
        .find(|execution| item.mode_at(*execution as u8).is_none())
        .map(|execution| execution as u8)
}

pub fn choose_mode(ctx: &mut Ctx, item: u16, execution: u8, mode: u8) -> Result<(), Refusal> {
    let Some(pending) = ctx.blob.pending(item).cloned() else {
        return Ok(());
    };
    if usize::from(mode) >= modes_of(ctx, &pending.item).len() {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    }
    if let Some(held) = ctx.blob.pending_mut(item) {
        held.item.set_mode(execution, mode);
    }
    advance(ctx, item)
}

fn paying_slot(item: &ChainItem, slot: usize) -> ChainItem {
    let mut with = item.clone();
    with.set_slot(slot, 1);
    with
}

pub fn affordable_choosing(ctx: &Ctx, item: &ChainItem, target: TargetRef) -> bool {
    let mut with = item.clone();
    with.targets.push(target);
    affordable_as(ctx, &with)
}

pub fn affordable_as(ctx: &Ctx, with: &ChainItem) -> bool {
    let seat = with.controller;
    let mut scratch = ctx.blob.clone();
    match scratch.pending_mut(with.id) {
        Some(held) => held.item = with.clone(),
        None => scratch.queue.push(Pending {
            item: with.clone(),
            needs: Needs::Choices,
        }),
    }
    let seen = Ctx::fresh(&ctx.table, &mut scratch, ctx.scripts, seat);
    pay::affordable_for(
        &seen,
        seat,
        &cost::of_item(&seen, with, None),
        Paying::Item(with),
    )
}

pub fn affordable(ctx: &Ctx, item: &ChainItem) -> Result<(), Refusal> {
    let seat = item.controller;
    let Err(refusal) = pay::plan_for(
        ctx,
        seat,
        &cost::of_item(ctx, item, None),
        Paying::Item(item),
    ) else {
        return Ok(());
    };
    let specs = targets::specs_of(ctx, item);
    let Some(spec) = specs.first() else {
        return Err(refusal);
    };
    if targets::candidates(ctx, item, spec)
        .iter()
        .copied()
        .any(|target| affordable_choosing(ctx, item, target))
    {
        Ok(())
    } else {
        Err(refusal)
    }
}

fn any_affordable(ctx: &Ctx, item: &ChainItem, candidates: &[TargetRef]) -> bool {
    let seat = item.controller;
    if candidates.is_empty() {
        return pay::affordable_for(
            ctx,
            seat,
            &cost::of_item(ctx, item, None),
            Paying::Item(item),
        );
    }
    candidates.iter().any(|target| {
        let mut with = item.clone();
        with.targets.push(*target);
        pay::affordable_for(
            ctx,
            seat,
            &cost::of_item(ctx, &with, None),
            Paying::Item(&with),
        )
    })
}

pub fn choose_targets(
    ctx: &mut Ctx,
    item: u16,
    spec_index: u8,
    picked: &[u32],
) -> Result<(), Refusal> {
    let Some(pending) = ctx.blob.pending(item).cloned() else {
        return Ok(());
    };
    let specs = targets::specs_of(ctx, &pending.item);
    let Some(spec) = specs.get(usize::from(spec_index)) else {
        return Ok(());
    };
    let Some(min) = targets::min_of(ctx, &pending.item, spec) else {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    };
    let mut chosen: Vec<TargetRef> = Vec::new();
    for value in picked {
        let Some(target) = targets::from_answer(spec.kind, *value) else {
            return Err(Refusal::Illegal(Reason::NotALegalTarget));
        };
        if !targets::candidates_with(ctx, &pending.item, usize::from(spec_index), spec, &chosen)
            .contains(&target)
        {
            return Err(Refusal::Illegal(Reason::NotALegalTarget));
        }
        chosen.push(target);
    }
    if chosen.len() < usize::from(min) || chosen.len() > usize::from(spec.max.max(1)) {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    }
    let mut with_all = pending.item.clone();
    with_all.targets.extend(chosen.iter().copied());
    if path_of(&pending.item) != Path::Trigger && !affordable_as(ctx, &with_all) {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    }
    if let Some(held) = ctx.blob.pending_mut(item) {
        while held.item.spec_counts.len() < usize::from(spec_index) {
            held.item.spec_counts.push(0);
        }
        held.item.spec_counts.push(chosen.len() as u8);
        held.item.targets.extend(chosen);
        held.item.stage = STAGE_TARGET + spec_index + 1;
    }
    advance(ctx, item)
}

pub fn choose_location(ctx: &mut Ctx, item: u16, zone: u16) -> Result<(), Refusal> {
    let legal = ctx
        .blob
        .pending(item)
        .map(|pending| locations_of(ctx, &pending.item))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|location| ctx.zone_of(location).map(|(zone, _)| zone))
        .any(|held| held == zone);
    if !legal {
        return Err(Refusal::Illegal(Reason::NotHeld));
    }
    if let Some(pending) = ctx.blob.pending_mut(item) {
        pending.item.targets.push(TargetRef::Zone(zone));
        pending.item.stage = STAGE_ACCELERATE;
    }
    advance(ctx, item)
}

pub fn choose_cost(ctx: &mut Ctx, item: u16, paid: bool) -> Result<(), Refusal> {
    if let Some(pending) = ctx.blob.pending_mut(item) {
        let stage = pending.item.stage;
        let next = match stage {
            STAGE_ACCELERATE => STAGE_ADDITIONAL,
            STAGE_REPEAT => STAGE_PROMISED_REPEAT,
            STAGE_PROMISED_REPEAT => STAGE_ADDITIONAL,
            STAGE_ADDITIONAL => stage_after_additional(&pending.item),
            STAGE_PAY => STAGE_PAY,
            _ => STAGE_PAY_WITH,
        };
        let slot = slot_of_stage(stage).unwrap_or(SLOT_ACCELERATE);
        pending.item.set_slot(slot, u8::from(paid));
        pending.item.stage = next;
    }
    advance(ctx, item)
}

pub fn choose_payment(ctx: &mut Ctx, item: u16, gold: Option<u32>) -> Result<(), Refusal> {
    let seat = ctx
        .blob
        .pending(item)
        .map(|pending| pending.item.controller)
        .unwrap_or(0);
    if let Some(gold) = gold {
        if !pay::ready_golds(ctx, seat).contains(&gold) {
            return Err(Refusal::Illegal(Reason::NotALegalTarget));
        }
        ctx.set_flag(gold, crate::state::FLAG_PAYING, true);
    }
    if let Some(pending) = ctx.blob.pending_mut(item) {
        pending.item.stage = STAGE_PAY;
    }
    advance(ctx, item)
}

pub fn cancel(ctx: &mut Ctx, item: u16) {
    let Some(pending) = ctx.blob.take_pending(item) else {
        return;
    };
    pay::clear_pins(ctx, pending.item.controller);
    let Some(card) = pending.item.kind.card() else {
        return;
    };
    if names_at_play(ctx, &pending.item).is_some() {
        ctx.unname(card);
    }
    let seat = pending.item.controller;
    let owner = ctx.owner(card);
    let home = match pending.item.origin {
        Origin::Hand | Origin::Board => ctx.zones.hand,
        Origin::Champion => ctx.zones.champion,
        Origin::Facedown { zone } => Some(zone),
        Origin::Trash { .. } => ctx.zones.trash,
        Origin::Revealed {
            from: RevealedFrom::Deck,
        } => ctx.zones.main_deck,
        Origin::Banishment => ctx.zones.banishment,
    };
    if let Some(zone) = home {
        let to_seat = if matches!(pending.item.origin, Origin::Revealed { .. }) {
            owner
        } else if ctx.zones.is_battlefield(zone) {
            0
        } else {
            seat
        };
        ctx.emit(Effect::Move {
            card,
            zone,
            seat: to_seat,
            index: TOP,
        });
    }
    ctx.narrate(format!("{{seat {seat}}} takes back {{card {card}}}"));
}

fn raise_chosen(ctx: &mut Ctx, item: &ChainItem) {
    for target in &item.targets {
        if let TargetRef::Card(card) = target {
            ctx.raise(Event::Chosen {
                card: *card,
                by: item.controller,
                item: item.id,
            });
        }
    }
}

fn close_with(ctx: &mut Ctx, mut item: ChainItem) {
    item.status = ItemStatus::Finalized;
    item.stage = 0;
    let controller = item.controller;
    ctx.blob.chain.push(item);
    ctx.blob.priority = Some(Priority {
        active: controller,
        passes: 0,
    });
}

fn finalize(ctx: &mut Ctx, item: ChainItem) {
    let seat = item.controller;
    let turn = ctx.turn();
    let opened_chain = ctx.blob.chain.is_empty();
    hide::finalized(ctx, &item);
    match item.kind {
        ItemKind::Permanent { card } => {
            let base = ctx.zones.base.unwrap_or(0);
            let zone = item.zone_target().unwrap_or(base);
            let location = attach::attached_to(ctx, card)
                .and_then(|unit| ctx.location(unit))
                .or_else(|| Location::of_zone(zone, seat, &ctx.zones))
                .unwrap_or(Location::Base(seat));
            let location = match ctx.script(card).and_then(|script| script.enters_at()) {
                Some(run) => run(ctx, card, location),
                None => location,
            };
            if ctx.location(card) != Some(location) {
                if let Some((zone, to_seat)) = ctx.zone_of(location) {
                    ctx.emit(Effect::Move {
                        card,
                        zone,
                        seat: to_seat,
                        index: TOP,
                    });
                }
            }
            if ctx.has_keyword(card, Keyword::Temporary) {
                ctx.mark_temporary(card);
            }
            let unit = ctx.is_unit(card);
            let accelerated = item.accelerated();
            let enters_exhausted = ctx
                .script(card)
                .is_some_and(|script| script.has_static(Static::EntersExhausted));
            {
                let row = ctx.state_mut(card);
                row.entered = turn;
                row.set(FLAG_ENTERED_THIS_TURN, true);
            }
            let kind = if unit { KIND_UNIT } else { KIND_GEAR };
            let contested = match location {
                Location::Battlefield(zone) if unit && !ctx.holds(seat, zone) => {
                    ctx.contest(zone, seat)
                }
                _ => false,
            };
            {
                let token = ctx.is_token(card);
                let counters = ctx.blob.seat_mut(seat);
                counters.cards_played = counters.cards_played.saturating_add(1);
                if !unit && !token {
                    counters.gear_played = counters.gear_played.saturating_add(1);
                }
            }
            ctx.raise(Event::Played {
                card,
                controller: seat,
                kind: kind.into(),
                origin: item.origin,
                paid_additional: item.paid_additional(),
            });
            ctx.raise(Event::Entered { card, at: location });
            let ready = ctx.enters_ready(card, seat, !unit || accelerated);
            if enters_exhausted || !ready {
                ctx.exhaust(card);
                if enters_exhausted {
                    ctx.narrate(format!("{{card {card}}} enters exhausted"));
                }
            } else if unit && !accelerated {
                ctx.narrate(format!("{{card {card}}} enters ready"));
            }
            let where_to = match location {
                Location::Base(_) => "their base".to_string(),
                Location::Battlefield(zone) => format!("{{zone {zone}}}"),
            };
            ctx.narrate(format!(
                "{{seat {seat}}} plays {{card {card}}} to {where_to}"
            ));
            if opened_chain {
                ctx.blob.note_play(seat);
            }
            if unit && location.battlefield().is_some() {
                combat::refresh_open(ctx);
            }
            triggers::collect(ctx);
            ctx.note_played(seat);
            if contested {
                cleanup::run(ctx, None);
            }
        }
        ItemKind::Spell { card } => {
            raise_chosen(ctx, &item);
            let origin = item.origin;
            let paid_additional = item.paid_additional();
            let nth = {
                let counters = ctx.blob.seat_mut(seat);
                counters.cards_played = counters.cards_played.saturating_add(1);
                counters.spells_played = counters.spells_played.saturating_add(1);
                counters.cards_played
            };
            let mut item = item;
            item.set_slot(SLOT_ORDINAL, nth);
            ctx.bind_spell_bonus(seat, item.id, card);
            close_with(ctx, item);
            ctx.raise(Event::Played {
                card,
                controller: seat,
                kind: KIND_SPELL.into(),
                origin,
                paid_additional,
            });
            ctx.narrate(format!("{{seat {seat}}} plays {{card {card}}}"));
            if opened_chain {
                ctx.blob.note_play(seat);
            }
        }
        ItemKind::Ability { .. } | ItemKind::Lent { .. } => {
            let source = item.kind.source();
            raise_chosen(ctx, &item);
            close_with(ctx, item);
            if ctx.is_gear(source) {
                let counters = ctx.blob.seat_mut(seat);
                counters.gear_abilities_activated =
                    counters.gear_abilities_activated.saturating_add(1);
            }
            ctx.narrate(format!("{{seat {seat}}} activates {{card {source}}}"));
            if opened_chain {
                ctx.blob.note_play(seat);
            }
        }
        ItemKind::Trigger { .. } | ItemKind::Granted { .. } => {
            let source = item.kind.source();
            raise_chosen(ctx, &item);
            close_with(ctx, item);
            ctx.narrate(format!("{{card {source}}} triggers"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, priority, prompts};
    use crate::state::{Expiry, GameBlob, Phase, Pool, Promise, PromiseEffect, PromiseKind};
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::prompt::Answer;

    static ADDITIONAL_CARD: crate::cards::Card = prelude::with_additional(
        prelude::spell("Additional Echo", &[], &[]),
        crate::cards::Cost {
            energy: 2,
            power: &[],
        },
    );

    fn item_qualified_add(
        _ctx: &Ctx,
        seat: u8,
        _source: u32,
        paying: Paying,
    ) -> Option<crate::cards::Adds> {
        let item = paying.item()?;
        (item.controller == seat && matches!(item.kind, ItemKind::Permanent { card: 99 }))
            .then_some(crate::cards::Adds::exhausting(crate::cards::Cost {
                energy: 2,
                power: &[],
            }))
    }

    static ITEM_ADD_LEGEND: crate::cards::Card = prelude::adding(
        prelude::legend("Item Add Legend", &[], &[]),
        item_qualified_add,
    );

    static ACCELERATED_UNIT: crate::cards::Card =
        prelude::unit("Item Add Unit", &[Keyword::Accelerate], &[]);

    fn additional_fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            98,
            fixtures::HAND,
            0,
            "Additional Echo",
            1,
            0,
        ));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(98, &ADDITIONAL_CARD);
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        fixture
    }

    #[test]
    fn a_unit_from_hand_to_the_base_is_paid_and_enters_exhausted() {
        let mut fixture = Fixture::enforced();
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::exhaust(fixtures::HAND_UNIT),
            ]
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.has_flag(fixtures::HAND_UNIT, FLAG_ENTERED_THIS_TURN));
        assert_eq!(ctx.state_of(fixtures::HAND_UNIT).unwrap().entered, 1);
        assert!(ctx.blob.seat(0).played_main);
        assert!(
            matches!(ctx.events[0], Event::Played { card, controller: 0, .. } if card == fixtures::HAND_UNIT)
        );
        assert_eq!(
            ctx.events[1],
            Event::Entered {
                card: fixtures::HAND_UNIT,
                at: Location::Base(0)
            }
        );
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} plays {card 70} to their base"
        );
        assert_eq!(ctx.blob.next_item, 1);
    }

    #[test]
    fn a_unit_dragged_to_the_chain_asks_where_it_enters_unless_only_the_base_is_legal() {
        let mut fixture = Fixture::enforced();
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_UNIT, Origin::Hand, None).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(fixtures::HAND_UNIT), Some(Location::Base(0)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::HAND_UNIT,
            zone: fixtures::BASE,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
        let mut holding = Fixture::enforced();
        holding.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = holding.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_UNIT, Origin::Hand, None).unwrap();
        let prompt = ctx.blob.prompt.clone().expect("a location prompt");
        assert_eq!(ctx.blob.why, Some(PromptWhy::PlayLocation { item: 1 }));
        assert!(prompt.cancel);
        assert_eq!(prompt.seat, 0);
        let options = prompts::offered(&ctx);
        assert_eq!(
            options
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["your base", "{zone 9}", "cancel"]
        );
        assert_eq!(options[1].answer, Answer::Zone(fixtures::BF1));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::PlayLocation { item: 1 }),
            "where does {card 70} enter?"
        );
        assert!(ctx.effects.is_empty());
        assert!(ctx
            .units_at(Location::Battlefield(fixtures::BF1))
            .is_empty());
        ctx.blob.close_prompt();
        choose_location(&mut ctx, 1, fixtures::BF1).unwrap();
        assert_eq!(
            ctx.location(fixtures::HAND_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [fixtures::HAND_UNIT]
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), None);
        assert!(ctx.blob.log.last().unwrap().ends_with("to {zone 9}"));
        let mut again = holding.ctx_for(0, &action);
        begin(&mut again, 0, fixtures::HAND_UNIT, Origin::Hand, None).unwrap();
        again.blob.close_prompt();
        assert_eq!(
            choose_location(&mut again, 2, fixtures::BF2),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
    }

    #[test]
    fn a_ready_gold_offers_itself_as_a_payment_source_and_pays_instead_of_a_rune() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::gold(85, 0, false));
        fixture.table.tokens.push(85);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let action = fixtures::move_action(fixtures::HAND_SPELL, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PayWith { item: 1 }));
        let options = prompts::offered(&ctx);
        assert_eq!(
            options
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["kill {card 85}", "recycle a rune", "cancel"],
            "a play can be taken back before anything is paid"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::PayWith { item: 1 }),
            format!("pay 1 power for {{card {}}} with", fixtures::HAND_SPELL)
        );
        assert!(ctx.effects.is_empty(), "nothing is spent before the answer");
        ctx.blob.close_prompt();
        choose_payment(&mut ctx, 1, Some(85)).unwrap();
        assert!(ctx.card(85).is_none(), "the Gold pays with its life");
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { card, zone, .. } if *card == fixtures::RUNE_A && *zone == fixtures::RUNE_DECK
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        let mut rune_instead = fixture.ctx_for(0, &action);
        begin(
            &mut rune_instead,
            0,
            fixtures::HAND_SPELL,
            Origin::Hand,
            None,
        )
        .unwrap();
        rune_instead.blob.close_prompt();
        choose_payment(&mut rune_instead, 2, None).unwrap();
        assert!(rune_instead.card(85).is_some(), "the Gold is left alone");
        assert!(rune_instead.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { card, zone, .. } if *card == fixtures::RUNE_A && *zone == fixtures::RUNE_DECK
        )));
        let mut plain = Fixture::enforced();
        let mut ctx = plain.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        assert_ne!(
            ctx.blob.why,
            Some(PromptWhy::PayWith { item: 1 }),
            "no Gold, no question"
        );
    }

    #[test]
    fn cancel_returns_the_card_to_where_it_came_from() {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_UNIT, Origin::Hand, None).unwrap();
        ctx.blob.close_prompt();
        cancel(&mut ctx, 1);
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: fixtures::HAND_UNIT,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} takes back {card 70}"
        );
        let champion = fixtures::move_action(fixtures::CHAMPION_CARD, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &champion);
        begin(&mut ctx, 0, fixtures::CHAMPION_CARD, Origin::Champion, None).unwrap();
        ctx.blob.close_prompt();
        cancel(&mut ctx, 2);
        assert!(
            matches!(ctx.effects.last(), Some(Effect::Move { zone, .. }) if *zone == fixtures::CHAMPION)
        );
        cancel(&mut ctx, 99);
    }

    #[test]
    fn gear_lands_ready_in_the_base_and_a_spell_waits_on_the_chain_until_every_seat_passes() {
        let mut fixture = Fixture::enforced();
        let action = fixtures::move_action(fixtures::HAND_GEAR, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_GEAR, Origin::Hand, None).unwrap();
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::Move {
                    card: fixtures::HAND_GEAR,
                    zone: fixtures::BASE,
                    seat: 0,
                    index: TOP
                },
            ]
        );
        assert!(!ctx.card(fixtures::HAND_GEAR).unwrap().exhausted);
        assert!(
            ctx.blob.seat(0).played_main,
            "gear is a main deck card, so it satisfies Legion"
        );
        let spell = fixtures::move_action(fixtures::HAND_SPELL, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &spell);
        begin(&mut ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        chain::proceed(&mut ctx);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(42),
                Effect::exhaust(41),
                Effect::Move {
                    card: fixtures::RUNE_A,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Finalized);
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            }),
            "the controller may respond to their own spell"
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(
            ctx.events,
            [Event::Played {
                card: fixtures::HAND_SPELL,
                controller: 0,
                kind: KIND_SPELL.into(),
                origin: Origin::Hand,
                paid_additional: false,
            }],
            "a finalized spell was played, whatever becomes of it"
        );
        assert_eq!(ctx.blob.seat(0).cards_played, 2, "the gear and the spell");
        assert_eq!(ctx.blob.seat(0).spells_played, 1);
        assert_eq!(ctx.blob.seat(0).gear_played, 1);
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} plays {card 71}");
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 1,
                passes: 1
            })
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::Move {
                card: fixtures::HAND_SPELL,
                zone: fixtures::TRASH,
                seat: 0,
                index: TOP
            })
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.seat(0).played_main);
        assert_eq!(
            ctx.events[1],
            Event::PlayedSpell {
                item: 2,
                controller: 0,
                nth: 2
            }
        );
        assert_eq!(
            &ctx.blob.log[ctx.blob.log.len() - 3..],
            ["{seat 0} passes", "{seat 1} passes", "{card 71} resolves"]
        );
    }

    #[test]
    fn optional_costs_answer_into_their_slots_and_the_new_stages_fall_through() {
        assert_eq!(slot_of_stage(STAGE_ACCELERATE), Some(SLOT_ACCELERATE));
        assert_eq!(slot_of_stage(STAGE_PAY), Some(SLOT_TRIGGER_COST));
        assert_eq!(slot_of_stage(STAGE_REPEAT), Some(SLOT_REPEAT));
        assert_eq!(slot_of_stage(STAGE_ADDITIONAL), Some(SLOT_ADDITIONAL));
        assert_eq!(slot_of_stage(STAGE_TARGET), None);
        let spell = ChainItem::new(1, ItemKind::Spell { card: 1 }, 0, Origin::Hand);
        let unit = ChainItem::new(2, ItemKind::Permanent { card: 2 }, 0, Origin::Hand);
        assert_eq!(stage_after_additional(&spell), STAGE_TARGET);
        assert_eq!(stage_after_additional(&unit), STAGE_PAY_WITH);
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().name = "Lillia - Fae Fawn".into();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().domain = vec!["Calm".into()];
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.resolve();
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_ACCELERATE as u8
            })
        );
        let pending = ctx.blob.pending(1).unwrap().item.clone();
        assert_eq!(
            pending.picks,
            [crate::state::UNANSWERED; crate::state::SLOTS]
        );
        assert!(!pending.accelerated());
        ctx.blob.close_prompt();
        if let Some(held) = ctx.blob.pending_mut(1) {
            held.item.set_slot(SLOT_ACCELERATE, 1);
        }
        assert!(ctx.blob.pending(1).unwrap().item.accelerated());
        assert!(
            cost::base_of_item(&ctx, &ctx.blob.pending(1).unwrap().item).energy > 1,
            "the accelerate slot is what the cost reads"
        );
        choose_cost(&mut ctx, 1, false).unwrap();
        assert!(ctx.blob.queue.is_empty());
        assert!(
            ctx.card(fixtures::HAND_UNIT).unwrap().exhausted,
            "the answer written by choose_cost overrides the slot"
        );
        let spell_action = fixtures::move_action(fixtures::HAND_SPELL, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &spell_action);
        begin(&mut ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "a spell starts at STAGE_REPEAT and walks to its targets and the payment"
        );
    }

    #[test]
    fn additional_cost_uses_the_full_item_with_pool_or_excess_discount() {
        let mut pooled = additional_fixture();
        pooled.blob.seat_mut(0).pool.energy = 3;
        let mut ctx = pooled.ctx();
        fixtures::play_from_hand(&mut ctx, 0, 98).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_ADDITIONAL as u8,
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.chain[0].paid_additional());
        assert_eq!(ctx.blob.seat(0).pool.energy, 0);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} spends 3 energy from their rune pool"));

        let mut discounted = additional_fixture();
        discounted.blob.seat_mut(0).promises = vec![Promise {
            kind: PromiseKind::Any,
            effect: PromiseEffect::Discount(Pool {
                energy: 3,
                ..Pool::default()
            }),
            until: Expiry::Permanent,
        }];
        let mut ctx = discounted.ctx();
        fixtures::play_from_hand(&mut ctx, 0, 98).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_ADDITIONAL as u8,
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.chain[0].paid_additional());
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.blob.seat(0).promises.is_empty());
    }

    #[test]
    fn a_saved_limited_queue_keeps_an_external_location_grant_after_its_source_leaves() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            90,
            fixtures::BASE,
            0,
            "Miss Fortune - Buccaneer",
            4,
        ));
        fixture.resolve();

        let mut ctx = fixture.ctx();
        let offered = ctx.play_locations_for(0, fixtures::HAND_UNIT);
        assert!(offered.contains(&Location::Battlefield(fixtures::BF1)));
        ctx.defer_limited = true;
        begin_limited(
            &mut ctx,
            LimitedPlay {
                card: fixtures::HAND_UNIT,
                by: 0,
                origin: Origin::Hand,
                locations: offered,
                price: Price::Free,
            },
        )
        .unwrap();
        ctx.defer_limited = false;
        assert_eq!(ctx.blob.queue.len(), 1);
        assert_eq!(
            ctx.blob.queue[0].item.limited.as_ref().unwrap().zones,
            [fixtures::BASE, fixtures::BF1]
        );

        let mut saved_table = ctx.table.clone();
        saved_table.card_mut(90).unwrap().zone = Some(fixtures::TRASH);
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();

        let mut reloaded = fixture.ctx();
        assert!(
            !reloaded
                .play_locations_for(0, fixtures::HAND_UNIT)
                .contains(&Location::Battlefield(fixtures::BF1)),
            "the external grant is gone from the current ordinary locations"
        );
        chain::proceed(&mut reloaded);
        assert_eq!(
            reloaded.blob.why,
            Some(PromptWhy::PlayLocation { item: 1 }),
            "the saved queue still reaches the captured-location prompt"
        );
        assert!(
            prompts::offered(&reloaded)
                .iter()
                .any(|option| option.answer == Answer::Zone(fixtures::BF1)),
            "the battlefield remains offered after the granting card leaves"
        );
        reloaded.blob.close_prompt();
        choose_location(&mut reloaded, 1, fixtures::BF1).unwrap();
        assert_eq!(
            reloaded.location(fixtures::HAND_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(reloaded.blob.queue.is_empty());
        assert!(reloaded
            .blob
            .log
            .iter()
            .all(|line| !line.contains("taken back")));
    }

    #[test]
    fn a_saved_limited_location_is_taken_back_before_payment_when_a_warden_closes_it() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            90,
            fixtures::BASE,
            0,
            "Miss Fortune - Buccaneer",
            4,
        ));
        fixture.table.cards.push(fixtures::unit(
            91,
            fixtures::TRASH,
            1,
            "Mageseeker Warden",
            5,
        ));
        fixture.table.cards.push(fixtures::gold(92, 0, false));
        fixture.table.tokens.push(92);
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().power = Some(1);
        fixture.resolve();

        let mut ctx = fixture.ctx();
        let offered = ctx.play_locations_for(0, fixtures::HAND_UNIT);
        assert!(offered.contains(&Location::Battlefield(fixtures::BF1)));
        ctx.defer_limited = true;
        begin_limited(
            &mut ctx,
            LimitedPlay {
                card: fixtures::HAND_UNIT,
                by: 0,
                origin: Origin::Hand,
                locations: offered,
                price: Price::PowerOnly,
            },
        )
        .unwrap();
        ctx.defer_limited = false;
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();

        let mut reloaded = fixture.ctx();
        chain::proceed(&mut reloaded);
        reloaded.blob.close_prompt();
        assert!(
            locations_of(&reloaded, &reloaded.blob.pending(1).unwrap().item)
                .contains(&Location::Battlefield(fixtures::BF1))
        );
        {
            let pending = reloaded.blob.pending_mut(1).unwrap();
            pending.item.targets.push(TargetRef::Zone(fixtures::BF1));
            pending.item.stage = STAGE_PAY;
        }

        let ready_runes = reloaded
            .ready_runes_of(0)
            .into_iter()
            .map(|rune| rune.id)
            .collect::<Vec<_>>();
        let gold = reloaded.card(92).unwrap().clone();
        reloaded.table.card_mut(91).unwrap().zone = Some(fixtures::BF1);
        assert!(
            !locations_of(&reloaded, &reloaded.blob.pending(1).unwrap().item)
                .contains(&Location::Battlefield(fixtures::BF1))
        );
        advance(&mut reloaded, 1).unwrap();
        assert_eq!(
            reloaded
                .card(fixtures::HAND_UNIT)
                .and_then(|card| card.zone),
            Some(fixtures::HAND),
            "the refused child returns to its hand"
        );
        assert!(reloaded.blob.queue.is_empty());
        assert!(reloaded
            .blob
            .log
            .iter()
            .any(|line| line.contains("taken back")));
        assert_eq!(
            reloaded
                .ready_runes_of(0)
                .into_iter()
                .map(|rune| rune.id)
                .collect::<Vec<_>>(),
            ready_runes
        );
        assert_eq!(reloaded.card(92).unwrap(), &gold);
        assert!(
            reloaded.effects.iter().all(|effect| matches!(
                effect,
                Effect::Move { card, zone, .. }
                    if *card == fixtures::HAND_UNIT && *zone == fixtures::HAND
            )),
            "the closed location is rejected before any source is paid"
        );
    }

    #[test]
    fn accelerate_affordability_uses_an_item_qualified_add_source() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().name = ITEM_ADD_LEGEND.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(99, fixtures::HAND, 0, "Item Add Unit", 1));
        fixture.table.card_mut(99).unwrap().energy = Some(0);
        for id in [40, 41, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &ITEM_ADD_LEGEND)
            .with_script(99, &ACCELERATED_UNIT);
        let action = fixtures::move_action(99, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, 99, Origin::Hand, Some(Location::Base(0))).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_ACCELERATE as u8,
            })
        );
        ctx.blob.close_prompt();
        choose_cost(&mut ctx, 1, true).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PayWith { item: 1 }));
        ctx.blob.close_prompt();
        choose_payment(&mut ctx, 1, None).unwrap();
        assert!(ctx.on_board(99));
        assert!(!ctx.card(99).unwrap().exhausted);
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { zone, .. } if *zone == ctx.zones.rune_deck.unwrap()
        )));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Annotate { key, .. } if key == "exhausted"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn accelerate_is_an_optional_cost_that_lets_the_unit_enter_ready() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().name = "Lillia - Fae Fawn".into();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().domain = vec!["Calm".into()];
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.resolve();
        fixture
            .blob
            .card_state_mut(fixtures::HAND_UNIT)
            .granted
            .push((
                crate::cards::Keyword::Accelerate,
                crate::state::Expiry::Permanent,
            ));
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(cost::can_accelerate(&ctx, fixtures::HAND_UNIT));
        begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 0 })
        );
        let options = prompts::offered(&ctx);
        assert_eq!(
            options
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["yes", "no", "cancel"]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::OptionalCost { item: 1, cost: 0 }),
            "accelerate {card 70} for 1 energy and 1 Calm power?"
        );
        assert!(ctx.effects.is_empty());
        ctx.blob.close_prompt();
        choose_cost(&mut ctx, 1, true).unwrap();
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(42),
                Effect::exhaust(41),
                Effect::Move {
                    card: 42,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        assert!(!ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        let mut declined = fixture.ctx_for(0, &action);
        begin(
            &mut declined,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        assert_eq!(
            declined.blob.why,
            Some(PromptWhy::OptionalCost { item: 2, cost: 0 })
        );
        declined.blob.close_prompt();
        choose_cost(&mut declined, 2, false).unwrap();
        assert_eq!(
            declined.effects,
            [Effect::exhaust(41), Effect::exhaust(fixtures::HAND_UNIT)]
        );
        let mut broke = Fixture::enforced();
        broke.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(3);
        broke
            .blob
            .card_state_mut(fixtures::HAND_UNIT)
            .granted
            .push((
                crate::cards::Keyword::Accelerate,
                crate::state::Expiry::Permanent,
            ));
        let mut ctx = broke.ctx_for(0, &action);
        begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }

    #[test]
    fn an_ambush_unit_dragged_to_the_chain_on_the_other_seats_turn_enters_its_ambush_battlefield() {
        let mut fixture = Fixture::enforced();
        fixture.blob.turn = Some(crate::state::TurnCore::start(2, 1));
        fixture.blob.set_phase(Phase::Action);
        let mut horror = fixtures::unit(90, fixtures::HAND, 0, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        fixture.table.cards.push(horror);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::spell(91, fixtures::HAND, 1, "Spark", 0, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 1, 91).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let to_base = crate::engine::ctx::EntryMove {
            card: 90,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.base,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            crate::engine::legal::classify(&ctx, 0, &to_base),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        let to_chain = crate::engine::ctx::EntryMove {
            to: ctx.zones.chain,
            ..to_base
        };
        assert!(crate::engine::legal::classify(&ctx, 0, &to_chain).is_ok());
        ctx.table
            .apply_entry(&fixtures::move_action(90, fixtures::CHAIN, 0), 0)
            .unwrap();
        begin(&mut ctx, 0, 90, Origin::Hand, None).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the one ambush battlefield is picked without a prompt"
        );
        assert_eq!(ctx.location(90), Some(Location::Battlefield(fixtures::BF1)));
        assert!(ctx.card(90).unwrap().exhausted);
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            None,
            "its own held battlefield"
        );
        assert!(ctx
            .events
            .iter()
            .all(|event| !matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.blob.chain.len(), 1, "the opponent's spell still waits");
        let mut two = Fixture::enforced();
        two.blob.turn = Some(crate::state::TurnCore::start(2, 1));
        two.blob.set_phase(Phase::Action);
        two.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        two.table.cards.push(fixtures::card(
            54,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        let mut horror = fixtures::unit(90, fixtures::HAND, 0, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        two.table.cards.push(horror);
        two.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        two.table
            .cards
            .push(fixtures::unit(92, fixtures::BF2, 0, "Ally", 2));
        two.table
            .cards
            .push(fixtures::spell(91, fixtures::HAND, 1, "Spark", 0, 0));
        two.resolve();
        let mut ctx = two.ctx();
        fixtures::play_from_hand(&mut ctx, 1, 91).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(90, fixtures::CHAIN, 0), 0)
            .unwrap();
        begin(&mut ctx, 0, 90, Origin::Hand, None).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PlayLocation { item: 2 }));
        assert_eq!(
            crate::engine::legal::locations_for(&ctx, 0, 90),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "only the ambush battlefields are open"
        );
        ctx.blob.close_prompt();
        assert_eq!(
            choose_location(&mut ctx, 2, fixtures::BASE),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
        choose_location(&mut ctx, 2, fixtures::BF2).unwrap();
        assert_eq!(ctx.location(90), Some(Location::Battlefield(fixtures::BF2)));
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(ctx.blob.staged.len(), 1);
        assert!(ctx.blob.staged[0].combat);
    }

    #[test]
    fn a_unit_ambushed_into_an_open_combat_is_designated_at_once_and_its_defend_trigger_fires() {
        let mut fixture = Fixture::enforced();
        let mut horror = fixtures::unit(90, fixtures::HAND, 0, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        fixture.table.cards.push(horror);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        let open = ctx.blob.showdown.clone().expect("the combat opens");
        assert!(open.combat);
        assert_eq!((open.attacker, open.defender), (1, 0));
        assert!(ctx.is_defender(fixtures::VI));
        crate::engine::showdown::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, 90).unwrap();
        assert_eq!(ctx.location(90), Some(Location::Battlefield(fixtures::BF1)));
        assert!(
            ctx.is_defender(90),
            "464.2.c.3.a · a unit arriving during the combat takes its controller's designation"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "When I defend, if an enemy unit is alone here"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source: 90, .. }
        ));
        assert!(
            ctx.blob.showdown.as_ref().is_some_and(|held| held.combat),
            "the combat stays open around the trigger"
        );
    }

    #[test]
    fn a_trigger_whose_only_cost_is_an_exhaust_asks_when_optional_and_pays_silently_otherwise() {
        use crate::cards::prelude::{exhausting_self, optional, triggered};
        use crate::cards::{Card, Flow, Trigger};
        static ASKER: Card = crate::cards::prelude::legend(
            "Asker",
            &[],
            &[optional(exhausting_self(triggered(
                Trigger::YouPlaySpell,
                &[],
                |ctx, item, _| {
                    ctx.narrate(format!("{{card {}}} asked and paid", item.kind.source()));
                    Flow::Done
                },
            )))],
        );
        static SILENT: Card = crate::cards::prelude::legend(
            "Silent",
            &[],
            &[exhausting_self(triggered(
                Trigger::YouPlaySpell,
                &[],
                |ctx, item, _| {
                    ctx.narrate(format!(
                        "{{card {}}} paid without asking",
                        item.kind.source()
                    ));
                    Flow::Done
                },
            ))],
        );
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &ASKER);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert!(!ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 75} asked and paid".to_string()));
        let mut silent = Fixture::enforced();
        silent.scripts = silent
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &SILENT);
        let mut ctx = silent.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "a mandatory exhaust is not a question"
        );
        assert!(ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 75} paid without asking".to_string()));
    }

    #[test]
    fn a_card_that_names_as_it_is_played_asks_before_its_targets_and_a_cancel_forgets_the_name() {
        use crate::cards::prelude::{a_unit, naming, play as play_ability, spell};
        use crate::cards::{Card, Flow, NameKind};
        static NAMER: Card = naming(
            spell(
                "Namer",
                &[],
                &[play_ability(&[a_unit("a unit")], |ctx, item, _| {
                    let named = ctx.named(item.kind.source()).unwrap_or("nothing");
                    ctx.narrate(format!(
                        "{{card {}}} resolves on {named}",
                        item.kind.source()
                    ));
                    Flow::Done
                })],
            ),
            NameKind::Tag,
        );
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &NAMER);
        let action = fixtures::move_action(fixtures::HAND_SPELL, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        begin(&mut ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Name {
                item: 1,
                kind: NameKind::Tag,
                stage: 0
            }),
            "the name comes before the target"
        );
        assert!(ctx.blob.prompt.as_ref().unwrap().cancel);
        assert_eq!(
            choose_name(&mut ctx, 1, NameKind::Tag, u16::MAX),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an index past the list is refused"
        );
        fixtures::choose(&mut ctx, 0, "Poro").unwrap();
        assert_eq!(ctx.named(fixtures::HAND_SPELL), Some("Poro"));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} names Poro", fixtures::HAND_SPELL)));
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 })),
            "then the target"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(
            ctx.named(fixtures::HAND_SPELL),
            None,
            "a cancelled play forgets"
        );
        assert!(ctx.blob.card_state(fixtures::HAND_SPELL).is_none());
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Name { .. })),
            "the next play asks again"
        );
        fixtures::choose(&mut ctx, 0, "Poro").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} resolves on Poro",
            fixtures::HAND_SPELL
        )));
        assert!(
            ctx.blob.card_state(fixtures::HAND_SPELL).is_none(),
            "the name leaves with the spell"
        );
        assert!(ctx.fault.is_none());
    }
}

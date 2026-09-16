use crate::cards::{
    Ability, Keyword, Once, Source, Trigger, Where, Who, IMPLICIT_HUNT, IMPLICIT_TEMPORARY,
    IMPLICIT_VISION, IMPLICIT_WEAPONMASTER, KIND_SPELL, KIND_UNIT,
};
use crate::engine::ctx::{Ctx, Event, Location};
use crate::engine::{attach, play, targets};
use crate::state::{
    once_by_seat, ChainItem, ItemKind, Needs, Noted, Origin, Pending, PromptWhy, TargetRef, When,
    FLAG_ONCE_USED,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub controller: u8,
    pub source: u32,
    pub index: u8,
    pub lender: Option<u32>,
    pub subject: Option<TargetRef>,
    pub noted: Option<Noted>,
    pub origin: Origin,
}

impl Match {
    pub fn kind(&self) -> ItemKind {
        match self.lender {
            Some(lender) => ItemKind::Granted {
                holder: self.source,
                lender,
                index: self.index,
            },
            None => ItemKind::Trigger {
                source: self.source,
                index: self.index,
            },
        }
    }
}

fn texts_of(ctx: &Ctx, card: u32) -> Vec<(Option<u32>, u8, &'static Ability)> {
    let printed = ctx
        .script(card)
        .map(|script| script.abilities)
        .unwrap_or(&[]);
    printed
        .iter()
        .enumerate()
        .map(|(index, ability)| (None, index as u8, ability))
        .chain(
            attach::granted_on(ctx, card)
                .into_iter()
                .map(|(lender, index, ability)| (Some(lender), index, ability)),
        )
        .collect()
}

pub fn subject_of(event: &Event) -> Option<TargetRef> {
    match event {
        Event::Played { card, .. }
        | Event::Moved { card, .. }
        | Event::Entered { card, .. }
        | Event::Died { card, .. }
        | Event::Chosen { card, .. }
        | Event::Readied { card, .. }
        | Event::Attacks { card }
        | Event::Defends { card }
        | Event::DamageDealt { card, .. } => Some(TargetRef::Card(*card)),
        Event::PlayedSpell { item, .. } => Some(TargetRef::Item(*item)),
        Event::Conquered { zone, .. } | Event::Held { zone, .. } => Some(TargetRef::Zone(*zone)),
        Event::Drew { seat, .. } | Event::BeginningPhase { seat } | Event::EndingStep { seat } => {
            Some(TargetRef::Seat(*seat))
        }
        Event::Empowered { card, .. }
        | Event::Disempowered { card }
        | Event::Burned { card, .. }
        | Event::Banished { card, .. } => Some(TargetRef::Card(*card)),
        Event::CombatWon { zone, .. } | Event::CombatLost { zone, .. } => {
            Some(TargetRef::Zone(*zone))
        }
        Event::Activated { item, .. } => Some(TargetRef::Item(*item)),
        Event::TurnQueued { seat } => Some(TargetRef::Seat(*seat)),
    }
}

pub fn collect(ctx: &mut Ctx) -> usize {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    collect_ordered(ctx, &seats)
}

pub fn deathknells(ctx: &Ctx, card: u32, noted: Noted) -> Vec<Match> {
    let event = Event::Died {
        card,
        controller: noted.controller,
        unit: ctx.is_unit(card),
        noted,
    };
    texts_of(ctx, card)
        .into_iter()
        .filter(|(_, _, ability)| ability.trigger == Trigger::Death)
        .filter(|(_, index, ability)| {
            let who = Source {
                card,
                ability: *index,
            };
            ability
                .condition
                .is_none_or(|condition| condition(ctx, &event, who))
        })
        .map(|(lender, index, _)| Match {
            controller: noted.controller,
            source: card,
            index,
            lender,
            subject: Some(TargetRef::Card(card)),
            noted: Some(noted),
            origin: Origin::Board,
        })
        .collect()
}

pub fn collect_ordered(ctx: &mut Ctx, seats: &[u8]) -> usize {
    let mut found: Vec<Match> = std::mem::take(&mut ctx.deaths);
    let sources = sources(ctx);
    while ctx.collected < ctx.events.len() {
        let event = ctx.events[ctx.collected].clone();
        ctx.collected += 1;
        found.extend(find_among(ctx, &sources, &event));
    }
    if found.is_empty() {
        return 0;
    }
    let mut queued = 0;
    for seat in seats.iter().copied() {
        let batch: Vec<Match> = found
            .iter()
            .copied()
            .filter(|held| held.controller == seat)
            .collect();
        let needs = if batch.len() > 1 {
            Needs::Order
        } else {
            Needs::Choices
        };
        for held in batch {
            match once_each_turn(ctx, held.kind()) {
                Once::Never => {}
                Once::PerTurn => {
                    if ctx.has_flag(held.source, FLAG_ONCE_USED) {
                        continue;
                    }
                    ctx.set_flag(held.source, FLAG_ONCE_USED, true);
                }
                Once::PerSeatPerTurn => {
                    let flag = once_by_seat(held.controller);
                    if ctx.has_flag(held.source, flag) {
                        continue;
                    }
                    ctx.set_flag(held.source, flag, true);
                }
            }
            let id = ctx.blob.next_item_id();
            let mut item = ChainItem::new(id, held.kind(), seat, held.origin);
            item.stage = play::STAGE_TARGET;
            item.subject = held.subject;
            item.noted = held.noted;
            ctx.blob.queue.push(Pending { item, needs });
            queued += 1;
        }
    }
    queued
}

fn once_each_turn(ctx: &Ctx, kind: ItemKind) -> Once {
    targets::ability_of_kind(ctx, kind)
        .map(|ability| ability.once)
        .unwrap_or_default()
}

fn once_spent(ctx: &Ctx, source: u32, once: Once, controller: u8) -> bool {
    match once {
        Once::Never => false,
        Once::PerTurn => ctx.has_flag(source, FLAG_ONCE_USED),
        Once::PerSeatPerTurn => ctx.has_flag(source, once_by_seat(controller)),
    }
}

fn sources(ctx: &Ctx) -> Vec<u32> {
    ctx.table
        .cards
        .iter()
        .filter(|card| !card.is_hidden())
        .filter(|card| ctx.face_in_play(card))
        .filter(|card| !ctx.is_pending_play(card.id) && !ctx.is_facedown(card.id))
        .filter(|card| !attach::is_attached(ctx, card.id))
        .map(|card| card.id)
        .collect()
}

pub fn find(ctx: &Ctx, event: &Event) -> Vec<Match> {
    find_among(ctx, &sources(ctx), event)
}

fn implicit_on_entry(ctx: &Ctx, card: u32) -> Vec<Match> {
    let controller = ctx.controller(card);
    let instance = |index: u8| Match {
        controller,
        source: card,
        index,
        subject: Some(TargetRef::Card(card)),
        noted: None,
        lender: None,
        origin: Origin::Board,
    };
    let vision = ctx.keyword_instances(card, Keyword::Vision);
    let weaponmaster = if ctx.is_unit(card) {
        ctx.keyword_instances(card, Keyword::Weaponmaster)
    } else {
        0
    };
    std::iter::repeat_n(IMPLICIT_VISION, vision)
        .chain(std::iter::repeat_n(IMPLICIT_WEAPONMASTER, weaponmaster))
        .map(instance)
        .collect()
}

fn find_among(ctx: &Ctx, sources: &[u32], event: &Event) -> Vec<Match> {
    let mut found = Vec::new();
    for source in sources.iter().copied() {
        if let Event::BeginningPhase { seat } = event {
            if ctx.is_temporary(source) && ctx.controller(source) == *seat && ctx.on_board(source) {
                found.push(Match {
                    controller: *seat,
                    source,
                    index: IMPLICIT_TEMPORARY,
                    lender: None,
                    subject: Some(TargetRef::Card(source)),
                    noted: None,
                    origin: Origin::Board,
                });
            }
        }
        if let Event::Conquered { zone, units, .. } | Event::Held { zone, units, .. } = event {
            if units.contains(&source) && ctx.hunt_value(source) > 0 {
                found.push(Match {
                    controller: ctx.controller(source),
                    source,
                    index: IMPLICIT_HUNT,
                    lender: None,
                    subject: Some(TargetRef::Zone(*zone)),
                    noted: None,
                    origin: Origin::Board,
                });
            }
        }
        for (lender, index, ability) in texts_of(ctx, source) {
            let Some(controller) = matches(ctx, ability.trigger, event, source) else {
                continue;
            };
            let who = Source {
                card: source,
                ability: index,
            };
            if ability
                .condition
                .is_some_and(|condition| !condition(ctx, event, who))
            {
                continue;
            }
            if once_spent(ctx, source, ability.once, controller) {
                continue;
            }
            found.push(Match {
                controller,
                source,
                index,
                lender,
                subject: subject_of(event),
                noted: noted_of(ctx, event),
                origin: played_origin(ability.trigger, event, source),
            });
        }
    }
    if let Event::Entered { card, .. } = event {
        found.extend(implicit_on_entry(ctx, *card));
    }
    found
}

pub fn played_origin(trigger: Trigger, event: &Event, source: u32) -> Origin {
    match event {
        Event::Played {
            card,
            origin: origin @ Origin::Facedown { .. },
            ..
        } if *card == source && matches!(trigger, Trigger::Play | Trigger::PlayFromFacedown) => {
            *origin
        }
        _ => Origin::Board,
    }
}

pub fn noted_of(ctx: &Ctx, event: &Event) -> Option<Noted> {
    match event {
        Event::Moved {
            card,
            from: Some(at),
            ..
        } => {
            let (zone, _) = ctx.zone_of(*at)?;
            Some(Noted {
                zone,
                might: ctx.current_might(*card),
                controller: ctx.controller(*card),
                alone: false,
                buffed: ctx.is_buffed(*card),
            })
        }
        Event::Died { noted, .. } => Some(*noted),
        _ => None,
    }
}

fn owner_of(ctx: &Ctx, source: u32) -> u8 {
    if ctx.is_battlefield_card(source) {
        let zone = ctx.card(source).and_then(|held| held.zone).unwrap_or(0);
        return ctx.blob.holder(zone).unwrap_or_else(|| ctx.turn_player());
    }
    ctx.controller(source)
}

fn at_source(ctx: &Ctx, source: u32, zone: u16) -> bool {
    ctx.card(source).and_then(|held| held.zone) == Some(zone)
}

pub fn matches(ctx: &Ctx, trigger: Trigger, event: &Event, source: u32) -> Option<u8> {
    if let Some(controller) = matches_as_written(ctx, trigger, event, source) {
        return Some(controller);
    }
    attach::mirrors_on(ctx, source)
        .into_iter()
        .filter_map(|mirror| mirror(trigger))
        .find_map(|mirrored| matches_as_written(ctx, mirrored, event, source))
}

fn matches_as_written(ctx: &Ctx, trigger: Trigger, event: &Event, source: u32) -> Option<u8> {
    let owner = owner_of(ctx, source);
    let battlefield = ctx.is_battlefield_card(source);
    let of = |who: Who, card: u32| match who {
        Who::Me => card == source,
        Who::Friendly | Who::You => ctx.controller(card) == owner,
        Who::Enemy => ctx.controller(card) != owner,
        Who::Any => true,
    };
    match (trigger, event) {
        (Trigger::Play, Event::Played { card, .. }) if *card == source => Some(owner),
        (
            Trigger::PlayFromFacedown,
            Event::Played {
                card,
                origin: Origin::Facedown { .. },
                ..
            },
        ) if *card == source => Some(owner),
        (
            Trigger::Move { of: who, to },
            Event::Moved {
                card,
                from,
                to: destination,
                ..
            },
        ) => {
            let where_ok = match to {
                Where::Any => true,
                Where::Battlefield => matches!(destination, Location::Battlefield(_)),
                Where::FromLocation => from.is_some(),
            };
            (of(who, *card) && where_ok).then_some(owner)
        }
        (Trigger::Conquer(who), Event::Conquered { zone, seat, units }) => {
            if battlefield {
                return at_source(ctx, source, *zone).then_some(*seat);
            }
            let hit = match who {
                Who::Me => units.contains(&source),
                Who::Friendly | Who::You => *seat == owner,
                Who::Enemy => *seat != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (Trigger::Hold(who), Event::Held { zone, seat, units }) => {
            if battlefield {
                return at_source(ctx, source, *zone).then_some(*seat);
            }
            let hit = match who {
                Who::Me => units.contains(&source),
                Who::Friendly | Who::You => *seat == owner,
                Who::Enemy => *seat != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (Trigger::Death, Event::Died { card, .. }) if *card == source => Some(owner),
        (
            Trigger::UnitDies(who),
            Event::Died {
                card,
                controller,
                unit: true,
                ..
            },
        ) => {
            let hit = match who {
                Who::Me => *card == source,
                Who::Friendly | Who::You => *card != source && *controller == owner,
                Who::Enemy => *controller != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (
            Trigger::OpponentPlaysUnit,
            Event::Played {
                controller, kind, ..
            },
        ) if *controller != owner && kind == KIND_UNIT => Some(owner),
        (Trigger::YouPlaySpell, Event::PlayedSpell { controller, .. }) if *controller == owner => {
            Some(owner)
        }
        (Trigger::AnyonePlaysSpell, Event::PlayedSpell { controller, .. }) => Some(*controller),
        (Trigger::Draw { nth }, Event::Drew { seat, nth: drawn })
            if *seat == owner && *drawn == nth =>
        {
            Some(owner)
        }
        (Trigger::Chosen, Event::Chosen { card, .. }) if *card == source => Some(owner),
        (Trigger::Readied(who), Event::Readied { card, .. }) => of(who, *card).then_some(owner),
        (Trigger::ChosenFriendly(Who::Any), Event::Chosen { card, by, .. })
            if ctx.controller(*card) == *by && ctx.is_unit(*card) =>
        {
            Some(*by)
        }
        (Trigger::ChosenFriendly(who), Event::Chosen { card, by, .. })
            if *by == owner && of(who, *card) && ctx.is_unit(*card) =>
        {
            Some(owner)
        }
        (Trigger::BeginningPhase, Event::BeginningPhase { seat }) if *seat == owner => Some(owner),
        (Trigger::EndOfTurn, Event::EndingStep { seat }) if *seat == owner => Some(owner),
        (Trigger::Attacks(who), Event::Attacks { card }) => of(who, *card).then_some(owner),
        (Trigger::Defends(who), Event::Defends { card }) => of(who, *card).then_some(owner),
        (Trigger::Damaged(who), Event::DamageDealt { card, .. }) => of(who, *card).then_some(owner),
        (Trigger::Empowered, Event::Empowered { card, .. }) if *card == source => Some(owner),
        (Trigger::YouEmpower, Event::Empowered { card, by }) if *by == owner && *card != source => {
            Some(owner)
        }
        (
            Trigger::Banished(who),
            Event::Banished {
                card,
                owner: whose,
                by,
                token,
            },
        ) => {
            let hit = match who {
                Who::Me => *card == source,
                Who::Friendly => *whose == owner,
                Who::You => *by == owner && *whose == owner && !token,
                Who::Enemy => *whose != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (Trigger::CombatWon(who), Event::CombatWon { zone, seat })
        | (Trigger::CombatLost(who), Event::CombatLost { zone, seat }) => {
            if battlefield {
                return at_source(ctx, source, *zone).then_some(*seat);
            }
            let hit = match who {
                Who::Me => {
                    *seat == owner && ctx.location(source) == Some(Location::Battlefield(*zone))
                }
                Who::Friendly | Who::You => *seat == owner,
                Who::Enemy => *seat != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (
            Trigger::Activation { of: who },
            Event::Activated {
                source: activated,
                controller,
                ..
            },
        ) => {
            let hit = match who {
                Who::Me => *activated == source,
                Who::Friendly | Who::You => *controller == owner,
                Who::Enemy => *controller != owner,
                Who::Any => true,
            };
            hit.then_some(owner)
        }
        (
            Trigger::YouPlayCard,
            Event::Played {
                controller, kind, ..
            },
        ) if *controller == owner && kind != KIND_SPELL => Some(owner),
        (Trigger::YouPlayCard, Event::PlayedSpell { controller, .. }) if *controller == owner => {
            Some(owner)
        }
        (
            Trigger::UnitPlayedHere,
            Event::Played {
                card,
                controller,
                kind,
                ..
            },
        ) if battlefield && kind == KIND_UNIT => {
            let zone = ctx.card(source).and_then(|held| held.zone)?;
            (ctx.location(*card) == Some(Location::Battlefield(zone))).then_some(*controller)
        }
        _ => None,
    }
}

pub fn batch_of(ctx: &Ctx, seat: u8) -> Vec<u16> {
    ctx.blob
        .queue
        .iter()
        .filter(|pending| pending.needs == Needs::Order && pending.item.controller == seat)
        .map(|pending| pending.item.id)
        .collect()
}

fn untargeted_ability(ctx: &Ctx, item: &ChainItem) -> Option<&'static Ability> {
    let ability = match item.kind {
        ItemKind::Trigger { source, index } => targets::ability_at(ctx, source, index),
        ItemKind::Granted { .. } => targets::ability_of_kind(ctx, item.kind),
        _ => None,
    }?;
    (ability.targets.is_empty() && item.targets.is_empty()).then_some(ability)
}

pub fn interchangeable(ctx: &Ctx, batch: &[u16]) -> bool {
    if batch.len() > 1 {
        return false;
    }
    let items: Vec<&ChainItem> = ctx
        .blob
        .queue
        .iter()
        .filter(|pending| batch.contains(&pending.item.id))
        .map(|pending| &pending.item)
        .collect();
    let Some((first, _rest)) = items.split_first() else {
        return false;
    };
    let Some(ability) = untargeted_ability(ctx, first) else {
        return false;
    };
    untargeted_ability(ctx, first).is_some_and(|other| std::ptr::eq(ability, other))
}

pub fn ask_order(ctx: &mut Ctx, seat: u8) -> bool {
    let batch = batch_of(ctx, seat);
    if batch.len() <= 1 || interchangeable(ctx, &batch) {
        for pending in &mut ctx.blob.queue {
            if batch.contains(&pending.item.id) {
                pending.needs = Needs::Choices;
            }
        }
        return false;
    }
    let count = batch.len() as u8;
    ctx.ask(seat, count, count, false, PromptWhy::OrderTriggers { seat });
    true
}

pub fn order(ctx: &mut Ctx, seat: u8, picked: &[u32]) {
    let batch = batch_of(ctx, seat);
    if batch.is_empty() {
        return;
    }
    let first = ctx
        .blob
        .queue
        .iter()
        .position(|pending| batch.contains(&pending.item.id))
        .unwrap_or(0);
    let mut taken: Vec<Pending> = Vec::new();
    ctx.blob.queue.retain(|pending| {
        if batch.contains(&pending.item.id) {
            taken.push(pending.clone());
            false
        } else {
            true
        }
    });
    let mut placed: Vec<Pending> = Vec::new();
    for value in picked {
        let Ok(id) = u16::try_from(*value) else {
            continue;
        };
        if let Some(index) = taken.iter().position(|pending| pending.item.id == id) {
            placed.push(taken.remove(index));
        }
    }
    placed.extend(taken);
    for (offset, mut pending) in placed.into_iter().enumerate() {
        pending.needs = Needs::Choices;
        ctx.blob.queue.insert(first + offset, pending);
    }
}

pub fn queue_delayed(ctx: &mut Ctx, when: When) -> usize {
    let due: Vec<crate::state::Delayed> = ctx
        .blob
        .delayed
        .iter()
        .filter(|delayed| delayed.when == when)
        .cloned()
        .collect();
    ctx.blob.delayed.retain(|delayed| delayed.when != when);
    let queued = due.len();
    for delayed in due {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: delayed.source,
                index: delayed.ability,
            },
            delayed.seat,
            Origin::Board,
        );
        item.targets = delayed
            .args
            .iter()
            .map(|arg| TargetRef::Card(*arg))
            .collect();
        item.stage = play::STAGE_PAY;
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
    }
    queued
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, triggered};
    use crate::cards::{Card, Flow, Keyword, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, targets};

    static HALL: Card = prelude::battlefield(
        "Hall",
        &[],
        &[triggered(Trigger::AnyonePlaysSpell, &[], |_, _, _| {
            Flow::Done
        })],
    );

    static WATCHER: Card = prelude::unit(
        "Watcher",
        &[],
        &[
            triggered(Trigger::YouPlaySpell, &[], |_, _, _| Flow::Done),
            triggered(
                Trigger::Move {
                    of: Who::Me,
                    to: Where::Battlefield,
                },
                &[],
                |_, _, _| Flow::Done,
            ),
            triggered(Trigger::OpponentPlaysUnit, &[], |_, _, _| Flow::Done),
            triggered(Trigger::Conquer(Who::You), &[], |_, _, _| Flow::Done),
            triggered(Trigger::BeginningPhase, &[], |_, _, _| Flow::Done),
        ],
    );

    static GATED: Card = prelude::unit(
        "Gated",
        &[],
        &[prelude::when(
            triggered(Trigger::YouPlaySpell, &[], |_, _, _| Flow::Done),
            |ctx, _, source| ctx.at_battlefield(source.card),
        )],
    );

    fn fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &HALL)
            .with_script(fixtures::VI, &WATCHER)
            .with_script(fixtures::THEIR_UNIT, &GATED);
        fixture
    }

    fn found(ctx: &Ctx, event: Event) -> Vec<(u8, u32, u8)> {
        find(ctx, &event)
            .into_iter()
            .map(|held| (held.controller, held.source, held.index))
            .collect()
    }

    #[test]
    fn events_match_the_trigger_words_and_battlefield_triggers_go_to_the_right_seat() {
        let mut fixture = fixture();
        let mut ctx = fixture.ctx();
        assert_eq!(
            found(
                &ctx,
                Event::PlayedSpell {
                    item: 1,
                    controller: 0,
                    nth: 1
                }
            ),
            [(0, fixtures::VI, 0), (0, fixtures::GROUNDS, 0)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::PlayedSpell {
                    item: 1,
                    controller: 1,
                    nth: 1
                }
            ),
            [(1, fixtures::GROUNDS, 0)],
            "Gated needs its unit at a battlefield"
        );
        assert_eq!(
            found(
                &ctx,
                Event::Moved {
                    card: fixtures::VI,
                    from: Some(Location::Base(0)),
                    to: Location::Battlefield(fixtures::BF1),
                    cause: crate::engine::ctx::MoveCause::Standard,
                    by: None
                }
            ),
            [(0, fixtures::VI, 1)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::Moved {
                    card: fixtures::VI,
                    from: Some(Location::Battlefield(fixtures::BF1)),
                    to: Location::Base(0),
                    cause: crate::engine::ctx::MoveCause::Standard,
                    by: None
                }
            ),
            Vec::<(u8, u32, u8)>::new()
        );
        assert_eq!(
            found(
                &ctx,
                Event::Played {
                    card: 999,
                    controller: 1,
                    kind: KIND_UNIT.into(),
                    origin: Origin::Hand,
                    paid_additional: false,
                }
            ),
            [(0, fixtures::VI, 2)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::Conquered {
                    zone: fixtures::BF2,
                    seat: 0,
                    units: vec![]
                }
            ),
            [(0, fixtures::VI, 3)]
        );
        assert_eq!(
            found(&ctx, Event::BeginningPhase { seat: 1 }),
            [(1, fixtures::SPRITE, IMPLICIT_TEMPORARY)],
            "Temporary is an implicit Beginning-phase trigger of its controller"
        );
        assert_eq!(
            found(&ctx, Event::BeginningPhase { seat: 0 }),
            [(0, fixtures::VI, 4)]
        );
        ctx.raise(Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1,
        });
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 2);
        assert_eq!(ctx.blob.queue[0].needs, Needs::Order);
        assert_eq!(ctx.blob.queue[0].item.stage, play::STAGE_TARGET);
        assert_eq!(batch_of(&ctx, 0), [1, 2]);
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 2, "events are collected once");
        order(&mut ctx, 0, &[2]);
        assert_eq!(
            ctx.blob
                .queue
                .iter()
                .map(|pending| (pending.item.id, pending.needs))
                .collect::<Vec<_>>(),
            [(2, Needs::Choices), (1, Needs::Choices)]
        );
        assert!(!ask_order(&mut ctx, 0));
        ctx.blob.queue.clear();
        ctx.raise(Event::PlayedSpell {
            item: 1,
            controller: 1,
            nth: 1,
        });
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 1);
        assert_eq!(ctx.blob.queue[0].item.controller, 1);
        assert_eq!(ctx.blob.queue[0].needs, Needs::Choices);
    }

    static BLASTCONE: Card = prelude::unit(
        "Blastcone Fae",
        prelude::HIDDEN,
        &[prelude::play(&[prelude::a_unit("a unit")], |_, _, _| {
            Flow::Done
        })],
    );

    const BLASTCONE_CARD: u32 = 95;

    fn blastcone_fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            BLASTCONE_CARD,
            fixtures::HAND,
            0,
            "Blastcone Fae",
            2,
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLASTCONE_CARD, &BLASTCONE);
        fixture
    }

    fn blastcone_face(fixture: &mut Fixture, shown: bool) {
        {
            let card = fixture.table.card_mut(BLASTCONE_CARD).unwrap();
            if shown {
                card.name = "Blastcone Fae".into();
                card.kind = Some("Unit".into());
                card.energy = Some(2);
                card.might = Some(2);
                card.domain = vec!["Fury".into()];
            } else {
                card.name = String::new();
                card.kind = None;
                card.energy = None;
                card.might = None;
                card.domain.clear();
            }
        }
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLASTCONE_CARD, &BLASTCONE);
    }

    fn queued_play_item(ctx: &Ctx) -> ChainItem {
        ctx.blob
            .queue
            .iter()
            .map(|pending| pending.item.clone())
            .chain(ctx.blob.chain.iter().cloned())
            .find(|item| {
                matches!(item.kind, ItemKind::Trigger { source, .. } if source == BLASTCONE_CARD)
            })
            .expect("the play trigger is on the queue or the chain")
    }

    #[test]
    fn a_play_trigger_of_a_hidden_permanent_keeps_the_facedown_origin_and_its_targets() {
        let mut fixture = blastcone_fixture();
        blastcone_face(&mut fixture, false);
        {
            let action = fixtures::move_action(BLASTCONE_CARD, fixtures::BF1, 0);
            let mut ctx = fixture.ctx_for(0, &action);
            let moved = ctx.entry.expect("a drag is an entry move");
            let intent = legal::classify(&ctx, 0, &moved).expect("the [A] hide is legal");
            assert_eq!(
                intent,
                legal::Intent::Hide {
                    card: BLASTCONE_CARD,
                    zone: fixtures::BF1
                }
            );
            crate::engine::act(&mut ctx, 0, intent).expect("the hide is charged and accepted");
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        fixture.blob.core_mut().unwrap().turn += 1;
        blastcone_face(&mut fixture, true);
        let action = fixtures::move_action(BLASTCONE_CARD, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        let intent = legal::classify(&ctx, 0, &moved).expect("737.6 · it reacts from facedown");
        assert_eq!(
            intent,
            legal::Intent::PlayFromFacedown {
                card: BLASTCONE_CARD
            }
        );
        crate::engine::act(&mut ctx, 0, intent).expect("737.1.b · played for nothing");
        let _ = crate::engine::settle(&mut ctx);
        let item = queued_play_item(&ctx);
        assert_eq!(
            item.origin,
            Origin::Facedown {
                zone: fixtures::BF1
            },
            "737.1.d.2 · a play effect of a hidden permanent stays a facedown choice"
        );
        let spec = &BLASTCONE.abilities[0].targets[0];
        let offered = targets::candidates(&ctx, &item, spec);
        assert_eq!(
            offered,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(BLASTCONE_CARD)
            ],
            "only units at the battlefield it was hidden at"
        );
        assert!(
            !offered.contains(&TargetRef::Card(fixtures::SPRITE)),
            "the unit at the other battlefield is out of reach"
        );
    }

    #[test]
    fn the_same_play_trigger_played_from_hand_offers_every_unit() {
        let mut fixture = blastcone_fixture();
        let action = fixtures::move_action(BLASTCONE_CARD, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        let intent = legal::classify(&ctx, 0, &moved).expect("a plain play to a held battlefield");
        assert!(
            matches!(
                intent,
                legal::Intent::Play {
                    card: BLASTCONE_CARD,
                    origin: Origin::Hand,
                    ..
                }
            ),
            "737.3 · a face-up Hidden card is played for its cost as normal"
        );
        crate::engine::act(&mut ctx, 0, intent).expect("its printed cost is payable");
        let _ = crate::engine::settle(&mut ctx);
        let item = queued_play_item(&ctx);
        assert_eq!(item.origin, Origin::Board);
        let spec = &BLASTCONE.abilities[0].targets[0];
        let offered = targets::candidates(&ctx, &item, spec);
        assert!(offered.contains(&TargetRef::Card(fixtures::VI)));
        assert!(
            offered.contains(&TargetRef::Card(fixtures::SPRITE)),
            "737.3 · played normally there is no targeting restriction"
        );
    }

    static HUNTER: Card = prelude::unit("Hunter", &[Keyword::Hunt(2)], &[]);

    #[test]
    fn a_hunt_unit_that_conquers_queues_one_implicit_item_that_scores_its_xp_on_resolution() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &HUNTER);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(fixtures::VI), 2);
        assert_eq!(
            found(
                &ctx,
                Event::Conquered {
                    zone: fixtures::BF1,
                    seat: 0,
                    units: vec![fixtures::VI]
                }
            ),
            [(0, fixtures::VI, IMPLICIT_HUNT)]
        );
        assert!(
            found(
                &ctx,
                Event::Conquered {
                    zone: fixtures::BF1,
                    seat: 0,
                    units: vec![fixtures::SPRITE]
                }
            )
            .is_empty(),
            "a unit that did not conquer gets nothing"
        );
        ctx.grant(
            fixtures::THEIR_UNIT,
            Keyword::Hunt(1),
            crate::state::Expiry::Permanent,
        );
        assert_eq!(
            ctx.hunt_value(fixtures::THEIR_UNIT),
            1,
            "823.2 · granted Hunt counts"
        );
        assert_eq!(
            found(
                &ctx,
                Event::Held {
                    zone: fixtures::BF2,
                    seat: 1,
                    units: vec![fixtures::THEIR_UNIT]
                }
            ),
            [(1, fixtures::THEIR_UNIT, IMPLICIT_HUNT)]
        );
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        });
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 1);
        let item = &ctx.blob.queue[0].item;
        assert!(matches!(
            item.kind,
            ItemKind::Trigger { source, index } if source == fixtures::VI && index == IMPLICIT_HUNT
        ));
        assert_eq!(item.subject, Some(TargetRef::Zone(fixtures::BF1)));
        assert!(std::ptr::eq(
            targets::ability_of(&ctx, item).unwrap(),
            &targets::HUNT_ABILITY
        ));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "383.4.c.2.a · the hunt waits on the chain"
        );
        assert_eq!(ctx.xp(0), 0);
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 2);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    static SPRING: Card = prelude::battlefield(
        "Spring",
        &[],
        &[prelude::once_per_seat_each_turn(
            prelude::on_unit_played_here(&[], |_, _, _| Flow::Done),
        )],
    );

    #[test]
    fn a_once_per_seat_trigger_fires_for_each_seat_once_a_turn_and_hands_the_item_to_the_player() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &SPRING);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let played = |card: u32, controller: u8| Event::Played {
            card,
            controller,
            kind: KIND_UNIT.into(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert_eq!(
            found(&ctx, played(fixtures::THEIR_UNIT, 1)),
            [(1, fixtures::GROUNDS, 0)],
            "the item goes to the seat that played the unit"
        );
        assert!(
            found(&ctx, played(fixtures::SPRITE, 1)).is_empty(),
            "a unit elsewhere is not played here"
        );
        ctx.raise(played(fixtures::VI, 0));
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 1);
        assert!(ctx.has_flag(fixtures::GROUNDS, crate::state::once_by_seat(0)));
        assert!(
            found(&ctx, played(fixtures::VI, 0)).is_empty(),
            "seat 0's once is spent"
        );
        assert_eq!(found(&ctx, played(fixtures::THEIR_UNIT, 1)).len(), 1);
        ctx.raise(played(fixtures::THEIR_UNIT, 1));
        collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 2);
        assert_eq!(ctx.blob.queue[1].item.controller, 1);
        crate::engine::expiry::at_expiration(&mut ctx);
        assert!(!ctx.has_flag(fixtures::GROUNDS, crate::state::once_by_seat(0)));
        assert_eq!(found(&ctx, played(fixtures::VI, 0)).len(), 1);
    }

    static LEGEND: Card = prelude::legend(
        "Legend",
        &[],
        &[
            prelude::on_combat_won(&[], |_, _, _| Flow::Done),
            prelude::on_activated(&[], |_, _, _| Flow::Done),
            prelude::on_you_play_card(&[], |_, _, _| Flow::Done),
            prelude::on_empowered(&[], |_, _, _| Flow::Done),
            triggered(Trigger::CombatLost(Who::You), &[], |_, _, _| Flow::Done),
            prelude::on_you_empower(&[], |_, _, _| Flow::Done),
            prelude::on_you_banish(&[], |_, _, _| Flow::Done),
        ],
    );

    #[test]
    fn the_new_trigger_words_match_their_events_for_the_right_seat() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &LEGEND);
        let ctx = fixture.ctx();
        let legend = fixtures::LEGEND_CARD;
        assert_eq!(
            found(
                &ctx,
                Event::CombatWon {
                    zone: fixtures::BF1,
                    seat: 0
                }
            ),
            [(0, legend, 0)]
        );
        assert!(found(
            &ctx,
            Event::CombatWon {
                zone: fixtures::BF1,
                seat: 1
            }
        )
        .is_empty());
        assert_eq!(
            found(
                &ctx,
                Event::CombatLost {
                    zone: fixtures::BF1,
                    seat: 0
                }
            ),
            [(0, legend, 4)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::Activated {
                    item: 3,
                    source: fixtures::VI,
                    index: 0,
                    controller: 0
                }
            ),
            [(0, legend, 1)]
        );
        assert!(found(
            &ctx,
            Event::Activated {
                item: 3,
                source: fixtures::THEIR_UNIT,
                index: 0,
                controller: 1
            }
        )
        .is_empty());
        assert_eq!(
            found(
                &ctx,
                Event::Played {
                    card: fixtures::HAND_UNIT,
                    controller: 0,
                    kind: KIND_UNIT.into(),
                    origin: Origin::Hand,
                    paid_additional: false,
                }
            ),
            [(0, legend, 2)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::Played {
                    card: fixtures::SPRITE,
                    controller: 0,
                    kind: KIND_UNIT.into(),
                    origin: Origin::Board,
                    paid_additional: false,
                }
            ),
            [(0, legend, 2)],
            "350.2 · a token played by an effect is played too"
        );
        assert_eq!(
            found(
                &ctx,
                Event::Empowered {
                    card: legend,
                    by: 0
                }
            ),
            [(0, legend, 3)],
            "her own Empowered is not something else"
        );
        assert_eq!(
            found(
                &ctx,
                Event::Empowered {
                    card: fixtures::VI,
                    by: 0
                }
            ),
            [(0, legend, 5)]
        );
        assert_eq!(
            found(
                &ctx,
                Event::Empowered {
                    card: fixtures::THEIR_UNIT,
                    by: 0
                }
            ),
            [(0, legend, 5)],
            "your Profiteer on an enemy unit is you empowering"
        );
        assert!(
            found(
                &ctx,
                Event::Empowered {
                    card: fixtures::VI,
                    by: 1
                }
            )
            .is_empty(),
            "an opponent's Sanction on your unit is not"
        );
        assert_eq!(
            found(
                &ctx,
                Event::Banished {
                    card: fixtures::HAND_SPELL,
                    owner: 0,
                    by: 0,
                    token: false
                }
            ),
            [(0, legend, 6)]
        );
        assert!(
            found(
                &ctx,
                Event::Banished {
                    card: fixtures::SPRITE,
                    owner: 0,
                    by: 0,
                    token: true
                }
            )
            .is_empty(),
            "185 · a token is not a card"
        );
        assert!(
            found(
                &ctx,
                Event::Banished {
                    card: fixtures::THEIR_HAND_CARD,
                    owner: 1,
                    by: 0,
                    token: false
                }
            )
            .is_empty(),
            "a card you own"
        );
        assert!(
            found(
                &ctx,
                Event::Banished {
                    card: fixtures::HAND_SPELL,
                    owner: 0,
                    by: 1,
                    token: false
                }
            )
            .is_empty(),
            "banished by you, not by the opponent"
        );
        for event in [
            Event::Empowered { card: 1, by: 0 },
            Event::Disempowered { card: 1 },
            Event::Burned { seat: 0, card: 1 },
            Event::Banished {
                card: 1,
                owner: 0,
                by: 0,
                token: false,
            },
        ] {
            assert_eq!(subject_of(&event), Some(TargetRef::Card(1)));
        }
        assert_eq!(
            subject_of(&Event::CombatWon { zone: 9, seat: 0 }),
            Some(TargetRef::Zone(9))
        );
        assert_eq!(
            subject_of(&Event::Activated {
                item: 4,
                source: 1,
                index: 0,
                controller: 0
            }),
            Some(TargetRef::Item(4))
        );
        assert_eq!(
            subject_of(&Event::TurnQueued { seat: 1 }),
            Some(TargetRef::Seat(1))
        );
    }

    static TWIN: Card = prelude::unit(
        "Twin",
        &[],
        &[prelude::on_you_play_card(&[], |_, _, _| Flow::Done)],
    );

    static AIMED: Card = prelude::unit(
        "Aimed",
        &[],
        &[prelude::on_you_play_card(
            &[prelude::a_card(prelude::UNIT, "unit")],
            |_, _, _| Flow::Done,
        )],
    );

    fn queued(ctx: &mut Ctx, id: u16, source: u32, subject: Option<TargetRef>) {
        let mut item = ChainItem::new(id, ItemKind::Trigger { source, index: 0 }, 0, Origin::Board);
        item.stage = play::STAGE_TARGET;
        item.subject = subject;
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Order,
        });
    }

    fn needs_of(ctx: &Ctx) -> Vec<(u16, Needs)> {
        ctx.blob
            .queue
            .iter()
            .map(|pending| (pending.item.id, pending.needs))
            .collect()
    }

    #[test]
    fn a_batch_of_the_same_untargeted_ability_from_the_same_event_is_ordered() {
        let played = Some(TargetRef::Card(fixtures::HAND_GEAR));
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::VI, &TWIN)
            .with_script(fixtures::THEIR_UNIT, &TWIN)
            .with_script(fixtures::HAND_UNIT, &AIMED)
            .with_script(fixtures::CHAMPION_CARD, &AIMED);
        let mut ctx = fixture.ctx();
        queued(&mut ctx, 1, fixtures::VI, played);
        queued(&mut ctx, 2, fixtures::THEIR_UNIT, played);
        assert!(!interchangeable(&ctx, &[1, 2]));
        assert!(ask_order(&mut ctx, 0));
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        ctx.blob.queue.clear();
        queued(&mut ctx, 3, fixtures::VI, played);
        queued(&mut ctx, 4, fixtures::HAND_UNIT, played);
        assert!(
            !interchangeable(&ctx, &[3, 4]),
            "two different abilities are a real choice"
        );
        assert!(ask_order(&mut ctx, 0));
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        ctx.blob.prompt = None;
        ctx.blob.why = None;
        ctx.blob.queue.clear();
        queued(&mut ctx, 5, fixtures::HAND_UNIT, played);
        queued(&mut ctx, 6, fixtures::CHAMPION_CARD, played);
        assert!(
            !interchangeable(&ctx, &[5, 6]),
            "the same ability with targets is a real choice"
        );
        ctx.blob.queue.clear();
        queued(
            &mut ctx,
            7,
            fixtures::VI,
            Some(TargetRef::Card(fixtures::VI)),
        );
        queued(
            &mut ctx,
            8,
            fixtures::THEIR_UNIT,
            Some(TargetRef::Card(fixtures::THEIR_UNIT)),
        );
        assert!(
            !interchangeable(&ctx, &[7, 8]),
            "the same ability from two events is a real choice"
        );
        ctx.blob.queue.clear();
        queued(&mut ctx, 9, fixtures::VI, played);
        assert!(!ask_order(&mut ctx, 0), "a single item is never asked");
        assert_eq!(needs_of(&ctx), [(9, Needs::Choices)]);
    }

    #[test]
    fn delayed_entries_become_trigger_items_with_their_arguments_as_targets() {
        let mut fixture = fixture();
        let mut ctx = fixture.ctx();
        ctx.delay(
            When::EndOfTurn(1),
            fixtures::VI,
            0,
            3,
            vec![fixtures::THEIR_UNIT],
        );
        ctx.delay(When::BeginningOf(1), fixtures::VI, 0, 4, Vec::new());
        queue_delayed(&mut ctx, When::EndOfTurn(1));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.queue.len(), 1);
        let item = &ctx.blob.queue[0].item;
        assert_eq!(item.targets, [TargetRef::Card(fixtures::THEIR_UNIT)]);
        assert_eq!(item.stage, play::STAGE_PAY);
        assert!(matches!(
            item.kind,
            ItemKind::Trigger { source, index: 3 } if source == fixtures::VI
        ));
    }

    static MOURNER: Card = prelude::legend(
        "Mourner",
        &[],
        &[
            prelude::on_friendly_unit_dies(&[], |_, _, _| Flow::Done),
            prelude::on_enemy_unit_dies(&[], |_, _, _| Flow::Done),
            triggered(
                Trigger::Move {
                    of: Who::Enemy,
                    to: Where::Any,
                },
                &[],
                |_, _, _| Flow::Done,
            ),
        ],
    );

    fn died(card: u32, controller: u8, unit: bool) -> Event {
        Event::Died {
            card,
            controller,
            unit,
            noted: Noted {
                zone: fixtures::BASE,
                might: 3,
                controller,
                alone: false,
                buffed: true,
            },
        }
    }

    #[test]
    fn a_unit_death_reaches_the_friendly_or_enemy_watchers_and_carries_its_snapshot() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &MOURNER);
        let ctx = fixture.ctx();
        let legend = fixtures::LEGEND_CARD;
        assert_eq!(found(&ctx, died(fixtures::VI, 0, true)), [(0, legend, 0)]);
        assert_eq!(
            found(&ctx, died(fixtures::THEIR_UNIT, 1, true)),
            [(0, legend, 1)]
        );
        assert!(
            found(&ctx, died(fixtures::HAND_GEAR, 0, false)).is_empty(),
            "gear is not a unit"
        );
        assert!(
            found(&ctx, died(legend, 0, true)).is_empty(),
            "its own death is Trigger::Death, not a friendly unit's"
        );
        let event = died(fixtures::VI, 0, true);
        let held = find(&ctx, &event);
        assert_eq!(held[0].subject, Some(TargetRef::Card(fixtures::VI)));
        assert_eq!(noted_of(&ctx, &event), held[0].noted);
        assert!(held[0].noted.is_some_and(|noted| noted.buffed));
        let moved = |card: u32| Event::Moved {
            card,
            from: Some(Location::Base(ctx.controller(card))),
            to: Location::Battlefield(fixtures::BF1),
            cause: crate::engine::ctx::MoveCause::Effect,
            by: Some(0),
        };
        assert_eq!(
            found(&ctx, moved(fixtures::THEIR_UNIT)),
            [(0, legend, 2)],
            "Who::Enemy reads the other seat's units"
        );
        assert!(found(&ctx, moved(fixtures::VI)).is_empty());
    }
}

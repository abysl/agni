use super::prelude::{battlefield, done, location_of, triggered, Location};
use super::{Card, Flow, Item, Once, Source, Stage, Trigger, Who, IMPLICIT_HUNT};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::play::STAGE_TARGET;
use crate::engine::triggers;
use crate::state::{
    once_by_seat, ChainItem, ItemKind, Needs, Origin, Pending, TargetRef, FLAG_ONCE_USED,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Activation {
    source: u32,
    index: u8,
    controller: u8,
}

fn once_spent(ctx: &Ctx, source: u32, once: Once, controller: u8) -> bool {
    match once {
        Once::Never => false,
        Once::PerTurn => ctx.has_flag(source, FLAG_ONCE_USED),
        Once::PerSeatPerTurn => ctx.has_flag(source, once_by_seat(controller)),
    }
}

fn spend_once(ctx: &mut Ctx, source: u32, once: Once, controller: u8) {
    match once {
        Once::Never => {}
        Once::PerTurn => ctx.set_flag(source, FLAG_ONCE_USED, true),
        Once::PerSeatPerTurn => ctx.set_flag(source, once_by_seat(controller), true),
    }
}

fn conquer_effects_of(ctx: &Ctx, unit: u32, zone: u16, units: &[u32]) -> Vec<Activation> {
    let controller = ctx.controller(unit);
    let as_if_conquered = Event::Conquered {
        zone,
        seat: controller,
        units: units.to_vec(),
    };
    let mut found = Vec::new();
    if ctx.hunt_value(unit) > 0 {
        found.push(Activation {
            source: unit,
            index: IMPLICIT_HUNT,
            controller,
        });
    }
    let Some(script) = ctx.script(unit) else {
        return found;
    };
    for (index, ability) in script.abilities.iter().enumerate() {
        if !matches!(ability.trigger, Trigger::Conquer(_)) {
            continue;
        }
        let Some(controller) = triggers::matches(ctx, ability.trigger, &as_if_conquered, unit)
        else {
            continue;
        };
        let source = Source {
            card: unit,
            ability: index as u8,
        };
        if ability
            .condition
            .is_some_and(|condition| !condition(ctx, &as_if_conquered, source))
        {
            continue;
        }
        if once_spent(ctx, unit, ability.once, controller) {
            continue;
        }
        found.push(Activation {
            source: unit,
            index: index as u8,
            controller,
        });
    }
    found
}

fn queue(ctx: &mut Ctx, activation: Activation, zone: u16, needs: Needs) {
    let Activation {
        source,
        index,
        controller,
    } = activation;
    if let Some(once) = ctx
        .script(source)
        .and_then(|script| script.abilities.get(usize::from(index)))
        .map(|ability| ability.once)
    {
        spend_once(ctx, source, once, controller);
    }
    let id = ctx.blob.next_item_id();
    let mut item = ChainItem::new(
        id,
        ItemKind::Trigger { source, index },
        controller,
        Origin::Board,
    );
    item.stage = STAGE_TARGET;
    item.subject = Some(TargetRef::Zone(zone));
    ctx.blob.queue.push(Pending { item, needs });
}

fn activate_conquer_effects_of_units_here(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(Location::Battlefield(zone)) = location_of(ctx, item.kind.source()) else {
        return done();
    };
    let units = ctx.units_at(Location::Battlefield(zone));
    let activations: Vec<Activation> = units
        .iter()
        .flat_map(|unit| conquer_effects_of(ctx, *unit, zone, &units))
        .collect();
    if activations.is_empty() {
        ctx.narrate(format!("no conquer effects to activate at {{zone {zone}}}"));
        return done();
    }
    for activation in &activations {
        let batch = activations
            .iter()
            .filter(|other| other.controller == activation.controller)
            .count();
        let needs = if batch > 1 {
            Needs::Order
        } else {
            Needs::Choices
        };
        queue(ctx, *activation, zone, needs);
    }
    ctx.narrate(format!(
        "{{card {}}} activates {} conquer effect{} at {{zone {zone}}}",
        item.kind.source(),
        activations.len(),
        if activations.len() == 1 { "" } else { "s" }
    ));
    done()
}

pub static CARD: Card = battlefield(
    "Reckoner's Arena",
    &[],
    &[triggered(
        Trigger::Hold(Who::You),
        &[],
        activate_conquer_effects_of_units_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        draw, on_conquer, on_conquer_me, once_each_turn, spawn_gold, unit, when,
    };
    use crate::cards::{Keyword, TOKEN_GOLD};
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::PromptWhy;

    const ARENA: u32 = fixtures::GROUNDS;
    const RAIDER: u32 = 90;
    const HUNTER: u32 = 91;
    const PICKY: u32 = 92;
    const THEIR_RAIDER: u32 = 93;
    const DRAWS: usize = 1;

    static RAIDER_CARD: Card = unit(
        "Raider",
        &[],
        &[on_conquer_me(&[], |ctx, item, _| {
            draw(ctx, item.controller, DRAWS);
            Flow::Done
        })],
    );

    static HUNTER_CARD: Card = unit("Hunter", &[Keyword::Hunt(1)], &[]);

    static PICKY_CARD: Card = unit(
        "Picky",
        &[],
        &[once_each_turn(when(
            on_conquer(&[], |ctx, item, _| {
                spawn_gold(ctx, item.controller, false);
                Flow::Done
            }),
            |ctx, _, source| ctx.current_might(source.card) >= 4,
        ))],
    );

    fn arena_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ARENA).unwrap().name = "Reckoner's Arena".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ARENA).unwrap(), &CARD));
        fixture
    }

    fn with_raiders() -> Fixture {
        let mut fixture = arena_held_by_me();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 0, "Raider", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(HUNTER, fixtures::BF1, 0, "Hunter", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(PICKY, fixtures::BF1, 0, "Picky", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_RAIDER, fixtures::BASE, 1, "Raider", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(RAIDER, &RAIDER_CARD)
            .with_script(HUNTER, &HUNTER_CARD)
            .with_script(PICKY, &PICKY_CARD)
            .with_script(THEIR_RAIDER, &RAIDER_CARD);
        fixture
    }

    fn trigger_sources(ctx: &Ctx) -> Vec<(u32, u8, u8)> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, index } => Some((source, index, item.controller)),
                _ => None,
            })
            .collect()
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn resolve_all(ctx: &mut Ctx) {
        for _ in 0..8 {
            if ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty() {
                return;
            }
            priority::pass(ctx, 0).unwrap();
            priority::pass(ctx, 1).unwrap();
        }
    }

    fn golds(ctx: &Ctx, seat: u8) -> usize {
        ctx.faces_on_board()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .count()
    }

    #[test]
    fn the_arena_is_a_plain_hold_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Reckoner's Arena").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert!(!ability.optional);
    }

    #[test]
    fn holding_queues_every_conquer_effect_of_the_units_here_as_one_ordered_batch() {
        let mut fixture = with_raiders();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let xp = ctx.xp(0);
        hold(&mut ctx);
        assert_eq!(
            trigger_sources(&ctx),
            [(ARENA, 0, 0), (HUNTER, IMPLICIT_HUNT, 0)],
            "the hold itself wakes the arena and Hunt; the raider's conquer effect waits"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        let arena_label = format!("{{card {ARENA}}} trigger · {{zone {}}}", fixtures::BF1);
        let hunt_label = fixtures::labels(&ctx)
            .into_iter()
            .find(|label| *label != arena_label)
            .unwrap();
        fixtures::choose(&mut ctx, 0, &hunt_label).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the last trigger is placed on its own"
        );
        assert_eq!(ctx.blob.chain.len(), 2);
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, .. }) if source == ARENA
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ARENA}}} activates 2 conquer effects at {{zone {}}}",
            fixtures::BF1
        )));
        let queued: Vec<(u32, u8, u8)> = ctx
            .blob
            .queue
            .iter()
            .filter_map(|pending| match pending.item.kind {
                ItemKind::Trigger { source, index } => {
                    Some((source, index, pending.item.controller))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            queued,
            [(RAIDER, 0, 0), (HUNTER, IMPLICIT_HUNT, 0)],
            "the raider's conquer effect and Hunt (823.1.b); Picky fails its own condition at 3 Might; a unit elsewhere is not here"
        );
        assert!(ctx
            .blob
            .queue
            .iter()
            .all(|pending| pending.needs == Needs::Order));
        assert!(ctx
            .blob
            .queue
            .iter()
            .all(|pending| pending.item.subject == Some(TargetRef::Zone(fixtures::BF1))));
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        while ctx.blob.prompt.is_some() {
            let label = fixtures::labels(&ctx)[0].clone();
            fixtures::choose(&mut ctx, 0, &label).unwrap();
        }
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS, "the raider drew once");
        assert_eq!(
            ctx.xp(0),
            xp + 2,
            "one XP for the hold, one for the activated conquer"
        );
        assert_eq!(
            ctx.points(0),
            1,
            "activating conquer effects scores no point"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_effect_with_a_once_and_a_condition_is_honoured_when_activated() {
        let mut fixture = with_raiders();
        fixture
            .table
            .cards
            .retain(|card| ![RAIDER, HUNTER].contains(&card.id));
        fixture.table.card_mut(PICKY).unwrap().might = Some(4);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(PICKY, &PICKY_CARD);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert_eq!(trigger_sources(&ctx), [(ARENA, 0, 0)]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(trigger_sources(&ctx), [(PICKY, 0, 0)]);
        assert!(
            ctx.has_flag(PICKY, FLAG_ONCE_USED),
            "its once is spent by the activation"
        );
        resolve_all(&mut ctx);
        assert_eq!(golds(&ctx, 0), 1);
        ctx.blob.clear_scored();
        hold(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            trigger_sources(&ctx).is_empty(),
            "once each turn: a second activation this turn is refused"
        );
        assert!(ctx.blob.log.contains(&format!(
            "no conquer effects to activate at {{zone {}}}",
            fixtures::BF1
        )));
        assert_eq!(golds(&ctx, 0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn units_without_conquer_effects_activate_nothing_and_a_hold_elsewhere_is_silent() {
        let mut fixture = arena_held_by_me();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx);
        assert_eq!(trigger_sources(&ctx), [(ARENA, 0, 0)]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.blob.log.contains(&format!(
            "no conquer effects to activate at {{zone {}}}",
            fixtures::BF1
        )));
        drop(ctx);
        let mut elsewhere = with_raiders();
        let mut ctx = elsewhere.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(trigger_sources(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}

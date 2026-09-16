use super::prelude::{
    became_mighty, channel_exhausted, done, exhausting_self, legend, optional, triggered, when,
};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub const CHANNELS: usize = 1;

pub const WHEN_ONE_OF_YOUR_UNITS_BECOMES_MIGHTY: Trigger = Trigger::Reflexive;

pub fn one_of_your_units_became_mighty(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

pub fn a_unit_of_yours_became_mighty_until_a_might_event_exists(
    ctx: &Ctx,
    unit: u32,
    before: i32,
    source: Source,
) -> bool {
    ctx.card(source.card)
        .is_some_and(|held| ctx.face_in_play(held))
        && ctx.is_unit(unit)
        && ctx.on_board(unit)
        && ctx.controller(unit) == ctx.controller(source.card)
        && became_mighty(before, ctx.current_might(unit))
}

fn riposte(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, CHANNELS);
    done()
}

pub static CARD: Card = legend(
    "Fiora - Grand Duelist",
    &[],
    &[when(
        optional(exhausting_self(triggered(
            WHEN_ONE_OF_YOUR_UNITS_BECOMES_MIGHTY,
            &[],
            riposte,
        ))),
        one_of_your_units_became_mighty,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{activate, priority, prompts, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy, TargetRef};

    const FIORA: u32 = fixtures::LEGEND_CARD;

    fn salon() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(FIORA).unwrap().name = CARD.name.into();
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FIORA).unwrap(), &CARD));
        fixture
    }

    fn queue_the_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: FIORA,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Card(fixtures::VI));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn source() -> Source {
        Source {
            card: FIORA,
            ability: 0,
        }
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    #[test]
    fn the_legend_has_one_may_trigger_that_exhausts_her_behind_a_mighty_watch_seam() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, WHEN_ONE_OF_YOUR_UNITS_BECOMES_MIGHTY);
        assert_eq!(
            WHEN_ONE_OF_YOUR_UNITS_BECOMES_MIGHTY,
            Trigger::Reflexive,
            "the engine raises nothing when Might crosses 5 · the trigger is queued by nothing yet"
        );
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(CHANNELS, 1);
        let mut fixture = salon();
        let ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
    }

    #[test]
    fn becoming_mighty_is_crossing_five_from_below_read_for_her_controllers_units_alone() {
        assert!(became_mighty(4, 5));
        assert!(became_mighty(0, 9));
        assert!(!became_mighty(5, 6), "already Mighty");
        assert!(!became_mighty(5, 4), "shrinking is not becoming");
        assert!(!became_mighty(3, 4));
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let seam = a_unit_of_yours_became_mighty_until_a_might_event_exists;
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!seam(&ctx, fixtures::VI, 3, source()), "still 3");
        let item = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &item, fixtures::VI, 2, None);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(seam(&ctx, fixtures::VI, 3, source()));
        assert!(
            !seam(&ctx, fixtures::VI, 5, source()),
            "it was Mighty already"
        );
        might_this_turn(&mut ctx, &item, fixtures::THEIR_UNIT, 4, None);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 6);
        assert!(
            !seam(&ctx, fixtures::THEIR_UNIT, 2, source()),
            "an enemy unit is not one of yours"
        );
        assert!(
            !seam(&ctx, fixtures::HAND_UNIT, 0, source()),
            "a card in hand is no unit on the board"
        );
        assert!(
            !one_of_your_units_became_mighty(&ctx, &Event::BeginningPhase { seat: 0 }, source()),
            "the condition reads nothing until an event exists"
        );
    }

    #[test]
    fn the_trigger_asks_to_exhaust_her_and_yes_channels_one_rune_exhausted() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        assert_eq!(pool.len(), 4);
        queue_the_trigger(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {FIORA}}} for the {{card {FIORA}}} trigger · {{card {}}}?",
                fixtures::VI
            )
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(FIORA).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FIORA
        ));
        assert_eq!(pool_of(&ctx, 0), pool, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let after = pool_of(&ctx, 0);
        assert_eq!(after.len(), 5, "one rune channelled");
        let arrived: Vec<(u32, bool)> = after
            .iter()
            .copied()
            .filter(|rune| !pool.contains(rune))
            .collect();
        assert_eq!(arrived.len(), 1);
        assert!(arrived[0].1, "channelled exhausted");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(
            pool_of(&ctx, 1).len(),
            2,
            "the opponent's pool is untouched"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_or_an_exhausted_fiora_channels_nothing() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        queue_the_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(FIORA).unwrap().exhausted);
        assert_eq!(pool_of(&ctx, 0), pool);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FIORA}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = salon();
        spent.table.card_mut(FIORA).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        let pool = pool_of(&ctx, 0);
        queue_the_trigger(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FIORA}}} trigger is removed · its source is exhausted"
        )));
    }

    #[test]
    fn a_unit_crossing_five_today_raises_nothing_so_nothing_queues_her() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let item = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &item, fixtures::VI, 2, None);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "engine gap · missing triggers: no event marks a unit crossing 5 Might and Trigger has no BecameMighty(Who); with an Event::BecameMighty { card } raised by every Might writer (might, buff, grants, attach) when current_might crosses from below 5, WHEN_ONE_OF_YOUR_UNITS_BECOMES_MIGHTY becomes that trigger and one_of_your_units_became_mighty reads a_unit_of_yours_became_mighty_until_a_might_event_exists"]
    fn a_friendly_unit_reaching_five_might_queues_her_once_and_she_channels_a_rune() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0).len();
        let item = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &item, fixtures::VI, 2, None);
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        assert_eq!(pool_of(&ctx, 0).len(), pool + 1);
        might_this_turn(&mut ctx, &item, fixtures::VI, 3, None);
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "growing past 5 is not becoming Mighty again"
        );
        might_this_turn(&mut ctx, &item, fixtures::THEIR_UNIT, 5, None);
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "an enemy unit becoming Mighty is not one of yours"
        );
    }
}

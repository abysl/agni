use super::prelude::{done, draw, exhausting_self, legend, optional, triggered};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn gloom(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = legend(
    "Vex - Gloomist",
    &[],
    &[optional(exhausting_self(triggered(
        Trigger::Hold(Who::You),
        &[],
        gloom,
    )))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, cleanup, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};

    const VEX: u32 = fixtures::LEGEND_CARD;

    fn gloomery() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(VEX).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(VEX).unwrap(), &CARD));
        fixture
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        cleanup::score_holds(ctx, seat);
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_one_may_hold_trigger_that_exhausts_her() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert_eq!(DRAWS, 1);
        let mut fixture = gloomery();
        let ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
    }

    #[test]
    fn holding_asks_to_exhaust_her_and_yes_draws_one_when_the_trigger_resolves() {
        let mut fixture = gloomery();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx, 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.points(0), 1, "the hold itself");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {VEX}}} for the {{card {VEX}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        assert!(!ctx.card(VEX).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(VEX).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VEX
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VEX}}} · {{seat 0}} draws 1")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_or_an_exhausted_vex_draws_nothing_and_the_opponents_hold_is_not_hers() {
        let mut fixture = gloomery();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(VEX).unwrap().exhausted);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VEX}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = gloomery();
        spent.table.card_mut(VEX).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        hold(&mut ctx, 0);
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VEX}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut theirs = gloomery();
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "seat 1 holding is not her controller holding · an ally's hold is a 2v2 reading the table does not seat"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn two_holds_in_one_beginning_phase_ask_twice_but_she_can_only_pay_once() {
        let mut fixture = gloomery();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Sentry", 2));
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx, 0);
        assert_eq!(ctx.points(0), 2, "two holds");
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 })
        ));
        fixtures::choose(
            &mut ctx,
            0,
            &format!("{{card {VEX}}} trigger · {{zone {}}}", fixtures::BF1),
        )
        .unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(VEX).unwrap().exhausted);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the second trigger is removed · its source is exhausted"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VEX}}} trigger is removed · its source is exhausted"
        )));
        resolve_top(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS, "one draw, not two");
        assert!(ctx.fault.is_none());
    }
}

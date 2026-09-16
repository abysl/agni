use super::prelude::{done, draw, legend, on_combat_won};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn glorious(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if draw(ctx, seat, DRAWS) == DRAWS {
        ctx.narrate(format!("{{seat {seat}}} draws {DRAWS} for the win"));
    }
    done()
}

pub static CARD: Card = legend(
    "Draven - Glorious Executioner",
    &[],
    &[on_combat_won(&[], glorious)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, settle};
    use crate::state::ItemKind;

    const DRAVEN: u32 = fixtures::LEGEND_CARD;

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(DRAVEN).unwrap().name = CARD.name.into();
        fixture.resolve();
        fixture
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn won(ctx: &mut Ctx, seat: u8) {
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat,
        });
        settle(ctx).unwrap();
    }

    #[test]
    fn the_legend_has_one_combat_won_trigger_that_draws_and_nothing_to_activate() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::CombatWon(Who::You));
        assert!(!ability.optional, "draw 1 is not a may");
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Auto);
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        let mut fixture = arena();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(DRAVEN).unwrap(), &CARD));
        assert!(
            activate::offers(&ctx, 0).is_empty(),
            "a trigger is nothing to activate"
        );
    }

    #[test]
    fn winning_a_combat_draws_one_when_the_trigger_resolves_and_never_exhausts_him() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        won(&mut ctx, 0);
        assert!(ctx.blob.prompt.is_none(), "no cost, nothing to confirm");
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger waits on the chain");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAVEN
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws 1 for the win".to_string()));
        assert!(
            !ctx.card(DRAVEN).unwrap().exhausted,
            "a trigger without an exhaust cost leaves him ready"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_draven_still_draws_and_every_win_this_turn_draws_again() {
        let mut fixture = arena();
        fixture.table.card_mut(DRAVEN).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        won(&mut ctx, 0);
        resolve_top(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "no exhaust cost gates it");
        won(&mut ctx, 0);
        resolve_top(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 2, "not once each turn");
    }

    #[test]
    fn the_opponents_win_is_not_his_and_nothing_is_drawn() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        won(&mut ctx, 1);
        assert!(ctx.blob.chain.is_empty(), "the other seat's win is not his");
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.hand_of(1).len(), theirs);
        ctx.raise(Event::CombatLost {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a loss is not a win");
    }
}

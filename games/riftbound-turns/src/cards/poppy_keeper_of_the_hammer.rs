use super::prelude::{
    activated, done, draw, exhausting_self, gain_xp, legend, named, spending_xp, triggered,
};
use super::{Card, Cost, Flow, Item, Stage, Timing, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const HOLD_XP: u8 = 1;
pub const DRAW_XP: u8 = 3;
pub const DRAWS: usize = 1;
pub const DRAW_ABILITY: u8 = 1;

fn hold_the_line(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, HOLD_XP);
    done()
}

fn hammer_time(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = legend(
    "Poppy - Keeper of the Hammer",
    &[],
    &[
        triggered(Trigger::Hold(Who::You), &[], hold_the_line),
        named(
            spending_xp(
                exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], hammer_time)),
                DRAW_XP,
            ),
            "draw 1",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, cost, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;

    const POPPY: u32 = fixtures::LEGEND_CARD;

    fn keep(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(POPPY).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(POPPY).unwrap(), &CARD));
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
    fn the_legend_gains_xp_on_a_hold_and_spends_three_with_an_exhaust_to_draw() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::You));
        assert!(!hold.optional);
        assert!(hold.cost.is_none());
        assert_eq!(hold.xp, 0);
        assert!(hold.targets.is_empty());
        let draw = &CARD.abilities[usize::from(DRAW_ABILITY)];
        assert_eq!(draw.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(draw.xp, DRAW_XP);
        assert_eq!(draw.self_cost, SelfCost::Exhaust);
        assert_eq!(draw.cost, Some(Cost::FREE));
        assert_eq!(draw.label, Some("draw 1"));
        assert!(draw.targets.is_empty());
        let mut fixture = keep(0);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, POPPY, DRAW_ABILITY).xp, 3);
        assert_eq!(
            cost::of_activation(&ctx, POPPY, DRAW_ABILITY).label(),
            "3 XP"
        );
    }

    #[test]
    fn holding_gains_one_xp_when_the_trigger_resolves_and_the_opponents_hold_is_not_hers() {
        let mut fixture = keep(0);
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.points(0), 1, "the hold itself");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == POPPY
        ));
        assert!(ctx.blob.prompt.is_none(), "no may, no cost");
        assert_eq!(ctx.xp(0), 0, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert!(
            !ctx.card(POPPY).unwrap().exhausted,
            "the hold trigger costs nothing"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut theirs = keep(0);
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert!(
            ctx.blob.chain.is_empty(),
            "seat 1 holding is not her controller holding"
        );
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn three_xp_and_an_exhaust_draw_one_when_the_ability_resolves() {
        let mut fixture = keep(3);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {POPPY}}}: draw 1 (3 XP, exhaust)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, POPPY, DRAW_ABILITY).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.xp(0), 0, "the XP lands with the rest of the cost");
        assert!(ctx.card(POPPY).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        assert!(ctx.blob.log.contains(&"{seat 0} spends 3 XP".to_string()));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {POPPY}}} · {{seat 0}} draws 1")));
        assert_eq!(
            activate::activate(&mut ctx, 0, POPPY, DRAW_ABILITY),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn short_of_three_xp_the_offer_is_greyed_and_the_activation_refused() {
        let mut fixture = keep(2);
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1, "the price is still shown");
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, POPPY, DRAW_ABILITY),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, POPPY, DRAW_ABILITY),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, POPPY, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility)),
            "the hold trigger is not activated"
        );
        assert_eq!(ctx.xp(0), 2);
        assert!(!ctx.card(POPPY).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn a_hold_can_bring_her_to_three_and_the_draw_follows_in_the_same_turn() {
        let mut fixture = keep(2);
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        resolve_top(&mut ctx);
        assert_eq!(ctx.xp(0), 3);
        assert!(activate::offers(&ctx, 0)[0].enabled);
        activate::activate(&mut ctx, 0, POPPY, DRAW_ABILITY).unwrap();
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx.fault.is_none());
    }
}

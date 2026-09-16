use super::prelude::{
    a_unit, activated, card_target, done, empower, exhausting_self, gear, is_empowered,
    might_this_turn, named,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const MIGHT: i16 = 2;
pub const MIGHT_WHILE_EMPOWERED: i16 = 4;

pub fn bonus(ctx: &Ctx, tools: u32) -> i16 {
    if is_empowered(ctx, tools) {
        MIGHT_WHILE_EMPOWERED
    } else {
        MIGHT
    }
}

fn arm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let delta = bonus(ctx, me);
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!("{{card {unit}}} gets +{delta} might this turn"));
    done()
}

pub static CARD: Card = gear(
    "Tools of Empire",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        named(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit("a unit to arm this turn")],
                arm,
            )),
            "+2 Might this turn, +4 while Empowered",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const TOOLS: u32 = 90;

    fn tools(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::gear(TOOLS, fixtures::BASE, 0, "Tools of Empire", 4)
        }
    }

    fn armory(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tools(exhausted));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TOOLS).unwrap(), &CARD));
        fixture
    }

    fn arm_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, TOOLS, 1).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.card(TOOLS).unwrap().exhausted, "exhausted as the cost");
        assert_eq!(ctx.current_might(fixtures::VI), 3, "not until it resolves");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_prints_empower_for_two_and_one_exhaust_activation_over_a_unit() {
        assert!(std::ptr::eq(script_of("Tools of Empire").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        let arm = &CARD.abilities[1];
        assert_eq!(arm.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(arm.cost, Some(Cost::FREE));
        assert_eq!(arm.self_cost, SelfCost::Exhaust);
        assert!(arm.usable.is_none(), "the arm works Empowered or not");
        assert_eq!(arm.targets.len(), 1);
        assert_eq!((arm.targets[0].min, arm.targets[0].max), (1, 1));
        assert_eq!((MIGHT, MIGHT_WHILE_EMPOWERED), (2, 4));
    }

    #[test]
    fn unempowered_the_tools_give_two_this_turn_and_the_bonus_expires_with_the_turn() {
        let mut fixture = armory(false);
        let mut ctx = fixture.ctx();
        assert_eq!(bonus(&ctx, TOOLS), MIGHT);
        arm_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +2 might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn empowered_for_two_energy_the_tools_give_four_instead() {
        let mut fixture = armory(false);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, TOOLS, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2);
        assert!(
            !ctx.card(TOOLS).unwrap().exhausted,
            "the Empower costs no exhaust"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(TOOLS));
        assert_eq!(bonus(&ctx, TOOLS), MIGHT_WHILE_EMPOWERED);
        arm_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +4 might this turn",
            fixtures::VI
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, TOOLS, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_tools_cannot_arm_and_the_other_seat_is_refused() {
        let mut fixture = armory(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, TOOLS, 1),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, TOOLS, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, TOOLS, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.is_empowered(TOOLS),
            "an exhausted tools can still be Empowered"
        );
    }
}

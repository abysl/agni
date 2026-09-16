use super::prelude::{
    a_friendly_unit, activated, card_target, done, exhausting_self, grant_this_turn, legend, named,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const GRANTS: Keyword = Keyword::Tank;

fn shield_an_ally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if grant_this_turn(ctx, unit, GRANTS) {
        ctx.narrate(format!("{{card {unit}}} has Tank this turn"));
    }
    done()
}

pub static CARD: Card = legend(
    "Shen - Eye of Twilight",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Action,
            Cost::FREE,
            &[a_friendly_unit("a friendly unit to give Tank")],
            shield_an_ally,
        )),
        "give a friendly unit Tank",
    )],
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

    const SHEN: u32 = fixtures::LEGEND_CARD;
    const ALLY: u32 = 90;

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SHEN).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Kinkou Monk", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_is_one_free_action_exhaust_that_chooses_a_friendly_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.label, Some("give a friendly unit Tank"));
        assert_eq!(GRANTS, Keyword::Tank);
        let mut fixture = dojo();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(SHEN).unwrap(), &CARD));
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {SHEN}}}: give a friendly unit Tank (exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn exhausting_him_gives_the_chosen_friendly_unit_tank_until_the_turn_ends() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SHEN, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY}}}"),
                "cancel".to_string()
            ],
            "friendly units only · the Sprite and Jinx are the opponent's"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert!(
            ctx.card(SHEN).unwrap().exhausted,
            "the exhaust is paid at finalization"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            !ctx.has_keyword(ALLY, Keyword::Tank),
            "nothing until it resolves"
        );
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(ALLY, Keyword::Tank));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Tank));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} has Tank this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(
            !ctx.has_keyword(ALLY, Keyword::Tank),
            "the grant ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_shen_the_opponents_turn_and_a_closed_chain_are_refused() {
        let mut spent = dojo();
        spent.table.card_mut(SHEN).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SHEN, 0),
            Err(Refusal::Exhausted)
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        drop(ctx);
        let mut theirs = dojo();
        theirs.blob.core_mut().unwrap().player = 1;
        let mut ctx = theirs.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SHEN, 0),
            Err(Refusal::NotYourTurn),
            "an Action outside a showdown needs your turn"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, SHEN, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut closed = dojo();
        let mut ctx = closed.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, SHEN, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "an Action is not a Reaction"
        );
        assert!(!ctx.card(SHEN).unwrap().exhausted);
    }
}

use super::prelude::{
    a_card, activated, bounce, card_target, done, exhausting_self, legend, named, spawn_gold,
    ONE_ENERGY,
};
use super::{Card, Filter, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

const GOLD_ARRIVES_READY: bool = false;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::AtBattlefield]);

fn rip(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    bounce(ctx, unit);
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = legend(
    "Pyke - Bloodharbor Ripper",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            ONE_ENERGY,
            &[a_card(
                FRIENDLY_UNIT_AT_A_BATTLEFIELD,
                "a friendly unit at a battlefield to return to hand",
            )],
            rip,
        )),
        "return a unit for a Gold",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger, TOKEN_GOLD};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;

    const PYKE: u32 = fixtures::LEGEND_CARD;

    fn harbor(vi_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(PYKE).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(vi_at);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PYKE).unwrap(), &CARD));
        fixture
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_one_exhaust_activation_for_one_energy_over_a_friendly_unit_at_a_battlefield()
    {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.xp, 0);
        assert_eq!(ability.label, Some("return a unit for a Gold"));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(ability.targets[0].min, 1);
        let mut fixture = harbor(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, PYKE, 0).energy, 1);
        assert_eq!(
            activate::offers(&ctx, 0)[0].label,
            format!("{{card {PYKE}}}: return a unit for a Gold (1 energy, exhaust)")
        );
    }

    #[test]
    fn activating_him_returns_the_unit_to_hand_and_plays_an_exhausted_gold_when_it_resolves() {
        let mut fixture = harbor(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, PYKE, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()],
            "Vi at the battlefield is the one candidate · the Sprite is theirs, the hand unit is in hand"
        );
        assert!(
            !ctx.card(PYKE).unwrap().exhausted,
            "nothing before the plan"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.card(PYKE).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy paid");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == PYKE
        ));
        assert!(ctx.on_board(fixtures::VI), "nothing until it resolves");
        assert!(golds_of(&ctx, 0).is_empty());
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::HAND),
            "returned to its owner's hand"
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} returns to hand", fixtures::VI)));
        let gold = *golds_of(&ctx, 0).first().expect("one Gold");
        assert!(ctx.is_token(gold));
        assert!(ctx.is_gear(gold));
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(
            activate::activate(&mut ctx, 0, PYKE, 0),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_gone_from_the_battlefield_before_resolution_fizzles_the_gold_with_it() {
        let mut fixture = harbor(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PYKE, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        ctx.recall(fixtures::VI, false);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.on_board(fixtures::VI),
            "355 · the target is no longer legal, nothing is returned"
        );
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "the Gold follows the return that did not happen"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_activation_is_refused_without_a_friendly_unit_at_a_battlefield_short_runes_or_by_the_wrong_seat(
    ) {
        let mut home = harbor(fixtures::BASE);
        let mut ctx = home.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, PYKE, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "Vi in the base is not at a battlefield · the Sprite is the opponent's"
        );
        assert!(
            activate::offers(&ctx, 0).is_empty(),
            "no target, no offer on the strip"
        );
        assert!(golds_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut short = harbor(fixtures::BF1);
        for rune in [41, 42, 43] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, PYKE, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        drop(ctx);
        let mut fixture = harbor(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, PYKE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, PYKE, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn a_cancelled_activation_leaves_him_ready_and_the_runes_untouched() {
        let mut fixture = harbor(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PYKE, 0).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert!(!ctx.card(PYKE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }
}

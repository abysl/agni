use super::prelude::{
    a_friendly_unit, activated, buff, card_target, done, exhausting_self, legend, named, ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const PRICE: super::Cost = ONE_ENERGY;

fn dragons_rage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        } else {
            ctx.narrate(format!("{{card {unit}}} already has a buff"));
        }
    }
    done()
}

pub static CARD: Card = legend(
    "Lee Sin - Blind Monk",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            PRICE,
            &[a_friendly_unit("a friendly unit to buff")],
            dragons_rage,
        )),
        "buff a friendly unit",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::Target;

    const LEE: u32 = fixtures::LEGEND_CARD;

    fn monastery() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LEE).unwrap().name = CARD.name.into();
        fixture.resolve();
        fixture
    }

    fn without_ready_runes(mut fixture: Fixture) -> Fixture {
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = true;
            }
        }
        fixture.resolve();
        fixture
    }

    fn buffed(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    #[test]
    fn the_legend_has_one_sorcery_activation_for_one_energy_and_his_exhaust() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(PRICE));
        assert_eq!(PRICE.energy, 1);
        assert!(PRICE.power.is_empty());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("buff a friendly unit"));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let mut fixture = monastery();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(LEE).unwrap(), &CARD));
        assert_eq!(cost::of_activation(&ctx, LEE, 0).label(), "1 energy");
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {LEE}}}: buff a friendly unit (1 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn the_activation_offers_friendly_units_only_pays_at_the_plan_and_buffs_when_it_resolves() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()],
            "Jinx is the enemy's and the Sprite is theirs too"
        );
        assert!(!ctx.card(LEE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(LEE).unwrap().exhausted, "the exhaust is his cost");
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy, one rune");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == LEE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(!ctx.is_buffed(fixtures::VI), "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(buffed(&ctx, fixtures::VI), 1);
        assert_eq!(ctx.current_might(fixtures::VI), 4, "a buff is +1 Might");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_already_has_a_buff_keeps_the_one_it_has() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(fixtures::VI));
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(buffed(&ctx, fixtures::VI), 1, "buffs do not stack");
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} already has a buff", fixtures::VI)));
    }

    #[test]
    fn a_cancelled_activation_refunds_nothing_because_nothing_was_paid() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert!(!ctx.card(LEE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn the_activation_is_refused_for_the_wrong_seat_an_exhausted_legend_short_runes_and_a_busy_chain(
    ) {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, LEE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        drop(ctx);
        let mut spent = monastery();
        spent.table.card_mut(LEE).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::Exhausted)
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        drop(ctx);
        let mut broke = without_ready_runes(monastery());
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled, "greyed, so the seat sees the price");
        drop(ctx);
        let mut busy = monastery();
        let mut ctx = busy.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "a sorcery activation waits for an empty chain"
        );
        assert!(!ctx.card(LEE).unwrap().exhausted);
    }
}

use super::prelude::{activated, buff, done, exhausting_self, named, unit, with_statics};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static, Timing};
use crate::engine::ctx::{Ctx, COUNTER_BUFFED};
use agni_plugin_sdk::table::Target;

pub fn buffs_on(ctx: &Ctx, me: u32) -> i32 {
    ctx.table
        .counter(Target::Card(me), COUNTER_BUFFED)
        .unwrap_or(0)
}

fn meditate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Lee Sin - Ascetic",
        &[Keyword::Shield(1)],
        &[named(
            exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], meditate)),
            "buff me",
        )],
    ),
    &[Static::CounterCap {
        counter: COUNTER_BUFFED,
        max: None,
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const LEE: u32 = 90;

    fn lee() -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(LEE, fixtures::BF1, 0, "Lee Sin - Ascetic", 5)
        }
    }

    fn monastery() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lee());
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == LEE)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn lee_sin_prints_shield_one_and_an_exhaust_ability_that_buffs_him() {
        assert!(std::ptr::eq(script_of("Lee Sin - Ascetic").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Shield(1)]);
        assert_eq!(CARD.abilities.len(), 1);
        let meditate = &CARD.abilities[0];
        assert_eq!(meditate.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(meditate.self_cost, SelfCost::Exhaust);
        assert_eq!(meditate.cost, Some(Cost::FREE));
        assert!(meditate.targets.is_empty());
        assert_eq!(meditate.label, Some("buff me"));
        assert!(matches!(
            CARD.statics,
            [Static::CounterCap {
                counter: COUNTER_BUFFED,
                max: None
            }]
        ));
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(LEE), 5);
        ctx.mark_defender(LEE);
        assert_eq!(ctx.current_might(LEE), 6, "Shield 1 while he defends");
    }

    #[test]
    fn exhausting_him_buffs_him_when_the_ability_resolves() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(format!("{{card {LEE}}}: buff me (exhaust)"), true)]
        );
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target to choose");
        assert!(ctx.card(LEE).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.is_buffed(LEE), "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(LEE));
        assert_eq!(buffs_on(&ctx, LEE), 1);
        assert_eq!(ctx.current_might(LEE), 6);
        assert!(ctx.blob.log.contains(&format!("{{card {LEE}}} is buffed")));
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::Exhausted),
            "once per ready"
        );
        assert!(his_offers(&ctx).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_exhausted_lee_sin_or_an_opponent_cannot_use_it() {
        let mut fixture = monastery();
        fixture.table.card_mut(LEE).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert!(his_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, LEE, 0),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, LEE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.is_buffed(LEE));
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn he_can_hold_any_number_of_buffs() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(buffs_on(&ctx, LEE), 1);
        ctx.ready(LEE);
        activate::activate(&mut ctx, 0, LEE, 0).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(buffs_on(&ctx, LEE), 2, "I can have any number of buffs");
        assert_eq!(ctx.current_might(LEE), 7);
    }
}

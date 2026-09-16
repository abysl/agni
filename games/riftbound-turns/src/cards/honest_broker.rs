use super::prelude::{deathknell, done, unit};
use super::trove_golem::play_golds;
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const GOLDS: usize = 1;

fn settle_accounts(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_golds(ctx, item.controller, GOLDS);
    done()
}

pub static CARD: Card = unit(
    "Honest Broker",
    &[Keyword::Deathknell],
    &[deathknell(&[], settle_accounts)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::trove_golem::tests::golds_of;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;

    const BROKER: u32 = 90;

    fn exchange(zone: u16, seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(BROKER, zone, seat, "Honest Broker", 2));
        fixture.table.card_mut(BROKER).unwrap().seat = seat;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BROKER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(script_of("Honest Broker").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(death.cost.is_none() && death.condition.is_none());
        assert_eq!(GOLDS, 1);
    }

    #[test]
    fn dying_at_a_battlefield_plays_one_exhausted_gold_into_the_base() {
        let mut fixture = exchange(fixtures::BF1, 0);
        let mut ctx = fixture.ctx();
        ctx.kill(BROKER, Cause::Rule);
        assert_eq!(ctx.card(BROKER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == BROKER
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BROKER
        ));
        assert!(ctx.blob.prompt.is_none());
        assert!(golds_of(&ctx, 0).is_empty(), "the Gold waits for the chain");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds_of(&ctx, 0), [next]);
        assert!(ctx.is_gear(next) && ctx.is_token(next));
        assert!(ctx.card(next).unwrap().exhausted, "played exhausted");
        assert_eq!(ctx.location(next), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_broker_pays_them_and_a_bounce_pays_nobody() {
        let mut fixture = exchange(fixtures::BASE, 1);
        let mut ctx = fixture.ctx();
        ctx.bounce(BROKER);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(golds_of(&ctx, 1).is_empty(), "a bounce is no Deathknell");
        drop(ctx);
        let mut fixture = exchange(fixtures::BASE, 1);
        let mut ctx = fixture.ctx();
        ctx.kill(BROKER, Cause::Rule);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let golds = golds_of(&ctx, 1);
        assert_eq!(golds.len(), GOLDS);
        assert_eq!(ctx.location(golds[0]), Some(Location::Base(1)));
        assert!(golds_of(&ctx, 0).is_empty());
    }
}

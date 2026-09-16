use super::prelude::{done, on_hold_me, unit};
use super::trove_golem::play_golds;
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const GOLDS: usize = 2;

fn endow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_golds(ctx, item.controller, GOLDS);
    done()
}

pub static CARD: Card = unit("Eminent Benefactor", &[], &[on_hold_me(&[], endow)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::trove_golem::tests::golds_of;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;

    const BENEFACTOR: u32 = 90;

    fn estate() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            BENEFACTOR,
            fixtures::BF1,
            0,
            "Eminent Benefactor",
            5,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BENEFACTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_hold_trigger() {
        assert!(std::ptr::eq(
            script_of("Eminent Benefactor").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
        assert_eq!(GOLDS, 2);
    }

    #[test]
    fn holding_with_him_plays_two_exhausted_golds_into_your_base() {
        let mut fixture = estate();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BENEFACTOR
        ));
        assert!(ctx.blob.prompt.is_none(), "the hold asks nothing");
        assert!(golds_of(&ctx, 0).is_empty(), "the Golds wait for the chain");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds, [next, next + 1]);
        for gold in golds {
            assert!(ctx.is_gear(gold) && ctx.is_token(gold));
            assert!(ctx.card(gold).unwrap().exhausted);
            assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        }
        assert_eq!(ctx.points(0), 1, "the hold point stays");
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_with_him_and_a_hold_without_him_pay_nothing() {
        let mut fixture = estate();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(golds_of(&ctx, 0).is_empty(), "a conquer is not a hold");
        drop(ctx);
        let mut fixture = estate();
        fixture.table.card_mut(BENEFACTOR).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi held, not the benefactor");
        assert!(golds_of(&ctx, 0).is_empty());
    }
}

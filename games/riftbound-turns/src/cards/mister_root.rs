use super::prelude::{done, gain_xp, on_move_to_battlefield, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 2;

fn take_root(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Mister Root",
    &[Keyword::Accelerate],
    &[on_move_to_battlefield(&[], take_root)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ROOT: u32 = 90;

    fn root(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(ROOT, zone, 0, "Mister Root", 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn grove(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(root(zone));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ROOT).unwrap(), &CARD));
        fixture
    }

    fn march_to(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, ROOT, from, to);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_accelerate_with_one_move_to_battlefield_trigger() {
        assert!(std::ptr::eq(script_of("Mister Root").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert_eq!(XP, 2);
    }

    #[test]
    fn marching_to_a_battlefield_gains_two_xp_when_the_trigger_resolves() {
        let mut fixture = grove(fixtures::BASE);
        let action = fixtures::move_action(ROOT, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.location(ROOT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ROOT
        ));
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 2 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_move_home_is_not_a_move_to_a_battlefield() {
        let mut fixture = grove(fixtures::BF1);
        let action = fixtures::move_action(ROOT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert_eq!(ctx.location(ROOT), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn without_ganking_a_march_between_battlefields_is_refused_and_another_units_march_is_silent() {
        let mut fixture = grove(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                ROOT,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3),
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        drop(ctx);
        let mut other = grove(fixtures::BASE);
        other.table.card_mut(fixtures::VI).unwrap().exhausted = false;
        let action = fixtures::move_action(fixtures::VI, fixtures::BF3, 0);
        let mut ctx = other.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF3),
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi's march is not Mister Root's");
        assert_eq!(ctx.xp(0), 0);
    }
}

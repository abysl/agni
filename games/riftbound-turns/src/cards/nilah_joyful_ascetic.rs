use super::prelude::{done, gain_xp, on_move, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 1;

fn rejoice(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Nilah - Joyful Ascetic",
    &[Keyword::Accelerate, Keyword::Ganking],
    &[on_move(&[], rejoice)],
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

    const NILAH: u32 = 90;
    const PLAIN: u32 = 54;

    fn nilah(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(NILAH, zone, 0, "Nilah - Joyful Ascetic", 4);
        card.domain = vec!["Body".into()];
        card.energy = Some(3);
        card.power = Some(1);
        card
    }

    fn river(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(nilah(zone));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(NILAH).unwrap(), &CARD));
        fixture
    }

    fn march_to(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, NILAH, from, to);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_accelerate_and_ganking_with_one_move_trigger() {
        assert!(std::ptr::eq(
            script_of("Nilah - Joyful Ascetic").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Accelerate, Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert_eq!(XP, 1);
    }

    #[test]
    fn marching_to_a_battlefield_gains_one_xp_when_the_trigger_resolves() {
        let mut fixture = river(fixtures::BASE);
        let action = fixtures::move_action(NILAH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.location(NILAH),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.prompt.is_none(), "Vi is exhausted · no companion");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == NILAH
        ));
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ganking_march_between_battlefields_and_a_move_home_both_count() {
        let mut fixture = river(fixtures::BF1);
        let action = fixtures::move_action(NILAH, fixtures::BF3, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::legal_destination(
            &ctx,
            NILAH,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3),
        )
        .unwrap();
        march_to(
            &mut ctx,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3),
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), i32::from(XP));
        drop(ctx);
        let mut home = river(fixtures::BF1);
        let action = fixtures::move_action(NILAH, fixtures::BASE, 0);
        let mut ctx = home.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert_eq!(ctx.location(NILAH), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1, "any move is a move");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), i32::from(XP));
    }

    #[test]
    fn another_units_move_is_not_hers_and_a_unit_without_ganking_cannot_take_her_route() {
        let mut fixture = river(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = false;
        let action = fixtures::move_action(fixtures::VI, fixtures::BF3, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF3),
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi moved, Nilah did not");
        assert_eq!(ctx.xp(0), 0);
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF3),
                Location::Battlefield(fixtures::BF1),
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
    }
}

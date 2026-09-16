use super::prelude::{done, gain_xp, on_friendly_unit_dies, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 1;

pub fn feast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Vicious Snapjaws",
    &[],
    &[on_friendly_unit_dies(&[], feast)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Once, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const SNAPJAWS: u32 = 90;
    const BAIT: u32 = 91;

    fn snapjaws(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(SNAPJAWS, zone, seat, "Vicious Snapjaws", 5);
        card.energy = Some(5);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn shallows() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(snapjaws(fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(BAIT, fixtures::BASE, 0, "Bait", 1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_the_pool_name_with_a_friendly_death_trigger() {
        assert!(std::ptr::eq(script_of("Vicious Snapjaws").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert_eq!(ability.once, Once::Never);
        assert!(ability.condition.is_none());
        assert_eq!(XP, 1);
        let fixture = shallows();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SNAPJAWS).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_effect_gains_one_xp_for_the_controller_of_the_snapjaws() {
        let mut fixture = shallows();
        let mut ctx = fixture.ctx();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: SNAPJAWS,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(feast(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
    }

    #[test]
    fn its_own_death_is_not_another_units() {
        let mut fixture = shallows();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(SNAPJAWS, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn every_other_friendly_death_gains_one_xp_and_an_enemy_death_gains_nothing() {
        let mut fixture = shallows();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(BAIT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "not once each turn");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2 * i32::from(XP));
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "an enemy death is not friendly");
        assert_eq!(ctx.xp(0), 2 * i32::from(XP));
    }
}

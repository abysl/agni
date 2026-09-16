use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static};

pub const LEVEL: u8 = 11;
pub const BONUS: i16 = 4;

pub static LEVELED: &[Grant] = &[Grant::Might(BONUS)];

pub static CARD: Card = with_statics(
    unit("Targonian Visionary", &[], &[]),
    &[Static::Level(LEVEL, LEVELED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{statics, triggers};
    use agni_plugin_sdk::table::CardInfo;

    const VISIONARY: u32 = 90;

    fn visionary(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(VISIONARY, zone, seat, "Targonian Visionary", 6)
        }
    }

    fn summit(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(visionary(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VISIONARY).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_a_level_eleven_of_plus_four() {
        assert!(std::ptr::eq(
            script_of("Targonian Visionary").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Level(11, [Grant::Might(4)])]
        ));
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn at_eleven_xp_it_is_a_ten_and_at_ten_the_printed_six() {
        let mut fixture = summit(10);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(VISIONARY), 6);
        assert!(statics::grants_on(&ctx, VISIONARY).is_empty());
        drop(ctx);

        let mut fixture = summit(11);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.current_might(VISIONARY),
            10,
            "824.1.c · Level 11 at 11 XP"
        );
        assert!(matches!(
            statics::grants_on(&ctx, VISIONARY).as_slice(),
            [Grant::Might(4)]
        ));
        drop(ctx);

        let mut fixture = summit(20);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(VISIONARY), 10, "the Level is a threshold");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_xp_does_not_level_it_and_it_hunts_nothing() {
        let mut fixture = summit(0);
        fixture.set_xp(1, 11);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.current_might(VISIONARY),
            6,
            "Level reads its controller's XP"
        );
        assert_eq!(ctx.hunt_value(VISIONARY), 0);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![VISIONARY],
        });
        assert_eq!(triggers::collect(&mut ctx), 0, "no Hunt, no XP");
    }

    #[test]
    fn xp_scored_mid_request_levels_it_at_once_and_spending_below_eleven_drops_the_bonus() {
        let mut fixture = summit(9);
        let mut ctx = fixture.ctx();
        ctx.score_xp(0, 2);
        assert_eq!(ctx.xp(0), 11);
        assert_eq!(ctx.current_might(VISIONARY), 10);
        assert!(ctx.spend_xp(0, 1));
        assert_eq!(ctx.current_might(VISIONARY), 6);
        assert!(ctx.fault.is_none());
    }
}

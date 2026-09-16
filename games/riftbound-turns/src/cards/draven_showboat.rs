use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: i32 = 16;

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(ctx.points(ctx.controller(card))).unwrap_or(i16::MAX)
}

fn at_least<const N: i32>(ctx: &Ctx, card: u32, _: u32) -> bool {
    i32::from(might_bonus(ctx, card)) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static POINTS: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Draven - Showboat", &[], &[]),
    &[Static::While(always, POINTS)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::rules::DEFAULT_VICTORY_SCORE;

    const DRAVEN: u32 = 90;
    const THEIR_DRAVEN: u32 = 91;

    fn showboating() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            DRAVEN,
            fixtures::BASE,
            0,
            "Draven - Showboat",
            3,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_DRAVEN,
            fixtures::BASE,
            1,
            "Draven - Showboat",
            3,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRAVEN).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_whose_while_is_one_might_per_point_up_to_the_ladder() {
        assert!(std::ptr::eq(script_of("Draven - Showboat").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(POINTS.len(), LADDER as usize);
        assert!(POINTS
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
        assert!(
            i32::try_from(POINTS.len()).unwrap() >= 2 * DEFAULT_VICTORY_SCORE,
            "every points total a standard game can reach is on the ladder"
        );
    }

    #[test]
    fn his_might_follows_his_controllers_points_as_they_are_scored() {
        let mut fixture = showboating();
        fixture.set_points(0, 3);
        fixture.set_points(1, 5);
        let mut ctx = fixture.ctx();
        assert_eq!(might_bonus(&ctx, DRAVEN), 3);
        assert_eq!(statics::grants_on(&ctx, DRAVEN).len(), 3);
        assert_eq!(ctx.current_might(DRAVEN), 6);
        assert_eq!(
            ctx.current_might(THEIR_DRAVEN),
            8,
            "your points · each reads its own controller's"
        );
        ctx.score_effect(0);
        assert_eq!(ctx.points(0), 4);
        assert_eq!(ctx.current_might(DRAVEN), 7);
        assert_eq!(ctx.current_might(THEIR_DRAVEN), 8);
        assert!(ctx.set_controller(THEIR_DRAVEN, 0, DRAVEN));
        assert_eq!(
            ctx.current_might(THEIR_DRAVEN),
            7,
            "a control change reads the new controller's points"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_zero_points_or_in_hand_he_is_a_plain_three() {
        let mut fixture = showboating();
        fixture.table.cards.push(fixtures::unit(
            92,
            fixtures::HAND,
            0,
            "Draven - Showboat",
            3,
        ));
        fixture.set_points(0, 4);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(THEIR_DRAVEN), 3);
        assert!(statics::grants_on(&ctx, THEIR_DRAVEN).is_empty());
        assert!(
            statics::grants_on(&ctx, 92).is_empty(),
            "365.1 · not on the board, so the passive is inactive"
        );
    }
}

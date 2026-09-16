use super::prelude::{unit, with_statics, Location};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn buffed_friendly_units_at_my_battlefield(ctx: &Ctx, card: u32) -> usize {
    let Some(at @ Location::Battlefield(_)) = ctx.location(card) else {
        return 0;
    };
    let seat = ctx.controller(card);
    ctx.units_at(at)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == seat && ctx.is_buffed(*unit))
        .count()
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(buffed_friendly_units_at_my_battlefield(ctx, card)).unwrap_or(i16::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32, _: u32) -> bool {
    buffed_friendly_units_at_my_battlefield(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static BUFFED_HERE: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Sett - Kingpin", &[Keyword::Tank], &[]),
    &[Static::While(always, BUFFED_HERE)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const SETT: u32 = 90;
    const ALLY: u32 = 91;
    const SECOND_ALLY: u32 = 92;

    fn ringside(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(SETT, zone, 0, "Sett - Kingpin", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, zone, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND_ALLY, fixtures::BF2, 0, "Second", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(zone);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SETT).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_tank_and_its_while_is_one_might_per_buffed_friendly_unit_here() {
        assert!(std::ptr::eq(script_of("Sett - Kingpin").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(BUFFED_HERE.len(), LADDER);
        assert!(BUFFED_HERE
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn each_buffed_friendly_unit_at_his_battlefield_is_one_might_himself_included() {
        let mut fixture = ringside(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(SETT), 5);
        assert!(statics::grants_on(&ctx, SETT).is_empty());
        assert!(ctx.buff(ALLY));
        assert_eq!(might_bonus(&ctx, SETT), 1);
        assert_eq!(ctx.current_might(SETT), 6);
        assert!(ctx.buff(SETT));
        assert_eq!(might_bonus(&ctx, SETT), 2, "he counts himself");
        assert_eq!(
            ctx.current_might(SETT),
            8,
            "his own buff and two buffed units"
        );
        assert!(ctx.buff(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(SETT),
            8,
            "friendly · the enemy's buff is not his"
        );
        assert!(ctx.buff(SECOND_ALLY));
        assert_eq!(
            ctx.current_might(SETT),
            8,
            "at my battlefield · elsewhere is not here"
        );
        assert_eq!(
            ctx.move_unit(
                SECOND_ALLY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(SETT), 9);
        ctx.recall(ALLY, false);
        assert_eq!(ctx.current_might(SETT), 8);
        assert_eq!(ctx.current_might(ALLY), 3, "the ally reads only its buff");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_base_there_is_no_my_battlefield_and_he_reads_his_buff_alone() {
        let mut fixture = ringside(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(ALLY));
        assert!(ctx.buff(SETT));
        assert_eq!(might_bonus(&ctx, SETT), 0);
        assert_eq!(ctx.current_might(SETT), 6);
        assert_eq!(
            ctx.move_unit(
                SETT,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(SETT),
            7,
            "alone and buffed at a battlefield"
        );
    }
}

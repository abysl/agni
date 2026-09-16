use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

pub fn other_buffed_friendly_at_my_battlefield(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source
        && ctx.at_battlefield(source)
        && ctx.controller(unit) == ctx.controller(source)
        && ctx.is_buffed(unit)
}

pub static CARD: Card = with_statics(
    unit("Lee Sin - Centered", &[Keyword::Accelerate], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: other_buffed_friendly_at_my_battlefield,
        grants: &[Grant::Might(BONUS)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const LEE: u32 = 90;
    const ALLY: u32 = 91;

    fn centered(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(LEE, zone, 0, "Lee Sin - Centered", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, zone, 0, "Ally", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(zone);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(LEE).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_accelerate_and_its_aura_gives_plus_two_to_other_buffed_friendly_units_here(
    ) {
        assert!(std::ptr::eq(
            script_of("Lee Sin - Centered").unwrap(),
            &CARD
        ));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(2)],
                ..
            }]
        ));
    }

    #[test]
    fn a_buffed_ally_at_his_battlefield_reads_plus_three_and_he_reads_his_buff_alone() {
        let mut fixture = centered(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(ALLY), 2, "unbuffed");
        assert!(statics::grants_on(&ctx, ALLY).is_empty());
        assert!(ctx.buff(ALLY));
        assert!(matches!(
            statics::grants_on(&ctx, ALLY).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.current_might(ALLY), 5, "the buff's +1 and his +2");
        assert!(ctx.buff(LEE));
        assert_eq!(ctx.current_might(LEE), 7, "other · his own buff is one");
        assert!(ctx.buff(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "friendly · the enemy's buff is one"
        );
        assert_eq!(
            ctx.move_unit(
                ALLY,
                Location::Battlefield(fixtures::BF2),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(ALLY),
            3,
            "at my battlefield · gone elsewhere"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_base_he_has_no_battlefield_so_a_buffed_ally_beside_him_gets_nothing() {
        let mut fixture = centered(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(ALLY));
        assert!(statics::grants_on(&ctx, ALLY).is_empty());
        assert_eq!(ctx.current_might(ALLY), 3);
        assert_eq!(
            ctx.move_unit(LEE, Location::Battlefield(fixtures::BF1), MoveCause::Effect),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(ALLY), 3, "he left the ally behind");
        assert_eq!(
            ctx.move_unit(
                ALLY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(ALLY), 5, "together at his battlefield");
    }
}

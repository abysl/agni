use super::prelude::{unit, with_statics, Location};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::rules::COUNTER_TEMPORARY;
use agni_plugin_sdk::table::Target;

pub const LADDER: usize = 16;

pub fn temporary_on_the_face(ctx: &Ctx, unit: u32) -> bool {
    ctx.table
        .counter(Target::Card(unit), COUNTER_TEMPORARY)
        .unwrap_or(0)
        > 0
        || ctx
            .script(unit)
            .is_some_and(|script| script.has_keyword(Keyword::Temporary))
        || ctx.blob.card_state(unit).is_some_and(|row| {
            row.granted
                .iter()
                .any(|(granted, _)| granted.same_kind(Keyword::Temporary))
        })
}

pub fn temporary_friendly_units_at_my_battlefield(ctx: &Ctx, card: u32) -> usize {
    let Some(at @ Location::Battlefield(_)) = ctx.location(card) else {
        return 0;
    };
    let seat = ctx.controller(card);
    ctx.units_at(at)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == seat && temporary_on_the_face(ctx, *unit))
        .count()
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(temporary_friendly_units_at_my_battlefield(ctx, card).min(LADDER))
        .unwrap_or(i16::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32, _: u32) -> bool {
    temporary_friendly_units_at_my_battlefield(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static TEMPORARY_HERE: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Petal Pixie", &[], &[]),
    &[Static::While(always, TEMPORARY_HERE)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{grant_this_turn, spawn, Token};
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const PIXIE: u32 = 90;
    const SECOND_PIXIE: u32 = 91;

    fn meadow(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut pixie = fixtures::unit(PIXIE, zone, 0, "Petal Pixie", 2);
        pixie.domain = vec!["Mind".into()];
        fixture.table.cards.push(pixie);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PIXIE).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_only_static_is_the_temporary_ladder() {
        assert!(std::ptr::eq(script_of("Petal Pixie").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(TEMPORARY_HERE.len(), LADDER);
        assert!(TEMPORARY_HERE
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn each_of_your_temporary_units_at_her_battlefield_is_one_might() {
        let mut fixture = meadow(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(PIXIE), 2);
        assert_eq!(might_bonus(&ctx, PIXIE), 0);
        let first = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert_eq!(temporary_friendly_units_at_my_battlefield(&ctx, PIXIE), 1);
        assert_eq!(might_bonus(&ctx, PIXIE), 1);
        assert_eq!(ctx.current_might(PIXIE), 3);
        assert_eq!(ctx.projected_might(PIXIE), 1);
        let second = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert_eq!(ctx.current_might(PIXIE), 4);
        assert_eq!(
            statics::grants_on(&ctx, PIXIE)
                .iter()
                .filter(|grant| matches!(grant, Grant::MightIf(_, 1)))
                .count(),
            2
        );
        let in_base = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert_eq!(
            ctx.current_might(PIXIE),
            4,
            "a Temporary unit in the base is not at her battlefield"
        );
        assert_eq!(ctx.current_might(in_base), 3);
        assert_eq!(
            ctx.current_might(first),
            3,
            "the Sprites themselves are unchanged"
        );
        ctx.kill(second, Cause::Rule);
        assert_eq!(ctx.current_might(PIXIE), 3, "the bonus follows the count");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_card_unit_granted_temporary_counts_and_enemy_temporary_units_do_not() {
        let mut fixture = meadow(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.is_temporary(fixtures::SPRITE));
        assert_eq!(
            ctx.current_might(PIXIE),
            2,
            "the enemy Sprite at her battlefield is not yours"
        );
        assert!(grant_this_turn(&mut ctx, fixtures::VI, Keyword::Temporary));
        assert!(ctx.is_temporary(fixtures::VI));
        assert_eq!(ctx.current_might(PIXIE), 3, "Vi is Temporary this turn");
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(PIXIE), 2);
    }

    #[test]
    fn two_pixies_at_one_battlefield_read_each_other_without_looping_and_the_face_reading_matches_the_engine(
    ) {
        let mut fixture = meadow(fixtures::BF1);
        let mut other = fixtures::unit(SECOND_PIXIE, fixtures::BF1, 0, "Petal Pixie", 2);
        other.domain = vec!["Mind".into()];
        fixture.table.cards.push(other);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(PIXIE), 2);
        assert_eq!(ctx.current_might(SECOND_PIXIE), 2);
        let sprite = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert_eq!(ctx.current_might(PIXIE), 3);
        assert_eq!(ctx.current_might(SECOND_PIXIE), 3);
        for unit in [PIXIE, SECOND_PIXIE, sprite, fixtures::SPRITE, fixtures::VI] {
            assert_eq!(
                temporary_on_the_face(&ctx, unit),
                ctx.is_temporary(unit),
                "{unit}: the face reading agrees with the engine"
            );
        }
    }

    #[test]
    fn in_the_base_she_counts_nothing_and_moving_to_the_battlefield_picks_up_the_count() {
        let mut fixture = meadow(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let sprite = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
        assert_eq!(
            temporary_friendly_units_at_my_battlefield(&ctx, PIXIE),
            0,
            "the base is not a battlefield"
        );
        assert_eq!(ctx.current_might(PIXIE), 2);
        assert_eq!(
            ctx.move_unit(
                PIXIE,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Standard
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(PIXIE), 2, "no Temporary unit came along");
        assert_eq!(
            ctx.move_unit(
                sprite,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Standard
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(PIXIE), 3);
    }
}

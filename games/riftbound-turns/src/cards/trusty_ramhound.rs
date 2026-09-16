use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 1;

pub fn another_friendly_unit_here(ctx: &Ctx, card: u32) -> bool {
    let Some(here) = ctx.location(card) else {
        return false;
    };
    let seat = ctx.controller(card);
    ctx.units_at(here)
        .into_iter()
        .any(|unit| unit != card && ctx.controller(unit) == seat)
}

pub static CARD: Card = with_statics(
    unit("Trusty Ramhound", &[], &[]),
    &[Static::While(
        another_friendly_unit_here,
        &[Grant::Might(BONUS)],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, Location, Moved};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const RAMHOUND: u32 = 90;
    const FRIEND: u32 = 91;
    const THEIRS: u32 = 92;

    fn kennel(friend_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            RAMHOUND,
            fixtures::BF1,
            0,
            "Trusty Ramhound",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(FRIEND, friend_at, 0, "Friend", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIRS, fixtures::BF1, 1, "Theirs", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RAMHOUND).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_keywordless_unit_whose_while_reads_plus_one_beside_a_friend() {
        assert!(std::ptr::eq(script_of("Trusty Ramhound").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(1)])]
        ));
    }

    #[test]
    fn a_friend_here_is_worth_one_and_an_enemy_here_or_a_friend_elsewhere_is_not() {
        let mut fixture = kennel(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(
            !another_friendly_unit_here(&ctx, RAMHOUND),
            "the enemy here is not yours and the friend is in base"
        );
        assert!(statics::grants_on(&ctx, RAMHOUND).is_empty());
        assert_eq!(ctx.current_might(RAMHOUND), 2);
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                FRIEND,
                Location::Battlefield(fixtures::BF1)
            ),
            Some(Moved::Moved)
        );
        assert!(another_friendly_unit_here(&ctx, RAMHOUND));
        assert!(matches!(
            statics::grants_on(&ctx, RAMHOUND).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(RAMHOUND), 3);
        assert_eq!(
            ctx.current_might(FRIEND),
            2,
            "the bonus is the hound's alone"
        );
        ctx.recall(FRIEND, false);
        assert_eq!(ctx.current_might(RAMHOUND), 2, "the friend left");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friend_in_the_same_base_counts_and_a_stolen_friend_stops_counting() {
        let mut fixture = kennel(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.table.card_mut(RAMHOUND).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(RAMHOUND), 3, "base is a location too");
        assert!(ctx.set_controller(FRIEND, 1, THEIRS));
        assert_eq!(ctx.location(FRIEND), Some(Location::Base(1)));
        assert_eq!(
            ctx.current_might(RAMHOUND),
            2,
            "a friend taken by the opponent is theirs now and in their base"
        );
        assert!(ctx.fault.is_none());
    }
}

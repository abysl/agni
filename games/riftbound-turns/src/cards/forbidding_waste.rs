use super::prelude::{battlefield, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::statics::defends_alone;

pub const PENALTY: i16 = -2;

pub static CARD: Card = with_statics(
    battlefield("Forbidding Waste", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: defends_alone,
        grants: &[Grant::Might(PENALTY)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown, statics};
    use crate::state::{GameBlob, Mode};

    const RAIDER: u32 = 90;
    const ALLY: u32 = 91;

    fn waste() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Forbidding Waste".into();
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn defended(defender_seat: u8, defender_might: u8, raider_might: u8) -> Fixture {
        let mut fixture = waste();
        let attacker = 1 - defender_seat;
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            defender_seat,
            "Holder",
            defender_might,
        ));
        fixture.table.cards.push(fixtures::unit(
            RAIDER,
            fixtures::BF1,
            attacker,
            "Raider",
            raider_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(defender_seat));
        fixture.blob.set_contested(fixtures::BF1, Some(attacker));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            showdown::pass(ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn the_script_is_a_battlefield_whose_aura_reaches_units_here_defending_alone() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Forbidding Waste").unwrap(),
            &CARD
        ));
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(-2)],
                ..
            }]
        ));
    }

    #[test]
    fn a_lone_defender_here_of_either_seat_reads_minus_two_and_nothing_elsewhere_does() {
        let mut fixture = waste();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3, "not defending");
        assert!(ctx.mark_defender(fixtures::VI));
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(-2)]
        ));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            1,
            "seat 0's lone defender here"
        );
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "a lone defender at another battlefield is untouched"
        );
        ctx.clear_designation(fixtures::VI);
        ctx.clear_designation(fixtures::THEIR_UNIT);
        assert_eq!(
            ctx.move_unit(
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "seat 1's lone defender here reads 2 - 2"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the enemy here is not a defender"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_friendly_unit_arriving_lifts_the_penalty_and_its_leaving_restores_it() {
        let mut fixture = waste();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_defender(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 1);
        assert_eq!(
            ctx.move_unit(
                ALLY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "no longer alone");
        assert!(ctx.mark_defender(ALLY));
        assert_eq!(ctx.current_might(ALLY), 2, "neither is alone");
        ctx.recall(ALLY, false);
        assert_eq!(ctx.current_might(fixtures::VI), 1, "alone again");
    }

    #[test]
    fn a_two_might_lone_defender_drops_to_zero_and_dies_to_one_damage() {
        let mut fixture = defended(1, 2, 1);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 1 might vs defenders 0 might"));
        assert!(
            !ctx.on_board(ALLY),
            "465.2 · one damage is lethal at zero Might"
        );
        assert!(ctx.on_board(RAIDER), "a 0-Might defender deals nothing");
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "the raider conquers"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = defended(0, 2, 1);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 1 might vs defenders 0 might"));
        assert!(
            !ctx.on_board(ALLY),
            "the aura is the battlefield's, whoever defends"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
    }

    #[test]
    fn a_defender_with_company_keeps_its_might_in_the_damage_step() {
        let mut fixture = defended(1, 2, 0);
        fixture
            .table
            .cards
            .push(fixtures::unit(92, fixtures::BF1, 1, "Company", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 0 might vs defenders 3 might"));
        assert!(!ctx.on_board(RAIDER));
        assert!(ctx.on_board(ALLY));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
    }
}

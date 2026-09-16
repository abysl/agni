use super::prelude::{battlefield, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 1;

fn every_unit_here(_: &Ctx, _: u32, _: u32) -> bool {
    true
}

pub static CARD: Card = with_statics(
    battlefield("Trifarian War Camp", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: every_unit_here,
        grants: &[Grant::Might(BONUS)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown, statics};
    use crate::state::{GameBlob, Mode};

    const RAIDER: u32 = 90;
    const HOLDER: u32 = 91;

    fn camp() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Trifarian War Camp".into();
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn contested(holder_might: u8, raider_might: u8) -> Fixture {
        let mut fixture = camp();
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.table.cards.push(fixtures::unit(
            HOLDER,
            fixtures::BF1,
            0,
            "Holder",
            holder_might,
        ));
        fixture.table.cards.push(fixtures::unit(
            RAIDER,
            fixtures::BF1,
            1,
            "Raider",
            raider_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
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
    fn the_camp_is_a_battlefield_whose_only_text_is_an_aura_over_units_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Trifarian War Camp").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(1)],
                ..
            }]
        ));
    }

    #[test]
    fn a_unit_here_of_either_seat_reads_plus_one_and_nothing_elsewhere_does() {
        let mut fixture = camp();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 4, "3 + 1 for seat 0 here");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "a unit in a base is untouched"
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Standard
            ),
            Moved::Moved
        );
        assert!(ctx.mark_attacker(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "the attacker here reads 2 + 1 too"
        );
        ctx.recall(fixtures::VI, false);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "leaving the camp sheds the bonus"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_at_another_battlefield_reads_its_printed_might() {
        let mut fixture = camp();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        let ctx = fixture.ctx();
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn both_sides_of_a_combat_here_fight_one_higher_and_the_damage_step_reads_the_bonus() {
        let mut fixture = contested(2, 2);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 3 might"));
        assert!(!ctx.on_board(HOLDER), "3 damage kills a 2 + 1 unit");
        assert!(!ctx.on_board(RAIDER));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = contested(3, 1);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 2 might vs defenders 4 might"));
        assert!(
            ctx.on_board(HOLDER),
            "465.2 · 2 damage does not reach 4 Might"
        );
        assert!(!ctx.on_board(RAIDER));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
    }
}

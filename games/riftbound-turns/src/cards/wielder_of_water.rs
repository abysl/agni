use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

pub fn fights_alone(ctx: &Ctx, card: u32) -> bool {
    ctx.in_combat(card) && ctx.alone_at(card)
}

pub static CARD: Card = with_statics(
    unit("Wielder of Water", &[], &[]),
    &[Static::While(fights_alone, &[Grant::Might(BONUS)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown, statics};
    use crate::state::{GameBlob, Mode, PromptWhy};

    const WIELDER: u32 = 90;
    const ALLY: u32 = 91;
    const RAIDER: u32 = 92;

    fn afield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE && card.id != fixtures::VI);
        fixture.table.cards.push(fixtures::unit(
            WIELDER,
            fixtures::BF1,
            0,
            "Wielder of Water",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WIELDER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn raided(might: u8) -> Fixture {
        let mut fixture = afield();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", might));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        for _ in 0..8 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            if ctx.blob.why == Some(PromptWhy::Assign) {
                let first = combat::candidates(ctx)[0];
                ctx.blob.close_prompt();
                combat::choose(ctx, first).unwrap();
                continue;
            }
            showdown::pass(ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn the_script_is_a_unit_whose_while_reads_plus_two_alone_in_combat() {
        assert!(std::ptr::eq(script_of("Wielder of Water").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(2)])]
        ));
    }

    #[test]
    fn the_bonus_needs_a_combat_designation_and_no_friendly_company() {
        let mut fixture = afield();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 1));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            ctx.alone_at(WIELDER),
            "740.2.a · an enemy here is not company"
        );
        assert!(!fights_alone(&ctx, WIELDER));
        assert_eq!(ctx.current_might(WIELDER), 2, "alone but not in combat");
        assert!(ctx.mark_attacker(WIELDER));
        assert!(matches!(
            statics::grants_on(&ctx, WIELDER).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.current_might(WIELDER), 4, "attacking alone");
        assert_eq!(
            ctx.move_unit(
                ALLY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(WIELDER),
            2,
            "a friendly unit here ends alone"
        );
        ctx.recall(ALLY, false);
        assert_eq!(ctx.current_might(WIELDER), 4);
        ctx.clear_designation(WIELDER);
        assert_eq!(ctx.current_might(WIELDER), 2);
        assert!(ctx.mark_defender(WIELDER));
        assert_eq!(ctx.current_might(WIELDER), 4, "defending alone");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "the enemy here reads nothing from it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_defender_at_four_kills_a_three_might_raider_and_survives() {
        let mut fixture = raided(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 4 might"));
        assert!(!ctx.on_board(RAIDER));
        assert!(ctx.on_board(WIELDER), "3 damage is not lethal at 4 Might");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(
            ctx.current_might(WIELDER),
            2,
            "the designation goes with the combat"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_company_the_defender_is_a_plain_two_and_dies_to_the_same_raider() {
        let mut fixture = raided(3);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 3 might"));
        assert!(!ctx.on_board(RAIDER));
        assert!(
            !ctx.on_board(WIELDER) || !ctx.on_board(ALLY),
            "3 damage across a 2 and a 1 kills at least one of them"
        );
    }
}

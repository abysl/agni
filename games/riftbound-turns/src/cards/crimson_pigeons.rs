use super::prelude::{attackers_at, unit, with_statics, Location};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

pub fn attacking_with_another_unit(ctx: &Ctx, me: u32) -> bool {
    if !ctx.is_attacker(me) {
        return false;
    }
    let Some(Location::Battlefield(zone)) = ctx.location(me) else {
        return false;
    };
    attackers_at(ctx, zone)
        .into_iter()
        .any(|unit| unit != me && ctx.controller(unit) == ctx.controller(me))
}

pub static CARD: Card = with_statics(
    unit("Crimson Pigeons", &[], &[]),
    &[Static::While(
        attacking_with_another_unit,
        &[Grant::Might(BONUS)],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown, statics};
    use crate::state::{GameBlob, Mode, PromptWhy};

    const PIGEONS: u32 = 90;
    const WINGMAN: u32 = 91;
    const GUARD: u32 = 92;
    const MIGHT: u8 = 3;

    fn arena(pigeons_seat: u8, wingman: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(fixtures::unit(
            PIGEONS,
            fixtures::BF1,
            pigeons_seat,
            "Crimson Pigeons",
            MIGHT,
        ));
        if wingman {
            fixture.table.cards.push(fixtures::unit(
                WINGMAN,
                fixtures::BF1,
                pigeons_seat,
                "Wingman",
                2,
            ));
        }
        fixture.table.cards.push(fixtures::unit(
            GUARD,
            fixtures::BF1,
            1 - pigeons_seat,
            "Guard",
            6,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PIGEONS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn fight(ctx: &mut Ctx) {
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
    fn the_script_is_a_keywordless_unit_whose_while_reads_plus_two_attacking_with_another() {
        assert!(std::ptr::eq(script_of("Crimson Pigeons").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(2)])]
        ));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn attacking_beside_a_wingman_they_swing_at_five_and_the_damage_step_reads_it() {
        let mut fixture = arena(0, true);
        let mut ctx = fixture.ctx();
        assert!(!attacking_with_another_unit(&ctx, PIGEONS));
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT));
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(PIGEONS));
        assert!(attacking_with_another_unit(&ctx, PIGEONS));
        assert!(matches!(
            statics::grants_on(&ctx, PIGEONS).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT) + 2);
        assert_eq!(
            ctx.current_might(WINGMAN),
            2,
            "the wingman gets nothing from them"
        );
        assert_eq!(combat::might_sum(&ctx, &[PIGEONS, WINGMAN]), 7);
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 7 might vs defenders 6 might"));
        assert!(!ctx.on_board(GUARD), "3 + 2 + 2 is lethal at 6");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn attacking_alone_or_defending_beside_another_they_stay_at_three() {
        let mut fixture = arena(0, false);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_attacker(PIGEONS));
        assert!(!attacking_with_another_unit(&ctx, PIGEONS));
        assert!(statics::grants_on(&ctx, PIGEONS).is_empty());
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT));
        drop(ctx);
        let mut fixture = arena(1, true);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_defender(PIGEONS));
        assert!(ctx.is_defender(WINGMAN));
        assert!(!attacking_with_another_unit(&ctx, PIGEONS));
        assert_eq!(
            ctx.current_might(PIGEONS),
            i32::from(MIGHT),
            "a defender with company is not attacking with anyone"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_goes_the_moment_the_wingman_leaves_and_an_enemy_attacker_is_no_company() {
        let mut fixture = arena(0, true);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT) + 2);
        ctx.recall(WINGMAN, true);
        assert!(!attacking_with_another_unit(&ctx, PIGEONS));
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT));
        assert!(ctx.mark_attacker(GUARD));
        assert!(
            !attacking_with_another_unit(&ctx, PIGEONS),
            "the enemy across the field is no wingman"
        );
        assert_eq!(ctx.current_might(PIGEONS), i32::from(MIGHT));
    }
}

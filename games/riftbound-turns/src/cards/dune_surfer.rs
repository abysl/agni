use super::prelude::{unit, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn you_ignore_tank_assigning_here(ctx: &Ctx, me: u32, assigner: u8, zone: u16) -> bool {
    ctx.script(me)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
        && statics::in_play(ctx, me)
        && ctx.controller(me) == assigner
        && ctx.location(me) == Some(Location::Battlefield(zone))
}

pub static CARD: Card = with_statics(
    unit("Dune Surfer", &[], &[]),
    &[Static::IgnoresTank(you_ignore_tank_assigning_here)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Keyword;
    use crate::engine::combat;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown};
    use crate::state::{GameBlob, Mode, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const SURFER: u32 = 90;
    const TANK: u32 = 91;
    const PLAIN: u32 = 92;
    const THEIR_SURFER: u32 = 93;

    fn surfer(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, seat, "Dune Surfer", 3)
        }
    }

    static WALL: Card = unit("Wall", &[Keyword::Tank], &[]);
    static RUNNER: Card = unit("Sand Runner", &[], &[]);

    fn dunes(surfer_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(surfer(SURFER, surfer_at, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(TANK, fixtures::BF1, 1, "Wall", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(PLAIN, fixtures::BF1, 1, "Archer", 2));
        if surfer_at != fixtures::BF1 {
            fixture
                .table
                .cards
                .push(surfer(THEIR_SURFER, fixtures::BF1, 1));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(TANK, &WALL);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SURFER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_whole_text_is_the_assignment_static() {
        assert!(std::ptr::eq(script_of("Dune Surfer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Dune Surfer");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::IgnoresTank(_)]));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn the_static_holds_for_its_controller_assigning_at_its_own_battlefield_only() {
        let mut fixture = dunes(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(you_ignore_tank_assigning_here(
            &ctx,
            SURFER,
            0,
            fixtures::BF1
        ));
        assert_eq!(ctx.ignores_tank(0, fixtures::BF1), Some(SURFER));
        assert!(
            !you_ignore_tank_assigning_here(&ctx, SURFER, 1, fixtures::BF1),
            "the opponent assigning here still honours Tank"
        );
        assert_eq!(ctx.ignores_tank(1, fixtures::BF1), None);
        assert_eq!(ctx.ignores_tank(0, fixtures::BF2), None);
        assert!(
            !you_ignore_tank_assigning_here(&ctx, SURFER, 0, fixtures::BF2),
            "here, not elsewhere"
        );
        assert!(
            !you_ignore_tank_assigning_here(&ctx, TANK, 1, fixtures::BF1),
            "read off a Dune Surfer only"
        );
        drop(ctx);
        let mut fixture = dunes(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            !you_ignore_tank_assigning_here(&ctx, SURFER, 0, fixtures::BF1),
            "a Surfer in your base is not here"
        );
        assert!(you_ignore_tank_assigning_here(
            &ctx,
            THEIR_SURFER,
            1,
            fixtures::BF1
        ));
        assert_eq!(ctx.ignores_tank(0, fixtures::BF1), None);
        assert_eq!(
            ctx.ignores_tank(1, fixtures::BF1),
            Some(THEIR_SURFER),
            "767 · the defender's own Surfer reads for the defender alone"
        );
        drop(ctx);
        let mut fixture = dunes(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            !you_ignore_tank_assigning_here(&ctx, SURFER, 0, fixtures::BF1),
            "365.1 · not on the board, so the passive is inactive"
        );
    }

    #[test]
    fn without_the_surfer_the_wall_soaks_the_three_without_a_question_and_the_archer_lives() {
        let mut fixture = dunes(fixtures::BF1);
        fixture.scripts = fixture.scripts.clone().with_script(SURFER, &RUNNER);
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(TANK, Keyword::Tank));
        assert_eq!(
            combat::ordered(&ctx, &[TANK, PLAIN], false),
            [TANK],
            "815.1.c.2 · the tank is the only first choice"
        );
        assert_eq!(
            combat::ordered_for(&ctx, &[TANK, PLAIN], false, true),
            [TANK, PLAIN],
            "766 · an ignored Tank is inactive for the assignment"
        );
        assert_eq!(ctx.ignores_tank(0, fixtures::BF1), None);
        fight(&mut ctx);
        assert!(
            ctx.blob.showdown.is_none(),
            "every assignment had one candidate: {:?}",
            ctx.blob.why
        );
        assert!(!ctx.on_board(TANK), "three on a three-Might wall");
        assert!(ctx.on_board(PLAIN), "nothing reached the archer");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_the_surfer_here_its_controller_may_assign_the_archer_first() {
        let mut fixture = dunes(fixtures::BF1);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            combat::assigner(&ctx),
            Some((0, 3)),
            "nothing is assigned without asking"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(combat::candidates(&ctx), [TANK, PLAIN]);
        ctx.blob.close_prompt();
        combat::choose(&mut ctx, PLAIN).unwrap();
        assert!(ctx.blob.showdown.is_none(), "{:?}", ctx.blob.why);
        assert!(!ctx.on_board(PLAIN), "two of the three killed the archer");
        assert!(ctx.on_board(TANK), "the wall took the one left over");
    }
}

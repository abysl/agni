use super::prelude::{battlefield, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;

pub fn tank_before_auras(ctx: &Ctx, unit: u32) -> bool {
    ctx.script(unit)
        .is_some_and(|script| script.has_keyword(Keyword::Tank))
        || ctx.state_of(unit).is_some_and(|row| {
            row.granted
                .iter()
                .any(|(keyword, _)| keyword.same_kind(Keyword::Tank))
        })
}

pub fn a_tank(ctx: &Ctx, _temple: u32, unit: u32) -> bool {
    tank_before_auras(ctx, unit)
}

pub static CARD: Card = with_statics(
    battlefield("Kinkou Temple", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: a_tank,
        grants: &[Grant::Might(MIGHT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{grant_this_turn, Location};
    use crate::cards::script_of;
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown, statics};
    use agni_plugin_sdk::table::CardInfo;

    const TEMPLE: u32 = fixtures::GROUNDS;
    const HORNS: u32 = 90;
    const THEIR_HORNS: u32 = 91;
    const RAIDER: u32 = 92;

    fn horns(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, seat, "Horns of the Dragon", 3)
        }
    }

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TEMPLE).unwrap().name = "Kinkou Temple".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(horns(HORNS, fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TEMPLE).unwrap(),
            &CARD
        ));
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
    fn the_script_is_a_battlefield_whose_aura_reaches_tanks_here() {
        assert!(std::ptr::eq(script_of("Kinkou Temple").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(1)],
                ..
            }]
        ));
        assert_eq!(MIGHT, 1);
    }

    #[test]
    fn a_tank_here_reads_one_more_and_a_unit_without_tank_here_reads_its_own() {
        let mut fixture = temple();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(HORNS, Keyword::Tank));
        assert!(a_tank(&ctx, TEMPLE, HORNS));
        assert!(!a_tank(&ctx, TEMPLE, fixtures::VI));
        assert!(matches!(
            statics::grants_on(&ctx, HORNS).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(HORNS), 4);
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi has no Tank");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_tank_elsewhere_gets_nothing_and_arriving_here_gains_it_whoever_controls_it() {
        let mut fixture = temple();
        fixture
            .table
            .cards
            .push(horns(THEIR_HORNS, fixtures::BF2, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            a_tank(&ctx, TEMPLE, THEIR_HORNS),
            "the predicate reads Tank alone"
        );
        assert_eq!(
            ctx.current_might(THEIR_HORNS),
            3,
            "at the other battlefield, out of the aura's scope"
        );
        assert_eq!(
            ctx.move_unit(
                THEIR_HORNS,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(THEIR_HORNS),
            4,
            "the aura is the battlefield's, whoever controls the Tank"
        );
        ctx.recall(HORNS, false);
        assert_eq!(ctx.current_might(HORNS), 3, "home, the Horns read printed");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn tank_granted_for_the_turn_counts_and_the_grant_ends_with_the_turn() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(grant_this_turn(&mut ctx, fixtures::VI, Keyword::Tank));
        assert!(a_tank(&ctx, TEMPLE, fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        let turn = ctx.turn();
        ctx.expire(crate::state::Expiry::EndOfTurn(turn));
        assert!(!a_tank(&ctx, TEMPLE, fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn a_defending_tank_here_kills_a_raider_of_its_printed_might_and_lives() {
        let mut fixture = temple();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 3));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 4 might"));
        assert!(ctx.on_board(HORNS), "three damage on four Might");
        assert!(!ctx.on_board(RAIDER), "four on three is lethal");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }
}

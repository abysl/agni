use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub fn other_friendly(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source && ctx.controller(unit) == ctx.controller(source)
}

pub static CARD: Card = with_statics(
    unit("Captain Farron", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: other_friendly,
        grants: &[Grant::Keyword(Keyword::Assault(1))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown, statics};
    use crate::state::{GameBlob, Mode, PromptWhy};

    const FARRON: u32 = 90;
    const ALLY: u32 = 91;
    const HOLDER: u32 = 92;

    fn marching() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE && card.id != fixtures::VI);
        fixture.table.cards.push(fixtures::unit(
            FARRON,
            fixtures::BF2,
            0,
            "Captain Farron",
            5,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF2, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(HOLDER, fixtures::BF2, 1, "Holder", 8));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FARRON).unwrap(),
            &CARD
        ));
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
    fn the_script_is_a_unit_whose_aura_gives_assault_to_other_friendly_units_here() {
        assert!(std::ptr::eq(script_of("Captain Farron").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty(), "he has no Assault of his own");
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Keyword(Keyword::Assault(1))],
                ..
            }]
        ));
    }

    #[test]
    fn the_ally_here_has_assault_while_he_the_enemy_and_a_friend_elsewhere_do_not() {
        let mut fixture = marching();
        fixture
            .table
            .cards
            .push(fixtures::unit(93, fixtures::BASE, 0, "Homebody", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(ALLY, Keyword::Assault(1)));
        assert!(matches!(
            statics::grants_on(&ctx, ALLY).as_slice(),
            [Grant::Keyword(Keyword::Assault(1))]
        ));
        assert!(!ctx.has_keyword(FARRON, Keyword::Assault(1)), "other");
        assert!(!ctx.has_keyword(HOLDER, Keyword::Assault(1)), "friendly");
        assert!(!ctx.has_keyword(93, Keyword::Assault(1)), "here");
        assert_eq!(
            ctx.current_might(ALLY),
            2,
            "Assault is Might only for an attacker"
        );
        assert!(ctx.mark_attacker(ALLY));
        assert_eq!(ctx.current_might(ALLY), 3);
        assert!(ctx.mark_attacker(FARRON));
        assert_eq!(ctx.current_might(FARRON), 5);
        ctx.recall(FARRON, false);
        assert!(
            !ctx.has_keyword(ALLY, Keyword::Assault(1)),
            "the aura leaves with him"
        );
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_granted_assault_is_read_by_the_damage_step_and_kills_an_eight_might_holder() {
        let mut fixture = marching();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 8 might vs defenders 8 might"));
        assert!(!ctx.on_board(HOLDER), "5 + 2 + 1 is lethal at 8");
        assert!(
            !ctx.on_board(FARRON) && !ctx.on_board(ALLY),
            "the holder's 8 is lethal to a 5 and a 2 in turn: a full trade"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "nobody is left to hold it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_him_the_same_march_falls_one_short() {
        let mut fixture = marching();
        fixture.table.card_mut(FARRON).unwrap().name = "Captain Nobody".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 7 might vs defenders 8 might"));
        assert!(ctx.on_board(HOLDER), "7 damage is not lethal at 8");
        assert!(!ctx.on_board(FARRON) && !ctx.on_board(ALLY));
        assert_eq!(ctx.blob.holder(fixtures::BF2), Some(1));
    }
}

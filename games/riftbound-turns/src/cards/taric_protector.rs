use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub fn other_friendly(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source && ctx.controller(unit) == ctx.controller(source)
}

pub static CARD: Card = with_statics(
    unit(
        "Taric - Protector",
        &[Keyword::Shield(1), Keyword::Tank],
        &[],
    ),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: other_friendly,
        grants: &[Grant::Keyword(Keyword::Shield(1))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown, statics};
    use crate::state::{GameBlob, Mode, PromptWhy};

    const TARIC: u32 = 90;
    const ALLY: u32 = 91;
    const RAIDER: u32 = 92;

    fn holding() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE && card.id != fixtures::VI);
        fixture.table.cards.push(fixtures::unit(
            TARIC,
            fixtures::BF1,
            0,
            "Taric - Protector",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TARIC).unwrap(), &CARD));
        fixture
    }

    fn raided(might: u8) -> Fixture {
        let mut fixture = holding();
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
    fn the_script_prints_shield_and_tank_and_its_aura_gives_shield_to_other_friendly_units_here() {
        assert!(std::ptr::eq(script_of("Taric - Protector").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Shield(1), Keyword::Tank]);
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Keyword(Keyword::Shield(1))],
                ..
            }]
        ));
    }

    #[test]
    fn the_ally_here_defends_at_plus_one_and_taric_keeps_his_own_shield_only() {
        let mut fixture = holding();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(ALLY, Keyword::Shield(1)));
        assert!(matches!(
            statics::grants_on(&ctx, ALLY).as_slice(),
            [Grant::Keyword(Keyword::Shield(1))]
        ));
        assert!(
            statics::grants_on(&ctx, TARIC).is_empty(),
            "other · his own Shield is printed, not projected"
        );
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Shield(1)));
        assert!(ctx.has_keyword(TARIC, Keyword::Tank));
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx.mark_defender(ALLY));
        assert_eq!(ctx.current_might(ALLY), 3, "Shield while a defender");
        assert!(ctx.mark_defender(TARIC));
        assert_eq!(ctx.current_might(TARIC), 5, "one Shield, not two");
        ctx.clear_designation(ALLY);
        assert!(ctx.mark_attacker(ALLY));
        assert_eq!(
            ctx.current_might(ALLY),
            2,
            "Shield is nothing to an attacker"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_granted_shield_is_read_by_the_damage_step() {
        let mut fixture = raided(7);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 7 might vs defenders 8 might"));
        assert!(!ctx.on_board(RAIDER), "4 + 1 + 2 + 1 is lethal at 7");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_at_another_battlefield_gets_nothing_from_him() {
        let mut fixture = holding();
        fixture.table.card_mut(ALLY).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(ALLY, Keyword::Shield(1)));
        assert!(ctx.mark_defender(ALLY));
        assert_eq!(ctx.current_might(ALLY), 2);
    }
}

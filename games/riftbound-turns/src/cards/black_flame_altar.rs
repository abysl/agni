use super::prelude::{battlefield, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;
use crate::rules::COUNTER_TEMPORARY;
use agni_plugin_sdk::table::Target;

pub const SHIELD: u8 = 1;

pub fn temporary_before_auras(ctx: &Ctx, unit: u32) -> bool {
    ctx.table
        .counter(Target::Card(unit), COUNTER_TEMPORARY)
        .unwrap_or(0)
        > 0
        || ctx
            .script(unit)
            .is_some_and(|script| script.has_keyword(Keyword::Temporary))
        || ctx.state_of(unit).is_some_and(|row| {
            row.granted
                .iter()
                .any(|(keyword, _)| keyword.same_kind(Keyword::Temporary))
        })
}

pub fn a_temporary_unit(ctx: &Ctx, _altar: u32, unit: u32) -> bool {
    temporary_before_auras(ctx, unit)
}

pub static CARD: Card = with_statics(
    battlefield("Black Flame Altar", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: a_temporary_unit,
        grants: &[Grant::Keyword(Keyword::Shield(SHIELD))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{grant_this_turn, Location, Token};
    use crate::cards::script_of;
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown, statics};

    const ALTAR: u32 = fixtures::GROUNDS;
    const RAIDER: u32 = 90;

    fn altar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ALTAR).unwrap().name = "Black Flame Altar".into();
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ALTAR).unwrap(), &CARD));
        fixture
    }

    fn with_a_sprite_here() -> Fixture {
        let mut fixture = altar();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
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
    fn the_script_is_a_battlefield_whose_aura_shields_temporary_units_here() {
        assert!(std::ptr::eq(script_of("Black Flame Altar").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Keyword(Keyword::Shield(1))],
                ..
            }]
        ));
        assert_eq!(SHIELD, 1);
    }

    #[test]
    fn a_temporary_unit_here_has_shield_and_reads_one_more_while_it_defends() {
        let mut fixture = with_a_sprite_here();
        let mut ctx = fixture.ctx();
        assert!(ctx.is_temporary(fixtures::SPRITE));
        assert!(a_temporary_unit(&ctx, ALTAR, fixtures::SPRITE));
        assert!(ctx.has_keyword(fixtures::SPRITE, Keyword::Shield(1)));
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::SPRITE).as_slice(),
            [Grant::Keyword(Keyword::Shield(1))]
        ));
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3, "not defending");
        assert!(ctx.mark_defender(fixtures::SPRITE));
        assert_eq!(ctx.current_might(fixtures::SPRITE), 4);
        ctx.clear_designation(fixtures::SPRITE);
        assert!(ctx.mark_attacker(fixtures::SPRITE));
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "Shield counts for defenders only"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lasting_unit_here_and_a_temporary_unit_elsewhere_get_nothing() {
        let mut fixture = altar();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!a_temporary_unit(&ctx, ALTAR, fixtures::VI));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Shield(1)));
        assert!(ctx.mark_defender(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi is not Temporary");
        assert!(
            a_temporary_unit(&ctx, ALTAR, fixtures::SPRITE),
            "the predicate reads Temporary alone"
        );
        assert!(
            !ctx.has_keyword(fixtures::SPRITE, Keyword::Shield(1)),
            "the Sprite is at the other battlefield, out of the aura's scope"
        );
        assert!(ctx.mark_defender(fixtures::SPRITE));
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        ctx.clear_designation(fixtures::SPRITE);
        assert_eq!(
            ctx.move_unit(
                fixtures::SPRITE,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(ctx.has_keyword(fixtures::SPRITE, Keyword::Shield(1)));
        assert!(ctx.mark_defender(fixtures::SPRITE));
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            4,
            "here, it is shielded"
        );
    }

    #[test]
    fn temporary_granted_for_the_turn_and_a_spawned_sprite_both_count() {
        let mut fixture = altar();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(grant_this_turn(&mut ctx, fixtures::VI, Keyword::Temporary));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Shield(1)));
        let spawned = crate::cards::prelude::spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert!(ctx.is_temporary(spawned));
        assert!(ctx.has_keyword(spawned, Keyword::Shield(1)));
        assert!(ctx.mark_defender(spawned));
        assert_eq!(ctx.current_might(spawned), 4);
    }

    #[test]
    fn a_defending_sprite_here_survives_a_three_might_raid_on_its_shield() {
        let mut fixture = with_a_sprite_here();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 0, "Raider", 3));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 4 might"));
        assert!(ctx.on_board(fixtures::SPRITE), "three damage on four Might");
        assert!(!ctx.on_board(RAIDER), "four on three is lethal");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert!(ctx.fault.is_none());
    }
}

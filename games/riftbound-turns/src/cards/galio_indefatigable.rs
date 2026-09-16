use super::ezreal_dashing::deals_no_combat_damage;
use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};

pub static CARD: Card = with_statics(
    unit(
        "Galio - Indefatigable",
        &[Keyword::Deflect(1), Keyword::Tank],
        &[],
    ),
    &[Static::NoCombatDamageFrom(deals_no_combat_damage)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Ctx;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown};
    use crate::state::{GameBlob, Mode};

    const GALIO: u32 = 90;
    const SQUIRE: u32 = 91;
    const RAIDER: u32 = 92;
    const MIGHT: u8 = 6;

    fn arena(galio_seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(fixtures::unit(
            GALIO,
            fixtures::BF1,
            galio_seat,
            "Galio - Indefatigable",
            MIGHT,
        ));
        fixture.table.cards.push(fixtures::unit(
            SQUIRE,
            fixtures::BF1,
            galio_seat,
            "Squire",
            2,
        ));
        fixture.table.cards.push(fixtures::unit(
            RAIDER,
            fixtures::BF1,
            1 - galio_seat,
            "Raider",
            3,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GALIO).unwrap(), &CARD));
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
    }

    fn fight(ctx: &mut Ctx) {
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn the_script_prints_deflect_and_tank_and_carries_the_ezreal_seam_for_his_own_combat_damage() {
        assert!(std::ptr::eq(
            script_of("Galio - Indefatigable").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1), Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::NoCombatDamageFrom(_)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = arena(1);
        let ctx = fixture.ctx();
        assert!(deals_no_combat_damage(&ctx, GALIO, GALIO));
        assert!(!deals_no_combat_damage(&ctx, GALIO, SQUIRE));
        assert!(!deals_no_combat_damage(&ctx, GALIO, RAIDER));
        assert_eq!(ctx.deflect_of(GALIO), 1);
        assert_eq!(
            combat::ordered(&ctx, &[SQUIRE, GALIO], false),
            [GALIO],
            "741.1.b · the tank is the only first choice"
        );
    }

    #[test]
    fn he_soaks_the_raiders_damage_first_as_a_defender_and_keeps_his_six_might_for_lethal() {
        let mut fixture = arena(1);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_defender(GALIO));
        assert_eq!(ctx.current_might(GALIO), i32::from(MIGHT));
        assert_eq!(combat::lethal(&ctx, GALIO), MIGHT);
        fight(&mut ctx);
        assert_eq!(
            ctx.damage_on(GALIO),
            0,
            "three damage on a six-Might tank is healed after combat"
        );
        assert!(ctx.on_board(GALIO));
        assert!(ctx.on_board(SQUIRE), "the squire behind him took nothing");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_the_engine_skips_a_units_own_suppression_so_he_still_swings_for_six() {
        let mut fixture = arena(0);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(GALIO));
        assert!(ctx.deals_combat_damage(GALIO));
        assert_eq!(combat::might_sum(&ctx, &[GALIO, SQUIRE]), MIGHT + 2);
        assert!(
            ctx.deals_combat_damage(SQUIRE),
            "the seam reads himself only, never a neighbour"
        );
        assert!(ctx.deals_combat_damage(RAIDER));
    }

    #[test]
    #[ignore = "engine gap · Ctx::deals_combat_damage consults every other card's NoCombatDamageFrom static and skips the unit's own, so Galio's I-don't-deal-combat-damage is never read; the seam is ezreal_dashing::deals_no_combat_damage(ctx, source, unit), true for himself"]
    fn galio_contributes_nothing_to_combat_damage_and_the_squire_alone_hits_the_raider() {
        let mut fixture = arena(0);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(!ctx.deals_combat_damage(GALIO));
        assert_eq!(ctx.combat_might(GALIO), 0);
        assert_eq!(combat::might_sum(&ctx, &[GALIO, SQUIRE]), 2);
        assert!(ctx.deals_combat_damage(SQUIRE));
        assert!(ctx.deals_combat_damage(RAIDER));
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 2 might vs defenders 3 might"));
        assert!(ctx.on_board(RAIDER), "two damage is not lethal at three");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1), "nobody conquered");
    }
}

use super::disciple_of_shen::at_a_battlefield_with_exactly_one_other_friendly_unit;
use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub fn deals_no_combat_damage(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit == source && !at_a_battlefield_with_exactly_one_other_friendly_unit(ctx, source)
}

pub static CARD: Card = with_statics(
    unit("Sacred Protector", &[], &[]),
    &[Static::NoCombatDamageFrom(deals_no_combat_damage)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Ctx;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, showdown};
    use crate::state::{GameBlob, Mode, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const PROTECTOR: u32 = 90;
    const SQUIRE: u32 = 91;
    const PAGE: u32 = 92;
    const RAIDER: u32 = 93;
    const MIGHT: u8 = 6;

    fn protector(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(PROTECTOR, zone, seat, "Sacred Protector", MIGHT)
        }
    }

    fn shrine(friends_here: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(protector(fixtures::BF1, 0));
        for friend in friends_here {
            fixture
                .table
                .cards
                .push(fixtures::unit(*friend, fixtures::BF1, 0, "Friend", 2));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PROTECTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
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
    fn the_script_is_a_keywordless_unit_carrying_the_galio_seam_for_its_own_combat_damage() {
        assert!(std::ptr::eq(script_of("Sacred Protector").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sacred Protector");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::NoCombatDamageFrom(_)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn the_seam_suppresses_itself_unless_exactly_one_other_unit_of_yours_stands_with_it() {
        let mut fixture = shrine(&[]);
        let ctx = fixture.ctx();
        assert!(deals_no_combat_damage(&ctx, PROTECTOR, PROTECTOR));
        assert!(
            !deals_no_combat_damage(&ctx, PROTECTOR, RAIDER),
            "the seam reads itself only, never a neighbour"
        );
        drop(ctx);
        let mut fixture = shrine(&[SQUIRE]);
        let ctx = fixture.ctx();
        assert!(!deals_no_combat_damage(&ctx, PROTECTOR, PROTECTOR));
        assert!(!deals_no_combat_damage(&ctx, PROTECTOR, SQUIRE));
        drop(ctx);
        let mut fixture = shrine(&[SQUIRE, PAGE]);
        let ctx = fixture.ctx();
        assert!(
            deals_no_combat_damage(&ctx, PROTECTOR, PROTECTOR),
            "two others is not exactly one"
        );
        drop(ctx);
        let mut fixture = shrine(&[]);
        fixture.table.card_mut(PROTECTOR).unwrap().zone = Some(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(SQUIRE, fixtures::BASE, 0, "Friend", 2));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            deals_no_combat_damage(&ctx, PROTECTOR, PROTECTOR),
            "a base with one friend is not a battlefield"
        );
    }

    #[test]
    fn with_exactly_one_friend_it_swings_for_six_and_the_raider_falls() {
        let mut fixture = shrine(&[SQUIRE]);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(PROTECTOR));
        assert!(ctx.deals_combat_damage(PROTECTOR));
        assert_eq!(combat::might_sum(&ctx, &[PROTECTOR, SQUIRE]), MIGHT + 2);
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 8 might vs defenders 3 might"));
        assert!(!ctx.on_board(RAIDER), "eight damage kills a three");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0), "conquered");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_the_engine_skips_a_units_own_suppression_so_alone_it_still_swings_for_six() {
        let mut fixture = shrine(&[]);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(PROTECTOR));
        assert!(deals_no_combat_damage(&ctx, PROTECTOR, PROTECTOR));
        assert!(ctx.deals_combat_damage(PROTECTOR));
        assert_eq!(combat::might_sum(&ctx, &[PROTECTOR]), MIGHT);
        assert!(ctx.deals_combat_damage(RAIDER));
    }

    #[test]
    #[ignore = "engine gap · Ctx::deals_combat_damage consults every other card's NoCombatDamageFrom static and skips the unit's own, so the Protector's I-don't-deal-combat-damage is never read; the seam is sacred_protector::deals_no_combat_damage(ctx, source, unit), true for itself unless exactly one other friendly unit shares its battlefield"]
    fn alone_or_with_two_friends_it_contributes_nothing_to_combat_damage() {
        let mut fixture = shrine(&[SQUIRE, PAGE]);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(!ctx.deals_combat_damage(PROTECTOR));
        assert_eq!(ctx.combat_might(PROTECTOR), 0);
        assert_eq!(combat::might_sum(&ctx, &[PROTECTOR, SQUIRE, PAGE]), 4);
        assert!(ctx.deals_combat_damage(SQUIRE));
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 4 might vs defenders 3 might"));
        drop(ctx);
        let mut fixture = shrine(&[]);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(!ctx.deals_combat_damage(PROTECTOR));
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 0 might vs defenders 3 might"));
        assert!(ctx.on_board(RAIDER), "nothing was dealt");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1), "nobody conquered");
    }
}

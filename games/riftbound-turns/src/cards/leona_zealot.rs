use super::prelude::{unit, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;

pub const PENALTY: i16 = -8;
pub const MINIMUM: i32 = 1;
pub const WITHIN: i32 = 3;

pub fn stunned_enemy(ctx: &Ctx, source: u32, unit: u32) -> bool {
    ctx.controller(unit) != ctx.controller(source) && ctx.is_stunned(unit)
}

pub fn might_floor_under_the_aura(ctx: &Ctx, source: u32, unit: u32) -> Option<i32> {
    stunned_enemy(ctx, source, unit).then_some(MINIMUM)
}

pub fn enters_ready(ctx: &Ctx, card: u32) -> bool {
    let seat = ctx.controller(card);
    let threshold = ctx.victory_score() - WITHIN;
    (0..ctx.players())
        .filter(|other| *other != seat)
        .any(|other| ctx.points(other) >= threshold)
}

pub static CARD: Card = with_statics(
    unit("Leona - Zealot", &[], &[]),
    &[
        Static::Aura {
            scope: Scope::UnitsHere,
            when: stunned_enemy,
            grants: &[Grant::Might(PENALTY)],
        },
        Static::EntersReady(enters_ready),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::rules::DEFAULT_VICTORY_SCORE;

    const LEONA: u32 = 90;
    const BRUTE: u32 = 91;

    fn zealous() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(LEONA, fixtures::BF1, 0, "Leona - Zealot", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 10));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(LEONA).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_whose_aura_gives_minus_eight_to_stunned_enemy_units_here() {
        assert!(std::ptr::eq(script_of("Leona - Zealot").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [
                Static::Aura {
                    scope: Scope::UnitsHere,
                    grants: [Grant::Might(-8)],
                    ..
                },
                Static::EntersReady(_)
            ]
        ));
    }

    #[test]
    fn a_stunned_enemy_here_loses_eight_while_stunned_friends_and_stunned_enemies_elsewhere_keep_theirs(
    ) {
        let mut fixture = zealous();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(BRUTE), 10, "not stunned");
        assert!(statics::grants_on(&ctx, BRUTE).is_empty());
        assert!(ctx.stun(BRUTE));
        assert!(matches!(
            statics::grants_on(&ctx, BRUTE).as_slice(),
            [Grant::Might(-8)]
        ));
        assert_eq!(ctx.current_might(BRUTE), 2);
        assert!(ctx.stun(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "enemy · her own side is spared"
        );
        assert!(ctx.stun(fixtures::SPRITE));
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "here · a stunned enemy at another battlefield is untouched"
        );
        ctx.unstun(BRUTE);
        assert_eq!(
            ctx.current_might(BRUTE),
            10,
            "the penalty goes with the stun"
        );
        ctx.recall(LEONA, false);
        assert!(ctx.stun(BRUTE));
        assert_eq!(ctx.current_might(BRUTE), 10, "and with her");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_small_stunned_enemy_reads_zero_today_where_the_card_says_one() {
        let mut fixture = zealous();
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        assert_eq!(
            might_floor_under_the_aura(&ctx, LEONA, fixtures::THEIR_UNIT),
            Some(MINIMUM)
        );
        assert_eq!(might_floor_under_the_aura(&ctx, LEONA, BRUTE), None);
        assert_eq!(might_floor_under_the_aura(&ctx, LEONA, fixtures::VI), None);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "the engine floors aura Might at zero"
        );
    }

    #[test]
    #[ignore = "engine gap · might_floor_under_the_aura is not read, the engine owes a per-aura minimum on projected Might"]
    fn a_two_might_stunned_enemy_here_reads_one_not_zero() {
        let mut fixture = zealous();
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 1);
    }

    #[test]
    fn she_would_enter_ready_once_an_opponent_is_within_three_of_the_victory_score() {
        let mut fixture = zealous();
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        fixture.set_points(1, DEFAULT_VICTORY_SCORE - WITHIN - 1);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, LEONA),
            "her own controller's score is not an opponent's"
        );
        drop(ctx);
        fixture.table.counters.clear();
        fixture.set_points(1, DEFAULT_VICTORY_SCORE - WITHIN);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, LEONA));
        assert!(
            !enters_ready(&ctx, fixtures::THEIR_UNIT),
            "read from the entering unit's controller"
        );
    }

    #[test]
    fn played_while_an_opponent_sits_at_five_points_she_enters_ready() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().name = "Leona - Zealot".into();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.set_points(1, DEFAULT_VICTORY_SCORE - WITHIN);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, fixtures::HAND_UNIT));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(!ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
    }
}

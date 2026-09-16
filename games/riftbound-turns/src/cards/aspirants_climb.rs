use super::prelude::{battlefield, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const EXTRA_POINT: i32 = 1;

pub static CARD: Card = with_statics(
    battlefield("Aspirant's Climb", &[], &[]),
    &[Static::VictoryScore(EXTRA_POINT)],
);

pub fn is_climb(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn victory_score_bonus(ctx: &Ctx, climb: u32) -> i32 {
    if is_climb(ctx, climb) && statics::in_play(ctx, climb) {
        EXTRA_POINT
    } else {
        0
    }
}

pub fn victory_score(ctx: &Ctx) -> i32 {
    ctx.victory_score()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, phases, priority, settle};
    use crate::rules::{COUNTER_POINTS, DEFAULT_VICTORY_SCORE};
    use crate::state::{GameBlob, Mode};

    const CLIMB: u32 = fixtures::GROUNDS;

    fn climb() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(CLIMB).unwrap().name = "Aspirant's Climb".into();
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(CLIMB).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_climb_is_a_battlefield_with_no_abilities_whose_text_is_a_victory_score_static() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Aspirant's Climb").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::VictoryScore(EXTRA_POINT)]));
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn the_climb_in_play_raises_the_victory_score_by_one_and_no_other_card_does() {
        let mut fixture = climb();
        let ctx = fixture.ctx();
        assert!(is_climb(&ctx, CLIMB));
        assert!(!is_climb(&ctx, fixtures::ROCKFALL));
        assert_eq!(victory_score_bonus(&ctx, CLIMB), EXTRA_POINT);
        assert_eq!(victory_score_bonus(&ctx, fixtures::ROCKFALL), 0);
        assert_eq!(victory_score_bonus(&ctx, fixtures::VI), 0);
        assert_eq!(victory_score(&ctx), DEFAULT_VICTORY_SCORE + EXTRA_POINT);
        drop(fixture);
        let mut plain = Fixture::enforced();
        let ctx = plain.ctx();
        assert_eq!(
            victory_score_bonus(&ctx, CLIMB),
            0,
            "Proving Grounds is not the Climb"
        );
        assert_eq!(victory_score(&ctx), DEFAULT_VICTORY_SCORE);
    }

    #[test]
    fn two_climbs_raise_the_score_by_two_and_a_table_option_is_the_base() {
        let mut fixture = climb();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Aspirant's Climb".into();
        fixture
            .table
            .options
            .push((crate::rules::OPTION_VICTORY_SCORE.into(), 6));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ctx.options.victory_score, 6);
        assert_eq!(victory_score(&ctx), 6 + 2 * EXTRA_POINT);
    }

    #[test]
    fn a_seat_one_short_of_the_default_score_still_scores_the_final_point_by_hold() {
        let mut fixture = climb();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = 3;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        for _ in 0..4 {
            let Some(holder) = priority::holder(&ctx) else {
                break;
            };
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert_eq!(ctx.points(0), DEFAULT_VICTORY_SCORE - 1);
        assert_eq!(ctx.winner(), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn reaching_the_default_score_is_not_a_win_while_the_climb_is_on_the_table() {
        let mut fixture = climb();
        fixture.set_points(0, DEFAULT_VICTORY_SCORE);
        assert_eq!(
            fixture
                .table
                .counter_bounds(COUNTER_POINTS)
                .and_then(|bounds| bounds.max),
            None,
            "the host declares no cap on points"
        );
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.points_winner(), None);
        assert_eq!(ctx.winner(), None);
        assert_eq!(cleanup::win_check(&mut ctx), None);
        assert_eq!(ctx.won, None);
        assert!(
            !ctx.score(0, false),
            "one short of nine: a conquer draws instead of the final point"
        );
        assert!(ctx.score(0, true), "a hold scores it");
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), DEFAULT_VICTORY_SCORE + EXTRA_POINT);
        assert_eq!(ctx.points_winner(), Some(0));
        assert_eq!(cleanup::win_check(&mut ctx), Some(0));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} wins with {} points",
            DEFAULT_VICTORY_SCORE + EXTRA_POINT
        )));
    }
}

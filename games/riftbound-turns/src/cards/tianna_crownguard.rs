use super::prelude::{at_battlefield, unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub fn holds_the_line(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| ctx.face_in_play(held) && !held.is_hidden())
        && !ctx.is_pending_play(card)
        && !ctx.is_facedown(card)
        && at_battlefield(ctx, card)
}

pub fn opponents_cannot_score_points(ctx: &Ctx, seat: u8) -> bool {
    ctx.score_veto(seat).is_some_and(|guard| {
        ctx.script(guard)
            .is_some_and(|script| std::ptr::eq(script, &CARD))
    })
}

pub static CARD: Card = with_statics(
    unit("Tianna Crownguard", &[Keyword::Deflect(1)], &[]),
    &[Static::OpponentsCannotScore(holds_the_line)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, score_point, Location, Moved};
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};

    const TIANNA: u32 = 90;
    const THEIR_TIANNA: u32 = 91;
    const RAIDER: u32 = 92;

    fn crownguard(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture
            .table
            .cards
            .push(fixtures::unit(TIANNA, at, 0, "Tianna Crownguard", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 3));
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TIANNA).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_deflect_unit_whose_veto_is_a_static() {
        assert!(std::ptr::eq(script_of("Tianna Crownguard").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::OpponentsCannotScore(applies)] if std::ptr::fn_addr_eq(*applies, holds_the_line as fn(&Ctx, u32) -> bool)
        ));
    }

    #[test]
    fn she_vetoes_the_opponents_points_at_a_battlefield_and_never_from_base_or_for_her_own_side() {
        let mut fixture = crownguard(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(!holds_the_line(&ctx, TIANNA));
        assert!(!opponents_cannot_score_points(&ctx, 1));
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                TIANNA,
                Location::Battlefield(fixtures::BF1)
            ),
            Some(Moved::Moved)
        );
        assert!(holds_the_line(&ctx, TIANNA));
        assert!(opponents_cannot_score_points(&ctx, 1));
        assert!(
            !opponents_cannot_score_points(&ctx, 0),
            "her own controller scores as ever"
        );
        assert!(ctx.stun(TIANNA));
        assert!(
            opponents_cannot_score_points(&ctx, 1),
            "a stunned Tianna still stands at the battlefield"
        );
        ctx.recall(TIANNA, true);
        assert!(!opponents_cannot_score_points(&ctx, 1));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_tianna_in_hand_or_taken_by_the_opponent_reads_by_her_controller_not_her_owner() {
        let mut fixture = crownguard(fixtures::BF1);
        fixture.table.cards.push(fixtures::unit(
            THEIR_TIANNA,
            fixtures::HAND,
            1,
            "Tianna Crownguard",
            4,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(opponents_cannot_score_points(&ctx, 1));
        assert!(!opponents_cannot_score_points(&ctx, 0), "theirs is in hand");
        assert!(ctx.set_controller(TIANNA, 1, RAIDER));
        assert!(
            !opponents_cannot_score_points(&ctx, 1),
            "she is theirs now and went to their base"
        );
        assert!(
            !opponents_cannot_score_points(&ctx, 0),
            "and their base is not a battlefield"
        );
    }

    #[test]
    fn while_she_stands_at_a_battlefield_the_opponents_conquer_scores_nothing() {
        let mut fixture = crownguard(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(opponents_cannot_score_points(&ctx, 1));
        let hand = ctx.hand_of(1).len();
        assert!(!cleanup::conquer(&mut ctx, fixtures::BF2, 1));
        assert_eq!(ctx.points(1), 0);
        assert_eq!(ctx.hand_of(1).len(), hand, "no draw either");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} cannot score · {{card {TIANNA}}}")));
        ctx.blob.set_holder(fixtures::BF2, Some(1));
        assert!(!cleanup::hold(&mut ctx, fixtures::BF2, 1));
        assert_eq!(ctx.points(1), 0);
        assert!(
            cleanup::conquer(&mut ctx, fixtures::BF1, 0),
            "her side scores"
        );
        assert_eq!(ctx.points(0), 1);
        ctx.recall(TIANNA, false);
        ctx.blob.clear_scored();
        assert!(cleanup::conquer(&mut ctx, fixtures::BF2, 1));
        assert_eq!(ctx.points(1), 1);
    }

    #[test]
    fn a_point_gained_from_an_effect_is_not_a_score_and_passes_her() {
        let mut fixture = crownguard(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(opponents_cannot_score_points(&ctx, 1));
        assert!(!cleanup::conquer(&mut ctx, fixtures::BF2, 1));
        assert_eq!(ctx.points(1), 0);
        score_point(&mut ctx, 1);
        assert_eq!(
            ctx.points(1),
            1,
            "468: an effect point is a gain, not a score"
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 1} scores 1 point");
        score_point(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }
}

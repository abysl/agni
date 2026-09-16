use super::ol_poro::turn_number_of;
use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const EARLY_TURNS: u16 = 2;
pub const DRAWS: usize = 1;

pub fn is_an_early_turn_of(ctx: &Ctx, seat: u8) -> bool {
    turn_number_of(ctx, seat).is_some_and(|turn| turn <= EARLY_TURNS)
}

pub fn an_otterpus_is_in_play(ctx: &Ctx) -> bool {
    let mut otters: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| statics::in_play(ctx, held.id))
        .map(|held| held.id)
        .collect();
    otters.sort_unstable();
    !otters.is_empty()
}

pub fn draws_instead_of_scoring(ctx: &Ctx, seat: u8) -> bool {
    an_otterpus_is_in_play(ctx) && is_an_early_turn_of(ctx, seat)
}

pub fn an_early_turn_of_the_scorer(ctx: &Ctx, _otterpus: u32, seat: u8) -> bool {
    is_an_early_turn_of(ctx, seat)
}

pub static CARD: Card = with_statics(
    unit("Otterpus", &[], &[]),
    &[Static::DrawInsteadOfScoring(an_early_turn_of_the_scorer)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::ctx::Scored;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{GameBlob, Mode, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const OTTERPUS: u32 = 90;

    fn otterpus(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Mind".into()],
            ..fixtures::unit(OTTERPUS, zone, seat, "Otterpus", 2)
        }
    }

    fn pond(zone: u16, seat: u8, turn: u16, player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture.table.cards.push(otterpus(zone, seat));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(OTTERPUS).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_scoring_replacement_is_a_static() {
        assert!(std::ptr::eq(script_of("Otterpus").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::DrawInsteadOfScoring(_)]));
        assert!(CARD.replacement.is_none());
        assert_eq!((EARLY_TURNS, DRAWS), (2, 1));
    }

    #[test]
    fn the_replacement_reads_the_scorers_first_two_turns_while_any_otterpus_is_on_the_board() {
        for (turn, player, early) in [(1, 0, true), (2, 1, true), (3, 0, true), (4, 1, true)] {
            let mut fixture = pond(fixtures::BASE, 1, turn, player);
            let ctx = fixture.ctx();
            assert!(
                an_otterpus_is_in_play(&ctx),
                "the opponent's Otterpus counts"
            );
            assert_eq!(is_an_early_turn_of(&ctx, player), early);
            assert!(draws_instead_of_scoring(&ctx, player));
            assert!(
                !draws_instead_of_scoring(&ctx, 1 - player),
                "the other seat is not taking a turn"
            );
        }
        for (turn, player) in [(5, 0), (6, 1), (9, 0)] {
            let mut fixture = pond(fixtures::BF1, 0, turn, player);
            let ctx = fixture.ctx();
            assert!(!is_an_early_turn_of(&ctx, player));
            assert!(!draws_instead_of_scoring(&ctx, player));
        }
        let mut pocketed = pond(fixtures::HAND, 0, 1, 0);
        let ctx = pocketed.ctx();
        assert!(!an_otterpus_is_in_play(&ctx));
        assert!(!draws_instead_of_scoring(&ctx, 0));
        drop(ctx);
        let mut trashed = pond(fixtures::TRASH, 0, 1, 0);
        let ctx = trashed.ctx();
        assert!(!an_otterpus_is_in_play(&ctx));
    }

    #[test]
    fn the_score_draws_one_on_an_early_turn_and_scores_as_ever_later() {
        let mut fixture = pond(fixtures::BASE, 0, 1, 0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.draws_instead_of_scoring(0), Some(OTTERPUS));
        assert_eq!(ctx.score_point(0, false), Scored::Drew { by: OTTERPUS });
        assert_eq!(ctx.points(0), 0, "no point");
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} draws 1 instead of scoring · {{card {OTTERPUS}}}"
        )));
        assert!(!ctx.score(0, true), "a hold draws too");
        assert_eq!(ctx.points(0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 2 * DRAWS);
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut later = pond(fixtures::BASE, 0, 5, 0);
        let mut ctx = later.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.draws_instead_of_scoring(0), None);
        assert!(ctx.score(0, false));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "the third turn scores as ever");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn on_a_players_first_two_turns_a_conquer_or_hold_draws_one_instead_of_the_point() {
        let mut fixture = pond(fixtures::BASE, 0, 1, 0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(!cleanup::conquer(&mut ctx, fixtures::BF1, 0));
        assert_eq!(ctx.points(0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        ctx.blob.set_holder(fixtures::BF2, Some(0));
        assert!(!cleanup::hold(&mut ctx, fixtures::BF2, 0));
        assert_eq!(ctx.points(0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 2 * DRAWS);
        drop(ctx);
        let mut later = pond(fixtures::BASE, 0, 5, 0);
        let mut ctx = later.ctx();
        assert!(cleanup::conquer(&mut ctx, fixtures::BF1, 0));
        assert_eq!(ctx.points(0), 1);
    }
}

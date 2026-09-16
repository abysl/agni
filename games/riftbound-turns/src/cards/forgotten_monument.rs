use super::prelude::{battlefield, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const THIRD_TURN: u16 = 3;

pub static CARD: Card = with_statics(
    battlefield("Forgotten Monument", &[], &[]),
    &[Static::NoScoreHere(scoring_vetoed_at)],
);

pub fn is_monument(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn turns_taken(ctx: &Ctx, seat: u8) -> u16 {
    let players = ctx.players().max(1);
    if seat >= players {
        return 0;
    }
    let first = ctx.blob.core().map(|core| core.first).unwrap_or(0) % players;
    let offset = u16::from((seat + players - first) % players);
    let turn = ctx.turn();
    if turn <= offset {
        return 0;
    }
    (turn - offset - 1) / u16::from(players) + 1
}

pub fn may_score_here(ctx: &Ctx, monument: u32, seat: u8) -> bool {
    !(is_monument(ctx, monument) && statics::in_play(ctx, monument))
        || turns_taken(ctx, seat) >= THIRD_TURN
}

pub fn scoring_vetoed_at(ctx: &Ctx, zone: u16, seat: u8) -> bool {
    ctx.table
        .cards
        .iter()
        .filter(|held| ctx.location(held.id) == Some(Location::Battlefield(zone)))
        .any(|held| ctx.is_battlefield_card(held.id) && !may_score_here(ctx, held.id, seat))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{GameBlob, Mode};

    const MONUMENT: u32 = fixtures::GROUNDS;

    fn monument(turn: u16, player: u8, first: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, first, Mode::Enforced);
        fixture.blob.set_phase(crate::state::Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        let core = fixture.blob.core_mut().unwrap();
        core.turn = turn;
        core.player = player;
        fixture.table.card_mut(MONUMENT).unwrap().name = "Forgotten Monument".into();
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MONUMENT).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_monument_is_a_battlefield_with_no_abilities_whose_text_is_a_score_veto() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Forgotten Monument").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::NoScoreHere(_)]));
        assert!(CARD.replacement.is_none());
        assert_eq!(THIRD_TURN, 3);
    }

    #[test]
    fn a_seats_turns_are_counted_from_the_first_player_including_the_turn_under_way() {
        for (turn, mine, theirs) in [
            (1, 1, 0),
            (2, 1, 1),
            (3, 2, 1),
            (4, 2, 2),
            (5, 3, 2),
            (6, 3, 3),
        ] {
            let mut fixture = monument(turn, u8::from(turn % 2 == 0), 0);
            let ctx = fixture.ctx();
            assert_eq!(turns_taken(&ctx, 0), mine, "seat 0 on turn {turn}");
            assert_eq!(turns_taken(&ctx, 1), theirs, "seat 1 on turn {turn}");
            assert_eq!(turns_taken(&ctx, 2), 0, "no such seat");
        }
        let mut second_goes_first = monument(5, 1, 1);
        let ctx = second_goes_first.ctx();
        assert_eq!(turns_taken(&ctx, 1), 3, "seat 1 opened: turns 1, 3 and 5");
        assert_eq!(turns_taken(&ctx, 0), 2, "seat 0 followed: turns 2 and 4");
    }

    #[test]
    fn nobody_scores_at_the_monument_before_their_third_turn_and_other_battlefields_are_untouched()
    {
        let mut early = monument(4, 1, 0);
        let ctx = early.ctx();
        assert!(is_monument(&ctx, MONUMENT));
        assert!(!is_monument(&ctx, fixtures::ROCKFALL));
        assert!(
            !may_score_here(&ctx, MONUMENT, 0),
            "seat 0 is on its second turn"
        );
        assert!(!may_score_here(&ctx, MONUMENT, 1));
        assert!(may_score_here(&ctx, fixtures::ROCKFALL, 0));
        assert!(may_score_here(&ctx, fixtures::VI, 0), "not a battlefield");
        assert!(scoring_vetoed_at(&ctx, fixtures::BF1, 0));
        assert!(scoring_vetoed_at(&ctx, fixtures::BF1, 1));
        assert!(!scoring_vetoed_at(&ctx, fixtures::BF2, 0));
        assert!(
            !scoring_vetoed_at(&ctx, fixtures::BF3, 1),
            "no battlefield card there"
        );
        drop(ctx);
        let mut fifth = monument(5, 0, 0);
        let ctx = fifth.ctx();
        assert!(may_score_here(&ctx, MONUMENT, 0), "seat 0's third turn");
        assert!(!may_score_here(&ctx, MONUMENT, 1), "seat 1 has had two");
        assert!(!scoring_vetoed_at(&ctx, fixtures::BF1, 0));
        assert!(scoring_vetoed_at(&ctx, fixtures::BF1, 1));
        drop(ctx);
        let mut sixth = monument(6, 1, 0);
        let ctx = sixth.ctx();
        assert!(may_score_here(&ctx, MONUMENT, 0));
        assert!(may_score_here(&ctx, MONUMENT, 1));
    }

    #[test]
    fn a_conquer_before_the_third_turn_takes_control_without_a_point_and_the_third_turns_hold_scores(
    ) {
        let mut fixture = monument(1, 0, 0);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.no_score_here(fixtures::BF1, 0));
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            Established::Kept(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0), "control changes");
        assert_eq!(ctx.points(0), 0, "no point before the third turn");
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Conquered { .. })));
        assert!(!ctx.blob.scored(fixtures::BF1, 0));
        drop(ctx);
        let mut third = monument(5, 0, 0);
        third.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        third.blob.set_holder(fixtures::BF1, Some(0));
        third.resolve();
        let mut ctx = third.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert_eq!(ctx.points(0), 1);
    }
}

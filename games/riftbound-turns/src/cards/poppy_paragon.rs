use super::aspirants_climb::victory_score;
use super::prelude::{done, gain_xp, play, ready, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const WITHIN: i32 = 3;
pub const XP: u8 = 3;

pub fn an_opponent_is_within_reach_of_victory(ctx: &Ctx, seat: u8) -> bool {
    let victory = victory_score(ctx);
    (0..ctx.players())
        .filter(|other| *other != seat)
        .any(|other| victory - ctx.points(other) <= WITHIN)
}

fn the_race_is_close(ctx: &Ctx, _: &Event, source: Source) -> bool {
    an_opponent_is_within_reach_of_victory(ctx, ctx.controller(source.card))
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Poppy - Paragon",
    &[Keyword::Deflect(1)],
    &[when(play(&[], rally), the_race_is_close)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const POPPY: u32 = 90;
    const SPARE_RUNES: [u32; 2] = [46, 47];

    fn poppy() -> CardInfo {
        let mut card = fixtures::unit(POPPY, fixtures::HAND, 0, "Poppy - Paragon", 5);
        card.energy = Some(5);
        card.domain = vec!["Body".into()];
        card
    }

    fn keep(their_points: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poppy());
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        if their_points > 0 {
            fixture.set_points(1, their_points);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(POPPY).unwrap(), &CARD));
        fixture
    }

    fn source() -> Source {
        Source {
            card: POPPY,
            ability: 0,
        }
    }

    #[test]
    fn the_script_prints_deflect_and_one_gated_play_trigger() {
        assert!(std::ptr::eq(script_of("Poppy - Paragon").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(
            ability.condition.is_some(),
            "383.2.a.1 · the if sits right after the condition"
        );
        assert_eq!(WITHIN, 3);
        assert_eq!(XP, 3);
        assert_eq!(DEFAULT_VICTORY_SCORE, 8);
    }

    #[test]
    fn the_gate_reads_any_opponent_within_three_of_the_victory_score_and_never_your_own_score() {
        let mut close = keep(DEFAULT_VICTORY_SCORE - WITHIN);
        let ctx = close.ctx();
        assert!(an_opponent_is_within_reach_of_victory(&ctx, 0));
        assert!(the_race_is_close(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source()
        ));
        assert!(
            !an_opponent_is_within_reach_of_victory(&ctx, 1),
            "seat 1's own score is not an opponent's"
        );
        drop(ctx);
        let mut far = keep(DEFAULT_VICTORY_SCORE - WITHIN - 1);
        let ctx = far.ctx();
        assert!(!an_opponent_is_within_reach_of_victory(&ctx, 0));
        assert!(!the_race_is_close(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source()
        ));
        drop(ctx);
        let mut mine = keep(0);
        mine.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        let ctx = mine.ctx();
        assert!(
            !an_opponent_is_within_reach_of_victory(&ctx, 0),
            "your own lead is no reason to rally"
        );
        assert!(an_opponent_is_within_reach_of_victory(&ctx, 1));
    }

    #[test]
    fn with_an_opponent_close_to_winning_she_readies_and_gains_three_xp_when_the_trigger_resolves()
    {
        let mut fixture = keep(DEFAULT_VICTORY_SCORE - WITHIN);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POPPY).unwrap();
        assert!(ctx.on_board(POPPY));
        assert!(
            ctx.card(POPPY).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == POPPY
        ));
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.card(POPPY).unwrap().exhausted,
            "the trigger readies her"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == POPPY)));
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&format!("{{card {POPPY}}} readies")));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 3 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_every_opponent_four_or_more_away_the_trigger_never_fires() {
        let mut fixture = keep(DEFAULT_VICTORY_SCORE - WITHIN - 1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POPPY).unwrap();
        assert!(ctx.on_board(POPPY));
        assert!(
            ctx.blob.chain.is_empty(),
            "the gate is part of the condition"
        );
        assert!(ctx.card(POPPY).unwrap().exhausted);
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_poppy_killed_in_response_still_gains_the_xp_and_readies_nothing() {
        let mut fixture = keep(DEFAULT_VICTORY_SCORE - 1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POPPY).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.kill(POPPY, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(POPPY));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == POPPY)));
        assert!(!ctx.blob.log.contains(&format!("{{card {POPPY}}} readies")));
    }
}

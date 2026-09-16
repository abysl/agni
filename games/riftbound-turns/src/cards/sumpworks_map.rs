use super::prelude::{done, draw, gear};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const DRAWS: usize = 1;

pub fn an_opponent_scored_until_event_scored_lands(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    let me = ctx.controller(source.card);
    match event {
        Event::Held { seat, .. } | Event::Conquered { seat, .. } => *seat != me,
        _ => false,
    }
}

pub fn chart(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = gear(
    "Sumpworks Map",
    &[Keyword::Reaction, Keyword::Temporary],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, phases, priority, settle};
    use crate::state::{GameBlob, ItemKind, Mode, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const MAP: u32 = 90;

    fn map(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::gear(MAP, zone, seat, "Sumpworks Map", 2)
        }
    }

    fn sumpworks(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(map(zone, 0));
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: MAP,
            ability: 0,
        }
    }

    fn held(zone: u16, seat: u8) -> Event {
        Event::Held {
            zone,
            seat,
            units: vec![],
        }
    }

    #[test]
    fn the_stub_prints_reaction_and_temporary_and_the_opponent_scores_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Sumpworks Map").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Reaction, Keyword::Temporary]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(DRAWS, 1);
        let mut fixture = sumpworks(fixtures::BASE);
        assert!(std::ptr::eq(fixture.scripts.of_card(MAP).unwrap(), &CARD));
        let ctx = fixture.ctx();
        assert!(ctx.is_temporary(MAP));
    }

    #[test]
    fn the_condition_reads_an_opponents_hold_or_conquer_and_never_your_own() {
        let mut fixture = sumpworks(fixtures::BASE);
        let ctx = fixture.ctx();
        let seam = an_opponent_scored_until_event_scored_lands;
        assert!(seam(&ctx, &held(fixtures::BF2, 1), source()));
        assert!(seam(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF1,
                seat: 1,
                units: vec![fixtures::THEIR_UNIT],
            },
            source()
        ));
        assert!(
            !seam(&ctx, &held(fixtures::BF1, 0), source()),
            "your own scoring is not an opponent's"
        );
        assert!(!seam(&ctx, &Event::Drew { seat: 1, nth: 1 }, source()));
    }

    #[test]
    fn the_effect_draws_one_for_the_maps_controller() {
        let mut fixture = sumpworks(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: MAP,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(chart(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
    }

    #[test]
    fn as_a_reaction_it_plays_with_the_chain_open_and_as_temporary_it_dies_at_your_next_beginning_phase(
    ) {
        let mut fixture = sumpworks(fixtures::HAND);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(!ctx.blob.chain.is_empty());
        while priority::holder(&ctx) != Some(0) {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(
            legal::timing(&ctx, 0, MAP).is_ok(),
            "a Reaction plays while the chain is open"
        );
        fixtures::play_from_hand(&mut ctx, 0, MAP).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(MAP));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = sumpworks(fixtures::BASE);
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = 3;
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(
            ctx.blob.chain.iter().any(|item| matches!(
                item.kind,
                ItemKind::Trigger { source, index } if source == MAP && index == crate::cards::IMPLICIT_TEMPORARY
            )),
            "the Temporary kill waits on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(MAP));
        assert!(ctx.in_trash(MAP));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_an_opponents_hold_queues_nothing_for_the_map() {
        let mut fixture = sumpworks(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.raise(held(fixtures::BF2, 1));
        settle(&mut ctx).unwrap();
        assert!(
            !ctx.blob.chain.iter().any(|item| item.kind.source() == MAP),
            "no trigger subject carries an opponent's scoring yet"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    #[ignore = "engine gap · missing trigger subjects: no Scored event (a hold, a conquer that scores, and a score effect raise none with the scoring seat) and no Who::Enemy on Hold/Conquer; with it the script is when(triggered(Scored(Who::Enemy), &[], chart), _) and an opponent's score draws one"]
    fn an_opponents_hold_draws_one_when_the_trigger_resolves() {
        let mut fixture = sumpworks(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.raise(held(fixtures::BF2, 1));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }
}

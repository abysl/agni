use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Blazing Scorcher", &[Keyword::Accelerate], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cost;
    use crate::engine::ctx::{Ctx, EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, prompts, settle};
    use crate::state::{Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const SCORCHER: u32 = 90;
    const THEIR_SCORCHER: u32 = 91;
    const ENERGY: u8 = 5;
    const MIGHT: u8 = 5;
    const ACCELERATED: u8 = ENERGY + 1;
    const EXTRA_RUNES: [u32; 2] = [100, 101];

    fn scorcher(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            ..fixtures::unit(id, zone, seat, "Blazing Scorcher", MIGHT)
        }
    }

    fn in_hand(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(scorcher(SCORCHER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(scorcher(THEIR_SCORCHER, fixtures::HAND, 1));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, rune) in runes.into_iter().enumerate() {
            fixture.table.card_mut(rune).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: Some(fixtures::BASE),
            to_seat: seat,
            index: TOP,
            hidden: false,
        }
    }

    fn play_to_base(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        play_engine::begin(ctx, seat, card, Origin::Hand, Some(Location::Base(seat)))?;
        settle(ctx)
    }

    fn recycled_runes(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { zone, .. } if *zone == fixtures::RUNE_DECK)
            })
            .count()
    }

    #[test]
    fn the_script_is_an_accelerate_unit_with_no_abilities() {
        assert_eq!(CARD.name, "Blazing Scorcher");
        assert_eq!(CARD.keywords, &[Keyword::Accelerate]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = in_hand(ACCELERATED.into());
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SCORCHER).unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SCORCHER, Keyword::Accelerate));
        assert!(cost::can_accelerate(&ctx, SCORCHER));
    }

    #[test]
    fn with_a_rune_to_spare_the_play_asks_to_accelerate_and_paying_one_and_a_fury_enters_ready() {
        let mut fixture = in_hand(ACCELERATED.into());
        let action = fixtures::move_action(SCORCHER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, SCORCHER).unwrap();
        let why = ctx.blob.why;
        assert!(
            matches!(why, Some(PromptWhy::OptionalCost { .. })),
            "731.2 · Accelerate is an optional additional cost: {why:?}"
        );
        assert_eq!(
            prompts::status(&ctx, why.unwrap()),
            format!("accelerate {{card {SCORCHER}}} for 1 energy and 1 Fury power?")
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        assert!(ctx.effects.is_empty(), "nothing is paid before the answer");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(SCORCHER), Some(Location::Base(0)));
        assert!(
            !ctx.card(SCORCHER).unwrap().exhausted,
            "731.6 · it enters ready"
        );
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "the printed cost plus one energy exhausts every rune"
        );
        assert_eq!(
            recycled_runes(&ctx),
            1,
            "one of the exhausted runes is recycled for the Fury power"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == SCORCHER
        )));
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_the_accelerate_pays_only_the_printed_cost_and_it_enters_exhausted() {
        let mut fixture = in_hand(ACCELERATED.into());
        let action = fixtures::move_action(SCORCHER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, SCORCHER).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(SCORCHER), Some(Location::Base(0)));
        assert!(ctx.card(SCORCHER).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one rune was spared");
        assert_eq!(recycled_runes(&ctx), 0, "no power was paid");
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn with_runes_for_the_printed_cost_only_nothing_is_asked_and_the_play_is_refused_when_short() {
        let mut fixture = in_hand(ENERGY.into());
        let action = fixtures::move_action(SCORCHER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, SCORCHER).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "an accelerate the seat cannot pay is not offered"
        );
        assert_eq!(ctx.location(SCORCHER), Some(Location::Base(0)));
        assert!(ctx.card(SCORCHER).unwrap().exhausted);
        assert!(ctx.ready_runes_of(0).is_empty());
        drop(ctx);
        let mut theirs = in_hand(ACCELERATED.into());
        let ctx = theirs.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SCORCHER)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut short = in_hand(usize::from(ENERGY) - 1);
        let mut ctx = short.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, SCORCHER),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }
}

use super::prelude::unit;
use super::Card;
use crate::engine::ctx::Ctx;

pub fn watches_the_reveals_of(ctx: &Ctx, me: u32, seat: u8) -> bool {
    ctx.on_board(me) && !ctx.is_facedown(me) && ctx.controller(me) == seat
}

pub fn hatchlings_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut found: Vec<u32> = ctx
        .faces_on_board()
        .filter(|card| card.name == CARD.name)
        .map(|card| card.id)
        .filter(|card| watches_the_reveals_of(ctx, *card, seat))
        .collect();
    found.sort_unstable();
    found
}

pub fn look_at_the_top_first(ctx: &mut Ctx, seat: u8) -> Option<u32> {
    if hatchlings_of(ctx, seat).is_empty() {
        return None;
    }
    ctx.peek_top(seat)
}

pub fn recycle_the_look(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    let Some(deck) = ctx.zones.main_deck else {
        return false;
    };
    if ctx.top_of(deck, seat, 1).first() != Some(&card) {
        return false;
    }
    ctx.recycle_to_bottom(card);
    ctx.narrate(format!(
        "{{seat {seat}}} recycles the top card of their deck before revealing"
    ));
    true
}

pub static CARD: Card = unit("Void Hatchling", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const HATCHLING: u32 = 90;
    const THEIR_HATCHLING: u32 = 91;

    fn hatchling(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, seat, "Void Hatchling", 2)
        }
    }

    fn nest(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hatchling(HATCHLING, zone, 0));
        fixture
            .table
            .cards
            .push(hatchling(THEIR_HATCHLING, fixtures::BASE, 1));
        fixture.resolve();
        fixture
    }

    fn top_of_deck(ctx: &Ctx, seat: u8) -> Option<u32> {
        ctx.top_of(ctx.zones.main_deck.unwrap(), seat, 1)
            .first()
            .copied()
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_reveal_replacement_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Void Hatchling").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.replacement.is_none(),
            "the kill path is not the reveal path"
        );
        let mut fixture = nest(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(HATCHLING).unwrap(), &CARD));
    }

    #[test]
    fn the_watch_reads_a_hatchling_of_yours_on_the_board_and_nothing_else() {
        let mut fixture = nest(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(watches_the_reveals_of(&ctx, HATCHLING, 0));
        assert!(
            !watches_the_reveals_of(&ctx, HATCHLING, 1),
            "your reveals, not the opponent's"
        );
        assert_eq!(hatchlings_of(&ctx, 0), [HATCHLING]);
        assert_eq!(hatchlings_of(&ctx, 1), [THEIR_HATCHLING]);
        drop(ctx);

        let mut fixture = nest(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            !watches_the_reveals_of(&ctx, HATCHLING, 0),
            "384.1 · in hand it watches nothing"
        );
        assert!(hatchlings_of(&ctx, 0).is_empty());
    }

    #[test]
    fn the_look_peeks_the_top_card_to_its_controller_alone_and_the_recycle_sends_it_to_the_bottom()
    {
        let mut fixture = nest(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let top = top_of_deck(&ctx, 0).unwrap();
        assert_eq!(look_at_the_top_first(&mut ctx, 0), Some(top));
        assert!(ctx.effects.contains(&Effect::Peek { card: top, seat: 0 }));
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Reveal { .. })),
            "a look is not a reveal"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top card of their deck".to_string()));
        assert!(recycle_the_look(&mut ctx, 0, top));
        assert_ne!(top_of_deck(&ctx, 0), Some(top), "recycled to the bottom");
        let deck = ctx.zones.main_deck.unwrap();
        assert_eq!(
            ctx.table.held(deck, 0).next().map(|card| card.id),
            Some(top),
            "the bottom of the deck"
        );
        assert!(
            !recycle_the_look(&mut ctx, 0, top),
            "only the card on top can be the look"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn without_a_hatchling_of_yours_there_is_no_look() {
        let mut fixture = nest(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(look_at_the_top_first(&mut ctx, 0), None);
        assert!(ctx.effects.is_empty());
        assert_eq!(
            look_at_the_top_first(&mut ctx, 1),
            top_of_deck(&ctx, 1),
            "the opponent's Hatchling watches the opponent's reveals"
        );
    }

    #[test]
    fn today_a_reveal_off_the_deck_looks_at_nothing_first() {
        let mut fixture = nest(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let top = top_of_deck(&ctx, 0).unwrap();
        assert_eq!(ctx.reveal_top(0), Some(top));
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Peek { .. })),
            "no look precedes the reveal yet"
        );
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    #[ignore = "engine gap · a reveal replacement: Ctx::reveal_top consults no in-play static before revealing; with it a seat with a Void Hatchling looks at the top card, is asked [recycle, keep] through look_at_the_top_first / recycle_the_look, and the reveal reads the card left on top"]
    fn with_a_hatchling_a_reveal_first_offers_to_recycle_the_top_card() {
        let mut fixture = nest(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let top = top_of_deck(&ctx, 0).unwrap();
        let next = ctx.top_of(ctx.zones.main_deck.unwrap(), 0, 2)[1];
        assert_eq!(ctx.reveal_top(0), None, "parked on the look");
        assert!(ctx.effects.contains(&Effect::Peek { card: top, seat: 0 }));
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "recycle").unwrap();
        assert!(ctx.effects.contains(&Effect::Reveal { card: next }));
    }
}

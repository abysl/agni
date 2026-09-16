use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 4;

fn seats_in_turn_order(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

pub fn discard_hand(ctx: &mut Ctx, seat: u8) -> usize {
    if ctx.zones.trash.is_none() {
        return 0;
    }
    let hand = ctx.hand_of(seat);
    for card in &hand {
        ctx.file_in_trash(*card, seat);
        ctx.blob.drop_card_state(*card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} discards their hand · {} cards",
        hand.len()
    ));
    hand.len()
}

fn invert(ctx: &mut Ctx, _: &Item, _: Stage) -> Flow {
    for seat in seats_in_turn_order(ctx) {
        discard_hand(ctx, seat);
        let drawn = draw(ctx, seat, DRAWS);
        ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    }
    done()
}

pub static CARD: Card = spell("Invert Timelines", &[], &[play(&[], invert)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const INVERT: u32 = 90;
    const THEIR_INVERT: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        for (id, seat) in [(INVERT, 0), (THEIR_INVERT, 1)] {
            let mut invert = fixtures::spell(id, fixtures::HAND, seat, "Invert Timelines", 0, 0);
            invert.domain = vec!["Chaos".into()];
            fixture.table.cards.push(invert);
        }
        for id in 26..=29 {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 1));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_targetless_sorcery() {
        assert!(std::ptr::eq(script_of("Invert Timelines").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.abilities[0].candidates.is_none());
    }

    #[test]
    fn every_hand_is_trashed_and_each_player_draws_four_turn_player_first() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let my_hand: Vec<u32> = ctx.hand_of(0);
        let their_hand: Vec<u32> = ctx.hand_of(1);
        assert_eq!(their_hand, [fixtures::THEIR_HAND_CARD, THEIR_INVERT]);
        fixtures::play_from_hand(&mut ctx, 0, INVERT).unwrap();
        assert!(ctx.blob.prompt.is_none());
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        for card in my_hand.iter().filter(|card| **card != INVERT) {
            assert_eq!(ctx.card(*card).unwrap().zone, Some(fixtures::TRASH));
            assert_eq!(ctx.card(*card).unwrap().seat, 0);
        }
        for card in &their_hand {
            assert_eq!(ctx.card(*card).unwrap().zone, Some(fixtures::TRASH));
            assert_eq!(ctx.card(*card).unwrap().seat, 1);
        }
        let mut fresh = ctx.hand_of(0);
        fresh.sort_unstable();
        assert_eq!(fresh, [20, 21, 22, 23], "the whole deck of four is in hand");
        assert_eq!(ctx.hand_of(1).len(), 4);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            4
        );
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 1, .. }))
                .count(),
            4
        );
        let log = &ctx.blob.log;
        let mine = log
            .iter()
            .position(|line| line == "{seat 0} discards their hand · 4 cards")
            .expect("the turn player discards first");
        let theirs = log
            .iter()
            .position(|line| line == "{seat 1} discards their hand · 2 cards")
            .expect("then the other seat");
        assert!(mine < theirs);
        assert!(log.contains(&"{seat 0} draws 4".to_string()));
        assert!(
            ctx.card(INVERT).unwrap().zone == Some(fixtures::TRASH),
            "the spell itself is on the chain while hands are discarded, then trashed"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_empty_hand_discards_nothing_and_a_short_deck_draws_what_it_can() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.seat != 1);
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::MAIN_DECK) || card.seat != 1 || card.id == 24
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, INVERT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob
                .log
                .contains(&"{seat 1} discards their hand · 0 cards".to_string()),
            "{:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.hand_of(1), [24]);
        assert!(ctx.blob.log.contains(&"{seat 1} draws 1".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_spell_is_a_sorcery_the_opponent_cannot_play_on_my_turn() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_INVERT,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(legal::classify(&ctx, 1, &entry), Err(Refusal::NotYourTurn));
    }
}

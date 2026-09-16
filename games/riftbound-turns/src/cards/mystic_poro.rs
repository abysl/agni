use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Mystic Poro", &[Keyword::Vision], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;
    const THEIR_PORO: u32 = 91;
    const ENERGY: u8 = 2;
    const MIGHT: u8 = 2;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const MY_TOP: u32 = 23;

    fn poro(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Mystic Poro", MIGHT)
        }
    }

    fn in_hand() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(PORO, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(poro(THEIR_PORO, fixtures::HAND, 1));
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

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_vision_unit_with_no_abilities() {
        assert_eq!(CARD.name, "Mystic Poro");
        assert_eq!(CARD.keywords, &[Keyword::Vision]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = in_hand();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(PORO, Keyword::Vision));
    }

    #[test]
    fn played_from_hand_it_pays_two_and_enters_the_base_exhausted() {
        let mut fixture = in_hand();
        let action = fixtures::move_action(PORO, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(deck_of(&ctx, 0), MY_DECK);
        play_to_base(&mut ctx, 0, PORO).unwrap();
        assert_eq!(ctx.location(PORO), Some(Location::Base(0)));
        assert!(ctx.card(PORO).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two of three ready runes");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == PORO
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn vision_looks_at_the_top_card_of_the_deck_as_it_is_played_and_may_recycle_it() {
        let mut fixture = in_hand();
        let action = fixtures::move_action(PORO, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, PORO).unwrap();
        assert_eq!(ctx.location(PORO), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "817.1.c · the trigger is the unit entering the board"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            peeks(&ctx),
            [(MY_TOP, 0)],
            "817.1.b · predict: the look reaches its controller only"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        assert_eq!(
            deck_of(&ctx, 0),
            [MY_TOP, 20, 21, 22],
            "403.1.a · recycled to the bottom of the Main Deck"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_other_seat_cannot_play_it_on_this_turn_and_one_ready_rune_is_not_enough() {
        let mut fixture = in_hand();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_PORO)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        for rune in [42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let action = fixtures::move_action(PORO, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, PORO),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        assert!(peeks(&ctx).is_empty(), "nothing was looked at");
    }
}

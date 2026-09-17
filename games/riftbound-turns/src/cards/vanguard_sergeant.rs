use super::prelude::unit;
use super::Card;

pub static CARD: Card = unit("Vanguard Sergeant", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SERGEANT: u32 = 90;
    const THEIR_SERGEANT: u32 = 91;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;
    const EXTRA_RUNES: [u32; 0] = [];

    fn sergeant(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Order".into()],
            ..fixtures::unit(id, zone, seat, "Vanguard Sergeant", MIGHT)
        }
    }

    fn in_hand(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(sergeant(SERGEANT, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(sergeant(THEIR_SERGEANT, fixtures::HAND, 1));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
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
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    #[test]
    fn the_script_is_a_plain_unit_with_no_keywords_and_no_abilities() {
        assert_eq!(CARD.name, "Vanguard Sergeant");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let fixture = in_hand(ENERGY.into());
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SERGEANT).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn played_from_hand_it_pays_four_enters_the_base_exhausted_and_puts_nothing_on_the_chain() {
        let mut fixture = in_hand(ENERGY.into());
        let action = fixtures::move_action(SERGEANT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.ready_runes_of(0).len(), usize::from(ENERGY));
        play_to_base(&mut ctx, 0, SERGEANT).unwrap();
        assert_eq!(ctx.location(SERGEANT), Some(Location::Base(0)));
        assert!(
            ctx.card(SERGEANT).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "four energy exhausts every rune"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == SERGEANT
        )));
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none(), "no optional cost to ask about");
        assert_eq!(ctx.current_might(SERGEANT), i32::from(MIGHT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_other_seat_cannot_play_it_on_this_turn_and_three_ready_runes_are_not_enough() {
        let mut fixture = in_hand(ENERGY.into());
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SERGEANT)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut short = in_hand(usize::from(ENERGY) - 1);
        let action = fixtures::move_action(SERGEANT, fixtures::BASE, 0);
        let mut ctx = short.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, SERGEANT),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }
}

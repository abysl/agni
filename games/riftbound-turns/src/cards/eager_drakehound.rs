use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub fn enters_ready(_: &Ctx, _: u32) -> bool {
    true
}

pub static CARD: Card = with_statics(
    unit("Eager Drakehound", &[], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DRAKEHOUND: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn drakehound(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(DRAKEHOUND, zone, seat, "Eager Drakehound", MIGHT)
        }
    }

    fn kennel(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(drakehound(zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRAKEHOUND).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_entry_rule_is_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Eager Drakehound").unwrap(), &CARD));
        assert_eq!(CARD.name, "Eager Drakehound");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = kennel(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DRAKEHOUND), "unconditional");
        assert!(enters_ready(&ctx, fixtures::THEIR_UNIT));
    }

    #[test]
    fn played_from_hand_it_lands_in_the_base_and_the_play_is_refused_when_short() {
        let mut fixture = kennel(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAKEHOUND).unwrap();
        assert_eq!(ctx.location(DRAKEHOUND), Some(Location::Base(0)));
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == DRAKEHOUND
        )));
        drop(ctx);
        let mut short = kennel(fixtures::HAND);
        short.table.card_mut(43).unwrap().exhausted = true;
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, DRAKEHOUND),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }

    #[test]
    fn played_from_hand_it_enters_ready() {
        let mut fixture = kennel(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAKEHOUND).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(DRAKEHOUND));
        assert!(
            !ctx.card(DRAKEHOUND).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
    }
}

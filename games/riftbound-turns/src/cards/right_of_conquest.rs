use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const BASE_DRAW: usize = 1;

pub fn battlefields_you_or_allies_control(ctx: &Ctx, seat: u8) -> usize {
    ctx.zones
        .battlefields
        .iter()
        .filter(|zone| ctx.blob.holder(**zone) == Some(seat))
        .count()
}

fn conquest(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let held = battlefields_you_or_allies_control(ctx, seat);
    let drawn = draw(ctx, seat, BASE_DRAW + held);
    ctx.narrate(format!(
        "{{seat {seat}}} draws {drawn} · one and one for each of {held} held battlefields"
    ));
    done()
}

pub static CARD: Card = spell("Right of Conquest", &[], &[play(&[], conquest)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CONQUEST: u32 = 90;
    const THEIR_CONQUEST: u32 = 91;
    const THIRD_FIELD: u32 = 92;

    fn conquest_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Right of Conquest", 3, 1);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(conquest_card(CONQUEST, 0));
        fixture.table.cards.push(conquest_card(THEIR_CONQUEST, 1));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_spell_that_counts_held_battlefields() {
        assert!(std::ptr::eq(script_of("Right of Conquest").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(battlefields_you_or_allies_control(&ctx, 0), 0);
        assert_eq!(
            battlefields_you_or_allies_control(&ctx, 1),
            1,
            "the opponent holds the second battlefield"
        );
    }

    #[test]
    fn holding_nothing_it_draws_one_and_the_opponents_battlefield_never_counts() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, CONQUEST).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "the spell left, one card came");
        assert!(ctx.blob.log.contains(
            &"{seat 0} draws 1 · one and one for each of 0 held battlefields".to_string()
        ));
        assert_eq!(ctx.card(CONQUEST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn each_battlefield_held_at_resolution_adds_a_draw() {
        let mut fixture = armed();
        fixture.table.cards.push(fixtures::card(
            THIRD_FIELD,
            fixtures::BF3,
            0,
            "Seat of Power",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(battlefields_you_or_allies_control(&ctx, 0), 2);
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, CONQUEST).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(draws(&ctx), 3, "one plus two held");
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        assert!(ctx.blob.log.contains(
            &"{seat 0} draws 3 · one and one for each of 2 held battlefields".to_string()
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thin_deck_draws_what_it_has_and_the_opponents_copy_waits_for_their_turn() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CONQUEST)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CONQUEST).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(draws(&ctx), 1, "one card left to draw of the two owed");
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.fault.is_none());
    }
}

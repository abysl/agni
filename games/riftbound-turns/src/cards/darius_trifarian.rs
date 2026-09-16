use super::prelude::{
    a_card_not_a_token, done, might_this_turn, on_you_play_card, ready, unit, when,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 2;
const SECOND: u8 = 2;

fn your_second_card(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let seat = ctx.controller(source.card);
    let second = match event {
        Event::PlayedSpell { nth, .. } => *nth == SECOND,
        _ => ctx.blob.seat(seat).cards_played == SECOND,
    };
    second && a_card_not_a_token(ctx, event)
}

fn apprehend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Darius - Trifarian",
    &[],
    &[when(on_you_play_card(&[], apprehend), your_second_card)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;

    const DARIUS: u32 = 90;

    fn armed(cards_played: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut darius = fixtures::unit(DARIUS, fixtures::BASE, 0, "Darius - Trifarian", 5);
        darius.exhausted = true;
        fixture.table.cards.push(darius);
        fixture.blob.seat_mut(0).cards_played = cards_played;
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_second_card_of_the_turn_gives_him_two_might_and_readies_him() {
        assert!(std::ptr::eq(
            script_of("Darius - Trifarian").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities[0].trigger, Trigger::YouPlayCard);
        let mut fixture = armed(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 2);
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == DARIUS
        ));
        assert_eq!(ctx.current_might(DARIUS), 5);
        assert!(ctx.card(DARIUS).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(DARIUS), 5 + i32::from(BONUS));
        assert!(!ctx.card(DARIUS).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == DARIUS
        )));
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(DARIUS), 5, "the Might is for the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_as_the_second_card_counts_when_it_resolves_and_he_counts_himself() {
        let mut spell = armed(1);
        let mut ctx = spell.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "419.4.a · a spell triggers nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == DARIUS
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(DARIUS), 5 + i32::from(BONUS));
        assert!(!ctx.card(DARIUS).unwrap().exhausted);
        let mut himself = armed(1);
        himself.table.card_mut(DARIUS).unwrap().zone = Some(fixtures::HAND);
        himself.table.card_mut(DARIUS).unwrap().exhausted = false;
        himself.resolve();
        let mut ctx = himself.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DARIUS).unwrap();
        assert!(
            matches!(
                ctx.blob.chain.last().map(|top| top.kind),
                Some(ItemKind::Trigger { source, index: 0 }) if source == DARIUS
            ),
            "played as the second card he is on the board when the play is seen"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(DARIUS), 5 + i32::from(BONUS));
        assert!(
            !ctx.card(DARIUS).unwrap().exhausted,
            "he enters exhausted and his own trigger readies him"
        );
    }

    #[test]
    fn a_token_played_as_the_second_play_is_not_his_second_card() {
        let mut fixture = armed(1);
        let mut ctx = fixture.ctx();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).is_some());
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(
            ctx.blob.seat(0).cards_played,
            1,
            "185 · a token is not a card"
        );
        assert_eq!(ctx.current_might(DARIUS), 5);
        assert!(ctx.card(DARIUS).unwrap().exhausted);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 2);
        assert!(!ctx.blob.chain.is_empty(), "the gear is his second card");
    }

    #[test]
    fn the_first_and_third_cards_of_the_turn_leave_him_as_he_is() {
        for played in [0, 2] {
            let mut fixture = armed(played);
            let mut ctx = fixture.ctx();
            fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
            assert_eq!(ctx.blob.seat(0).cards_played, played + 1);
            assert!(
                ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
                "card number {} is not the second",
                played + 1
            );
            assert_eq!(ctx.current_might(DARIUS), 5);
            assert!(ctx.card(DARIUS).unwrap().exhausted);
        }
    }
}

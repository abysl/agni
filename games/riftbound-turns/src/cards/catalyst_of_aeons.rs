use super::prelude::{channel_exhausted, done, draw, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 2;
pub const DRAWS: usize = 1;

fn catalyse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let channelled = channel_exhausted(ctx, seat, RUNES);
    if channelled < RUNES {
        ctx.narrate(format!(
            "{{seat {seat}}} channelled {channelled} of {RUNES} · draws {DRAWS}"
        ));
        draw(ctx, seat, DRAWS);
    }
    done()
}

pub static CARD: Card = spell("Catalyst of Aeons", &[], &[play(&[], catalyse)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CATALYST: u32 = 90;
    const RUNE_DECK_TOP: [u32; 2] = [32, 31];

    fn catalyst(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(CATALYST, fixtures::HAND, seat, "Catalyst of Aeons", 4, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn with_rune_deck(size: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(catalyst(0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut kept = 0;
        fixture.table.cards.retain(|card| {
            if card.zone != Some(fixtures::RUNE_DECK) || card.owner != 0 {
                return true;
            }
            kept += 1;
            kept <= size
        });
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CATALYST).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn pool_size(ctx: &Ctx) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, 0).count()
    }

    fn cast_and_resolve(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, CATALYST).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy from four runes"
        );
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_a_plain_spell_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Catalyst of Aeons").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!((RUNES, DRAWS), (2, 1));
    }

    #[test]
    fn two_runes_arrive_exhausted_and_nothing_is_drawn() {
        let mut fixture = with_rune_deck(3);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_size(&ctx);
        cast_and_resolve(&mut ctx);
        assert_eq!(pool_size(&ctx), pool + RUNES);
        for rune in RUNE_DECK_TOP {
            assert_eq!(ctx.card(rune).unwrap().zone, Some(fixtures::RUNE_POOL));
            assert!(ctx.card(rune).unwrap().exhausted);
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 2 runes exhausted".to_string()));
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
    }

    #[test]
    fn one_rune_short_channels_what_it_can_and_draws_one() {
        let mut fixture = with_rune_deck(1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_size(&ctx);
        cast_and_resolve(&mut ctx);
        assert_eq!(pool_size(&ctx), pool + 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channelled 1 of 2 · draws 1".to_string()));
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn an_empty_rune_deck_still_draws_exactly_one() {
        let mut fixture = with_rune_deck(0);
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx);
        cast_and_resolve(&mut ctx);
        assert_eq!(pool_size(&ctx), pool);
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channelled 0 of 2 · draws 1".to_string()));
    }

    #[test]
    fn it_is_refused_off_turn() {
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(catalyst(1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = EntryMove {
            card: CATALYST,
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

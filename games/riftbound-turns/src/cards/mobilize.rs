use super::prelude::{channel_exhausted, done, draw, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;
pub const DRAWS: usize = 1;

fn mobilize(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if channel_exhausted(ctx, seat, RUNES) < RUNES {
        ctx.narrate(format!(
            "{{seat {seat}}} can't channel · draws {DRAWS} instead"
        ));
        draw(ctx, seat, DRAWS);
    }
    done()
}

pub static CARD: Card = spell("Mobilize", &[], &[play(&[], mobilize)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const MOBILIZE: u32 = 90;
    const TOP_RUNE: u32 = 32;

    fn mobilize_card(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(MOBILIZE, fixtures::HAND, seat, "Mobilize", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed(rune_deck: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mobilize_card(0));
        if !rune_deck {
            fixture
                .table
                .cards
                .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.owner != 0);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MOBILIZE).unwrap(),
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
        fixtures::play_from_hand(ctx, 0, MOBILIZE).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_a_plain_spell_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Mobilize").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!((RUNES, DRAWS), (1, 1));
    }

    #[test]
    fn mobilize_channels_one_rune_exhausted_and_draws_nothing() {
        let mut fixture = armed(true);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_size(&ctx);
        cast_and_resolve(&mut ctx);
        assert_eq!(pool_size(&ctx), pool + 1);
        assert_eq!(ctx.card(TOP_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        assert!(
            ctx.card(TOP_RUNE).unwrap().exhausted,
            "it arrives exhausted"
        );
        assert!(ctx.effects.contains(&Effect::exhaust(TOP_RUNE)));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(drew(&ctx, 0), 0, "the draw is only the fallback");
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert_eq!(ctx.card(MOBILIZE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_rune_deck_it_draws_one_instead() {
        let mut fixture = armed(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_size(&ctx);
        cast_and_resolve(&mut ctx);
        assert_eq!(pool_size(&ctx), pool, "nothing to channel");
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can't channel · draws 1 instead".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("channels")));
    }

    #[test]
    fn it_is_refused_off_turn_and_short_of_energy() {
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(mobilize_card(1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = EntryMove {
            card: MOBILIZE,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(legal::classify(&ctx, 1, &entry), Err(Refusal::NotYourTurn));
        let mut broke = armed(true);
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = broke.ctx();
        let entry = EntryMove {
            card: MOBILIZE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
    }
}

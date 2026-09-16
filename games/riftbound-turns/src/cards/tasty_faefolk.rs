use super::prelude::{channel_exhausted, deathknell, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 2;
pub const DRAWS: usize = 1;

fn last_feast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    channel_exhausted(ctx, seat, RUNES);
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Tasty Faefolk",
    &[Keyword::Accelerate, Keyword::Deathknell],
    &[deathknell(&[], last_feast)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const FAEFOLK: u32 = 90;
    const THEIR_FAEFOLK: u32 = 91;
    const RUNE_DECK_TOP: [u32; 2] = [32, 31];

    fn faefolk(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Tasty Faefolk", 6);
        card.energy = Some(7);
        card.domain = vec!["Calm".into()];
        card
    }

    fn served() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(faefolk(FAEFOLK, fixtures::BASE, 0));
        fixture.resolve();
        fixture
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    #[test]
    fn the_script_prints_accelerate_and_deathknell_with_one_untargeted_death_ability() {
        assert!(std::ptr::eq(script_of("Tasty Faefolk").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Accelerate, Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Death);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!((RUNES, DRAWS), (2, 1));
        let mut fixture = served();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(FAEFOLK).unwrap(), &CARD));
        assert!(
            ctx.has_keyword(FAEFOLK, Keyword::Accelerate),
            "the engine reads Accelerate off the keyword line"
        );
    }

    #[test]
    fn a_dead_faefolk_channels_two_runes_exhausted_and_draws_one_when_its_trigger_resolves() {
        let mut fixture = served();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(FAEFOLK, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FAEFOLK
        ));
        assert_eq!(pool_of(&ctx, 0), pool);
        assert_eq!(ctx.hand_of(0).len(), hand);
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool + RUNES);
        for rune in RUNE_DECK_TOP {
            assert!(ctx.effects.contains(&Effect::Move {
                card: rune,
                zone: fixtures::RUNE_POOL,
                seat: 0,
                index: TOP
            }));
            assert!(ctx.card(rune).unwrap().exhausted);
        }
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        let channel = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 0} channels 2 runes exhausted")
            .expect("the channel is narrated");
        let drew = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 0} draws 1")
            .expect("the draw is narrated");
        assert!(channel < drew, "channel, then draw");
        assert_eq!(pool_of(&ctx, 1), 2);
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_rune_deck_with_one_rune_left_channels_that_one_and_still_draws() {
        let mut fixture = served();
        fixture
            .table
            .cards
            .retain(|card| ![30, 31].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        let hand = ctx.hand_of(0).len();
        ctx.kill(FAEFOLK, Cause::Rule);
        settle(&mut ctx).unwrap();
        resolve(&mut ctx);
        assert_eq!(pool_of(&ctx, 0), pool + 1);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
    }

    #[test]
    fn a_living_faefolk_does_nothing_and_the_other_seat_cannot_play_one_now() {
        let mut fixture = served();
        fixture
            .table
            .cards
            .push(faefolk(THEIR_FAEFOLK, fixtures::HAND, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        let hand = ctx.hand_of(0).len();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!((pool_of(&ctx, 0), ctx.hand_of(0).len()), (pool, hand));
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_FAEFOLK,
            from: Some(fixtures::HAND),
            from_seat: 1,
            to: Some(fixtures::BASE),
            to_seat: 1,
            index: TOP,
            hidden: false,
        };
        assert_eq!(legal::classify(&ctx, 1, &entry), Err(Refusal::NotYourTurn));
    }
}

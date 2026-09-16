use super::prelude::{channel_exhausted, deathknell, done, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;

fn last_flight(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, RUNES);
    done()
}

pub static CARD: Card = unit(
    "Soaring Scout",
    &[Keyword::Deathknell],
    &[deathknell(&[], last_flight)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const SCOUT: u32 = 90;
    const THEIR_SCOUT: u32 = 91;
    const RUNE_DECK_TOP: u32 = 32;

    fn scout(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Soaring Scout", 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn aloft() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(scout(SCOUT, fixtures::BASE, 0));
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
    fn the_script_is_a_deathknell_unit_that_channels_one_rune_exhausted() {
        assert!(std::ptr::eq(script_of("Soaring Scout").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Death);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!(RUNES, 1);
        let fixture = aloft();
        assert!(std::ptr::eq(fixture.scripts.of_card(SCOUT).unwrap(), &CARD));
    }

    #[test]
    fn a_dead_scout_channels_the_top_rune_exhausted_once_its_trigger_resolves() {
        let mut fixture = aloft();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        assert_eq!(ctx.top_of(fixtures::RUNE_DECK, 0, 1), [RUNE_DECK_TOP]);
        assert_eq!(ctx.kill(SCOUT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SCOUT
        ));
        assert_eq!(pool_of(&ctx, 0), pool, "the channel waits for the chain");
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool + RUNES);
        assert!(ctx.effects.contains(&Effect::Move {
            card: RUNE_DECK_TOP,
            zone: fixtures::RUNE_POOL,
            seat: 0,
            index: TOP
        }));
        assert!(
            ctx.card(RUNE_DECK_TOP).unwrap().exhausted,
            "it arrives exhausted"
        );
        assert_eq!(pool_of(&ctx, 1), 2, "only its controller channels");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_rune_deck_channels_nothing_and_the_trigger_still_resolves_cleanly() {
        let mut fixture = aloft();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        ctx.kill(SCOUT, Cause::Rule);
        settle(&mut ctx).unwrap();
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool);
        assert!(!ctx.blob.log.iter().any(|line| line.contains("channels")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_living_scout_channels_nothing_and_the_other_seat_cannot_play_one_now() {
        let mut fixture = aloft();
        fixture
            .table
            .cards
            .push(scout(THEIR_SCOUT, fixtures::HAND, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool);
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_SCOUT,
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

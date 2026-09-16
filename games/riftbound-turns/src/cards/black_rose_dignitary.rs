use super::prelude::{channel_exhausted, deathknell, done, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;

fn last_rites(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if channel_exhausted(ctx, seat, RUNES) == 0 {
        ctx.narrate(format!("{{seat {seat}}} has no rune left to channel"));
    }
    done()
}

pub static CARD: Card = unit(
    "Black Rose Dignitary",
    &[Keyword::Assault(1), Keyword::Deathknell],
    &[deathknell(&[], last_rites)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DIGNITARY: u32 = 90;
    const MY_RUNE_DECK: [u32; 3] = [30, 31, 32];
    const THEIR_RUNE_DECK: [u32; 3] = [33, 34, 35];

    fn dignitary(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(DIGNITARY, zone, seat, "Black Rose Dignitary", 2)
        }
    }

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dignitary(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    fn rune_deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::RUNE_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_prints_assault_one_and_deathknell_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(
            script_of("Black Rose Dignitary").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Black Rose Dignitary");
        assert_eq!(CARD.keywords, [Keyword::Assault(1), Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(death.condition.is_none());
        assert_eq!(RUNES, 1);
        let mut fixture = court();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(DIGNITARY).unwrap(), &CARD));
        assert!(ctx.has_keyword(DIGNITARY, Keyword::Assault(1)));
    }

    #[test]
    fn dying_channels_the_top_rune_of_its_controllers_rune_deck_exhausted_once_the_chain_resolves()
    {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        let before = pool_of(&ctx, 0);
        assert_eq!(rune_deck_of(&ctx, 0), MY_RUNE_DECK);
        assert_eq!(ctx.kill(DIGNITARY, Cause::Rule), Killed::Yes);
        assert!(ctx.in_trash(DIGNITARY));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DIGNITARY
        ));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(pool_of(&ctx, 0), before, "the channel waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let after = pool_of(&ctx, 0);
        assert_eq!(after.len(), before.len() + RUNES);
        assert_eq!(
            after.last(),
            Some(&(32, true)),
            "the top rune of the deck arrives exhausted"
        );
        assert_eq!(rune_deck_of(&ctx, 0), [30, 31]);
        assert_eq!(
            rune_deck_of(&ctx, 1),
            THEIR_RUNE_DECK,
            "the opponent's rune deck is not yours"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_rune_deck_channels_nothing_and_the_trigger_says_so() {
        let mut fixture = court();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.seat != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let before = pool_of(&ctx, 0);
        assert!(rune_deck_of(&ctx, 0).is_empty());
        assert_eq!(ctx.kill(DIGNITARY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert_eq!(pool_of(&ctx, 0), before);
        assert_eq!(
            rune_deck_of(&ctx, 1),
            THEIR_RUNE_DECK,
            "315.3.b.1 · as many as possible, never the opponent's"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no rune left to channel".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_dignitary_channels_for_the_opponent() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dignitary(fixtures::BASE, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mine = pool_of(&ctx, 0);
        let theirs = pool_of(&ctx, 1);
        assert_eq!(ctx.kill(DIGNITARY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].controller, 1);
        resolve_chain(&mut ctx);
        assert_eq!(pool_of(&ctx, 0), mine);
        assert_eq!(pool_of(&ctx, 1).len(), theirs.len() + RUNES);
        assert_eq!(rune_deck_of(&ctx, 1), [33, 34]);
        assert!(ctx.fault.is_none());
    }
}

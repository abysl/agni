use super::prelude::{done, friendly_units, gain_xp, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const XP_PER_UNIT: u8 = 1;

pub fn xp_for_the_ranks(ctx: &Ctx, seat: u8) -> u8 {
    let ranks = friendly_units(ctx, seat).len();
    u8::try_from(ranks)
        .unwrap_or(u8::MAX)
        .saturating_mul(XP_PER_UNIT)
}

fn scrutinize(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let earned = xp_for_the_ranks(ctx, seat);
    if earned == 0 {
        ctx.narrate(format!("{{seat {seat}}} has no units · no XP"));
        return done();
    }
    gain_xp(ctx, seat, earned);
    done()
}

pub static CARD: Card = unit("Scrutinizing Sergeant", &[], &[play(&[], scrutinize)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SERGEANT: u32 = 90;
    const RECRUIT: u32 = 91;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn sergeant() -> CardInfo {
        let mut card = fixtures::unit(SERGEANT, fixtures::HAND, 0, "Scrutinizing Sergeant", 6);
        card.domain = vec!["Order".into()];
        card.energy = Some(6);
        card
    }

    fn barracks() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sergeant());
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(RECRUIT, fixtures::BASE, 0, "Recruit", 1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SERGEANT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn muster(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SERGEANT).unwrap();
        assert!(ctx.on_board(SERGEANT));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SERGEANT
        ));
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_play_trigger_that_counts_the_ranks() {
        assert!(std::ptr::eq(
            script_of("Scrutinizing Sergeant").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(XP_PER_UNIT, 1);
        let mut fixture = barracks();
        let ctx = fixture.ctx();
        assert_eq!(
            xp_for_the_ranks(&ctx, 0),
            2,
            "Vi and the Recruit at the base"
        );
        assert_eq!(
            xp_for_the_ranks(&ctx, 1),
            2,
            "the Sprite and Jinx are the other seat's"
        );
    }

    #[test]
    fn the_sergeant_counts_itself_and_every_other_friendly_unit_when_the_trigger_resolves() {
        let mut fixture = barracks();
        let mut ctx = fixture.ctx();
        muster(&mut ctx);
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 3, "Vi, the Recruit and the Sergeant");
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 3 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_ranks_are_counted_at_resolution_so_a_response_changes_the_xp() {
        let mut fixture = barracks();
        let mut ctx = fixture.ctx();
        muster(&mut ctx);
        ctx.kill(SERGEANT, Cause::Rule);
        ctx.kill(RECRUIT, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.xp(0), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 1, "Vi is the one unit left");
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }

    #[test]
    fn with_no_friendly_unit_left_the_trigger_gains_nothing() {
        let mut fixture = barracks();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::VI, RECRUIT].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        muster(&mut ctx);
        ctx.kill(SERGEANT, Cause::Rule);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no units · no XP".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("gains 0 XP")));
    }

    #[test]
    fn a_pool_short_of_six_refuses_the_play_and_counts_nothing() {
        let mut fixture = barracks();
        for rune in SPARE_RUNES {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, SERGEANT),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 3
            })
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }
}

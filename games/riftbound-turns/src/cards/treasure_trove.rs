use super::prelude::{
    activated, channel_exhausted, done, draw, exhausting_self, gear, kill, named, triggered, CHAOS,
};
use super::{Card, Flow, Item, Stage, Timing, Trigger};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;
const RUNES: usize = 1;

fn plunder(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    draw(ctx, seat, DRAWS);
    channel_exhausted(ctx, seat, RUNES);
    done()
}

pub fn when_leaving_the_board_alive(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    plunder(ctx, item, stage)
}

fn bury(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    kill(ctx, item, item.kind.source());
    done()
}

pub static CARD: Card = gear(
    "Treasure Trove",
    &[],
    &[
        triggered(Trigger::Death, &[], plunder),
        named(
            exhausting_self(activated(Timing::Sorcery, CHAOS, &[], bury)),
            "kill this",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const TROVE: u32 = 90;
    const CHAOS_RUNE: u32 = 48;
    const KILL: u8 = 1;

    fn trove(seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(TROVE, fixtures::BASE, seat, "Treasure Trove", 2)
        }
    }

    fn with_trove(seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(trove(seat, exhausted));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        fixture
    }

    fn trove_trigger_on_chain(ctx: &Ctx) -> bool {
        ctx.blob.chain.iter().any(
            |item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == TROVE),
        )
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    #[test]
    fn the_trove_is_a_death_trigger_and_a_chaos_exhaust_activation_that_kills_itself() {
        assert!(std::ptr::eq(script_of("Treasure Trove").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let leaves = &CARD.abilities[0];
        assert_eq!(leaves.trigger, Trigger::Death);
        assert!(leaves.targets.is_empty());
        assert!(!leaves.optional);
        let kill = &CARD.abilities[usize::from(KILL)];
        assert_eq!(kill.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(kill.cost, Some(CHAOS));
        assert_eq!(kill.self_cost, SelfCost::Exhaust);
        assert_eq!(kill.label, Some("kill this"));
        assert!(kill.targets.is_empty());
    }

    #[test]
    fn the_activation_exhausts_and_pays_chaos_then_the_kill_draws_one_and_channels_one_exhausted() {
        let mut fixture = with_trove(0, false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_of(&ctx, 0);
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == TROVE && offer.index == KILL && offer.enabled));
        activate::activate(&mut ctx, 0, TROVE, KILL).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.card(TROVE).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert!(
            ctx.on_board(TROVE),
            "the kill is the effect, so it waits for the chain"
        );
        assert!(
            !ctx.runes_of(0).iter().any(|rune| rune.id == CHAOS_RUNE),
            "the Chaos rune was recycled for the power"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(TROVE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, unit: false, .. } if *card == TROVE
        )));
        assert!(
            trove_trigger_on_chain(&ctx),
            "leaving the board queues the trigger"
        );
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        let after = pool_of(&ctx, 0);
        assert_eq!(after.len(), pool.len(), "one rune out, one rune in");
        let arrived: Vec<(u32, bool)> = after
            .iter()
            .copied()
            .filter(|rune| !pool.contains(rune))
            .collect();
        assert_eq!(arrived.len(), 1);
        assert!(arrived[0].1, "the channelled rune arrives exhausted");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
    }

    #[test]
    fn a_trove_killed_by_an_effect_draws_and_channels_for_its_controller() {
        let mut fixture = with_trove(1, false);
        let mut ctx = fixture.ctx();
        let mine = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        let pool = pool_of(&ctx, 1);
        ctx.kill(TROVE, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(trove_trigger_on_chain(&ctx));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(1).len(), theirs + 1);
        assert_eq!(ctx.hand_of(0).len(), mine);
        assert_eq!(pool_of(&ctx, 1).len(), pool.len() + 1);
    }

    #[test]
    fn an_exhausted_trove_an_unpaid_trove_and_an_opponents_trove_refuse_the_activation() {
        let mut spent = with_trove(0, true);
        let ctx = spent.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, TROVE, KILL).err(),
            Some(Refusal::Exhausted)
        );
        drop(ctx);
        let mut unpaid = with_trove(0, false);
        unpaid.table.cards.retain(|card| card.id != CHAOS_RUNE);
        unpaid.resolve();
        let ctx = unpaid.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, TROVE, KILL).err(),
            Some(Refusal::NoPowerOf),
            "a Chaos power needs a Chaos rune"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == TROVE && offer.index == KILL && !offer.enabled));
        drop(ctx);
        let mut theirs = with_trove(1, false);
        let ctx = theirs.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, TROVE, KILL).err(),
            Some(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::legal(&ctx, 1, TROVE, KILL).err(),
            Some(Refusal::NotYourTurn)
        );
    }

    #[test]
    #[ignore = "engine gap · Trigger::Death covers only a kill; a Trove bounced or banished off the board must draw and channel through when_leaving_the_board_alive"]
    fn a_bounced_trove_still_draws_one_and_channels_one_exhausted() {
        let mut fixture = with_trove(0, false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = pool_of(&ctx, 0);
        assert!(ctx.bounce(TROVE));
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(TROVE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 2, "the Trove and the draw");
        assert_eq!(pool_of(&ctx, 0).len(), pool.len() + 1);
    }
}

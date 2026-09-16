use super::prelude::{done, draw, gear, play, triggered};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;

fn scrap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub fn when_discarded(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    scrap(ctx, item, stage)
}

pub static CARD: Card = gear(
    "Scrapheap",
    &[],
    &[play(&[], scrap), triggered(Trigger::Death, &[], scrap)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const HEAP: u32 = 90;
    const MERCHANT: u32 = 91;

    fn heap(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            ..fixtures::gear(HEAP, zone, seat, "Scrapheap", 2)
        }
    }

    fn with_heap(zone: u16, seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(heap(zone, seat));
        fixture.resolve();
        fixture
    }

    fn heap_trigger(ctx: &Ctx, index: u8) -> bool {
        ctx.blob.chain.iter().any(|item| {
            matches!(item.kind, ItemKind::Trigger { source, index: held } if source == HEAP && held == index)
        })
    }

    #[test]
    fn the_heap_is_gear_with_a_play_trigger_and_a_death_trigger_that_target_nothing() {
        assert!(std::ptr::eq(script_of("Scrapheap").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[1].trigger, Trigger::Death);
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(!ability.optional);
            assert!(ability.cost.is_none());
            assert!(ability.condition.is_none());
        }
    }

    #[test]
    fn playing_the_heap_draws_one_once_the_trigger_resolves() {
        let mut fixture = with_heap(fixtures::HAND, 0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, HEAP).unwrap();
        assert_eq!(ctx.card(HEAP).unwrap().zone, Some(fixtures::BASE));
        assert!(heap_trigger(&ctx, 0), "the play trigger waits on the chain");
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand, "the Scrapheap replaced itself");
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
    }

    #[test]
    fn a_killed_heap_draws_one_for_its_controller_and_an_opponents_heap_draws_for_them() {
        let mut fixture = with_heap(fixtures::BASE, 0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        ctx.kill(HEAP, Cause::Rule);
        assert_eq!(ctx.card(HEAP).unwrap().zone, Some(fixtures::TRASH));
        settle(&mut ctx).unwrap();
        assert!(
            heap_trigger(&ctx, 1),
            "the death trigger waits on the chain"
        );
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.hand_of(1).len(), theirs, "only its controller draws");
        drop(ctx);
        let mut enemy = with_heap(fixtures::BASE, 1);
        let mut ctx = enemy.ctx();
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        ctx.kill(HEAP, Cause::Rule);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.hand_of(1).len(), theirs + 1);
    }

    #[test]
    fn a_heap_that_is_not_on_the_board_cannot_be_killed_and_draws_nothing() {
        let mut fixture = with_heap(fixtures::HAND, 0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            ctx.kill(HEAP, Cause::Rule),
            crate::engine::ctx::Killed::NotOnBoard
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.deaths.is_empty());
    }

    #[test]
    #[ignore = "engine gap · no Discarded event and triggers::sources lists in-play cards only; a Scrapheap discarded from the hand must draw one through when_discarded"]
    fn a_heap_discarded_from_the_hand_draws_one_beside_the_discarder_s_own_draw() {
        let mut fixture = with_heap(fixtures::HAND, 0);
        fixture.table.cards.push(fixtures::unit(
            MERCHANT,
            fixtures::BASE,
            0,
            "Traveling Merchant",
            2,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.move_unit(
            MERCHANT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {HEAP}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(HEAP).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + 2,
            "the Merchant's draw and the Scrapheap's"
        );
    }
}

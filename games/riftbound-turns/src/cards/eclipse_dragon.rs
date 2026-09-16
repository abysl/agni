use super::prelude::{done, draw, on_move, unit, when};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;

pub const RUNE_LIMIT: usize = 4;
pub const DRAWS: usize = 1;

pub fn controls_four_or_fewer_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() <= RUNE_LIMIT
}

fn few_runes(ctx: &Ctx, _: &Event, source: Source) -> bool {
    controls_four_or_fewer_runes(ctx, ctx.controller(source.card))
}

fn eclipse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Eclipse Dragon",
    &[Keyword::Accelerate],
    &[when(on_move(&[], eclipse), few_runes)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, march, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DRAGON: u32 = 90;
    const FIFTH_RUNE: u32 = 46;

    fn dragon(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(8),
            domain: vec!["Fury".into()],
            ..fixtures::unit(DRAGON, zone, seat, "Eclipse Dragon", 8)
        }
    }

    fn roost(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dragon(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRAGON).unwrap(),
            &CARD
        ));
        fixture
    }

    fn dragon_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == DRAGON))
            .count()
    }

    #[test]
    fn the_script_prints_accelerate_and_one_conditioned_move_trigger() {
        assert!(std::ptr::eq(script_of("Eclipse Dragon").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(
            ability.condition.is_some(),
            "if you control 4 or fewer runes"
        );
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none());
        assert_eq!(RUNE_LIMIT, 4);
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn a_dragged_move_with_four_runes_draws_one_when_the_trigger_resolves() {
        let mut fixture = roost(fixtures::BASE);
        let action = fixtures::move_action(DRAGON, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(controls_four_or_fewer_runes(&ctx, 0));
        let hand = ctx.hand_of(0).len();
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(DRAGON),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(dragon_items(&ctx), 1, "the trigger waits on the chain");
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_walk_home_draws_too_and_a_recall_is_not_a_move() {
        let mut fixture = roost(fixtures::BF1);
        let action = fixtures::move_action(DRAGON, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march::standard_move(
            &mut ctx,
            0,
            DRAGON,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
        assert_eq!(dragon_items(&ctx), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        drop(ctx);

        let mut fixture = roost(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.recall(DRAGON, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn five_runes_refuse_the_trigger_and_an_exhausted_fifth_still_counts() {
        let mut fixture = roost(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Fury", true));
        fixture.resolve();
        let action = fixtures::move_action(DRAGON, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.runes_of(0).len(), 5);
        assert!(
            !controls_four_or_fewer_runes(&ctx, 0),
            "runes you control, ready or exhausted"
        );
        assert!(
            controls_four_or_fewer_runes(&ctx, 1),
            "the opponent's two are not yours"
        );
        let hand = ctx.hand_of(0).len();
        march::standard_move(
            &mut ctx,
            0,
            DRAGON,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(DRAGON),
            Some(Location::Battlefield(fixtures::BF1)),
            "the move itself is never refused"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "383.2.a.1 · the rune count is part of the condition"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.fault.is_none());
    }
}

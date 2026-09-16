use super::prelude::{battlefield, channel_exhausted, done, triggered};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;

pub fn seats_from(ctx: &Ctx, first: u8) -> Vec<u8> {
    let players = ctx.players();
    (0..players)
        .map(|offset| (first + offset) % players)
        .collect()
}

fn each_player_channels_one_exhausted(ctx: &mut Ctx, _: &Item, _: Stage) -> Flow {
    for seat in seats_from(ctx, ctx.turn_player()) {
        if channel_exhausted(ctx, seat, RUNES) == 0 {
            ctx.narrate(format!("{{seat {seat}}} has no rune to channel"));
        }
    }
    done()
}

pub static CARD: Card = battlefield(
    "The Papertree",
    &[],
    &[triggered(
        Trigger::Hold(Who::You),
        &[],
        each_player_channels_one_exhausted,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, settle};
    use crate::state::ItemKind;
    use crate::Refusal;

    const PAPERTREE: u32 = fixtures::GROUNDS;

    fn papertree_held_by(seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(PAPERTREE).unwrap().name = "The Papertree".into();
        let unit = if seat == 0 {
            fixtures::VI
        } else {
            fixtures::THEIR_UNIT
        };
        fixture.table.card_mut(unit).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(seat));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PAPERTREE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn papertree_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == PAPERTREE => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn pool(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    fn rune_deck(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_DECK, seat).count()
    }

    #[test]
    fn the_papertree_is_one_free_hold_trigger_and_nothing_else() {
        assert!(std::ptr::eq(
            crate::cards::script_of("The Papertree").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert!(ability.cost.is_none());
        assert!(!ability.optional);
        assert!(ability.timing().is_none());
        assert_eq!(RUNES, 1);
    }

    #[test]
    fn each_player_is_walked_in_turn_order_from_the_turn_player() {
        let mut fixture = papertree_held_by(0);
        let ctx = fixture.ctx();
        assert_eq!(seats_from(&ctx, 0), [0, 1]);
        assert_eq!(seats_from(&ctx, 1), [1, 0], "303.2.a");
        assert_eq!(ctx.turn_player(), 0);
    }

    #[test]
    fn holding_channels_one_exhausted_rune_for_every_player_once_the_trigger_resolves() {
        let mut fixture = papertree_held_by(0);
        let mut ctx = fixture.ctx();
        let mine = pool(&ctx, 0);
        let theirs = pool(&ctx, 1);
        assert_eq!((rune_deck(&ctx, 0), rune_deck(&ctx, 1)), (3, 3));
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(papertree_items(&ctx), [0]);
        assert_eq!(pool(&ctx, 0), mine, "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!((rune_deck(&ctx, 0), rune_deck(&ctx, 1)), (2, 2));
        assert_eq!(pool(&ctx, 0).len(), mine.len() + 1);
        assert_eq!(pool(&ctx, 1).len(), theirs.len() + 1);
        for (seat, before) in [(0, &mine), (1, &theirs)] {
            let arrived: Vec<(u32, bool)> = pool(&ctx, seat)
                .into_iter()
                .filter(|held| !before.contains(held))
                .collect();
            assert_eq!(arrived.len(), 1, "{seat} channelled one");
            assert!(arrived[0].1, "{seat}'s rune arrives exhausted");
        }
        let log = ctx.blob.log.join("\n");
        let mine_at = log.find("{seat 0} channels 1 rune exhausted").unwrap();
        let theirs_at = log.find("{seat 1} channels 1 rune exhausted").unwrap();
        assert!(mine_at < theirs_at, "the turn player channels first");
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_holding_it_on_their_turn_channels_for_both_starting_with_themselves() {
        let mut fixture = papertree_held_by(1);
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        assert_eq!(
            cleanup::score_holds(&mut ctx, 1),
            [fixtures::BF1, fixtures::BF2]
        );
        settle(&mut ctx).unwrap();
        assert_eq!(papertree_items(&ctx), [1], "Rockfall Path adds no trigger");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!((rune_deck(&ctx, 0), rune_deck(&ctx, 1)), (2, 2));
        let log = ctx.blob.log.join("\n");
        let theirs_at = log.find("{seat 1} channels 1 rune exhausted").unwrap();
        let mine_at = log.find("{seat 0} channels 1 rune exhausted").unwrap();
        assert!(theirs_at < mine_at);
    }

    #[test]
    fn an_empty_rune_deck_channels_nothing_for_that_player_and_the_other_still_channels() {
        let mut fixture = papertree_held_by(0);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::RUNE_DECK) && card.owner == 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let theirs = pool(&ctx, 1);
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(rune_deck(&ctx, 0), 2);
        assert_eq!(pool(&ctx, 1), theirs);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no rune to channel".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_the_trigger_is_not_an_affordance() {
        let mut fixture = papertree_held_by(0);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(papertree_items(&ctx).is_empty());
        assert_eq!((rune_deck(&ctx, 0), rune_deck(&ctx, 1)), (3, 3));
        assert_eq!(
            activate::activate(&mut ctx, 0, PAPERTREE, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
    }
}

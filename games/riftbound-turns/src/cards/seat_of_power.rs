use super::prelude::{battlefield, done, draw, location_of, triggered, Location};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

fn here(ctx: &Ctx, card: u32) -> Option<u16> {
    match location_of(ctx, card) {
        Some(Location::Battlefield(zone)) => Some(zone),
        _ => None,
    }
}

fn other_battlefields(ctx: &Ctx, item: &Item) -> usize {
    let source = here(ctx, item.kind.source());
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| Some(*zone) != source)
        .filter(|zone| ctx.blob.holder(*zone) == Some(item.controller))
        .count()
}

fn draw_for_each_other_battlefield(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let others = other_battlefields(ctx, item);
    if others > 0 {
        let drawn = draw(ctx, seat, others);
        ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Seat of Power",
    &[],
    &[triggered(
        Trigger::Conquer(Who::You),
        &[],
        draw_for_each_other_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;

    const SEAT: u32 = fixtures::GROUNDS;
    const THIRD_FIELD: u32 = 92;

    fn seat_of_power() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SEAT).unwrap().name = "Seat of Power".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(fixtures::card(
            THIRD_FIELD,
            fixtures::BF3,
            1,
            "Proving Grounds",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SEAT).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx, seat: u8) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(seat)
        );
        settle(ctx).unwrap();
    }

    fn pass_ring(ctx: &mut Ctx, first: u8) {
        priority::pass(ctx, first).unwrap();
        priority::pass(ctx, 1 - first).unwrap();
    }

    fn trigger_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == SEAT => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_seat_is_a_battlefield_with_one_conquer_trigger_that_asks_for_nothing() {
        assert_eq!(CARD.name, "Seat of Power");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.timing().is_none());
    }

    #[test]
    fn conquering_the_seat_draws_one_card_for_each_other_battlefield_the_conqueror_holds() {
        let mut fixture = seat_of_power();
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        assert_eq!(trigger_items(&ctx), [0], "the conqueror holds the trigger");
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the draw waits for the trigger to resolve"
        );
        pass_ring(&mut ctx, 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SEAT}}} ability resolves")));
    }

    #[test]
    fn every_other_battlefield_counts_and_the_conquered_one_never_counts_itself() {
        let mut fixture = seat_of_power();
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        pass_ring(&mut ctx, 0);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "the seat itself is held now"
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 2, "two others, two cards");
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
    }

    #[test]
    fn a_conqueror_holding_nothing_else_draws_nothing_and_an_enemy_conquer_reads_their_own_board() {
        let mut fixture = seat_of_power();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        assert_eq!(trigger_items(&ctx), [0]);
        pass_ring(&mut ctx, 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "no other battlefield, no card");
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert!(!ctx.blob.log.iter().any(|line| line.contains("draws")));

        let mut fixture = seat_of_power();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let theirs = ctx.hand_of(1).len();
        let mine = ctx.hand_of(0).len();
        conquer(&mut ctx, 1);
        assert_eq!(trigger_items(&ctx), [1], "the ability follows control");
        pass_ring(&mut ctx, 1);
        assert_eq!(ctx.hand_of(1).len(), theirs + 1);
        assert_eq!(ctx.hand_of(0).len(), mine);
    }

    #[test]
    fn the_seats_trigger_is_not_an_affordance_any_seat_can_activate() {
        let mut fixture = seat_of_power();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SEAT, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, SEAT, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
    }
}

use super::prelude::{done, draw, legend, triggered};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const HAND_AT_MOST: usize = 1;
pub const DRAWS: usize = 1;

fn get_excited(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let held = ctx.hand_of(seat).len();
    if held > HAND_AT_MOST {
        ctx.narrate(format!(
            "{{seat {seat}}} holds {held} cards · {{card {}}} draws nothing",
            item.kind.source()
        ));
        return done();
    }
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = legend(
    "Jinx - Loose Cannon",
    &[],
    &[triggered(Trigger::BeginningPhase, &[], get_excited)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use crate::rules::CARDS_PER_TURN;
    use crate::state::{ItemKind, Phase};

    const JINX: u32 = fixtures::LEGEND_CARD;

    fn arsenal(hand: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(JINX).unwrap().name = CARD.name.into();
        let mut held: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.zone == Some(fixtures::HAND) && card.owner == 0)
            .map(|card| card.id)
            .collect();
        held.sort_unstable();
        for card in held.into_iter().skip(hand) {
            fixture.table.cards.retain(|held| held.id != card);
        }
        fixture.resolve();
        fixture
    }

    fn jinx_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == JINX))
            .count()
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    #[test]
    fn the_legend_has_one_unconditional_beginning_phase_trigger_whose_if_is_the_effect() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(
            ability.condition.is_none(),
            "383.2.a.1 · the if is not immediately after the condition, so it is part of the effect"
        );
        assert_eq!(HAND_AT_MOST, 1);
        assert_eq!(DRAWS, 1);
        let mut fixture = arsenal(1);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(JINX).unwrap(), &CARD));
    }

    #[test]
    fn with_one_card_in_hand_she_draws_one_before_the_turn_draw() {
        let mut fixture = arsenal(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hand_of(0).len(), 1);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(jinx_items(&ctx), 1, "the trigger is on the chain");
        assert_eq!(ctx.hand_of(0).len(), 1, "the draw waits for the chain");
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), 1 + DRAWS + CARDS_PER_TURN);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_she_draws_one_too() {
        let mut fixture = arsenal(0);
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        phases::start_turn(&mut ctx);
        assert_eq!(jinx_items(&ctx), 1);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), DRAWS + CARDS_PER_TURN);
    }

    #[test]
    fn with_two_cards_in_hand_the_trigger_still_goes_on_the_chain_and_draws_nothing() {
        let mut fixture = arsenal(2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hand_of(0).len(), 2);
        phases::start_turn(&mut ctx);
        assert_eq!(
            jinx_items(&ctx),
            1,
            "the condition is met · the if is read when it resolves"
        );
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), 2 + CARDS_PER_TURN);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} holds 2 cards · {{card {JINX}}} draws nothing"
        )));
    }

    #[test]
    fn the_hand_is_read_as_the_trigger_resolves_not_as_it_triggers() {
        let mut fixture = arsenal(1);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(jinx_items(&ctx), 1);
        ctx.draw(0, 1);
        assert_eq!(ctx.hand_of(0).len(), 2, "a card arrived in reaction");
        resolve_the_chain(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            2 + CARDS_PER_TURN,
            "two in hand at resolution · nothing from the legend"
        );
    }

    #[test]
    fn the_opponents_beginning_phase_is_not_hers() {
        let mut fixture = arsenal(0);
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(
            jinx_items(&ctx),
            0,
            "seat 1's Beginning Phase is not seat 0's"
        );
        resolve_the_chain(&mut ctx);
        assert!(ctx.hand_of(0).is_empty());
    }
}

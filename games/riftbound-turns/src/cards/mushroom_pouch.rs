use super::prelude::{done, draw, facedown_of, gear, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::hide;

const DRAWS: usize = 1;

fn a_facedown_card_at_a_battlefield(ctx: &Ctx, _: &Event, source: Source) -> bool {
    let seat = ctx.controller(source.card);
    facedown_of(ctx, seat)
        .into_iter()
        .any(|card| hide::zone_of(ctx, card).is_some_and(|zone| ctx.zones.is_battlefield(zone)))
}

fn forage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = gear(
    "Mushroom Pouch",
    &[],
    &[when(
        triggered(Trigger::BeginningPhase, &[], forage),
        a_facedown_card_at_a_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use crate::rules::CARDS_PER_TURN;
    use crate::state::{ItemKind, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const POUCH: u32 = 90;
    const SHROOM: u32 = fixtures::HAND_HIDDEN;

    fn pouch(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::gear(POUCH, fixtures::BASE, seat, "Mushroom Pouch", 2)
        }
    }

    fn with_pouch(seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pouch(seat));
        fixture.resolve();
        fixture
    }

    fn with_a_facedown_card_at(mut fixture: Fixture, zone: u16) -> Fixture {
        fixture.table.card_mut(SHROOM).unwrap().zone = Some(zone);
        fixture.blob.card_state_mut(SHROOM).hidden_at = Some(zone);
        fixture.resolve();
        fixture
    }

    fn pouch_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == POUCH))
            .count()
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    #[test]
    fn the_pouch_is_a_conditional_beginning_phase_trigger_that_targets_nothing() {
        assert!(std::ptr::eq(script_of("Mushroom Pouch").unwrap(), &CARD));
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
            ability.condition.is_some(),
            "383.2.a.1 · the if is part of the trigger condition"
        );
    }

    #[test]
    fn with_a_facedown_card_at_a_battlefield_the_pouch_draws_one_at_the_start_of_the_turn() {
        let mut fixture = with_a_facedown_card_at(with_pouch(0), fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.is_facedown(SHROOM));
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(pouch_items(&ctx), 1, "the trigger is on the chain");
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1 + CARDS_PER_TURN);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }

    #[test]
    fn with_no_facedown_card_the_pouch_stays_quiet() {
        let mut fixture = with_pouch(0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(facedown_of(&ctx, 0).is_empty());
        phases::start_turn(&mut ctx);
        assert_eq!(pouch_items(&ctx), 0);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + CARDS_PER_TURN);
    }

    #[test]
    fn a_pouch_reads_its_own_controllers_facedown_cards_on_that_seats_turn() {
        let mut fixture = with_a_facedown_card_at(with_pouch(1), fixtures::BF1);
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(SHROOM), 0, "the facedown card is seat 0's");
        phases::start_turn(&mut ctx);
        assert_eq!(pouch_items(&ctx), 0, "seat 1's pouch reads seat 1's cards");
        resolve_the_chain(&mut ctx);
        drop(ctx);
        let mut theirs = with_pouch(1);
        theirs.blob.core_mut().unwrap().player = 1;
        theirs
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        theirs.blob.set_holder(fixtures::BF2, None);
        theirs
            .table
            .card_mut(fixtures::THEIR_HAND_CARD)
            .unwrap()
            .zone = Some(fixtures::BF2);
        theirs
            .blob
            .card_state_mut(fixtures::THEIR_HAND_CARD)
            .hidden_at = Some(fixtures::BF2);
        for id in 26..29 {
            theirs
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 1));
        }
        theirs.resolve();
        let mut ctx = theirs.ctx();
        let hand = ctx.hand_of(1).len();
        let mine = ctx.hand_of(0).len();
        phases::start_turn(&mut ctx);
        assert_eq!(pouch_items(&ctx), 1);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.hand_of(1).len(), hand + 1 + CARDS_PER_TURN);
        assert_eq!(ctx.hand_of(0).len(), mine, "seat 0 draws nothing");
    }
}

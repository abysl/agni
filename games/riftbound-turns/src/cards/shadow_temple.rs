use super::prelude::{battlefield, burn, done, triggered};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const BURN: usize = 3;

fn burn_three(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if burn(ctx, seat, BURN) == 0 {
        ctx.narrate(format!("{{seat {seat}}} has no card to burn"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Shadow Temple",
    &[],
    &[triggered(Trigger::Hold(Who::You), &[], burn_three)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, triggers};
    use crate::state::ItemKind;

    const TEMPLE: u32 = fixtures::GROUNDS;

    fn temple_held_by(seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TEMPLE).unwrap().name = "Shadow Temple".into();
        let holder = if seat == 0 {
            fixtures::VI
        } else {
            fixtures::THEIR_UNIT
        };
        fixture.table.card_mut(holder).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(seat));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TEMPLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn temple_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == TEMPLE => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn burned(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Burned { seat: by, card } if *by == seat => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_temple_is_a_battlefield_with_one_mandatory_hold_trigger_and_no_cost() {
        assert!(std::ptr::eq(script_of("Shadow Temple").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.burn, 0, "the Burn is the effect, not a cost");
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(BURN, 3);
    }

    #[test]
    fn holding_the_temple_burns_the_top_three_cards_of_the_holders_deck() {
        let mut fixture = temple_held_by(0);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(temple_items(&ctx), [0]);
        assert!(ctx.blob.prompt.is_none(), "nothing to ask");
        assert!(burned(&ctx, 0).is_empty(), "the burn waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(burned(&ctx, 0), [23, 22, 21], "the top three, in order");
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), 1);
        for card in [21, 22, 23] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::TRASH));
        }
        assert!(ctx.blob.log.contains(&"{seat 0} burns 3".to_string()));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_holding_it_burns_their_own_deck_and_a_short_deck_burns_what_it_has() {
        let mut fixture = temple_held_by(1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::score_holds(&mut ctx, 1),
            [fixtures::BF1, fixtures::BF2]
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            temple_items(&ctx),
            [1],
            "the Rockfall Path hold is not the temple's"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            burned(&ctx, 1),
            [25, 24],
            "seat 1 has only two cards to burn"
        );
        assert!(burned(&ctx, 0).is_empty());
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), 4);
        assert!(ctx.blob.log.contains(&"{seat 1} burns 2".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_deck_burns_nothing_and_a_hold_elsewhere_or_a_conquer_never_fires_it() {
        let mut fixture = temple_held_by(0);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(temple_items(&ctx), [0]);
        resolve_chain(&mut ctx);
        assert!(burned(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to burn".to_string()));
        assert!(ctx.fault.is_none());
        let elsewhere = Event::Held {
            zone: fixtures::BF2,
            seat: 0,
            units: vec![],
        };
        assert!(triggers::find(&ctx, &elsewhere).is_empty());
        let conquered = Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        };
        assert!(
            triggers::find(&ctx, &conquered).is_empty(),
            "a conquer is not a hold"
        );
    }
}

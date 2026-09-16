use super::prelude::{battlefield, burn, done, on_conquer};
use super::{Ability, Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MILLED: usize = 2;

fn mill_two(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let milled = burn(ctx, seat, MILLED);
    ctx.narrate(format!(
        "{{card {}}} puts {milled} of {{seat {seat}}}'s top {MILLED} cards into their trash",
        item.kind.source()
    ));
    done()
}

pub const WHEN_YOU_CONQUER_HERE: Ability = on_conquer(&[], mill_two);

pub static CARD: Card = battlefield("Minefield", &[], &[WHEN_YOU_CONQUER_HERE]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, settle};
    use crate::state::ItemKind;
    use crate::Refusal;

    const MINEFIELD: u32 = fixtures::GROUNDS;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];

    fn minefield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(MINEFIELD).unwrap().name = "Minefield".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MINEFIELD).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(ctx: &mut Ctx, seat: u8) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(seat)
        );
        settle(ctx).unwrap();
    }

    fn minefield_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == MINEFIELD => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn deck(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn trash(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::TRASH, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_minefield_script_is_one_free_conquer_trigger_and_the_fixtures_blank_battlefield_is_not_it(
    ) {
        assert!(std::ptr::eq(
            crate::cards::script_of("Minefield").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(!ability.optional);
        assert!(ability.timing().is_none());
        assert_eq!(MILLED, 2);
        let blank = fixtures::table()
            .cards
            .into_iter()
            .find(|card| card.id == MINEFIELD)
            .unwrap();
        assert_eq!(blank.name, "Proving Grounds");
        assert!(
            crate::cards::script_of(&blank.name).is_none(),
            "the fixture's first battlefield is a blank every engine test conquers without a trigger"
        );
    }

    #[test]
    fn conquering_the_minefield_puts_the_conquerors_top_two_cards_into_their_trash_when_the_trigger_resolves(
    ) {
        let mut fixture = minefield();
        let mut ctx = fixture.ctx();
        assert_eq!(deck(&ctx, 0), MY_DECK);
        assert!(trash(&ctx, 0).is_empty());
        conquer(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(minefield_items(&ctx), [0]);
        assert_eq!(deck(&ctx, 0), MY_DECK, "not before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck(&ctx, 0), [20, 21]);
        assert_eq!(
            trash(&ctx, 0).len(),
            2,
            "the top two, which are the last two held"
        );
        assert!(trash(&ctx, 0).contains(&23) && trash(&ctx, 0).contains(&22));
        assert_eq!(deck(&ctx, 1).len(), 2, "the opponent's deck is untouched");
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Burned { seat: 0, .. }))
                .count(),
            2
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MINEFIELD}}} puts 2 of {{seat 0}}'s top 2 cards into their trash"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_one_card_deck_mills_what_it_has_and_the_opponent_conquering_mills_their_own_deck() {
        let mut fixture = minefield();
        fixture
            .table
            .cards
            .retain(|card| !MY_DECK[..3].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(deck(&ctx, 0), [23]);
        conquer(&mut ctx, 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(deck(&ctx, 0).is_empty());
        assert_eq!(trash(&ctx, 0), [23]);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MINEFIELD}}} puts 1 of {{seat 0}}'s top 2 cards into their trash"
        )));
        drop(ctx);
        let mut theirs = minefield();
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_contested(fixtures::BF1, Some(1));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        conquer(&mut ctx, 1);
        assert_eq!(minefield_items(&ctx), [1]);
        fixtures::pass_until_open(&mut ctx);
        assert!(deck(&ctx, 1).is_empty(), "both of their two cards");
        assert_eq!(trash(&ctx, 1).len(), 2);
        assert_eq!(deck(&ctx, 0), MY_DECK);
    }

    #[test]
    fn a_hold_is_not_a_conquer_a_conquer_elsewhere_is_not_here_and_it_is_not_an_affordance() {
        let mut fixture = minefield();
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(minefield_items(&ctx).is_empty());
        assert_eq!(deck(&ctx, 0), MY_DECK);
        assert_eq!(
            activate::activate(&mut ctx, 0, MINEFIELD, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        drop(ctx);
        let mut elsewhere = minefield();
        elsewhere.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        elsewhere
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        elsewhere.blob.set_holder(fixtures::BF2, None);
        elsewhere.blob.set_contested(fixtures::BF1, None);
        elsewhere.blob.set_contested(fixtures::BF2, Some(0));
        elsewhere.resolve();
        let mut ctx = elsewhere.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF2),
            Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(minefield_items(&ctx).is_empty());
        assert_eq!(deck(&ctx, 0), MY_DECK);
    }

    #[test]
    fn the_registered_minefield_mills_two_on_conquer_and_the_fixtures_blank_mills_nothing() {
        let mut fixture = minefield();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, 0);
        assert_eq!(minefield_items(&ctx), [0]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(deck(&ctx, 0), [20, 21]);
        drop(ctx);
        let mut blank = Fixture::enforced();
        blank.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        blank.blob.set_contested(fixtures::BF1, Some(0));
        blank.resolve();
        let mut ctx = blank.ctx();
        conquer(&mut ctx, 0);
        assert!(minefield_items(&ctx).is_empty());
        assert_eq!(deck(&ctx, 0), MY_DECK);
    }
}

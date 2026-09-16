use super::prelude::{done, draw, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const CARDS: usize = 1;

fn soar(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if draw(ctx, seat, CARDS) == 0 {
        ctx.narrate(format!("{{seat {seat}}} has nothing to draw"));
    }
    done()
}

pub static CARD: Card = unit("Cloud Drake", &[], &[play(&[], soar)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DRAKE: u32 = 90;

    fn drake(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Mind".into()],
            ..fixtures::unit(DRAKE, zone, 0, "Cloud Drake", 5)
        }
    }

    fn sky() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(drake(fixtures::HAND));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_untargeted_play_trigger() {
        assert!(std::ptr::eq(script_of("Cloud Drake").unwrap(), &CARD));
        assert_eq!(CARD.name, "Cloud Drake");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(CARDS, 1);
    }

    #[test]
    fn playing_it_draws_one_once_the_trigger_resolves() {
        let mut fixture = sky();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DRAKE).unwrap();
        assert_eq!(ctx.location(DRAKE), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAKE
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the draw waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_main_deck_the_trigger_draws_nothing_and_says_so() {
        let mut fixture = sky();
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DRAKE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing to draw".to_string()));
    }
}

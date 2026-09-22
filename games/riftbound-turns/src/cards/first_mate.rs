use super::prelude::{a_card, card_target, done, play, ready, unit};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const ANOTHER_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::NotSelf]);

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(target) = card_target(ctx, item, 0) {
        if ready(ctx, target) {
            ctx.narrate(format!("{{card {target}}} readies"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "First Mate",
    &[],
    &[play(
        &[a_card(ANOTHER_UNIT, "another unit to ready")],
        rally,
    )],
);

#[cfg(test)]
mod tests {
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Origin, PromptWhy};

    const MATE: u32 = 90;

    #[test]
    fn the_first_mate_offers_ready_and_exhausted_other_units_of_either_side_but_not_itself() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(MATE, fixtures::HAND, 0, "First Mate", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        let action = fixtures::move_action(MATE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, MATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}"],
            "the ready and exhausted other units on both sides are legal, but the mate is not"
        );
    }

    #[test]
    fn the_first_mate_leaves_a_ready_target_unchanged() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(MATE, fixtures::HAND, 0, "First Mate", 3));
        fixture.resolve();
        let action = fixtures::move_action(MATE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, MATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == fixtures::THEIR_UNIT
        )));
    }

    #[test]
    fn the_first_mate_readies_an_exhausted_target() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(MATE, fixtures::HAND, 0, "First Mate", 3));
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.resolve();
        let action = fixtures::move_action(MATE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, MATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == fixtures::THEIR_UNIT
        )));
    }

    #[test]
    fn the_first_mate_does_not_prompt_without_another_unit() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.kind.as_deref() != Some("Unit"));
        fixture
            .table
            .cards
            .push(fixtures::unit(MATE, fixtures::HAND, 0, "First Mate", 3));
        fixture.resolve();
        let action = fixtures::move_action(MATE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, MATE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(MATE), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
    }
}

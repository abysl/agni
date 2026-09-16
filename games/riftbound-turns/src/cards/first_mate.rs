use super::prelude::{a_card, card_target, done, play, ready, unit};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const ANOTHER_EXHAUSTED_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::NotSelf, Filter::Exhausted]);

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
        &[a_card(ANOTHER_EXHAUSTED_UNIT, "another unit to ready")],
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
    fn the_first_mate_readies_another_exhausted_unit_of_either_side() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(MATE, fixtures::HAND, 0, "First Mate", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
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
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 81}"],
            "the exhausted units on both sides, never the mate who just entered exhausted"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == fixtures::THEIR_UNIT
        )));
    }
}

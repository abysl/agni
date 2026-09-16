use super::prelude::{battlefield, done, draw, triggered};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;

fn draw_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = battlefield(
    "Grove of the God-Willow",
    &[],
    &[triggered(Trigger::Hold(Who::You), &[], draw_one)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle, triggers};
    use crate::state::ItemKind;
    use crate::Refusal;

    const GROVE: u32 = fixtures::GROUNDS;

    fn grove_held_by(seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(GROVE).unwrap().name = "Grove of the God-Willow".into();
        let unit = if seat == 0 {
            fixtures::VI
        } else {
            fixtures::THEIR_UNIT
        };
        fixture.table.card_mut(unit).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(seat));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GROVE).unwrap(), &CARD));
        fixture
    }

    fn grove_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == GROVE => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_grove_is_a_battlefield_with_one_free_hold_trigger_and_nothing_else() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Grove of the God-Willow").unwrap(),
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
    }

    #[test]
    fn holding_the_grove_draws_one_for_the_holder_once_the_trigger_resolves() {
        let mut fixture = grove_held_by(0);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(grove_items(&ctx), [0]);
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {GROVE}}} ability resolves")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_trigger_belongs_to_whoever_holds_and_only_fires_for_this_battlefield() {
        let mut fixture = grove_held_by(1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(1).len();
        assert_eq!(
            cleanup::score_holds(&mut ctx, 1),
            [fixtures::BF1, fixtures::BF2]
        );
        settle(&mut ctx).unwrap();
        assert_eq!(grove_items(&ctx), [1], "Rockfall Path adds no trigger");
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.hand_of(1).len(), hand + DRAWS);
        drop(ctx);
        let ctx = fixture.ctx();
        assert!(triggers::find(
            &ctx,
            &Event::Held {
                zone: fixtures::BF2,
                seat: 1,
                units: vec![fixtures::SPRITE],
            },
        )
        .iter()
        .all(|found| found.source != GROVE));
    }

    #[test]
    fn conquering_the_grove_draws_nothing_and_the_trigger_is_not_an_affordance() {
        let mut fixture = grove_held_by(0);
        fixture.blob.set_holder(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(grove_items(&ctx).is_empty(), "a conquest is not a hold");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(
            activate::activate(&mut ctx, 0, GROVE, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
    }
}

use super::prelude::{done, draw, on_conquer_me, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const CONQUER_DRAW: usize = 1;

fn evolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, CONQUER_DRAW);
    done()
}

pub static CARD: Card = unit(
    "Kai'Sa - Survivor",
    &[Keyword::Accelerate],
    &[on_conquer_me(&[], evolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const KAISA: u32 = 90;
    const TOP_OF_DECK: u32 = 23;

    fn kaisa(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Fury".into()],
            ..fixtures::unit(KAISA, zone, seat, "Kai'Sa - Survivor", 4)
        }
    }

    fn contested(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kaisa(zone, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_accelerate_and_one_conquer_trigger_of_her_own() {
        assert!(std::ptr::eq(script_of("Kai'Sa - Survivor").unwrap(), &CARD));
        assert_eq!(CARD.name, "Kai'Sa - Survivor");
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(conquer.targets.is_empty());
        assert!(!conquer.optional);
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
        assert_eq!(CONQUER_DRAW, 1);
    }

    #[test]
    fn conquering_with_her_draws_one_when_the_trigger_resolves() {
        let mut fixture = contested(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![KAISA]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KAISA
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "she asks nothing");
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + CONQUER_DRAW);
        assert!(ctx.effects.contains(&Effect::Move {
            card: TOP_OF_DECK,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP,
        }));
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(ctx.blob.seat(1).draws, 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_without_her_and_a_hold_with_her_draw_nothing() {
        let mut fixture = contested(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "Vi conquered; she stayed home");
        assert_eq!(ctx.hand_of(0).len(), hand);

        let mut fixture = contested(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a hold is not a conquer");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.points(0), 1);
    }
}

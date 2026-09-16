use super::prelude::{done, draw, on_hold_me, unit, with_statics};
use super::{Card, Flow, Item, Stage, Static};
use crate::engine::ctx::Ctx;

pub const HAND_AT_MOST: usize = 2;
pub const HOLD_DRAW: usize = 2;

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    ctx.hand_of(ctx.controller(me)).len() <= HAND_AT_MOST
}

fn breach(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let drawn = draw(ctx, item.controller, HOLD_DRAW);
    ctx.narrate(format!(
        "{{card {}}}: {{seat {}}} draws {drawn}",
        item.kind.source(),
        item.controller
    ));
    done()
}

pub static CARD: Card = with_statics(
    unit("Dunebreaker", &[], &[on_hold_me(&[], breach)]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DUNEBREAKER: u32 = 90;
    const ENERGY: u8 = 7;
    const MIGHT: u8 = 7;

    fn dunebreaker(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(DUNEBREAKER, zone, seat, "Dunebreaker", MIGHT)
        }
    }

    fn dunes(zone: u16, hand: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dunebreaker(zone, 0));
        let mut kept = 0;
        fixture.table.cards.retain(|card| {
            if card.zone != Some(fixtures::HAND) || card.owner != 0 || card.id == DUNEBREAKER {
                return true;
            }
            kept += 1;
            kept <= hand
        });
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DUNEBREAKER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_one_hold_trigger_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Dunebreaker").unwrap(), &CARD));
        assert_eq!(CARD.name, "Dunebreaker");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.condition.is_none());
    }

    #[test]
    fn the_seam_reads_two_or_fewer_cards_in_its_controllers_hand() {
        let mut fixture = dunes(fixtures::CHAIN, 2);
        let ctx = fixture.ctx();
        assert_eq!(ctx.hand_of(0).len(), 2);
        assert!(enters_ready(&ctx, DUNEBREAKER));
        drop(ctx);
        let mut fixture = dunes(fixtures::CHAIN, 0);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DUNEBREAKER), "an empty hand counts");
        drop(ctx);
        let mut fixture = dunes(fixtures::CHAIN, 3);
        let ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, DUNEBREAKER), "three is one too many");
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(
            enters_ready(&ctx, fixtures::THEIR_UNIT),
            "read from the entering unit's controller, whose hand holds one"
        );
        drop(ctx);
        let mut fixture = dunes(fixtures::HAND, 2);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, DUNEBREAKER),
            "in hand it still counts itself; played, it has left the hand"
        );
    }

    #[test]
    fn holding_with_it_draws_two_after_the_trigger_resolves() {
        let mut fixture = dunes(fixtures::BF1, 4);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let before = ctx.hand_of(0).len();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![DUNEBREAKER]
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DUNEBREAKER
        ));
        assert_eq!(ctx.hand_of(0).len(), before, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), before + HOLD_DRAW);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            HOLD_DRAW
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DUNEBREAKER}}}: {{seat 0}} draws 2")));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_opponents_hold_or_a_hold_elsewhere_is_not_its_own() {
        let mut fixture = dunes(fixtures::BASE, 4);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi holds, it sits in base");
        drop(ctx);
        let mut fixture = dunes(fixtures::BASE, 4);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the Sprite's hold is not its own"
        );
    }

    #[test]
    fn played_with_two_cards_left_in_hand_it_enters_ready() {
        let mut fixture = dunes(fixtures::HAND, 2);
        for rune in [100, 101, 102, 103] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUNEBREAKER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(DUNEBREAKER));
        assert_eq!(ctx.hand_of(0).len(), 2);
        assert!(!ctx.card(DUNEBREAKER).unwrap().exhausted);
    }
}

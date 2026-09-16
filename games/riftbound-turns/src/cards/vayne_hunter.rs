use super::prelude::{
    bounce, done, on_conquer_me, optional, unit, with_cost, with_statics, ONE_ENERGY,
};
use super::{Card, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 3;

pub fn an_opponent_controls_a_battlefield(ctx: &Ctx, me: u32) -> bool {
    let mine = ctx.controller(me);
    ctx.zones
        .battlefields
        .iter()
        .filter_map(|zone| ctx.blob.holder(*zone))
        .any(|holder| holder != mine)
}

fn tumble_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if bounce(ctx, me) {
        ctx.narrate(format!("{{card {me}}} returns to her owner's hand"));
    }
    done()
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    an_opponent_controls_a_battlefield(ctx, me)
}

pub static CARD: Card = with_statics(
    unit(
        "Vayne - Hunter",
        &[Keyword::Assault(ASSAULT)],
        &[optional(with_cost(
            on_conquer_me(&[], tumble_home),
            ONE_ENERGY,
        ))],
    ),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const VAYNE: u32 = 90;

    fn vayne(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(VAYNE, zone, seat, "Vayne - Hunter", 2)
        }
    }

    fn hunt(ready_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vayne(fixtures::BF1, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut count = 0;
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = count >= ready_runes;
                count += 1;
            }
        }
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
    fn the_script_prints_assault_three_and_one_optional_paid_conquer_trigger() {
        assert!(std::ptr::eq(script_of("Vayne - Hunter").unwrap(), &CARD));
        assert_eq!(CARD.name, "Vayne - Hunter");
        assert_eq!(CARD.keywords, [Keyword::Assault(3)]);
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(conquer.optional, "you may pay");
        assert_eq!(conquer.cost, Some(ONE_ENERGY));
        assert!(conquer.targets.is_empty());
        assert!(conquer.condition.is_none());
    }

    #[test]
    fn conquering_asks_for_one_energy_and_paying_returns_her_to_hand_when_the_trigger_resolves() {
        let mut fixture = hunt(3);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        let item = match ctx.blob.why {
            Some(PromptWhy::OptionalCost { item, .. }) => item,
            other => panic!("392.2 · the may is the cost confirm, not {other:?}"),
        };
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {VAYNE}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "one rune for the energy"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].id, item);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VAYNE
        ));
        assert_eq!(
            ctx.location(VAYNE),
            Some(Location::Battlefield(fixtures::BF1)),
            "the return waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(VAYNE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(VAYNE).unwrap().seat, 0, "her owner's hand");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VAYNE}}} returns to her owner's hand")));
        assert_eq!(ctx.points(0), 1, "the point she conquered stays");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_keeps_her_at_the_battlefield() {
        let mut fixture = hunt(3);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "the declined trigger is removed");
        assert_eq!(ctx.ready_runes_of(0).len(), ready);
        assert_eq!(
            ctx.location(VAYNE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VAYNE}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn without_a_ready_rune_the_trigger_is_removed_without_asking() {
        let mut fixture = hunt(0);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no confirm for a cost that can't be paid"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(VAYNE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VAYNE}}} trigger is removed · its cost can't be paid"
        )));
        assert_eq!(ctx.points(0), 1);
    }

    #[test]
    fn another_units_conquer_is_not_hers() {
        let mut fixture = hunt(3);
        fixture.table.card_mut(VAYNE).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(VAYNE), Some(Location::Base(0)));
    }

    #[test]
    fn the_seam_reads_whether_an_opponent_holds_any_battlefield() {
        let mut fixture = hunt(3);
        fixture.blob.set_holder(fixtures::BF2, None);
        let ctx = fixture.ctx();
        assert!(!an_opponent_controls_a_battlefield(&ctx, VAYNE));
        let mut fixture = hunt(3);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, None);
        let ctx = fixture.ctx();
        assert!(
            !an_opponent_controls_a_battlefield(&ctx, VAYNE),
            "her own controller's battlefield does not count"
        );
        let mut fixture = hunt(3);
        let ctx = fixture.ctx();
        assert!(
            an_opponent_controls_a_battlefield(&ctx, VAYNE),
            "the fixture's seat 1 holds {{zone 10}}"
        );
    }

    #[test]
    fn played_while_an_opponent_controls_a_battlefield_she_enters_ready() {
        let mut fixture = hunt(3);
        fixture.table.card_mut(VAYNE).unwrap().zone = Some(fixtures::HAND);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, VAYNE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == VAYNE
        )));
        assert_eq!(ctx.location(VAYNE), Some(Location::Base(0)));
        assert!(
            !ctx.card(VAYNE).unwrap().exhausted,
            "seat 1 holds {{zone 10}}, so she enters ready"
        );
    }
}

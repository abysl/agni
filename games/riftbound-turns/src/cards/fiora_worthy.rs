use super::prelude::{done, ready, trigger_subject, unit};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage};
use crate::engine::ctx::Ctx;

pub const ORDER: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};

pub fn ready_the_unit_that_became_mighty(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if !ctx.is_unit(unit) || !ctx.on_board(unit) || ctx.controller(unit) != item.controller {
        return done();
    }
    if ready(ctx, unit) {
        ctx.narrate(format!(
            "{{card {}}} readies {{card {unit}}}",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit("Fiora - Worthy", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::became_mighty;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const FIORA: u32 = 90;
    const TIRED: u32 = 91;
    const ORDER_RUNE: u32 = 46;

    fn fiora() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(FIORA, fixtures::BASE, 0, "Fiora - Worthy", 3)
        }
    }

    fn salon() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fiora());
        let mut tired = fixtures::unit(TIRED, fixtures::BF1, 0, "Pit Rookie", 4);
        tired.exhausted = true;
        fixture.table.cards.push(tired);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_on(unit: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: FIORA,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(TargetRef::Card(unit));
        item
    }

    #[test]
    fn the_script_stays_a_stub_until_the_engine_raises_a_became_mighty_event() {
        assert!(std::ptr::eq(script_of("Fiora - Worthy").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(
            ORDER,
            Cost {
                energy: 0,
                power: &[Power::Domain(Domain::Order)]
            }
        );
    }

    #[test]
    fn a_unit_becomes_mighty_only_when_its_might_crosses_from_below_five_to_five_or_more() {
        assert!(became_mighty(4, 5), "709 · 4 with +1");
        assert!(became_mighty(3, 6));
        assert!(!became_mighty(5, 6), "already Mighty");
        assert!(!became_mighty(4, 4));
        assert!(!became_mighty(6, 4), "losing Might is not becoming Mighty");
    }

    #[test]
    fn the_run_readies_the_friendly_unit_the_trigger_is_about() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ready_the_unit_that_became_mighty(&mut ctx, &trigger_on(TIRED), Stage(0)),
            Flow::Done
        );
        assert!(!ctx.card(TIRED).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == TIRED
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FIORA}}} readies {{card {TIRED}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_unit_an_enemy_unit_and_a_unit_off_the_board_are_left_alone() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        ready_the_unit_that_became_mighty(&mut ctx, &trigger_on(fixtures::VI), Stage(0));
        ready_the_unit_that_became_mighty(&mut ctx, &trigger_on(fixtures::THEIR_UNIT), Stage(0));
        ready_the_unit_that_became_mighty(&mut ctx, &trigger_on(fixtures::HAND_UNIT), Stage(0));
        let mut blank = trigger_on(TIRED);
        blank.subject = None;
        ready_the_unit_that_became_mighty(&mut ctx, &blank, Stage(0));
        assert!(ctx.effects.is_empty());
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "a unit you control, not an enemy's"
        );
        assert!(ctx.card(TIRED).unwrap().exhausted);
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::might raises no BecameMighty event (709) and Trigger has no BecameMighty(Who); when a friendly unit crosses to 5 Might Fiora must offer to pay an Order rune to ready it, as optional(with_cost(triggered(BecameMighty(Who::Friendly), &[], ready_the_unit_that_became_mighty), ORDER))"]
    fn a_friendly_unit_crossing_to_five_might_offers_an_order_rune_to_ready_it() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        let pump = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &pump, TIRED, 1, None);
        assert_eq!(ctx.current_might(TIRED), 5);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(TIRED).unwrap().exhausted);
        assert!(!ctx.runes_of(0).iter().any(|rune| rune.id == ORDER_RUNE));
        might_this_turn(&mut ctx, &pump, TIRED, 1, None);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "5 to 6 is not becoming Mighty");
    }
}

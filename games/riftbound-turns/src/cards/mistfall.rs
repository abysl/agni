use super::prelude::{done, gear, ready, trigger_subject};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage};
use crate::engine::ctx::Ctx;

pub const BODY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub fn ready_the_buffed_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
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

pub static CARD: Card = gear("Mistfall", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const MISTFALL: u32 = 90;
    const TIRED: u32 = 91;

    fn mistfall() -> CardInfo {
        let mut card = fixtures::gear(MISTFALL, fixtures::BASE, 0, "Mistfall", 3);
        card.domain = vec!["Body".into()];
        card
    }

    fn mist() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mistfall());
        let mut tired = fixtures::unit(TIRED, fixtures::BF1, 0, "Pit Rookie", 2);
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
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_on(unit: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: MISTFALL,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(TargetRef::Card(unit));
        item
    }

    #[test]
    fn the_script_stays_a_stub_until_the_engine_raises_a_buffed_event() {
        assert!(std::ptr::eq(script_of("Mistfall").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(
            BODY,
            Cost {
                energy: 0,
                power: &[Power::Domain(Domain::Body)]
            }
        );
    }

    #[test]
    fn the_run_readies_the_buffed_friendly_unit_the_trigger_is_about() {
        let mut fixture = mist();
        let mut ctx = fixture.ctx();
        ctx.buff(TIRED);
        assert_eq!(
            ready_the_buffed_unit(&mut ctx, &trigger_on(TIRED), Stage(0)),
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
            .contains(&format!("{{card {MISTFALL}}} readies {{card {TIRED}}}")));
        assert!(ctx.is_buffed(TIRED), "readied, the buff stays");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_unit_an_enemy_unit_and_a_unit_off_the_board_are_left_alone() {
        let mut fixture = mist();
        let mut ctx = fixture.ctx();
        ready_the_buffed_unit(&mut ctx, &trigger_on(fixtures::VI), Stage(0));
        ready_the_buffed_unit(&mut ctx, &trigger_on(fixtures::THEIR_UNIT), Stage(0));
        ready_the_buffed_unit(&mut ctx, &trigger_on(fixtures::HAND_UNIT), Stage(0));
        let mut blank = trigger_on(TIRED);
        blank.subject = None;
        ready_the_buffed_unit(&mut ctx, &blank, Stage(0));
        assert!(ctx.effects.is_empty());
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "702 · a buff on an enemy unit is not one you gave"
        );
        assert!(ctx.card(TIRED).unwrap().exhausted);
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::buff raises no Buffed event and Trigger has no Buffed(Who); when you buff a friendly unit Mistfall must offer to pay a Body rune and exhaust itself to ready that unit"]
    fn buffing_a_friendly_unit_offers_a_body_rune_and_the_exhaust_to_ready_it() {
        let mut fixture = mist();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(TIRED));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(MISTFALL).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(TIRED).unwrap().exhausted);
        assert!(!ctx.runes_of(0).iter().any(|rune| rune.id == 46));
    }
}

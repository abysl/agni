use super::prelude::{done, ready, trigger_subject, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub fn ready_me_when_buffed(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if trigger_subject(item) != Some(me) || !ctx.on_board(me) {
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = unit("Simian Ancestor", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const ANCESTOR: u32 = 90;

    fn ancestor(exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            exhausted,
            domain: vec!["Calm".into()],
            ..fixtures::unit(ANCESTOR, fixtures::BF1, 0, "Simian Ancestor", 5)
        }
    }

    fn grove(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ancestor(exhausted));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_on(unit: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: ANCESTOR,
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
        assert!(std::ptr::eq(script_of("Simian Ancestor").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn the_run_readies_him_when_the_trigger_is_about_him() {
        let mut fixture = grove(true);
        let mut ctx = fixture.ctx();
        ctx.buff(ANCESTOR);
        assert_eq!(
            ready_me_when_buffed(&mut ctx, &trigger_on(ANCESTOR), Stage(0)),
            Flow::Done
        );
        assert!(!ctx.card(ANCESTOR).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == ANCESTOR
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ANCESTOR}}} readies")));
        assert!(ctx.is_buffed(ANCESTOR), "readied, the buff stays");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_buff_on_another_unit_a_ready_ancestor_and_one_off_the_board_ready_nothing() {
        let mut fixture = grove(true);
        let mut ctx = fixture.ctx();
        ready_me_when_buffed(&mut ctx, &trigger_on(fixtures::VI), Stage(0));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(
            ctx.card(ANCESTOR).unwrap().exhausted,
            "you buffed Vi, not him"
        );
        let mut blank = trigger_on(ANCESTOR);
        blank.subject = None;
        ready_me_when_buffed(&mut ctx, &blank, Stage(0));
        assert!(ctx.effects.is_empty());
        drop(ctx);
        let mut awake = grove(false);
        let mut ctx = awake.ctx();
        ready_me_when_buffed(&mut ctx, &trigger_on(ANCESTOR), Stage(0));
        assert!(ctx.effects.is_empty(), "already ready");
        drop(ctx);
        let mut gone = grove(true);
        gone.table.card_mut(ANCESTOR).unwrap().zone = Some(fixtures::TRASH);
        gone.resolve();
        let mut ctx = gone.ctx();
        ready_me_when_buffed(&mut ctx, &trigger_on(ANCESTOR), Stage(0));
        assert!(ctx.effects.is_empty());
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::buff raises no Buffed event and Trigger has no Buffed(Who); when you buff him Simian Ancestor must ready himself, as triggered(Buffed(Who::Me), &[], ready_me_when_buffed)"]
    fn buffing_him_readies_him_when_the_trigger_resolves() {
        let mut fixture = grove(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(ANCESTOR));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ANCESTOR
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(ANCESTOR).unwrap().exhausted);
        assert!(
            !ctx.buff(ANCESTOR),
            "426.1.b.1 · a second buff does not land, so nothing triggers again"
        );
    }
}

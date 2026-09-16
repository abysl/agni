use super::prelude::{done, on_conquer_me, once_each_turn, ready, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn relentless(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Lucian - Merciless",
    &[Keyword::Weaponmaster],
    &[once_each_turn(on_conquer_me(&[], relentless))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{move_unit, Location, Moved};
    use crate::cards::script_of;
    use crate::cards::{Once, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::{ItemKind, FLAG_ONCE_USED};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const LUCIAN: u32 = 90;
    const PLAIN: u32 = 54;

    fn lucian(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(3),
            exhausted,
            domain: vec!["Body".into()],
            ..fixtures::unit(LUCIAN, zone, 0, "Lucian - Merciless", 3)
        }
    }

    fn contesting(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lucian(zone, true));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx, zone: u16) {
        assert_eq!(
            cleanup::establish(ctx, zone),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_weaponmaster_and_one_conquer_trigger_of_his_own_once_a_turn() {
        assert!(std::ptr::eq(
            script_of("Lucian - Merciless").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Lucian - Merciless");
        assert_eq!(CARD.keywords, [Keyword::Weaponmaster]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(
            CARD.abilities.len(),
            1,
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert_eq!(conquer.once, Once::PerTurn, "the first time each turn");
        assert!(conquer.targets.is_empty());
        assert!(!conquer.optional);
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
    }

    #[test]
    fn his_first_conquer_of_the_turn_readies_him_when_the_trigger_resolves() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, fixtures::BF1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![LUCIAN]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LUCIAN
        ));
        assert!(ctx.blob.prompt.is_none(), "he asks nothing");
        assert!(
            ctx.card(LUCIAN).unwrap().exhausted,
            "the ready waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(LUCIAN).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::ready(LUCIAN)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == LUCIAN
        )));
        assert!(ctx.blob.log.contains(&format!("{{card {LUCIAN}}} readies")));
        assert!(ctx.has_flag(LUCIAN, FLAG_ONCE_USED));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn his_second_conquer_in_the_same_turn_readies_nothing() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, fixtures::BF1);
        resolve_chain(&mut ctx);
        assert!(!ctx.card(LUCIAN).unwrap().exhausted);
        ctx.exhaust(LUCIAN);
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                LUCIAN,
                Location::Battlefield(fixtures::BF3)
            ),
            Some(Moved::Moved)
        );
        conquer(&mut ctx, fixtures::BF3);
        assert_eq!(ctx.points(0), 2, "the second conquer still scores");
        assert!(
            ctx.blob.chain.is_empty(),
            "the first time each turn has passed"
        );
        assert!(ctx.card(LUCIAN).unwrap().exhausted);
    }

    #[test]
    fn a_conquer_without_him_and_a_hold_with_him_ready_nothing() {
        let mut fixture = contesting(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, fixtures::BF1);
        assert!(ctx.blob.chain.is_empty(), "Vi conquered; he stayed home");
        assert!(ctx.card(LUCIAN).unwrap().exhausted);
        drop(ctx);
        let mut fixture = contesting(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a hold is not a conquer");
        assert!(ctx.card(LUCIAN).unwrap().exhausted);
        assert_eq!(ctx.points(0), 1);
    }
}

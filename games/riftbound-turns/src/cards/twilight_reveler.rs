use super::prelude::{
    a_card, card_target, done, on_attack, ready, unit, ANOTHER_FRIENDLY_UNIT_THAN_ME,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const TARGET: TargetSpec = a_card(
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
    "another friendly unit to ready",
);

fn revel(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ready(ctx, unit) {
        ctx.narrate(format!(
            "{{card {}}} readies {{card {unit}}}",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit("Twilight Reveler", &[], &[on_attack(&[TARGET], revel)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const REVELER: u32 = 90;
    const TIRED: u32 = 91;

    fn reveler() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Fury".into()],
            ..fixtures::unit(REVELER, fixtures::BF1, 0, "Twilight Reveler", 3)
        }
    }

    fn revel_hall() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(reveler());
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut tired = fixtures::unit(TIRED, fixtures::BF2, 0, "Tired", 2);
        tired.exhausted = true;
        fixture.table.cards.push(tired);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REVELER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(REVELER));
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_unit_with_one_attack_trigger_aimed_at_another_friendly_unit() {
        assert!(std::ptr::eq(script_of("Twilight Reveler").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!((TARGET.min, TARGET.max), (1, 1));
        assert_eq!(TARGET.filter, ANOTHER_FRIENDLY_UNIT_THAN_ME);
    }

    #[test]
    fn attacking_offers_every_other_friendly_unit_anywhere_and_readies_the_pick() {
        let mut fixture = revel_hall();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {TIRED}}}")
            ],
            "friendly units at the base and at another battlefield · not himself, not Jinx"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == REVELER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(TIRED)]);
        assert!(
            ctx.card(TIRED).unwrap().exhausted,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(TIRED).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == TIRED
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REVELER}}} readies {{card {TIRED}}}")));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted, "only the pick");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_reveler_alone_has_no_target_and_a_pick_that_left_readies_nothing() {
        let mut lonely = revel_hall();
        lonely
            .table
            .cards
            .retain(|card| ![fixtures::VI, TIRED].contains(&card.id));
        lonely.resolve();
        let mut ctx = lonely.ctx();
        attacks(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "no other friendly unit · the trigger is removed"
        );
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "an enemy is never readied"
        );
        drop(ctx);

        let mut fixture = revel_hall();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        ctx.table.card_mut(TIRED).unwrap().zone = Some(fixtures::TRASH);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
    }
}

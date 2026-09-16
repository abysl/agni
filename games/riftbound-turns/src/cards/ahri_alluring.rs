use super::prelude::{done, on_hold_me, score_point, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn allure(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit("Ahri - Alluring", &[], &[on_hold_me(&[], allure)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const AHRI: u32 = 90;

    fn ahri(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(AHRI, zone, seat, "Ahri - Alluring", 4)
        }
    }

    fn shrine(zone: u16, holder: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ahri(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, holder);
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_champion_whose_only_ability_is_her_own_hold() {
        assert!(std::ptr::eq(script_of("Ahri - Alluring").unwrap(), &CARD));
        assert_eq!(CARD.name, "Ahri - Alluring");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none());
        assert!(hold.condition.is_none());
    }

    #[test]
    fn holding_with_ahri_scores_the_hold_point_and_a_second_when_her_trigger_resolves() {
        let mut fixture = shrine(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![AHRI]
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == AHRI
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "she asks nothing");
        assert_eq!(ctx.points(0), 1, "the hold itself, the trigger waits");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2);
        assert_eq!(ctx.points(1), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_a_hold_elsewhere_is_not_hers() {
        let mut fixture = shrine(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "469.2 · conquering is not holding"
        );
        assert_eq!(ctx.points(0), 1);

        let mut fixture = shrine(fixtures::BASE, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "Vi holds the battlefield; Ahri sits in the base"
        );
        assert_eq!(ctx.points(0), 1);
    }

    #[test]
    fn the_opponents_hold_of_their_own_battlefield_scores_nothing_for_her() {
        let mut fixture = shrine(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(1), 1);
        assert_eq!(ctx.points(0), 0);
    }
}

use super::prelude::{done, excess_damage_assigned_in_my_attack, on_conquer_me, unit, when};
use super::trove_golem::play_golds;
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;

pub const EXCESS_FOR_THE_GOLDS: u8 = 3;
pub const GOLDS: usize = 2;

fn conquered_after_an_attack_with_three_excess(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    ctx.controller(source.card) == *seat
        && excess_damage_assigned_in_my_attack(ctx, *seat, *zone)
            .is_some_and(|excess| excess >= EXCESS_FOR_THE_GOLDS)
}

fn plunder(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_golds(ctx, item.controller, GOLDS);
    done()
}

pub static CARD: Card = unit(
    "Yeti Brawler",
    &[],
    &[when(
        on_conquer_me(&[], plunder),
        conquered_after_an_attack_with_three_excess,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::trove_golem::tests::golds_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, showdown};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const YETI: u32 = 90;
    const DEFENDER: u32 = 91;

    fn yeti(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(YETI, zone, seat, "Yeti Brawler", 6)
        }
    }

    fn contested() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(yeti(fixtures::BF1, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn against(defender_might: u8) -> Fixture {
        let mut fixture = contested();
        fixture.table.cards.push(fixtures::unit(
            DEFENDER,
            fixtures::BF1,
            1,
            "Defender",
            defender_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        settle(ctx).unwrap();
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: YETI,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_conditional_conquer_trigger() {
        assert!(std::ptr::eq(script_of("Yeti Brawler").unwrap(), &CARD));
        assert_eq!(CARD.name, "Yeti Brawler");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(
            conquer.condition.is_some(),
            "after an attack, with 3 or more excess"
        );
        assert!(!conquer.optional);
        assert!(conquer.targets.is_empty());
        assert!(conquer.cost.is_none());
        assert_eq!(EXCESS_FOR_THE_GOLDS, 3);
        assert_eq!(GOLDS, 2);
    }

    #[test]
    fn the_run_plays_two_exhausted_golds_into_his_controllers_base() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        let next = ctx.table.next_id;
        assert_eq!(plunder(&mut ctx, &trigger_item(7), Stage(0)), Flow::Done);
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds, [next, next + 1]);
        for gold in golds {
            assert!(ctx.is_gear(gold) && ctx.is_token(gold));
            assert!(ctx.card(gold).unwrap().exhausted);
            assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        }
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_that_follows_no_attack_plays_no_gold() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "no attack, no excess, no trigger"
        );
        assert_eq!(ctx.points(0), 1);
        assert!(golds_of(&ctx, 0).is_empty());
    }

    #[test]
    fn one_excess_damage_after_an_attack_plays_no_gold() {
        let mut fixture = against(5);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(DEFENDER).unwrap().zone,
            Some(fixtures::TRASH),
            "six on five is lethal"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(1)
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(golds_of(&ctx, 0).is_empty());
    }

    #[test]
    fn four_excess_damage_after_his_attack_plays_two_golds_and_two_excess_does_not() {
        let mut fixture = against(2);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(4)
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == YETI
        ));
        assert!(golds_of(&ctx, 0).is_empty(), "the Golds wait for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(golds_of(&ctx, 0).len(), GOLDS);
        drop(ctx);
        let mut fixture = against(4);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(2)
        );
        assert!(ctx.blob.chain.is_empty(), "two excess is not three");
        assert!(golds_of(&ctx, 0).is_empty());
    }
}

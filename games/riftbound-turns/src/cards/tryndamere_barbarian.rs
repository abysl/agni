use super::prelude::{
    done, excess_damage_assigned_in_my_attack, on_conquer_me, score_point, unit, when,
};
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;

pub const EXCESS_FOR_THE_POINT: u8 = 5;

fn conquered_after_an_attack_with_five_excess(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    ctx.controller(source.card) == *seat
        && excess_damage_assigned_in_my_attack(ctx, *seat, *zone)
            .is_some_and(|excess| excess >= EXCESS_FOR_THE_POINT)
}

fn battle_fury(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Tryndamere - Barbarian",
    &[],
    &[when(
        on_conquer_me(&[], battle_fury),
        conquered_after_an_attack_with_five_excess,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, showdown};
    use agni_plugin_sdk::table::CardInfo;

    const TRYNDAMERE: u32 = 90;
    const DEFENDER: u32 = 91;

    fn tryndamere(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(TRYNDAMERE, zone, seat, "Tryndamere - Barbarian", 8)
        }
    }

    fn contested() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tryndamere(fixtures::BF1, 0));
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
        assert!(
            ctx.blob.showdown.is_none(),
            "one candidate a side needs no prompt"
        );
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_conditional_conquer_trigger() {
        assert!(std::ptr::eq(
            script_of("Tryndamere - Barbarian").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Tryndamere - Barbarian");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(
            conquer.condition.is_some(),
            "after an attack, with 5 or more excess"
        );
        assert!(!conquer.optional);
        assert!(conquer.targets.is_empty());
        assert!(conquer.cost.is_none());
        assert_eq!(EXCESS_FOR_THE_POINT, 5);
    }

    #[test]
    fn a_conquer_that_follows_no_attack_scores_only_the_conquer() {
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
    }

    #[test]
    fn one_excess_damage_after_an_attack_is_not_five() {
        let mut fixture = against(7);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 8 might vs defenders 7 might"));
        assert_eq!(
            ctx.card(DEFENDER).unwrap().zone,
            Some(fixtures::TRASH),
            "eight on seven is lethal"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.points(0), 1, "the conquer alone");
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_record_reads_the_attackers_excess_and_nothing_for_the_other_seat_or_zone() {
        let mut fixture = against(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(5)
        );
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 1, fixtures::BF1),
            Some(0),
            "the defender's three on eight is no excess"
        );
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF2),
            None
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} assigned 5 excess damage at {{zone {}}}",
            fixtures::BF1
        )));
    }

    #[test]
    fn five_excess_damage_after_his_attack_scores_a_second_point_when_the_trigger_resolves() {
        let mut fixture = against(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(DEFENDER).unwrap().zone,
            Some(fixtures::TRASH),
            "eight on three: five excess"
        );
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(5)
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.points(0), 1, "the conquer's point; the trigger waits");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.points(0), 2);
        assert!(ctx.fault.is_none());
    }
}

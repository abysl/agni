use super::prelude::{
    card_target, deal, done, excess_damage_assigned_in_my_attack, location_of, on_conquer_me,
    target, unit, when, Location, ENEMY_UNIT,
};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const EXCESS_TO_DEAL: u8 = 5;

pub const TARGET: TargetSpec = target(
    ENEMY_UNIT,
    0,
    1,
    TargetKind::Card,
    "an enemy unit to deal the excess combat damage to",
);

pub fn excess_to_deal(ctx: &Ctx, seat: u8, zone: u16) -> Option<u8> {
    excess_damage_assigned_in_my_attack(ctx, seat, zone).filter(|excess| *excess >= EXCESS_TO_DEAL)
}

fn conquered_after_an_attack_with_five_excess(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    ctx.controller(source.card) == *seat && excess_to_deal(ctx, *seat, *zone).is_some()
}

pub fn ricochet(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(Location::Battlefield(zone)) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} is no longer at the battlefield"));
        return done();
    };
    let Some(amount) = excess_to_deal(ctx, item.controller, zone) else {
        ctx.narrate(format!("{{card {me}}}: no excess damage is recorded"));
        return done();
    };
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, amount) {
            ctx.narrate(format!(
                "{{card {me}}} deals {amount} to {{card {unit}}} · the excess of her attack"
            ));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Sivir - Ambitious",
    &[Keyword::Deflect(2)],
    &[when(
        on_conquer_me(&[TARGET], ricochet),
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
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const SIVIR: u32 = 90;
    const DEFENDER: u32 = 91;
    const BYSTANDER: u32 = 92;

    fn sivir(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(3),
            domain: vec!["Body".into()],
            ..fixtures::unit(SIVIR, zone, 0, "Sivir - Ambitious", 7)
        }
    }

    fn contested() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sivir(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BYSTANDER, fixtures::BASE, 1, "Jinx", 6));
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

    fn trigger_with(target: Option<u32>) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: SIVIR,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets.extend(target.map(TargetRef::Card));
        item
    }

    #[test]
    fn the_script_prints_deflect_two_and_one_conditional_conquer_trigger_with_a_may_target() {
        assert!(std::ptr::eq(script_of("Sivir - Ambitious").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sivir - Ambitious");
        assert_eq!(CARD.keywords, [Keyword::Deflect(2)]);
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
        assert!(!conquer.optional, "the may is the 0-of-1 target");
        assert_eq!(conquer.targets, [TARGET]);
        assert_eq!((TARGET.min, TARGET.max), (0, 1));
        assert_eq!(TARGET.filter, ENEMY_UNIT);
        assert_eq!(EXCESS_TO_DEAL, 5);
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
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.damage_on(BYSTANDER), 0);
    }

    #[test]
    fn two_excess_damage_after_an_attack_is_not_five() {
        let mut fixture = against(5);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 7 might vs defenders 5 might"));
        assert_eq!(ctx.card(DEFENDER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.points(0), 1, "the conquer alone");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(excess_to_deal(&ctx, 0, fixtures::BF1), None);
    }

    #[test]
    fn the_run_deals_nothing_without_a_recorded_excess_and_nothing_without_a_target() {
        let mut fixture = against(5);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(2)
        );
        assert_eq!(
            ricochet(&mut ctx, &trigger_with(Some(BYSTANDER)), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.damage_on(BYSTANDER), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SIVIR}}}: no excess damage is recorded")));
        drop(ctx);
        let mut home = contested();
        home.table.card_mut(SIVIR).unwrap().zone = Some(fixtures::BASE);
        home.resolve();
        let mut ctx = home.ctx();
        ricochet(&mut ctx, &trigger_with(Some(BYSTANDER)), Stage(0));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SIVIR}}} is no longer at the battlefield")));
        assert_eq!(ctx.damage_on(BYSTANDER), 0);
        ricochet(&mut ctx, &trigger_with(None), Stage(0));
        assert_eq!(ctx.damage_on(BYSTANDER), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn five_excess_damage_after_her_attack_may_be_dealt_to_an_enemy_unit() {
        let mut fixture = against(2);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(DEFENDER).unwrap().zone,
            Some(fixtures::TRASH),
            "seven on two: five excess"
        );
        assert_eq!(excess_to_deal(&ctx, 0, fixtures::BF1), Some(5));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.points(0), 1, "the conquer's point; the trigger waits");
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BYSTANDER}}}"),
                "skip".to_string()
            ],
            "every enemy unit on the board, and the may"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BYSTANDER}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.damage_on(BYSTANDER), 5);
        assert!(ctx.on_board(BYSTANDER), "five on six is not lethal");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SIVIR}}} deals 5 to {{card {BYSTANDER}}} · the excess of her attack"
        )));
        assert!(ctx.fault.is_none());
    }
}

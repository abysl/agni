use super::prelude::{a_card, card_target, deal, done, on_conquer_me, unit, when};
use super::{Card, Event, Filter, Flow, Item, Keyword, Source, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_IN_A_BASE: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::InBase]);

pub const TARGET: TargetSpec = a_card(
    ENEMY_UNIT_IN_A_BASE,
    "an enemy unit in a base to deal damage equal to my Might",
);

pub fn open_before_the_conquer(_ctx: &Ctx, _event: &Event) -> Option<bool> {
    None
}

fn conquered_an_open_battlefield(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Conquered { seat, .. } = event else {
        return false;
    };
    ctx.controller(source.card) == *seat
        && open_before_the_conquer(ctx, event).is_some_and(|open| open)
}

pub fn spirit_cleave(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Ok(amount) = u8::try_from(ctx.current_might(me).max(0)) else {
        return done();
    };
    if amount == 0 {
        ctx.narrate(format!("{{card {me}}} has no Might to deal"));
        return done();
    }
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!(
            "{{card {me}}} deals {amount} to {{card {unit}}} · equal to his Might"
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Yone - Blademaster",
    &[Keyword::Weaponmaster],
    &[when(
        on_conquer_me(&[TARGET], spirit_cleave),
        conquered_an_open_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, targets};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const YONE: u32 = 90;
    const BASED: u32 = 91;
    const AFIELD: u32 = 92;

    fn yone(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(YONE, zone, 0, "Yone - Blademaster", 5)
        }
    }

    fn contesting(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(yone(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(BASED, fixtures::BASE, 1, "Jinx", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(AFIELD, fixtures::BF3, 1, "Jinx", 4));
        fixture.blob.set_holder(fixtures::BF3, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_with(target: Option<u32>) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: YONE,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets.extend(target.map(TargetRef::Card));
        item
    }

    #[test]
    fn the_script_prints_weaponmaster_and_a_conditional_conquer_trigger_aimed_at_a_base() {
        assert!(std::ptr::eq(
            script_of("Yone - Blademaster").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Yone - Blademaster");
        assert_eq!(CARD.keywords, [Keyword::Weaponmaster]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(
            CARD.abilities.len(),
            1,
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(conquer.condition.is_some(), "an open battlefield");
        assert!(!conquer.optional);
        assert_eq!(conquer.targets, [TARGET]);
        assert_eq!((TARGET.min, TARGET.max), (1, 1));
        assert_eq!(TARGET.filter, ENEMY_UNIT_IN_A_BASE);
    }

    #[test]
    fn the_target_is_an_enemy_unit_in_a_base_never_one_at_a_battlefield_nor_ours() {
        let mut fixture = contesting(fixtures::BF1);
        let ctx = fixture.ctx();
        let item = trigger_with(None);
        assert_eq!(
            targets::candidates(&ctx, &item, &TARGET),
            [TargetRef::Card(fixtures::THEIR_UNIT), TargetRef::Card(BASED)],
            "both enemy units in their base; not the Jinx at a battlefield, not the Sprite there, not Vi in our base"
        );
    }

    #[test]
    fn the_run_deals_his_current_might_to_the_chosen_unit_and_nothing_without_a_target() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            spirit_cleave(&mut ctx, &trigger_with(None), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.damage_on(BASED), 0);
        let item = trigger_with(Some(BASED));
        assert_eq!(spirit_cleave(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.damage_on(BASED), 5, "printed 5, no modifiers");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {YONE}}} deals 5 to {{card {BASED}}} · equal to his Might"
        )));
        crate::cards::prelude::might_this_turn(&mut ctx, &item, YONE, 2, None);
        spirit_cleave(&mut ctx, &item, Stage(0));
        assert_eq!(ctx.damage_on(BASED), 12, "710 · current Might, 5 plus 2");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_seam_reports_nothing_and_no_conquer_fires_him_until_the_engine_records_the_holder_before(
    ) {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        let conquered = ctx
            .events
            .iter()
            .find(|event| matches!(event, Event::Conquered { .. }))
            .cloned()
            .expect("a conquer");
        assert_eq!(open_before_the_conquer(&ctx, &conquered), None);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    #[ignore = "engine gap · Event::Conquered carries the taker and the units, not the holder before cleanup::establish; open_before_the_conquer is the seam that must read it (170.11.c: unoccupied and uncontrolled) so a conquer of an open battlefield deals his Might to an enemy unit in a base"]
    fn conquering_an_open_battlefield_deals_his_might_to_an_enemy_unit_in_a_base() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.holder(fixtures::BF1), None, "nobody held it");
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BASED}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BASED}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == YONE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BASED)]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.damage_on(BASED), 5);
        assert_eq!(ctx.damage_on(AFIELD), 0);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn conquering_a_battlefield_the_enemy_held_is_not_an_open_one() {
        let mut fixture = contesting(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.blob.chain.is_empty(), "held by the enemy: not open");
        assert!(ctx.blob.prompt.is_none());
    }
}

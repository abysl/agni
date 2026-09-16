use super::prelude::{a_friendly_unit, buff, done, gear, on_friendly_unit_dies, when};
use super::{Card, Flow, Item, Source, Stage, TargetSpec};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::targets;
use crate::state::{Noted, TargetRef};

pub const ANOTHER_FRIENDLY_UNIT: TargetSpec = a_friendly_unit("another friendly unit to buff");

pub fn a_buffed_unit_died(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(
        event,
        Event::Died {
            unit: true,
            noted: Noted { buffed: true, .. },
            ..
        }
    )
}

pub fn buff_another_friendly_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(TargetRef::Card(unit)) = item.targets.first().copied() else {
        return done();
    };
    if Some(unit) == item.subject_card()
        || !ctx.on_board(unit)
        || !targets::matches(
            ctx,
            item,
            &ANOTHER_FRIENDLY_UNIT.filter,
            TargetRef::Card(unit),
        )
    {
        return done();
    }
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    }
    done()
}

pub static CARD: Card = gear(
    "Vanguard Helm",
    &[],
    &[when(
        on_friendly_unit_dies(&[ANOTHER_FRIENDLY_UNIT], buff_another_friendly_unit),
        a_buffed_unit_died,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const HELM: u32 = 90;
    const FALLEN: u32 = 91;
    const HEIR: u32 = 92;

    fn helm() -> CardInfo {
        let mut card = fixtures::gear(HELM, fixtures::BASE, 0, "Vanguard Helm", 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn vanguard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(helm());
        fixture.table.cards.push(fixtures::unit(
            FALLEN,
            fixtures::BF1,
            0,
            "Vanguard Sergeant",
            2,
        ));
        fixture.table.cards.push(fixtures::unit(
            HEIR,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_for(dead: u32, target: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: HELM,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(TargetRef::Card(dead));
        item.targets.push(TargetRef::Card(target));
        item
    }

    fn died(card: u32, controller: u8, unit: bool, buffed: bool) -> Event {
        Event::Died {
            card,
            controller,
            unit,
            noted: Noted {
                zone: fixtures::BF1,
                might: 2,
                controller,
                alone: false,
                buffed,
            },
        }
    }

    fn source() -> Source {
        Source {
            card: HELM,
            ability: 0,
        }
    }

    #[test]
    fn the_script_watches_buffed_friendly_deaths_and_asks_for_another_friendly_unit() {
        assert!(std::ptr::eq(script_of("Vanguard Helm").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ANOTHER_FRIENDLY_UNIT.filter, FRIENDLY_UNIT);
        assert_eq!(
            (ANOTHER_FRIENDLY_UNIT.min, ANOTHER_FRIENDLY_UNIT.max),
            (1, 1)
        );
    }

    #[test]
    fn the_condition_reads_the_buffed_flag_of_the_death_snapshot() {
        let mut fixture = vanguard();
        let ctx = fixture.ctx();
        assert!(a_buffed_unit_died(
            &ctx,
            &died(FALLEN, 0, true, true),
            source()
        ));
        assert!(
            !a_buffed_unit_died(&ctx, &died(FALLEN, 0, true, false), source()),
            "an unbuffed unit dying is not a buffed unit dying"
        );
        assert!(
            !a_buffed_unit_died(&ctx, &died(HELM, 0, false, true), source()),
            "gear is not a unit"
        );
        assert!(!a_buffed_unit_died(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source()
        ));
    }

    #[test]
    fn an_unbuffed_friendly_death_and_a_buffed_enemy_death_queue_nothing() {
        let mut fixture = vanguard();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(FALLEN, Cause::Combat), Killed::Yes);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none());
        ctx.buff(fixtures::THEIR_UNIT);
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Combat), Killed::Yes);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none(),
            "an enemy unit is not friendly"
        );
        assert!(!ctx.is_buffed(HEIR));
    }

    #[test]
    fn the_run_buffs_the_chosen_friendly_unit_once() {
        let mut fixture = vanguard();
        let mut ctx = fixture.ctx();
        ctx.buff(FALLEN);
        assert_eq!(ctx.kill(FALLEN, Cause::Combat), Killed::Yes);
        assert_eq!(
            buff_another_friendly_unit(&mut ctx, &trigger_for(FALLEN, HEIR), Stage(0)),
            Flow::Done
        );
        assert!(ctx.is_buffed(HEIR));
        assert_eq!(ctx.current_might(HEIR), 2);
        assert!(ctx.blob.log.contains(&format!("{{card {HEIR}}} is buffed")));
        buff_another_friendly_unit(&mut ctx, &trigger_for(FALLEN, HEIR), Stage(0));
        assert_eq!(
            ctx.table.counter(Target::Card(HEIR), COUNTER_BUFFED),
            Some(1),
            "702.3 · one buff at a time"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_dead_unit_itself_an_enemy_and_a_gone_target_get_nothing() {
        let mut fixture = vanguard();
        let mut ctx = fixture.ctx();
        buff_another_friendly_unit(&mut ctx, &trigger_for(FALLEN, FALLEN), Stage(0));
        assert!(
            !ctx.is_buffed(FALLEN),
            "another unit, not the one that died"
        );
        buff_another_friendly_unit(
            &mut ctx,
            &trigger_for(FALLEN, fixtures::THEIR_UNIT),
            Stage(0),
        );
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT), "not friendly");
        ctx.table
            .apply_entry(&fixtures::move_action(HEIR, fixtures::TRASH, 0), 0)
            .unwrap();
        buff_another_friendly_unit(&mut ctx, &trigger_for(FALLEN, HEIR), Stage(0));
        assert!(!ctx.is_buffed(HEIR), "356.3.e · gone before it resolved");
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn a_buffed_friendly_unit_dying_triggers_the_helm_to_buff_another() {
        let mut fixture = vanguard();
        let mut ctx = fixture.ctx();
        ctx.buff(FALLEN);
        assert_eq!(ctx.kill(FALLEN, Cause::Combat), Killed::Yes);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {HEIR}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(HEIR));
        ctx.kill(HEIR, Cause::Combat);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.blob.prompt.is_none(),
            "355.10.d.2 · the lone friendly unit is still a target, and the engine picks it without a click"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == fixtures::VI
        )));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(fixtures::VI));
        ctx.kill(fixtures::VI, Cause::Combat);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no other friendly unit: nothing to choose"
        );
    }
}

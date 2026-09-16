use super::iascylla::at_the_start_of_your_next_main_phase;
use super::prelude::{done, on_hold_me, triggered, unit, Location};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const SHIELD: u8 = 2;
pub const EXTRA_TRIGGERS: usize = 1;
pub const ADD: u8 = 1;
pub const ADDS: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow],
};

pub fn extra_hold_triggers_here_until_triggers_collect_queues_each_hold_match_again(
    ctx: &Ctx,
    seat: u8,
    zone: u16,
) -> usize {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|held| ctx.controller(*held) == seat)
        .filter(|held| {
            ctx.script(*held)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .count()
        * EXTRA_TRIGGERS
}

fn remember_the_hold(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    at_the_start_of_your_next_main_phase(ctx, item, ADD, Vec::new());
    done()
}

fn add_a_rainbow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.add_to_pool(item.controller, item.kind.source(), &ADDS);
    done()
}

pub static CARD: Card = unit(
    "Blue Sentinel",
    &[Keyword::Shield(SHIELD)],
    &[
        on_hold_me(&[], remember_the_hold),
        triggered(Trigger::Reflexive, &[], add_a_rainbow),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Who;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, triggers};
    use crate::state::{Delayed, ItemKind, Pool, Pooled, When};
    use agni_plugin_sdk::table::CardInfo;

    const SENTINEL: u32 = 90;
    const THEIR_SENTINEL: u32 = 91;

    fn sentinel(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, zone, seat, "Blue Sentinel", 4)
        }
    }

    fn post(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sentinel(SENTINEL, zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_shield_two_a_hold_trigger_and_a_delayed_add_of_its_own() {
        assert!(std::ptr::eq(script_of("Blue Sentinel").unwrap(), &CARD));
        assert_eq!(CARD.name, "Blue Sentinel");
        assert_eq!(CARD.keywords, [Keyword::Shield(2)]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
        let add = &CARD.abilities[usize::from(ADD)];
        assert_eq!(add.trigger, Trigger::Reflexive);
        assert!(add.targets.is_empty());
        assert!(add.condition.is_none());
        assert!(add.timing().is_none());
        assert_eq!(ADDS.energy, 0);
        assert_eq!(ADDS.power, [Power::Rainbow]);
        assert_eq!(EXTRA_TRIGGERS, 1);
    }

    #[test]
    fn holding_with_it_narrates_the_promise_and_schedules_nothing_today() {
        let mut fixture = post(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![SENTINEL]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SENTINEL
        ));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.delayed.is_empty(),
            "seam · no When for the start of a Main Phase"
        );
        assert!(ctx.blob.seat(0).pool.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SENTINEL}}} · at the start of {{seat 0}}'s next Main Phase (the engine keeps no Main Phase delay yet)"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_delayed_add_banks_one_rainbow_for_its_controller_when_it_resolves() {
        let mut fixture = post(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.blob.delayed.push(Delayed {
            when: When::BeginningOf(0),
            source: SENTINEL,
            seat: 0,
            ability: ADD,
            args: Vec::new(),
        });
        assert_eq!(triggers::queue_delayed(&mut ctx, When::BeginningOf(0)), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: ADD } if source == SENTINEL
        ));
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.seat(0).pool.is_empty(),
            "the add waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob.seat(0).pool,
            Pool {
                energy: 0,
                power: vec![Pooled::Rainbow]
            }
        );
        assert!(ctx.blob.seat(1).pool.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} adds 1 rainbow to their rune pool".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_the_opponents_hold_is_not_its() {
        let mut fixture = post(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, None);
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
        drop(ctx);
        let mut fixture = post(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_seam_counts_its_controllers_sentinels_at_the_held_battlefield() {
        let mut fixture = post(fixtures::BF1);
        fixture
            .table
            .cards
            .push(sentinel(THEIR_SENTINEL, fixtures::BF1, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            extra_hold_triggers_here_until_triggers_collect_queues_each_hold_match_again(
                &ctx,
                0,
                fixtures::BF1
            ),
            1
        );
        assert_eq!(
            extra_hold_triggers_here_until_triggers_collect_queues_each_hold_match_again(
                &ctx,
                1,
                fixtures::BF1
            ),
            1,
            "theirs counts for them alone"
        );
        assert_eq!(
            extra_hold_triggers_here_until_triggers_collect_queues_each_hold_match_again(
                &ctx,
                0,
                fixtures::BF2
            ),
            0,
            "here, not elsewhere"
        );
    }

    #[test]
    #[ignore = "engine gap · triggers::collect queues each Hold match once; with Blue Sentinel at the held battlefield it should queue each of its controller's hold triggers there an additional time, read from extra_hold_triggers_here_until_triggers_collect_queues_each_hold_match_again"]
    fn its_own_hold_trigger_queues_twice() {
        let mut fixture = post(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 2, "one hold, two triggers");
        assert!(ctx.blob.chain.iter().all(
            |item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == SENTINEL)
        ));
    }

    #[test]
    #[ignore = "engine gap · delayed triggers: no When::MainPhaseOf(seat) for phases::continue_beginning to queue as the Action phase opens (316.4), so iascylla::at_the_start_of_your_next_main_phase schedules nothing; with it the hold delays the add to that turn's Main Phase, after the pool empties (316.3)"]
    fn its_hold_adds_a_rainbow_at_the_start_of_that_turns_main_phase() {
        let mut fixture = post(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].ability, ADD);
        assert!(
            ctx.blob.seat(0).pool.is_empty(),
            "nothing before the Main Phase"
        );
        crate::engine::phases::continue_beginning(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).pool.power, [Pooled::Rainbow]);
    }
}

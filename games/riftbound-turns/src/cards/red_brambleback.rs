use super::prelude::{a_friendly_unit, buff, card_target, done, on_conquer_me, unit, Location};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const EXTRA_TRIGGERS: usize = 1;
pub const TARGET: TargetSpec = a_friendly_unit("a friendly unit to buff");

pub fn extra_conquer_triggers_here_until_triggers_collect_queues_each_conquer_match_again(
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

fn bramble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        } else {
            ctx.narrate(format!("{{card {unit}}} already has a buff"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Red Brambleback",
    &[Keyword::Accelerate],
    &[on_conquer_me(&[TARGET], bramble)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BRAMBLEBACK: u32 = 90;
    const ALLY: u32 = 91;
    const THEIR_BRAMBLEBACK: u32 = 92;

    fn brambleback(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, seat, "Red Brambleback", 4)
        }
    }

    fn thicket(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(brambleback(BRAMBLEBACK, zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn buffed(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_prints_accelerate_and_one_targeted_conquer_trigger_and_the_doubling_is_a_seam() {
        assert!(std::ptr::eq(script_of("Red Brambleback").unwrap(), &CARD));
        assert_eq!(CARD.name, "Red Brambleback");
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(!conquer.optional);
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
        assert_eq!(conquer.targets.len(), 1);
        assert_eq!(conquer.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((conquer.targets[0].min, conquer.targets[0].max), (1, 1));
        assert_eq!(EXTRA_TRIGGERS, 1);
    }

    #[test]
    fn conquering_with_him_asks_for_a_friendly_unit_and_buffs_it_once() {
        let mut fixture = thicket(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![BRAMBLEBACK, ALLY]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {BRAMBLEBACK}}}"),
                format!("{{card {ALLY}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BRAMBLEBACK
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY)]);
        assert!(!ctx.is_buffed(ALLY), "the buff waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(ALLY));
        assert_eq!(buffed(&ctx, ALLY), 1);
        assert_eq!(ctx.current_might(ALLY), 3, "a buff is +1 Might");
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_already_has_a_buff_keeps_the_one_it_has() {
        let mut fixture = thicket(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(ALLY));
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(buffed(&ctx, ALLY), 1, "buffs do not stack");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} already has a buff")));
    }

    #[test]
    fn a_conquer_without_him_and_a_hold_with_him_trigger_nothing() {
        let mut fixture = thicket(fixtures::BASE);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "the ally conquered alone");
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);
        let mut fixture = thicket(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a hold is not a conquer");
    }

    #[test]
    fn the_seam_counts_his_controllers_bramblebacks_at_the_conquered_battlefield() {
        let mut fixture = thicket(fixtures::BF1);
        fixture
            .table
            .cards
            .push(brambleback(THEIR_BRAMBLEBACK, fixtures::BF1, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            extra_conquer_triggers_here_until_triggers_collect_queues_each_conquer_match_again(
                &ctx,
                0,
                fixtures::BF1
            ),
            1
        );
        assert_eq!(
            extra_conquer_triggers_here_until_triggers_collect_queues_each_conquer_match_again(
                &ctx,
                1,
                fixtures::BF1
            ),
            1,
            "theirs counts for them alone"
        );
        assert_eq!(
            extra_conquer_triggers_here_until_triggers_collect_queues_each_conquer_match_again(
                &ctx,
                0,
                fixtures::BF2
            ),
            0,
            "here, not elsewhere"
        );
    }

    #[test]
    #[ignore = "engine gap · triggers::collect queues each Conquer match once; with Red Brambleback at the conquered battlefield it should queue each of his controller's conquer triggers there an additional time, read from extra_conquer_triggers_here_until_triggers_collect_queues_each_conquer_match_again"]
    fn his_own_conquer_trigger_queues_twice_and_buffs_two_units() {
        let mut fixture = thicket(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRAMBLEBACK}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "one conquer, two triggers");
        resolve_chain(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(ALLY));
        assert!(ctx.is_buffed(BRAMBLEBACK));
    }
}

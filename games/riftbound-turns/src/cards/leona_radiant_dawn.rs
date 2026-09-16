use super::prelude::{a_friendly_unit, buff, card_target, done, legend, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub const WHEN_YOU_STUN: Trigger = Trigger::Reflexive;

pub fn you_stunned_one_or_more_enemy_units(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

fn radiant(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        } else {
            ctx.narrate(format!("{{card {unit}}} already has a buff"));
        }
    }
    done()
}

pub static CARD: Card = legend(
    "Leona - Radiant Dawn",
    &[],
    &[when(
        triggered(
            WHEN_YOU_STUN,
            &[a_friendly_unit("a friendly unit to buff")],
            radiant,
        ),
        you_stunned_one_or_more_enemy_units,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{stun, FRIENDLY_UNIT};
    use crate::cards::script_of;
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{priority, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::Target;

    const LEONA: u32 = fixtures::LEGEND_CARD;
    const GUARD: u32 = 90;

    fn dawn() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LEONA).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(GUARD, fixtures::BF1, 0, "Guard", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn buffed(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn queue_the_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: LEONA,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Card(fixtures::SPRITE));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_one_stun_watch_trigger_that_buffs_a_chosen_friendly_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, WHEN_YOU_STUN);
        assert_eq!(
            WHEN_YOU_STUN,
            Trigger::Reflexive,
            "the engine has no Stunned event · the trigger is queued by nothing yet"
        );
        assert!(ability.condition.is_some());
        assert!(!ability.optional, "buff a friendly unit is not a may");
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let mut fixture = dawn();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(LEONA).unwrap(), &CARD));
    }

    #[test]
    fn the_trigger_asks_for_a_friendly_unit_and_buffs_it_when_it_resolves() {
        let mut fixture = dawn();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {GUARD}}}")
            ],
            "her controller's units · a trigger's target has no cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {GUARD}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEONA
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(GUARD)]);
        assert!(!ctx.is_buffed(GUARD), "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(GUARD));
        assert_eq!(buffed(&ctx, GUARD), 1);
        assert_eq!(ctx.current_might(GUARD), 3, "a buff is +1 Might");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {GUARD}}} is buffed")));
        assert!(!ctx.is_buffed(fixtures::VI));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_already_has_a_buff_keeps_the_one_it_has() {
        let mut fixture = dawn();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(GUARD));
        queue_the_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {GUARD}}}")).unwrap();
        resolve_top(&mut ctx);
        assert_eq!(buffed(&ctx, GUARD), 1, "buffs do not stack");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {GUARD}}} already has a buff")));
    }

    #[test]
    fn with_no_friendly_unit_the_trigger_is_removed_for_want_of_a_target() {
        let mut fixture = dawn();
        fixture
            .table
            .cards
            .retain(|card| card.id != GUARD && card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn stunning_an_enemy_today_raises_no_event_so_nothing_queues_her() {
        let mut fixture = dawn();
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::SPRITE));
        assert!(ctx.is_stunned(fixtures::SPRITE));
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
        assert!(!you_stunned_one_or_more_enemy_units(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            Source {
                card: LEONA,
                ability: 0
            }
        ));
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::stun raises no Stunned event and Trigger has no Stunned(Who); with an Event::Stunned { units, by } raised once per stun instruction (one or more enemy units, one trigger), WHEN_YOU_STUN becomes that trigger and you_stunned_one_or_more_enemy_units reads it"]
    fn stunning_one_or_more_enemy_units_queues_her_once_and_she_buffs_a_friendly_unit() {
        let mut fixture = dawn();
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::SPRITE));
        assert!(stun(&mut ctx, fixtures::THEIR_UNIT));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "two units stunned by one instruction · one trigger"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {GUARD}}}")).unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.is_buffed(GUARD));
        drop(ctx);
        let mut own = dawn();
        let mut ctx = own.ctx();
        assert!(stun(&mut ctx, fixtures::VI));
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "stunning her own unit is not stunning an enemy unit"
        );
    }
}

use super::prelude::{done, legend, might_this_turn, trigger_subject, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::{Ctx, Event, Location};

pub const MIGHT: i16 = -1;
pub const MINIMUM: i32 = 1;
pub const AN_ENEMY_ATTACKS: Trigger = Trigger::Attacks(Who::Enemy);

pub fn an_enemy_unit_attacks_a_battlefield_you_control(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    let Event::Attacks { card } = event else {
        return false;
    };
    let me = ctx.controller(source.card);
    let Some(Location::Battlefield(zone)) = ctx.location(*card) else {
        return false;
    };
    ctx.is_unit(*card) && ctx.controller(*card) != me && ctx.holds(me, zone)
}

fn charm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if !ctx.is_unit(unit) || !ctx.on_board(unit) {
        return done();
    }
    might_this_turn(ctx, item, unit, MIGHT, Some(MINIMUM));
    ctx.narrate(format!(
        "{{card {unit}}} gets {MIGHT} might this turn · to a minimum of {MINIMUM}"
    ));
    done()
}

pub static CARD: Card = legend(
    "Ahri - Nine-Tailed Fox",
    &[],
    &[when(
        triggered(AN_ENEMY_ATTACKS, &[], charm),
        an_enemy_unit_attacks_a_battlefield_you_control,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{expiry, priority, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, TargetRef};
    use agni_plugin_sdk::table::Target;

    const AHRI: u32 = fixtures::LEGEND_CARD;
    const RAIDER: u32 = 90;
    const RUNT: u32 = 91;

    fn shrine() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(AHRI).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BF1, 1, "Runt", 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn queue_the_trigger(ctx: &mut Ctx, attacker: u32) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: AHRI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Card(attacker));
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
    fn the_legend_has_one_attack_trigger_whose_enemy_reading_is_the_condition() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, AN_ENEMY_ATTACKS);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty(), "the attacker is the subject");
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(MIGHT, -1);
        assert_eq!(MINIMUM, 1);
        let mut fixture = shrine();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(AHRI).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_wants_an_enemy_unit_attacking_a_battlefield_her_controller_holds() {
        let mut fixture = shrine();
        let ctx = fixture.ctx();
        let me = Source {
            card: AHRI,
            ability: 0,
        };
        let attacks = |card: u32| Event::Attacks { card };
        assert!(an_enemy_unit_attacks_a_battlefield_you_control(
            &ctx,
            &attacks(RAIDER),
            me
        ));
        assert!(
            !an_enemy_unit_attacks_a_battlefield_you_control(&ctx, &attacks(fixtures::VI), me),
            "Vi is her controller's own"
        );
        assert!(
            !an_enemy_unit_attacks_a_battlefield_you_control(&ctx, &attacks(fixtures::SPRITE), me),
            "the Sprite stands at a battlefield seat 1 holds"
        );
        assert!(
            !an_enemy_unit_attacks_a_battlefield_you_control(
                &ctx,
                &attacks(fixtures::THEIR_UNIT),
                me
            ),
            "Jinx is in her own base"
        );
        assert!(!an_enemy_unit_attacks_a_battlefield_you_control(
            &ctx,
            &Event::Defends { card: RAIDER },
            me
        ));
    }

    #[test]
    fn the_trigger_charms_the_attacker_down_by_one_for_the_turn_and_never_below_one() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, RAIDER);
        assert!(ctx.blob.prompt.is_none(), "no target to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == AHRI
        ));
        assert_eq!(ctx.blob.chain[0].subject, Some(TargetRef::Card(RAIDER)));
        assert_eq!(ctx.current_might(RAIDER), 4, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RAIDER), 3);
        assert_eq!(might_counter(&ctx, RAIDER), -1);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {RAIDER}}} gets -1 might this turn · to a minimum of 1"
        )));
        queue_the_trigger(&mut ctx, RUNT);
        resolve_top(&mut ctx);
        assert_eq!(ctx.current_might(RUNT), 1, "454.3.b · -1 on 1 stops at 1");
        assert_eq!(might_counter(&ctx, RUNT), 0);
        expiry::at_expiration(&mut ctx);
        assert_eq!(ctx.current_might(RAIDER), 4, "the charm is for the turn");
        assert_eq!(might_counter(&ctx, RAIDER), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_attacker_that_left_the_board_before_resolution_is_left_alone() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, RAIDER);
        ctx.kill(RAIDER, crate::engine::ctx::Cause::Cost);
        assert!(!ctx.on_board(RAIDER));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, RAIDER), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn her_own_units_attacking_never_fire_her_and_the_enemy_attack_awaits_the_engine() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: fixtures::VI });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "Who::Enemy stops a friendly attacker at the matcher"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn an_enemy_marching_into_her_held_battlefield_fires_the_trigger_through_the_engine() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: RAIDER });
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].subject, Some(TargetRef::Card(RAIDER)));
        resolve_top(&mut ctx);
        assert_eq!(ctx.current_might(RAIDER), 3);
    }
}

use super::prelude::{
    a_unit, card_target, done, excess_damage_assigned_in_my_attack, exhausting_self, legend,
    on_conquer, optional, ready, when,
};
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;

pub const EXCESS_TO_READY: u8 = 3;

pub fn conquered_after_an_attack_with_three_excess(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    ctx.controller(source.card) == *seat
        && excess_damage_assigned_in_my_attack(ctx, *seat, *zone)
            .is_some_and(|excess| excess >= EXCESS_TO_READY)
}

fn enforce(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if ready(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} readies"));
        }
    }
    done()
}

pub static CARD: Card = legend(
    "Vi - Piltover Enforcer",
    &[],
    &[optional(exhausting_self(when(
        on_conquer(&[a_unit("a unit to ready")], enforce),
        conquered_after_an_attack_with_three_excess,
    )))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{cleanup, priority, prompts, settle, showdown};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy, TargetRef};

    const ENFORCER: u32 = fixtures::LEGEND_CARD;
    const BRUISER: u32 = 90;
    const DEFENDER: u32 = 91;

    fn precinct() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ENFORCER).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUISER, fixtures::BF1, 0, "Bruiser", 6));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ENFORCER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn against(defender_might: u8) -> Fixture {
        let mut fixture = precinct();
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

    fn queue_the_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: ENFORCER,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Zone(fixtures::BF1));
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
    fn the_legend_has_one_conditional_may_conquer_trigger_that_exhausts_her_over_a_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.condition.is_some(), "three or more excess damage");
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].min, 1);
        assert_eq!(EXCESS_TO_READY, 3);
    }

    #[test]
    fn the_condition_wants_her_controllers_conquer_and_reads_the_excess_of_the_attack() {
        let mut fixture = against(4);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "six on four conquers"
        );
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(2)
        );
        let me = Source {
            card: ENFORCER,
            ability: 0,
        };
        let conquered = |seat: u8| Event::Conquered {
            zone: fixtures::BF1,
            seat,
            units: vec![BRUISER],
        };
        assert!(
            !conquered_after_an_attack_with_three_excess(&ctx, &conquered(0), me),
            "two excess is not three"
        );
        ctx.record_excess_damage(0, fixtures::BF1, 3);
        assert!(conquered_after_an_attack_with_three_excess(
            &ctx,
            &conquered(0),
            me
        ));
        assert!(
            !conquered_after_an_attack_with_three_excess(&ctx, &conquered(1), me),
            "the other seat's conquer is not hers"
        );
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &Event::Held {
                zone: fixtures::BF1,
                seat: 0,
                units: vec![BRUISER]
            },
            me
        ));
        assert!(ctx.blob.chain.is_empty(), "no trigger today");
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
    }

    #[test]
    fn a_conquer_that_follows_no_attack_queues_nothing() {
        let mut fixture = precinct();
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
        assert!(!ctx.card(ENFORCER).unwrap().exhausted);
    }

    #[test]
    fn once_queued_the_trigger_asks_for_a_unit_then_the_exhaust_and_readies_the_pick_on_resolution()
    {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {ENFORCER}}}: choose a unit to ready (0 of 1)")
        );
        let offered = fixtures::labels(&ctx);
        assert!(offered.contains(&format!("{{card {}}}", fixtures::VI)));
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)),
            "any unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "392.2 · the may is the cost confirm"
        );
        assert!(!ctx.card(ENFORCER).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(ENFORCER).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} readies", fixtures::VI)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_exhaust_or_an_exhausted_vi_readies_nothing() {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(ENFORCER).unwrap().exhausted);
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ENFORCER}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = precinct();
        spent.table.card_mut(ENFORCER).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        queue_the_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ENFORCER}}} trigger is removed · its source is exhausted"
        )));
    }

    #[test]
    fn three_excess_damage_after_an_attack_lets_her_exhaust_to_ready_a_unit() {
        let mut fixture = against(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(3)
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.card(ENFORCER).unwrap().exhausted);
        drop(ctx);
        let mut close = against(4);
        let mut ctx = close.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(2)
        );
        assert!(ctx.blob.chain.is_empty(), "two excess is not three");
        assert!(ctx.blob.prompt.is_none());
    }
}

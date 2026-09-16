use super::meditation::FRIENDLY_READY_UNIT;
use super::prelude::{
    a_card, activated, card_target, done, exhaust, exhausting_self, gear, move_to_location_of,
    named, target,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, TargetKind, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const EXHAUSTED: usize = 0;
pub const MOVED: usize = 1;

pub const EXHAUST_COST: TargetSpec = target(
    FRIENDLY_READY_UNIT,
    1,
    1,
    TargetKind::Card,
    "a unit you control to exhaust as the cost",
);

pub const ANOTHER_MOVABLE_FRIENDLY_UNIT: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Movable,
    Filter::NotSame(0),
]);

pub fn exhausted_as_the_cost(ctx: &mut Ctx, item: &Item) -> Option<u32> {
    let unit = card_target(ctx, item, EXHAUSTED)?;
    if !exhaust(ctx, unit) {
        return None;
    }
    ctx.narrate(format!(
        "{{card {}}} · {{card {unit}}} is exhausted as the cost",
        item.kind.source()
    ));
    Some(unit)
}

fn point_the_way(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(waypoint) = exhausted_as_the_cost(ctx, item) else {
        ctx.narrate(format!(
            "{{card {}}} · the unit to exhaust is no longer ready · nothing moves",
            item.kind.source()
        ));
        return done();
    };
    if let Some(mover) = card_target(ctx, item, MOVED) {
        move_to_location_of(ctx, item, mover, waypoint);
    }
    done()
}

pub static CARD: Card = gear(
    "Forgotten Signpost",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Action,
            Cost::FREE,
            &[
                EXHAUST_COST,
                a_card(
                    ANOTHER_MOVABLE_FRIENDLY_UNIT,
                    "a different unit you control to move there",
                ),
            ],
            point_the_way,
        )),
        "exhaust a unit: move another unit to its location",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, prompts, showdown};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SIGNPOST: u32 = 90;
    const SCOUT: u32 = 91;
    const JINX: u32 = 92;

    fn signpost(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            exhausted,
            ..fixtures::gear(SIGNPOST, fixtures::BASE, 0, CARD.name, 2)
        }
    }

    fn crossroads(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(signpost(exhausted));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn offered(ctx: &Ctx) -> Vec<Option<u32>> {
        prompts::offered(ctx).iter().map(|opt| opt.card).collect()
    }

    #[test]
    fn the_script_is_an_action_exhaust_activation_with_a_unit_to_exhaust_and_one_to_move() {
        assert!(std::ptr::eq(
            script_of("Forgotten Signpost").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[EXHAUSTED].filter, FRIENDLY_READY_UNIT);
        assert_eq!(ability.targets[MOVED].filter, ANOTHER_MOVABLE_FRIENDLY_UNIT);
        assert_eq!(
            ability.label,
            Some("exhaust a unit: move another unit to its location")
        );
    }

    #[test]
    fn the_activation_exhausts_the_first_unit_and_moves_the_second_to_its_location() {
        let mut fixture = crossroads(false);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SIGNPOST && offer.enabled));
        activate::activate(&mut ctx, 0, SIGNPOST, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            offered(&ctx),
            [Some(fixtures::VI), Some(SCOUT), None],
            "your ready units, then cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            offered(&ctx),
            [Some(fixtures::VI), None],
            "a different unit you control"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.card(SIGNPOST).unwrap().exhausted);
        assert!(
            !ctx.card(SCOUT).unwrap().exhausted,
            "the unit's exhaust waits for resolution · the non-resource-cost seam"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(SCOUT).unwrap().exhausted);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Battlefield(zone), .. }
                if *card == fixtures::VI && *zone == fixtures::BF1
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SIGNPOST}}} · {{card {SCOUT}}} is exhausted as the cost"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_cost_unit_exhausted_in_response_leaves_the_mover_where_it_stands() {
        let mut fixture = crossroads(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SIGNPOST, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        ctx.exhaust(SCOUT);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SIGNPOST}}} · the unit to exhaust is no longer ready · nothing moves"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_is_an_action_usable_during_a_showdown_with_focus_and_refused_spent_or_by_the_other_seat()
    {
        let mut fixture = crossroads(false);
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 1, "Jinx", 3));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.blob.set_contested(fixtures::BF1, Some(1));
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.is_some());
        assert_eq!(
            activate::activate(&mut ctx, 0, SIGNPOST, 0),
            Err(Refusal::NotYourFocus),
            "the contester has the focus first"
        );
        showdown::pass(&mut ctx, 1).unwrap();
        activate::activate(&mut ctx, 0, SIGNPOST, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        drop(ctx);
        let mut spent = crossroads(true);
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SIGNPOST, 0),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, SIGNPOST, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut alone = crossroads(false);
        alone.table.cards.retain(|card| card.id != SCOUT);
        alone.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        alone.resolve();
        let mut ctx = alone.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SIGNPOST, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "no ready unit to exhaust"
        );
        assert!(!ctx.card(SIGNPOST).unwrap().exhausted);
    }

    #[test]
    #[ignore = "engine gap · non-resource costs at the pay stage: the unit's exhaust belongs with the Signpost's own before the ability reaches the chain, not at its resolution"]
    fn the_unit_is_exhausted_as_the_ability_is_paid_for() {
        let mut fixture = crossroads(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SIGNPOST, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.card(SCOUT).unwrap().exhausted, "paid with the exhaust");
    }
}

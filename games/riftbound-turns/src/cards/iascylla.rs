use super::prelude::{
    asking, done, enemy_units, move_destinations, move_unit, on_hold_me, optional, triggered, unit,
    with_candidates, Location,
};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const LURE: u8 = 1;
const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "an enemy unit to move to the battlefield she held";

pub fn at_the_start_of_your_next_main_phase(
    ctx: &mut Ctx,
    item: &Item,
    _ability: u8,
    _args: Vec<u32>,
) -> bool {
    let me = item.kind.source();
    let seat = item.controller;
    ctx.narrate(format!(
        "{{card {me}}} · at the start of {{seat {seat}}}'s next Main Phase (the engine keeps no Main Phase delay yet)"
    ));
    false
}

pub fn battlefield_card_at(ctx: &Ctx, zone: u16) -> Option<u32> {
    ctx.table
        .cards
        .iter()
        .find(|held| held.zone == Some(zone) && ctx.is_battlefield_card(held.id))
        .map(|held| held.id)
}

pub fn held_battlefield(ctx: &Ctx, item: &Item) -> Option<u16> {
    let TargetRef::Card(battlefield) = *item.targets.first()? else {
        return None;
    };
    let zone = ctx.card(battlefield)?.zone?;
    ctx.zones.is_battlefield(zone).then_some(zone)
}

pub fn lures(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(zone) = held_battlefield(ctx, item) else {
        return Vec::new();
    };
    let there = Location::Battlefield(zone);
    enemy_units(ctx, item.controller)
        .into_iter()
        .filter(|unit| move_destinations(ctx, *unit).contains(&there))
        .map(TargetRef::Card)
        .collect()
}

fn remember_the_hold(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(TargetRef::Zone(zone)) = item.subject else {
        return done();
    };
    let Some(battlefield) = battlefield_card_at(ctx, zone) else {
        return done();
    };
    at_the_start_of_your_next_main_phase(ctx, item, LURE, vec![battlefield]);
    done()
}

fn lure(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_PICKED => {
            let offered = lures(ctx, item, stage);
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| offered.contains(&TargetRef::Card(*unit)))
            else {
                return done();
            };
            if let Some(zone) = held_battlefield(ctx, item) {
                move_unit(ctx, item, unit, Location::Battlefield(zone));
            }
            done()
        }
        _ => {
            if lures(ctx, item, Stage(STAGE_PICKED)).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Iascylla",
    &[],
    &[
        on_hold_me(&[], remember_the_hold),
        asking(
            with_candidates(optional(triggered(Trigger::Reflexive, &[], lure)), lures),
            QUESTION,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Who;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle, triggers};
    use crate::state::{Delayed, ItemKind, PromptWhy, When};
    use agni_plugin_sdk::table::CardInfo;

    const IASCYLLA: u32 = 90;
    const THEIR_RAIDER: u32 = 91;

    fn iascylla(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(IASCYLLA, zone, seat, "Iascylla", 6)
        }
    }

    fn reef(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(iascylla(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_RAIDER, fixtures::BASE, 1, "Raider", 2));
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

    fn queue_the_lure(ctx: &mut Ctx) {
        ctx.blob.delayed.push(Delayed {
            when: When::BeginningOf(0),
            source: IASCYLLA,
            seat: 0,
            ability: LURE,
            args: vec![fixtures::GROUNDS],
        });
        assert_eq!(triggers::queue_delayed(ctx, When::BeginningOf(0)), 1);
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: LURE } if source == IASCYLLA
        ));
        assert!(ctx.blob.prompt.is_none(), "the may is asked at resolution");
    }

    #[test]
    fn the_script_is_a_hold_trigger_that_schedules_an_optional_lure_of_her_own() {
        assert!(std::ptr::eq(script_of("Iascylla").unwrap(), &CARD));
        assert_eq!(CARD.name, "Iascylla");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
        let lure = &CARD.abilities[usize::from(LURE)];
        assert_eq!(lure.trigger, Trigger::Reflexive);
        assert!(lure.optional, "you may move");
        assert!(lure.candidates.is_some());
        assert_eq!(lure.question, Some(QUESTION));
        assert!(lure.targets.is_empty());
        assert!(lure.timing().is_none());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn holding_with_her_narrates_the_promise_and_schedules_nothing_today() {
        let mut fixture = reef(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![IASCYLLA]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == IASCYLLA
        ));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.delayed.is_empty(),
            "seam · no When for the start of a Main Phase"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {IASCYLLA}}} · at the start of {{seat 0}}'s next Main Phase (the engine keeps no Main Phase delay yet)"
        )));
        assert_eq!(ctx.location(THEIR_RAIDER), Some(Location::Base(1)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_lure_offers_enemy_units_that_can_reach_the_held_battlefield_and_moves_the_pick() {
        let mut fixture = reef(fixtures::BF1);
        let mut ctx = fixture.ctx();
        queue_the_lure(&mut ctx);
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICKED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_RAIDER}}}"),
                "skip".to_string()
            ],
            "every enemy unit, wherever it stands"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {IASCYLLA}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_RAIDER}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(THEIR_RAIDER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Battlefield(zone), .. } if *card == THEIR_RAIDER && *zone == fixtures::BF1
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_moves_nothing_and_a_friendly_unit_is_not_offered() {
        let mut fixture = reef(fixtures::BF1);
        let mut ctx = fixture.ctx();
        queue_the_lure(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(!fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::VI)));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(THEIR_RAIDER), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn with_no_enemy_unit_the_lure_resolves_without_asking_and_a_hold_elsewhere_is_not_hers() {
        let mut fixture = reef(fixtures::BF1);
        fixture.table.cards.retain(|card| {
            card.id != THEIR_RAIDER
                && card.id != fixtures::SPRITE
                && card.id != fixtures::THEIR_UNIT
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        queue_the_lure(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut fixture = reef(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "Vi holds; she sits in the base");
    }

    #[test]
    #[ignore = "engine gap · delayed triggers: no When::MainPhaseOf(seat) for phases::continue_beginning to queue as the Action phase opens (316.4), so at_the_start_of_your_next_main_phase schedules nothing; with it the hold delays the lure to that turn's Main Phase with the held battlefield as its argument (359.3.f.3.b)"]
    fn her_hold_lures_an_enemy_unit_at_the_start_of_that_turns_main_phase() {
        let mut fixture = reef(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].args, [fixtures::GROUNDS]);
        assert_eq!(ctx.blob.delayed[0].ability, LURE);
        crate::engine::phases::continue_beginning(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_RAIDER}}}")).unwrap();
        assert_eq!(
            ctx.location(THEIR_RAIDER),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}

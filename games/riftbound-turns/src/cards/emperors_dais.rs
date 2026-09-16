use super::prelude::{
    a_card, battlefield, bounce, card_target, done, location_of, on_conquer, optional, spawn,
    with_cost, Location, Token, FRIENDLY_UNIT_HERE, ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

const SOLDIER_ARRIVES_READY: bool = false;

fn here(ctx: &Ctx, card: u32) -> Option<Location> {
    match location_of(ctx, card) {
        at @ Some(Location::Battlefield(_)) => at,
        _ => None,
    }
}

fn return_a_unit_for_a_soldier(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let source = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(at) = here(ctx, source) else {
        return done();
    };
    if location_of(ctx, unit) != Some(at) || !bounce(ctx, unit) {
        return done();
    }
    let Location::Battlefield(zone) = at else {
        return done();
    };
    if !ctx.units_played_here(zone) {
        ctx.narrate(format!(
            "no Sand Soldier · units can't be played at {}",
            describe(at)
        ));
        return done();
    }
    if let Some(soldier) = spawn(ctx, seat, Token::SandSoldier, at, SOLDIER_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {soldier}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Emperor's Dais",
    &[],
    &[optional(with_cost(
        on_conquer(
            &[a_card(
                FRIENDLY_UNIT_HERE,
                "a unit you control here to return",
            )],
            return_a_unit_for_a_soldier,
        ),
        ONE_ENERGY,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who, TOKEN_SAND_SOLDIER};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const DAIS: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;
    const SOLDIER: u32 = 200;

    fn dais() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(DAIS).unwrap().name = "Emperor's Dais".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 0, "Jinx", 2));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DAIS).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_dais_is_a_battlefield_with_one_paid_may_conquer_trigger_over_a_unit_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Emperor's Dais").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT_HERE);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn paying_returns_the_chosen_unit_and_plays_an_exhausted_sand_soldier_here() {
        let mut fixture = dais();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 90}"],
            "the conqueror's units at the Dais, not the Sprite elsewhere"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 90}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "the [1] is the trigger's cost, confirmed at finalization"
        );
        assert!(ctx.effects.iter().all(|effect| !matches!(
            effect,
            Effect::Move { card, .. } if *card == JINX
        )));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.effects.contains(&Effect::exhaust(41)),
            "one ready rune pays the energy: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(JINX)]);
        assert!(ctx.on_board(JINX), "the bounce waits for resolution");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: JINX,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == TOKEN_SAND_SOLDIER && *zone == fixtures::BF1
        )));
        assert!(ctx.effects.contains(&Effect::exhaust(SOLDIER)));
        let soldier = ctx.card(SOLDIER).unwrap();
        assert_eq!(
            (soldier.name.as_str(), soldier.might),
            (TOKEN_SAND_SOLDIER, Some(2))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == SOLDIER
        )));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [fixtures::VI, SOLDIER]
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_removes_the_trigger_and_nothing_moves() {
        let mut fixture = dais();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 90}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.on_board(JINX));
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("trigger is removed")));
    }

    #[test]
    fn a_unit_gone_at_resolution_plays_no_soldier_and_no_ready_rune_asks_nothing() {
        let mut fixture = dais();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 90}").unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        ctx.bounce(JINX);
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        let mut broke = dais();
        broke.table.cards.retain(|card| card.id != JINX);
        for rune in broke.table.cards.iter_mut() {
            if rune.zone == Some(fixtures::RUNE_POOL) && rune.owner == 0 {
                rune.exhausted = true;
            }
        }
        broke.resolve();
        let mut ctx = broke.ctx();
        conquer(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "the lone unit is picked without a click and the unpayable [1] removes the trigger"
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("its cost can't be paid")));
    }
}

use super::faithful_manufactor::recruits_playable_at;
use super::prelude::{done, location_of, play, spawn, unit, Token};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const SOLDIER_ARRIVES_READY: bool = false;

pub fn play_sand_soldier_here(ctx: &mut Ctx, item: &Item) -> Option<u32> {
    let me = item.kind.source();
    let seat = item.controller;
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!(
            "{{card {me}}} has left the board · no Sand Soldier"
        ));
        return None;
    };
    if !recruits_playable_at(ctx, here) {
        ctx.narrate(format!(
            "no Sand Soldier · units can't be played at {}",
            describe(here)
        ));
        return None;
    }
    let soldier = spawn(ctx, seat, Token::SandSoldier, here, SOLDIER_ARRIVES_READY)?;
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {soldier}}} to {}",
        describe(here)
    ));
    Some(soldier)
}

fn guard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_sand_soldier_here(ctx, item);
    done()
}

pub static CARD: Card = unit("Royal Guard", &[], &[play(&[], guard)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::{script_of, Static, Trigger, TOKEN_SAND_SOLDIER};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const GUARD: u32 = 90;

    static NO_FACTORY: Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("No Factory", &[], &[]),
        &[Static::NoUnitsPlayedHere],
    );

    fn guard_card() -> CardInfo {
        let mut card = fixtures::unit(GUARD, fixtures::HAND, 0, "Royal Guard", 2);
        card.domain = vec!["Order".into()];
        card.energy = Some(1);
        card
    }

    fn soldiers_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SAND_SOLDIER && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn palace() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guard_card());
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GUARD).unwrap(), &CARD));
        fixture
    }

    fn post(ctx: &mut Ctx, at: Location) {
        play_engine::begin(ctx, 0, GUARD, Origin::Hand, Some(at)).unwrap();
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(script_of("Royal Guard").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty(), "the soldier is played here");
    }

    #[test]
    fn played_to_a_battlefield_it_plays_an_exhausted_two_might_sand_soldier_there() {
        let mut fixture = palace();
        let mut ctx = fixture.ctx();
        post(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == GUARD
        ));
        assert!(
            soldiers_of(&ctx, 0).is_empty(),
            "the soldier waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(soldiers_of(&ctx, 0), [next]);
        let soldier = next;
        assert_eq!(
            ctx.location(soldier),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · where the guard was played"
        );
        assert!(ctx.is_token(soldier));
        assert!(ctx.is_unit(soldier));
        assert_eq!(ctx.current_might(soldier), 2);
        assert!(!ctx.is_temporary(soldier));
        assert_eq!(ctx.controller(soldier), 0);
        assert!(ctx.card(soldier).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::exhaust(soldier)));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == TOKEN_SAND_SOLDIER && *zone == fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == soldier
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {soldier}}} to {{zone 9}}"
        )));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [GUARD, soldier]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_the_base_the_soldier_lands_in_the_base() {
        let mut fixture = palace();
        let mut ctx = fixture.ctx();
        post(&mut ctx, Location::Base(0));
        resolve_chain(&mut ctx);
        let soldier = *soldiers_of(&ctx, 0).first().expect("the guard's soldier");
        assert_eq!(ctx.location(soldier), Some(Location::Base(0)));
        assert_eq!(
            ctx.units_at(Location::Base(0)).len(),
            3,
            "Vi, the guard and the soldier"
        );
        assert!(soldiers_of(&ctx, 1).is_empty());
    }

    #[test]
    fn a_guard_killed_in_response_has_no_here_and_plays_no_soldier() {
        let mut fixture = palace();
        let mut ctx = fixture.ctx();
        post(&mut ctx, Location::Battlefield(fixtures::BF1));
        ctx.kill(GUARD, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(GUARD));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(soldiers_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {GUARD}}} has left the board · no Sand Soldier"
        )));
    }

    #[test]
    fn a_battlefield_where_units_cannot_be_played_refuses_the_soldier() {
        let mut fixture = palace();
        fixture.table.card_mut(GUARD).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_FACTORY);
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF1));
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: GUARD,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(guard(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(soldiers_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Sand Soldier · units can't be played at")));
    }
}

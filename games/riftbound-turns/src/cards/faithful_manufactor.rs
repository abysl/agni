use super::prelude::{done, location_of, play, unit, Location};
use super::{Card, Flow, Item, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march::describe;
use crate::state::Origin;
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Face;

pub const RECRUIT: &str = "Recruit";
pub const RECRUIT_MIGHT: u8 = 1;
pub const RECRUIT_ARRIVES_READY: bool = false;

pub fn recruit_face_until_token_recruit_lands() -> Face {
    Face::named(RECRUIT)
        .with_kind(KIND_UNIT)
        .with_might(Some(RECRUIT_MIGHT))
}

pub fn is_recruit(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| held.name == RECRUIT && held.is_kind(KIND_UNIT))
}

pub fn recruits_playable_at(ctx: &Ctx, at: Location) -> bool {
    match at {
        Location::Battlefield(zone) => ctx.units_played_here(zone),
        Location::Base(_) => true,
    }
}

pub fn spawn_recruit(ctx: &mut Ctx, owner: u8, at: Location) -> Option<u32> {
    let (zone, seat) = ctx.zone_of(at)?;
    let id = ctx.table.next_id;
    ctx.emit(Effect::Spawn {
        face: recruit_face_until_token_recruit_lands(),
        zone,
        seat,
        owner: Some(owner),
    });
    ctx.spawned += 1;
    if !RECRUIT_ARRIVES_READY {
        ctx.exhaust(id);
    }
    ctx.raise(Event::Played {
        card: id,
        controller: owner,
        kind: KIND_UNIT.into(),
        origin: Origin::Board,
        paid_additional: false,
    });
    ctx.narrate(format!(
        "{{seat {owner}}} plays {{card {id}}} to {}",
        describe(at)
    ));
    Some(id)
}

pub fn play_recruits(ctx: &mut Ctx, owner: u8, at: Location, count: usize) -> Vec<u32> {
    if !recruits_playable_at(ctx, at) {
        ctx.narrate(format!(
            "no Recruit · units can't be played at {}",
            describe(at)
        ));
        return Vec::new();
    }
    (0..count)
        .filter_map(|_| spawn_recruit(ctx, owner, at))
        .collect()
}

pub fn play_recruits_here(ctx: &mut Ctx, item: &Item, count: usize) -> Vec<u32> {
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} has left the board · no Recruit"));
        return Vec::new();
    };
    play_recruits(ctx, item.controller, here, count)
}

fn manufacture(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_recruits_here(ctx, item, 1);
    done()
}

pub static CARD: Card = unit("Faithful Manufactor", &[], &[play(&[], manufacture)]);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::{script_of, Static, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MANUFACTOR: u32 = 90;
    const ORDER_RUNES: [u32; 3] = [46, 47, 48];

    static NO_FACTORY: Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("No Factory", &[], &[]),
        &[Static::NoUnitsPlayedHere],
    );

    pub fn recruits_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == RECRUIT && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    pub fn order_unit(id: u32, zone: u16, seat: u8, name: &str, might: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, name, might);
        card.domain = vec!["Order".into()];
        card.energy = Some(3);
        card
    }

    pub fn with_order_runes(fixture: &mut Fixture, seat: u8) {
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, seat, "Order", false));
        }
    }

    fn factory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(order_unit(
            MANUFACTOR,
            fixtures::HAND,
            0,
            "Faithful Manufactor",
            2,
        ));
        with_order_runes(&mut fixture, 0);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MANUFACTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn build(ctx: &mut Ctx, at: Location) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, MANUFACTOR, Origin::Hand, Some(at))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: MANUFACTOR,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(
            script_of("Faithful Manufactor").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(ability.targets.is_empty(), "the Recruit is played here");
        let face = recruit_face_until_token_recruit_lands();
        assert_eq!(face.name, RECRUIT);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(RECRUIT_MIGHT));
        assert!(face.domain.is_empty(), "187.1 · domainless");
    }

    #[test]
    fn played_to_a_battlefield_it_plays_an_exhausted_one_might_recruit_there() {
        let mut fixture = factory();
        let action = fixtures::move_action(MANUFACTOR, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        build(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MANUFACTOR
        ));
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits, [next]);
        let recruit = recruits[0];
        assert_eq!(
            ctx.location(recruit),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · where the Manufactor was played"
        );
        assert!(ctx.is_token(recruit));
        assert!(ctx.is_unit(recruit));
        assert_eq!(ctx.current_might(recruit), i32::from(RECRUIT_MIGHT));
        assert!(!ctx.is_temporary(recruit));
        assert!(ctx.card(recruit).unwrap().domain.is_empty());
        assert_eq!(ctx.controller(recruit), 0);
        assert!(ctx.card(recruit).unwrap().exhausted, "it enters exhausted");
        assert!(ctx.effects.contains(&Effect::exhaust(recruit)));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == RECRUIT && *zone == fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == recruit
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {recruit}}} to {{zone 9}}"
        )));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [MANUFACTOR, recruit]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_the_base_the_recruit_lands_in_the_base() {
        let mut fixture = factory();
        let action = fixtures::move_action(MANUFACTOR, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        build(&mut ctx, Location::Base(0)).unwrap();
        resolve_chain(&mut ctx);
        let recruit = *recruits_of(&ctx, 0)
            .first()
            .expect("the Manufactor's Recruit");
        assert_eq!(ctx.location(recruit), Some(Location::Base(0)));
        assert_eq!(
            ctx.units_at(Location::Base(0)).len(),
            3,
            "Vi, the Manufactor and the Recruit"
        );
        assert!(recruits_of(&ctx, 1).is_empty());
    }

    #[test]
    fn a_manufactor_killed_in_response_has_no_here_and_plays_no_recruit() {
        let mut fixture = factory();
        let action = fixtures::move_action(MANUFACTOR, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        build(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        ctx.kill(MANUFACTOR, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(MANUFACTOR));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MANUFACTOR}}} has left the board · no Recruit"
        )));
    }

    #[test]
    fn a_battlefield_where_units_cannot_be_played_refuses_the_recruit() {
        let mut fixture = factory();
        fixture.table.card_mut(MANUFACTOR).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(MANUFACTOR).unwrap().seat = 0;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_FACTORY);
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF1));
        assert!(!recruits_playable_at(
            &ctx,
            Location::Battlefield(fixtures::BF1)
        ));
        assert!(recruits_playable_at(&ctx, Location::Base(0)));
        assert_eq!(
            manufacture(&mut ctx, &trigger_item(7), Stage(0)),
            Flow::Done
        );
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Recruit · units can't be played at")));
    }

    #[test]
    fn the_recruit_is_a_token_that_despawns_when_it_dies_and_is_told_apart_by_name() {
        let mut fixture = factory();
        let mut ctx = fixture.ctx();
        let recruit = spawn_recruit(&mut ctx, 1, Location::Base(1)).unwrap();
        assert!(is_recruit(&ctx, recruit));
        assert!(!is_recruit(&ctx, fixtures::VI));
        assert!(!is_recruit(&ctx, fixtures::SPRITE));
        assert_eq!(ctx.controller(recruit), 1);
        assert_eq!(ctx.location(recruit), Some(Location::Base(1)));
        assert_eq!(
            play_recruits(&mut ctx, 0, Location::Battlefield(fixtures::BF1), 2).len(),
            2
        );
        assert_eq!(recruits_of(&ctx, 0).len(), 2);
        ctx.kill(recruit, Cause::Rule);
        assert!(!ctx.on_board(recruit));
        assert!(ctx.effects.contains(&Effect::Despawn { card: recruit }));
        assert!(
            ctx.card(recruit).is_none(),
            "185 · a token leaving the board ceases to exist"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · Token::Recruit: engine/ctx.rs has no Recruit face, so recruit_face_until_token_recruit_lands builds it and spawn_recruit replays Ctx::spawn; with the token, spawn_recruit is spawn(ctx, owner, Token::Recruit, at, RECRUIT_ARRIVES_READY) and cards/mod.rs knows the name"]
    fn the_engine_knows_the_recruit_as_a_token_name() {
        assert!(crate::cards::is_token_name(RECRUIT));
    }
}

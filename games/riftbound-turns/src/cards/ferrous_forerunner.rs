use super::faithful_manufactor::recruits_playable_at;
use super::prelude::{deathknell, done, unit, Location};
use super::rumble_mechanized_menace::MECH_TOKEN;
use super::{Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march::describe;
use crate::state::Origin;
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Face;

pub const MECH_MIGHT: u8 = 3;
pub const MECH_ARRIVES_READY: bool = false;
pub const MECHS: usize = 2;

pub fn mech_face_until_token_mech_lands() -> Face {
    Face::named(MECH_TOKEN)
        .with_kind(KIND_UNIT)
        .with_might(Some(MECH_MIGHT))
}

pub fn spawn_mech(ctx: &mut Ctx, owner: u8, at: Location) -> Option<u32> {
    let (zone, seat) = ctx.zone_of(at)?;
    let id = ctx.table.next_id;
    ctx.emit(Effect::Spawn {
        face: mech_face_until_token_mech_lands(),
        zone,
        seat,
        owner: Some(owner),
    });
    ctx.spawned += 1;
    if !MECH_ARRIVES_READY {
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

pub fn play_mechs(ctx: &mut Ctx, owner: u8, at: Location, count: usize) -> Vec<u32> {
    if !recruits_playable_at(ctx, at) {
        ctx.narrate(format!(
            "no Mech · units can't be played at {}",
            describe(at)
        ));
        return Vec::new();
    }
    (0..count)
        .filter_map(|_| spawn_mech(ctx, owner, at))
        .collect()
}

fn forerun(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_mechs(ctx, seat, Location::Base(seat), MECHS);
    done()
}

pub static CARD: Card = unit(
    "Ferrous Forerunner",
    &[Keyword::Deathknell],
    &[deathknell(&[], forerun)],
);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::rumble_mechanized_menace::is_mech;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const FORERUNNER: u32 = 90;

    pub fn mechs_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == MECH_TOKEN && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn forerunner(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(FORERUNNER, zone, 0, "Ferrous Forerunner", 6);
        card.energy = Some(6);
        card.power = Some(1);
        card
    }

    fn vanguard(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(forerunner(zone));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FORERUNNER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(
            script_of("Ferrous Forerunner").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(death.cost.is_none() && death.condition.is_none());
        assert_eq!(MECHS, 2);
        let face = mech_face_until_token_mech_lands();
        assert_eq!(face.name, MECH_TOKEN);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(MECH_MIGHT));
        assert!(face.domain.is_empty(), "187.1 · domainless");
    }

    #[test]
    fn dying_at_a_battlefield_plays_two_exhausted_three_might_mechs_into_the_base() {
        let mut fixture = vanguard(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.kill(FORERUNNER, Cause::Rule);
        assert_eq!(ctx.card(FORERUNNER).unwrap().zone, Some(fixtures::TRASH));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FORERUNNER
        ));
        assert!(ctx.blob.prompt.is_none(), "the Deathknell chooses nothing");
        assert!(
            mechs_of(&ctx, 0).is_empty(),
            "the Mechs wait for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let mechs = mechs_of(&ctx, 0);
        assert_eq!(mechs, [next, next + 1]);
        for mech in mechs {
            assert_eq!(ctx.location(mech), Some(Location::Base(0)), "to your base");
            assert!(ctx.is_token(mech));
            assert!(ctx.is_unit(mech));
            assert!(
                is_mech(&ctx, mech),
                "the token is a Mech for every Mech aura"
            );
            assert_eq!(ctx.current_might(mech), i32::from(MECH_MIGHT));
            assert!(!ctx.is_temporary(mech));
            assert!(ctx.card(mech).unwrap().domain.is_empty());
            assert_eq!(ctx.controller(mech), 0);
            assert!(ctx.card(mech).unwrap().exhausted, "it enters exhausted");
            assert!(ctx.effects.contains(&Effect::exhaust(mech)));
            assert!(ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                    if face.name == MECH_TOKEN && *zone == fixtures::BASE
            )));
            assert!(ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == mech
            )));
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{seat 0}} plays {{card {mech}}} to their base")));
        }
        assert!(mechs_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_forerunner_of_the_other_seat_fills_their_base_and_a_bounce_is_no_death() {
        let mut fixture = vanguard(fixtures::BASE);
        fixture.table.card_mut(FORERUNNER).unwrap().seat = 1;
        fixture.table.card_mut(FORERUNNER).unwrap().owner = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.bounce(FORERUNNER);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(mechs_of(&ctx, 1).is_empty(), "a bounce is no Deathknell");
        drop(ctx);
        let mut fixture = vanguard(fixtures::BASE);
        fixture.table.card_mut(FORERUNNER).unwrap().seat = 1;
        fixture.table.card_mut(FORERUNNER).unwrap().owner = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.kill(FORERUNNER, Cause::Rule);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let mechs = mechs_of(&ctx, 1);
        assert_eq!(mechs.len(), MECHS);
        for mech in mechs {
            assert_eq!(ctx.location(mech), Some(Location::Base(1)));
            assert_eq!(ctx.controller(mech), 1);
        }
        assert!(mechs_of(&ctx, 0).is_empty());
    }

    #[test]
    fn the_mech_is_a_token_that_despawns_when_it_dies() {
        let mut fixture = vanguard(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let mech = spawn_mech(&mut ctx, 1, Location::Base(1)).unwrap();
        assert!(is_mech(&ctx, mech));
        assert_eq!(ctx.controller(mech), 1);
        assert_eq!(
            play_mechs(&mut ctx, 0, Location::Battlefield(fixtures::BF1), 2).len(),
            2
        );
        ctx.kill(mech, Cause::Rule);
        assert!(!ctx.on_board(mech));
        assert!(ctx.effects.contains(&Effect::Despawn { card: mech }));
        assert!(
            ctx.card(mech).is_none(),
            "185 · a token leaving the board ceases to exist"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · Token::Mech: engine/ctx.rs has no Mech face, so mech_face_until_token_mech_lands builds it and spawn_mech replays Ctx::spawn; with the token, spawn_mech is spawn(ctx, owner, Token::Mech, at, MECH_ARRIVES_READY) and cards/mod.rs knows the name"]
    fn the_engine_knows_the_mech_as_a_token_name() {
        assert!(crate::cards::is_token_name(MECH_TOKEN));
    }
}

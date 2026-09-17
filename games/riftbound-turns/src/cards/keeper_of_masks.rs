use super::faithful_manufactor::{recruits_playable_at, RECRUIT};
use super::frisky_hunter::BIRD;
use super::prelude::{done, location_of, play, unit, Location};
use super::rumble_mechanized_menace::MECH_TOKEN;
use super::{is_token_name, Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march::describe;
use crate::state::Origin;
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Face;

pub const REFLECTION: &str = "Reflection";
pub const REFLECTION_MIGHT: u8 = 0;
pub const REFLECTION_ARRIVES_READY: bool = false;
pub const REFLECTIONS: usize = 2;

pub fn reflection_face_until_token_reflection_lands() -> Face {
    Face::named(REFLECTION)
        .with_kind(KIND_UNIT)
        .with_might(Some(REFLECTION_MIGHT))
}

pub fn is_reflection(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| held.name == REFLECTION && held.is_kind(KIND_UNIT))
}

pub fn spawn_reflection(ctx: &mut Ctx, owner: u8, at: Location, ready: bool) -> Option<u32> {
    let (zone, seat) = ctx.zone_of(at)?;
    let id = ctx.table.next_id;
    ctx.emit(Effect::Spawn {
        face: reflection_face_until_token_reflection_lands(),
        zone,
        seat,
        owner: Some(owner),
    });
    ctx.spawned += 1;
    if !ready {
        ctx.exhaust(id);
    }
    ctx.raise(Event::Played {
        card: id,
        controller: owner,
        kind: KIND_UNIT.into(),
        origin: Origin::Board,
        paid_additional: false,
    });
    let state = if ready { "a ready " } else { "" };
    ctx.narrate(format!(
        "{{seat {owner}}} plays {state}{{card {id}}} to {}",
        describe(at)
    ));
    Some(id)
}

pub fn play_reflections(ctx: &mut Ctx, owner: u8, at: Location, count: usize) -> Vec<u32> {
    if !recruits_playable_at(ctx, at) {
        ctx.narrate(format!(
            "no Reflection · units can't be played at {}",
            describe(at)
        ));
        return Vec::new();
    }
    (0..count)
        .filter_map(|_| spawn_reflection(ctx, owner, at, REFLECTION_ARRIVES_READY))
        .collect()
}

pub fn is_a_token_face(name: &str) -> bool {
    is_token_name(name) || matches!(name, REFLECTION | RECRUIT | MECH_TOKEN | BIRD)
}

pub fn wears_a_copied_face(ctx: &Ctx, token: u32) -> bool {
    ctx.is_token(token)
        && ctx.on_board(token)
        && ctx
            .card(token)
            .is_some_and(|held| !is_a_token_face(&held.name))
}

pub fn become_copy_of(ctx: &mut Ctx, token: u32, of: u32) -> bool {
    if !ctx.on_board(token) || !ctx.on_board(of) {
        return false;
    }
    if !ctx.is_token(token) {
        ctx.narrate(format!(
            "{{card {token}}} cannot become a copy yet · non-token copying is not supported"
        ));
        return false;
    }
    let Some(face) = ctx
        .card(of)
        .map(|card| card.face())
        .filter(|face| !face.is_hidden())
    else {
        return false;
    };
    ctx.emit(Effect::Transform { card: token, face });
    ctx.narrate(format!("{{card {token}}} becomes a copy of {{card {of}}}"));
    true
}

fn unmask(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} has left the board · no Reflection"));
        return done();
    };
    for reflection in play_reflections(ctx, item.controller, here, REFLECTIONS) {
        become_copy_of(ctx, reflection, me);
    }
    done()
}

pub static CARD: Card = unit(
    "Keeper of Masks",
    &[Keyword::Hidden, Keyword::Temporary],
    &[play(&[], unmask)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, IMPLICIT_TEMPORARY};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, play as play_engine, priority, settle, triggers};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const KEEPER: u32 = 90;
    const MIND_RUNES: [u32; 2] = [46, 47];

    fn keeper(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(KEEPER, zone, 0, "Keeper of Masks", 1);
        card.domain = vec!["Mind".into()];
        card.energy = Some(2);
        card
    }

    fn masquerade(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(keeper(zone));
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KEEPER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn reflections_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == REFLECTION && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_hidden_temporary_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(script_of("Keeper of Masks").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Temporary]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(
            ability.targets.is_empty(),
            "the Reflections are played here"
        );
        let face = reflection_face_until_token_reflection_lands();
        assert_eq!(face.name, REFLECTION);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(REFLECTION_MIGHT), "187.6 · 0 Might");
        assert!(face.domain.is_empty(), "187.6 · domainless");
        assert_eq!(REFLECTIONS, 2);
    }

    #[test]
    fn played_to_a_battlefield_it_plays_two_exhausted_copies_of_the_keeper() {
        let mut fixture = masquerade(fixtures::HAND);
        let action = fixtures::move_action(KEEPER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(
            &mut ctx,
            0,
            KEEPER,
            Origin::Hand,
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.is_temporary(KEEPER));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KEEPER
        ));
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert!(reflections_of(&ctx, 0).is_empty());
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let reflections = [next, next + 1];
        for reflection in reflections {
            assert!(ctx.is_token(reflection));
            assert!(ctx.is_unit(reflection));
            assert_eq!(
                ctx.location(reflection),
                Some(Location::Battlefield(fixtures::BF1)),
                "here · where the Keeper was played"
            );
            assert_eq!(ctx.controller(reflection), 0);
            assert!(
                ctx.card(reflection).unwrap().exhausted,
                "they enter exhausted"
            );
            assert!(ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == reflection
            )));
            assert!(ctx.blob.log.contains(&format!(
                "{{card {reflection}}} becomes a copy of {{card {KEEPER}}}"
            )));
            assert!(wears_a_copied_face(&ctx, reflection));
        }
        assert!(!is_reflection(&ctx, KEEPER));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [KEEPER, next, next + 1]
        );
        assert!(reflections_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_keeper_and_its_reflections_are_owed_to_the_next_beginning_phase() {
        let mut fixture = masquerade(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let reflections = play_reflections(&mut ctx, 0, Location::Base(0), REFLECTIONS);
        assert_eq!(reflections.len(), 2);
        settle(&mut ctx).unwrap();
        assert_eq!(
            triggers::find(&ctx, &Event::BeginningPhase { seat: 0 })
                .into_iter()
                .map(|held| (held.source, held.index))
                .collect::<Vec<_>>(),
            [(KEEPER, IMPLICIT_TEMPORARY)],
            "816 · the Keeper is Temporary; a Reflection that is not yet a copy is not"
        );
        phases::start_turn(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(!ctx.on_board(KEEPER));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KEEPER}}} is Temporary and dies")));
    }

    #[test]
    fn a_keeper_killed_in_response_has_no_here_and_plays_nothing() {
        let mut fixture = masquerade(fixtures::HAND);
        let action = fixtures::move_action(KEEPER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, KEEPER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        ctx.kill(KEEPER, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(KEEPER));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(reflections_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KEEPER}}} has left the board · no Reflection"
        )));
        assert!(
            !become_copy_of(&mut ctx, 999, KEEPER),
            "nothing to copy onto"
        );
        assert!(
            !wears_a_copied_face(&ctx, fixtures::SPRITE),
            "a Sprite wears its own face"
        );
        assert!(
            !wears_a_copied_face(&ctx, fixtures::VI),
            "a card is never a copy"
        );
    }

    #[test]
    fn the_reflections_become_copies_of_the_keeper_and_the_engine_knows_the_token_name() {
        let mut fixture = masquerade(fixtures::HAND);
        let action = fixtures::move_action(KEEPER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, KEEPER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        for reflection in [next, next + 1] {
            assert!(wears_a_copied_face(&ctx, reflection));
            assert_eq!(ctx.card(reflection).unwrap().name, "Keeper of Masks");
            assert_eq!(ctx.current_might(reflection), 1);
            assert!(ctx.is_temporary(reflection));
            assert!(ctx.is_token(reflection));
        }
        assert!(crate::cards::is_token_name(REFLECTION));
    }
}

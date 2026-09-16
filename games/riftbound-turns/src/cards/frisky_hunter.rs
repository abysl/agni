use super::faithful_manufactor::recruits_playable_at;
use super::prelude::{done, location_of, play, unit, Location};
use super::{Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march::describe;
use crate::state::{Expiry, Origin};
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Face;

pub const BIRD: &str = "Bird";
pub const BIRD_MIGHT: u8 = 1;
pub const BIRD_DEFLECT: u8 = 1;
pub const BIRD_ARRIVES_READY: bool = false;

pub fn bird_face_until_token_bird_lands() -> Face {
    Face::named(BIRD)
        .with_kind(KIND_UNIT)
        .with_might(Some(BIRD_MIGHT))
}

pub fn is_bird(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| held.name == BIRD && held.is_kind(KIND_UNIT))
}

pub fn spawn_bird(ctx: &mut Ctx, owner: u8, at: Location) -> Option<u32> {
    let (zone, seat) = ctx.zone_of(at)?;
    let id = ctx.table.next_id;
    ctx.emit(Effect::Spawn {
        face: bird_face_until_token_bird_lands(),
        zone,
        seat,
        owner: Some(owner),
    });
    ctx.spawned += 1;
    ctx.grant(id, Keyword::Deflect(BIRD_DEFLECT), Expiry::Permanent);
    if !BIRD_ARRIVES_READY {
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

pub fn play_birds(ctx: &mut Ctx, owner: u8, at: Location, count: usize) -> Vec<u32> {
    if !recruits_playable_at(ctx, at) {
        ctx.narrate(format!(
            "no Bird · units can't be played at {}",
            describe(at)
        ));
        return Vec::new();
    }
    (0..count)
        .filter_map(|_| spawn_bird(ctx, owner, at))
        .collect()
}

pub fn play_birds_here(ctx: &mut Ctx, item: &Item, count: usize) -> Vec<u32> {
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} has left the board · no Bird"));
        return Vec::new();
    };
    play_birds(ctx, item.controller, here, count)
}

fn flush(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_birds_here(ctx, item, 1);
    done()
}

pub static CARD: Card = unit("Frisky Hunter", &[], &[play(&[], flush)]);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, play as play_engine, priority, settle};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HUNTER: u32 = 90;
    const CALM_RUNES: [u32; 3] = [46, 47, 48];

    static NO_NEST: Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("No Nest", &[], &[]),
        &[crate::cards::Static::NoUnitsPlayedHere],
    );

    pub fn birds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == BIRD && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    pub fn calm_unit(id: u32, zone: u16, seat: u8, name: &str, might: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, name, might);
        card.domain = vec!["Calm".into()];
        card.energy = Some(4);
        card
    }

    pub fn with_calm_runes(fixture: &mut Fixture, seat: u8) {
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, seat, "Calm", false));
        }
    }

    fn glade() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(calm_unit(HUNTER, fixtures::HAND, 0, "Frisky Hunter", 3));
        with_calm_runes(&mut fixture, 0);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HUNTER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pounce(ctx: &mut Ctx, at: Location) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, HUNTER, Origin::Hand, Some(at))?;
        settle(ctx)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(script_of("Frisky Hunter").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(ability.targets.is_empty(), "the Bird is played here");
        let face = bird_face_until_token_bird_lands();
        assert_eq!(face.name, BIRD);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(BIRD_MIGHT));
        assert!(face.domain.is_empty(), "187.7 · domainless");
        assert_eq!(BIRD_DEFLECT, 1);
    }

    #[test]
    fn played_to_a_battlefield_it_plays_an_exhausted_one_might_bird_with_deflect_there() {
        let mut fixture = glade();
        let action = fixtures::move_action(HUNTER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        pounce(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HUNTER
        ));
        assert!(
            birds_of(&ctx, 0).is_empty(),
            "the Bird waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds, [next]);
        let bird = birds[0];
        assert_eq!(
            ctx.location(bird),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · where the Hunter was played"
        );
        assert!(ctx.is_token(bird));
        assert!(ctx.is_unit(bird));
        assert!(is_bird(&ctx, bird));
        assert!(!is_bird(&ctx, HUNTER));
        assert_eq!(ctx.current_might(bird), i32::from(BIRD_MIGHT));
        assert_eq!(
            ctx.deflect_of(bird),
            BIRD_DEFLECT,
            "187.7 · it carries Deflect"
        );
        assert!(ctx.has_keyword(bird, Keyword::Deflect(1)));
        assert!(!ctx.is_temporary(bird));
        assert!(ctx.card(bird).unwrap().domain.is_empty());
        assert_eq!(ctx.controller(bird), 0);
        assert!(ctx.card(bird).unwrap().exhausted, "it enters exhausted");
        assert!(ctx.effects.contains(&Effect::exhaust(bird)));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == BIRD && *zone == fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == bird
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {bird}}} to {{zone 9}}")));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [HUNTER, bird]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_opponents_spell_choosing_the_bird_pays_its_deflect_and_its_own_controllers_does_not() {
        let mut fixture = glade();
        let mut ctx = fixture.ctx();
        let bird = spawn_bird(&mut ctx, 0, Location::Base(0)).unwrap();
        let theirs = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        assert_eq!(
            cost::deflect(&ctx, &theirs, Some(TargetRef::Card(bird))),
            usize::from(BIRD_DEFLECT),
            "809 · an opponent pays one rainbow to choose it"
        );
        let mine = ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(cost::deflect(&ctx, &mine, Some(TargetRef::Card(bird))), 0);
        ctx.kill(bird, Cause::Rule);
        assert!(!ctx.on_board(bird));
        assert!(ctx.effects.contains(&Effect::Despawn { card: bird }));
        assert!(
            ctx.card(bird).is_none(),
            "185 · a token leaving the board ceases to exist"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_the_base_the_bird_lands_in_the_base() {
        let mut fixture = glade();
        let action = fixtures::move_action(HUNTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        pounce(&mut ctx, Location::Base(0)).unwrap();
        resolve_chain(&mut ctx);
        let bird = *birds_of(&ctx, 0).first().expect("the Hunter's Bird");
        assert_eq!(ctx.location(bird), Some(Location::Base(0)));
        assert_eq!(
            ctx.units_at(Location::Base(0)).len(),
            3,
            "Vi, the Hunter and the Bird"
        );
        assert!(birds_of(&ctx, 1).is_empty());
    }

    #[test]
    fn a_hunter_killed_in_response_has_no_here_and_plays_no_bird() {
        let mut fixture = glade();
        let action = fixtures::move_action(HUNTER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        pounce(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        ctx.kill(HUNTER, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(HUNTER));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(birds_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {HUNTER}}} has left the board · no Bird")));
    }

    #[test]
    fn a_battlefield_where_units_cannot_be_played_refuses_the_bird() {
        let mut fixture = glade();
        fixture.table.card_mut(HUNTER).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(HUNTER).unwrap().seat = 0;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_NEST);
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF1));
        let item = ChainItem::new(
            7,
            ItemKind::Trigger {
                source: HUNTER,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(flush(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(birds_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Bird · units can't be played at")));
    }

    #[test]
    #[ignore = "engine gap · Token::Bird: engine/ctx.rs has no Bird face, so bird_face_until_token_bird_lands builds it and spawn_bird replays Ctx::spawn with a permanent Deflect grant; with the token, spawn_bird is spawn(ctx, owner, Token::Bird, at, BIRD_ARRIVES_READY), the face carries the Bird tag and Deflect, and cards/mod.rs knows the name"]
    fn the_engine_knows_the_bird_as_a_token_name() {
        assert!(crate::cards::is_token_name(BIRD));
    }
}

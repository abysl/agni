use super::prelude::{done, location_of, play, spawn, unit, Location, Token};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

const SPRITE_ARRIVES_READY: bool = true;

fn playable(ctx: &Ctx, at: Location) -> bool {
    match at {
        Location::Battlefield(zone) => ctx.units_played_here(zone),
        Location::Base(_) => true,
    }
}

fn mother(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} has left the board · no Sprite"));
        return done();
    };
    if !playable(ctx, here) {
        ctx.narrate(format!(
            "no Sprite · units can't be played at {}",
            describe(here)
        ));
        return done();
    }
    if let Some(sprite) = spawn(ctx, seat, Token::Sprite, here, SPRITE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to {}",
            describe(here)
        ));
    }
    done()
}

pub static CARD: Card = unit("Sprite Mother", &[], &[play(&[], mother)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Static, Trigger, KIND_UNIT, TOKEN_SPRITE};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const MOTHER: u32 = 90;
    const MIND_RUNE: u32 = 46;

    static NO_NURSERY: Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("No Nursery", &[], &[]),
        &[Static::NoUnitsPlayedHere],
    );

    fn sprite_mother(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::card(id, zone, seat, "Sprite Mother", "Unit")
        }
    }

    fn glade() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(sprite_mother(MOTHER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn nest(ctx: &mut Ctx, at: Location) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, MOTHER, Origin::Hand, Some(at))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn sprites_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_unit_with_a_targetless_play_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Sprite Mother").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Sprite Mother");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty(), "the Sprite is played here");
    }

    #[test]
    fn played_to_a_battlefield_the_mother_plays_a_ready_three_might_temporary_sprite_there() {
        let mut fixture = glade();
        let action = fixtures::move_action(MOTHER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        nest(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(MOTHER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MOTHER
        ));
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "the Sprite waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let sprite = *sprites_of(&ctx, 0).first().expect("the mother's Sprite");
        assert_eq!(sprite, next);
        assert_eq!(
            ctx.location(sprite),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · where the mother was played"
        );
        assert_eq!(ctx.card(sprite).unwrap().kind.as_deref(), Some(KIND_UNIT));
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.current_might(sprite), 3);
        assert!(ctx.is_temporary(sprite));
        assert!(!ctx.card(sprite).unwrap().exhausted, "played ready");
        assert!(!ctx.effects.contains(&Effect::exhaust(sprite)));
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { zone, seat: 0, owner: Some(0), .. } if *zone == fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == sprite
        )));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| { line.starts_with(&format!("{{seat 0}} plays {{card {sprite}}} to")) }));
        assert_eq!(
            sprites_of(&ctx, 1),
            [fixtures::SPRITE],
            "the new Sprite is ours; the enemy keeps only the one it had"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_the_base_the_sprite_lands_in_the_base() {
        let mut fixture = glade();
        let action = fixtures::move_action(MOTHER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        nest(&mut ctx, Location::Base(0)).unwrap();
        resolve_chain(&mut ctx);
        let sprite = *sprites_of(&ctx, 0).first().expect("the mother's Sprite");
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
        assert_eq!(
            ctx.units_at(Location::Base(0)).len(),
            3,
            "Vi, the mother and the Sprite"
        );
    }

    #[test]
    fn a_mother_killed_in_response_has_no_here_and_plays_no_sprite() {
        let mut fixture = glade();
        let action = fixtures::move_action(MOTHER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        nest(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        ctx.kill(MOTHER, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(MOTHER));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(sprites_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MOTHER}}} has left the board · no Sprite")));
    }

    #[test]
    fn a_battlefield_where_units_cannot_be_played_refuses_the_sprite() {
        let mut fixture = glade();
        fixture.table.card_mut(MOTHER).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(MOTHER).unwrap().seat = 0;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_NURSERY);
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF1));
        let item = crate::state::ChainItem::new(
            7,
            ItemKind::Trigger {
                source: MOTHER,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(mother(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(sprites_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Sprite · units can't be played at")));
    }
}

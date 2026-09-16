use super::prelude::{done, play, spawn, triggered, unit, Location, Token};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const SPRITE_ARRIVES_READY: bool = true;
pub const ON_PLAY: u8 = 0;
pub const AT_BEGINNING: u8 = 1;

fn court(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(sprite) = spawn(
        ctx,
        seat,
        Token::Sprite,
        Location::Base(seat),
        SPRITE_ARRIVES_READY,
    ) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to their base"
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Sprite Queen",
    &[],
    &[
        play(&[], court),
        triggered(Trigger::BeginningPhase, &[], court),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TOKEN_SPRITE};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const QUEEN: u32 = 90;
    const MIND_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn queen(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(QUEEN, zone, seat, "Sprite Queen", 6);
        card.domain = vec!["Mind".into()];
        card.energy = Some(7);
        card.power = Some(1);
        card
    }

    fn court_of(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(queen(zone, 0));
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(QUEEN).unwrap(), &CARD));
        fixture
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

    fn queen_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, index } if source == QUEEN => Some(index),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn a_ready_temporary_sprite_in_the_base(ctx: &Ctx, sprite: u32) {
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.current_might(sprite), 3);
        assert!(ctx.is_temporary(sprite));
        assert!(!ctx.card(sprite).unwrap().exhausted, "played ready");
        assert_eq!(ctx.controller(sprite), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {sprite}}} to their base")));
    }

    #[test]
    fn the_script_is_a_plain_unit_with_a_play_trigger_and_a_beginning_phase_trigger() {
        assert!(std::ptr::eq(script_of("Sprite Queen").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let on_play = &CARD.abilities[usize::from(ON_PLAY)];
        assert_eq!(on_play.trigger, Trigger::Play);
        let at_beginning = &CARD.abilities[usize::from(AT_BEGINNING)];
        assert_eq!(at_beginning.trigger, Trigger::BeginningPhase);
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty(), "the Sprite goes to your base");
            assert!(!ability.optional);
            assert!(ability.cost.is_none() && ability.condition.is_none());
        }
    }

    #[test]
    fn played_to_a_battlefield_she_plays_a_ready_temporary_sprite_to_the_base() {
        let mut fixture = court_of(fixtures::HAND);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let action = fixtures::move_action(QUEEN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(
            &mut ctx,
            0,
            QUEEN,
            Origin::Hand,
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(QUEEN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(queen_items(&ctx), [ON_PLAY]);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "the Sprite waits for the chain"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(sprites_of(&ctx, 0), [next]);
        a_ready_temporary_sprite_in_the_base(&ctx, next);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == next
        )));
        assert_eq!(sprites_of(&ctx, 1), [fixtures::SPRITE]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn her_controllers_beginning_phase_plays_another_sprite_that_outlives_the_last_one() {
        let mut fixture = court_of(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let first = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(sprites_of(&ctx, 0), [first]);
        let next = ctx.table.next_id;
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OrderTriggers { seat: 0 }),
            "her trigger and the old Sprite's Temporary form one batch"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {QUEEN}}} trigger 2")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.phase(), Some(Phase::Action), "the turn moved on");
        assert!(
            !ctx.on_board(first),
            "816 · the last Sprite died before scoring"
        );
        assert_eq!(sprites_of(&ctx, 0), [next]);
        a_ready_temporary_sprite_in_the_base(&ctx, next);
        assert!(
            ctx.on_board(next),
            "it arrived after the batch, so this Beginning Phase never touched it"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_opponents_beginning_phase_and_a_queen_in_hand_play_nothing() {
        let mut fixture = court_of(fixtures::BASE);
        let core = fixture.blob.core_mut().unwrap();
        core.turn = 2;
        core.player = 1;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        phases::start_turn(&mut ctx);
        assert!(queen_items(&ctx).is_empty(), "not her controller's phase");
        resolve_chain(&mut ctx);
        assert!(sprites_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = court_of(fixtures::HAND);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(queen_items(&ctx).is_empty(), "384.1 · only on the board");
        resolve_chain(&mut ctx);
        assert!(sprites_of(&ctx, 0).is_empty());
    }
}

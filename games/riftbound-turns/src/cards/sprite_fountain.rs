use super::prelude::{deathknell, done, gear, play, spawn, Location, Token};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const SPRITE_ARRIVES_READY: bool = true;

fn fountain(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if spawn(
        ctx,
        seat,
        Token::Sprite,
        Location::Base(seat),
        SPRITE_ARRIVES_READY,
    )
    .is_some()
    {
        ctx.narrate(format!("{{seat {seat}}} plays a Sprite"));
    }
    done()
}

pub static CARD: Card = gear(
    "Sprite Fountain",
    &[Keyword::Temporary, Keyword::Deathknell],
    &[play(&[], fountain), deathknell(&[], fountain)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, IMPLICIT_TEMPORARY, TOKEN_SPRITE};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play as play_engine, priority, settle, triggers};
    use crate::state::{ItemKind, Origin, Phase};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const FOUNTAIN: u32 = 90;
    const MIND_RUNE: u32 = 46;

    fn fountain_face(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::card(id, zone, seat, "Sprite Fountain", "Gear")
        }
    }

    fn poor(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fountain_face(FOUNTAIN, zone, 0));
        fixture.resolve();
        fixture
    }

    fn funded(zone: u16) -> Fixture {
        let mut fixture = poor(zone);
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.resolve();
        fixture
    }

    fn sprites(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn chain_sources(ctx: &Ctx) -> Vec<u32> {
        ctx.blob
            .chain
            .iter()
            .map(|item| item.kind.source())
            .collect()
    }

    fn refuse(fixture: &mut Fixture, seat: u8) -> Refusal {
        let action = fixtures::move_action(FOUNTAIN, fixtures::BASE, 0);
        let ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        legal::classify(&ctx, seat, &entry).expect_err("the play is refused")
    }

    fn pass_twice(ctx: &mut Ctx) {
        let first = priority::holder(ctx).expect("someone holds priority");
        priority::pass(ctx, first).unwrap();
        let second = priority::holder(ctx).expect("priority moves on");
        priority::pass(ctx, second).unwrap();
    }

    #[test]
    fn the_fountain_is_a_temporary_deathknell_gear_whose_play_and_death_run_one_effect() {
        assert_eq!(CARD.name, "Sprite Fountain");
        assert!(CARD.has_keyword(Keyword::Temporary));
        assert!(CARD.has_keyword(Keyword::Deathknell));
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(
            CARD.abilities[1].trigger,
            Trigger::Death,
            "the Deathknell repeats the play effect"
        );
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty(), "the Sprite goes to your base");
            assert!(!ability.optional);
            assert!(ability.cost.is_none());
            assert!(ability.condition.is_none());
            assert_eq!(
                ability.run as *const () as usize, fountain as *const () as usize,
                "one body, repeated"
            );
        }
        let fixture = funded(fixtures::HAND);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FOUNTAIN).unwrap(),
            &CARD
        ));
        assert!(std::ptr::eq(
            super::super::script_of("Sprite Fountain").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn playing_the_fountain_queues_its_play_trigger_which_plays_a_ready_temporary_sprite() {
        let mut fixture = funded(fixtures::HAND);
        let action = fixtures::move_action(FOUNTAIN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let next = ctx.table.next_id;
        play_engine::begin(&mut ctx, 0, FOUNTAIN, Origin::Hand, None).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(FOUNTAIN).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.is_gear(FOUNTAIN));
        assert!(
            ctx.is_temporary(FOUNTAIN),
            "742: the keyword makes the gear Temporary"
        );
        assert_eq!(
            ctx.table
                .counter(Target::Card(FOUNTAIN), crate::rules::COUNTER_TEMPORARY),
            Some(1),
            "and the mirror is stamped so kai draws the glyph"
        );
        assert_eq!(chain_sources(&ctx), [FOUNTAIN]);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FOUNTAIN
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "the fountain asks nothing");
        assert!(
            sprites(&ctx, 0).is_empty(),
            "the Sprite waits for the chain"
        );
        pass_twice(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let sprite = *sprites(&ctx, 0).first().expect("one Sprite in the base");
        assert_eq!(sprite, next);
        assert!(ctx.is_unit(sprite));
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.card(sprite).unwrap().name, TOKEN_SPRITE);
        assert_eq!(ctx.current_might(sprite), 3);
        assert_eq!(ctx.card(sprite).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(sprite).unwrap().owner, 0);
        assert!(!ctx.card(sprite).unwrap().exhausted, "played ready");
        assert!(ctx.is_temporary(sprite));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays a Sprite".to_string()));
    }

    #[test]
    fn the_beginning_phase_kills_the_fountain_then_its_deathknell_plays_a_surviving_sprite() {
        let mut fixture = funded(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let next = ctx.table.next_id;
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(chain_sources(&ctx), [FOUNTAIN]);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == FOUNTAIN && index == IMPLICIT_TEMPORARY
        ));
        assert!(ctx.on_board(FOUNTAIN), "742.1.b kills it on resolution");
        pass_twice(&mut ctx);
        assert!(!ctx.on_board(FOUNTAIN));
        assert_eq!(ctx.card(FOUNTAIN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FOUNTAIN}}} is Temporary and dies")));
        assert_eq!(
            chain_sources(&ctx),
            [FOUNTAIN],
            "734.1.d.2: the Deathknell was queued before the gear left"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == FOUNTAIN
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(sprites(&ctx, 0).is_empty());
        assert_eq!(
            ctx.blob.phase(),
            Some(Phase::Beginning),
            "scoring waits for the whole chain"
        );
        pass_twice(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let sprite = *sprites(&ctx, 0).first().expect("the Deathknell's Sprite");
        assert_eq!(sprite, next);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action), "the turn moved on");
        assert!(
            ctx.on_board(sprite),
            "it arrived after the batch, so this Beginning Phase never touched it"
        );
        assert!(!ctx.card(sprite).unwrap().exhausted);
        assert!(ctx.is_temporary(sprite));
        assert_eq!(
            triggers::find(&ctx, &Event::BeginningPhase { seat: 0 })
                .into_iter()
                .map(|held| (held.controller, held.source, held.index))
                .collect::<Vec<_>>(),
            [(0, sprite, IMPLICIT_TEMPORARY)],
            "it is owed to the next Beginning Phase instead"
        );
    }

    #[test]
    fn a_fountain_killed_by_an_enemy_still_pays_its_deathknell_to_its_own_controller() {
        let mut fixture = funded(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        assert_eq!(ctx.kill(FOUNTAIN, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(FOUNTAIN).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(chain_sources(&ctx), [FOUNTAIN]);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.controller),
            Some(0)
        );
        let theirs = sprites(&ctx, 1);
        pass_twice(&mut ctx);
        assert_eq!(sprites(&ctx, 1), theirs, "your base, not the killer's");
        let sprite = *sprites(&ctx, 0).first().expect("the Deathknell's Sprite");
        assert_eq!(ctx.card(sprite).unwrap().seat, 0);
        assert!(ctx.is_temporary(sprite));
    }

    #[test]
    fn a_fountain_the_rune_pool_cannot_pay_for_is_refused_and_plays_no_sprite() {
        let mut fixture = poor(fixtures::HAND);
        assert_eq!(
            refuse(&mut fixture, 0),
            Refusal::NoPowerOf,
            "no Mind rune in the pool"
        );
        assert_eq!(
            fixture.table.card(FOUNTAIN).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(fixture.blob.chain.is_empty());
        assert!(fixture.blob.queue.is_empty());
        assert!(!fixture
            .table
            .cards
            .iter()
            .any(|card| card.name == TOKEN_SPRITE && card.owner == 0));

        let mut fixture = funded(fixtures::HAND);
        assert_eq!(
            refuse(&mut fixture, 1),
            Refusal::Illegal(Reason::NotYourCard)
        );
        assert_eq!(
            fixture.table.card(FOUNTAIN).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(fixture.blob.chain.is_empty());
        assert!(fixture.blob.queue.is_empty());
        assert!(!fixture
            .table
            .cards
            .iter()
            .any(|card| card.name == TOKEN_SPRITE && card.owner == 0));
    }
}

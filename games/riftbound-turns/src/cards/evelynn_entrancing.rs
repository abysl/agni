use super::prelude::{
    card_target, done, hiding_battlefield, move_unit, on_play_from_facedown, optional, target,
    unit, when, Location,
};
use super::{Card, Event, Filter, Flow, Item, Keyword, Source, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_ELSEWHERE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::Not(&Filter::Here),
    Filter::Movable,
]);

pub const LURE: TargetSpec = target(
    ENEMY_UNIT_ELSEWHERE,
    0,
    1,
    TargetKind::Card,
    "an enemy unit at a different location to move here",
);

fn on_your_turn(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.turn_player() == ctx.controller(source.card)
}

fn allure(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(zone) = hiding_battlefield(ctx, item) else {
        ctx.narrate(format!("{{card {me}}} is not at a battlefield"));
        return done();
    };
    move_unit(ctx, item, unit, Location::Battlefield(zone));
    done()
}

pub static CARD: Card = unit(
    "Evelynn - Entrancing",
    &[Keyword::Hidden, Keyword::Backline],
    &[when(
        optional(on_play_from_facedown(&[LURE], allure)),
        on_your_turn,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Moved;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, settle};
    use crate::state::{GameBlob, ItemKind, Mode, Origin, Phase, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const EVELYNN: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const THEIR_SCOUT: u32 = 92;

    fn evelynn(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(EVELYNN, zone, seat, "Evelynn - Entrancing", 2)
        }
    }

    fn lair(turn_player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, turn_player, Mode::Enforced);
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.push(evelynn(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCOUT, fixtures::BASE, 1, "Scout", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.card_state_mut(EVELYNN).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EVELYNN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_facedown(ctx: &mut Ctx) {
        play_engine::begin(
            ctx,
            0,
            EVELYNN,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(ctx).unwrap();
        fixtures::pass_until_open(ctx);
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 0 } if source == EVELYNN)
            })
            .map(|pending| pending.item.id)
            .expect("her trigger is pending")
    }

    #[test]
    fn the_script_prints_hidden_and_backline_with_one_optional_played_from_hidden_trigger() {
        assert!(std::ptr::eq(
            script_of("Evelynn - Entrancing").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Evelynn - Entrancing");
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Backline]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let entrance = &CARD.abilities[0];
        assert_eq!(entrance.trigger, Trigger::PlayFromFacedown);
        assert!(entrance.optional, "you may");
        assert!(entrance.condition.is_some(), "on your turn");
        assert_eq!(entrance.targets, [LURE]);
        assert_eq!((LURE.min, LURE.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(LURE.kind, TargetKind::Card);
        assert_eq!(LURE.filter, ENEMY_UNIT_ELSEWHERE);
    }

    #[test]
    fn played_from_face_down_on_your_turn_she_offers_enemies_elsewhere_and_the_pick_moves_here() {
        let mut fixture = lair(0);
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx);
        assert_eq!(
            ctx.location(EVELYNN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = [
            format!("{{card {}}}", fixtures::SPRITE),
            format!("{{card {}}}", fixtures::THEIR_UNIT),
            format!("{{card {THEIR_SCOUT}}}"),
            "skip".to_string(),
        ];
        expected.sort();
        assert_eq!(
            offered, expected,
            "the enemies in their base and at the other battlefield · not the brute already here"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy at her own battlefield is not at a different location"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is not an enemy"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == EVELYNN
        ));
        assert_eq!(
            ctx.blob.chain.last().unwrap().targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "the move waits for the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Battlefield(zone), .. }
                if *card == fixtures::SPRITE && *zone == fixtures::BF1
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_may_moves_nothing() {
        let mut fixture = lair(0);
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.location(THEIR_SCOUT), Some(Location::Base(1)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_face_down_on_the_opponents_turn_she_asks_nothing() {
        let mut fixture = lair(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        play_facedown(&mut ctx);
        assert_eq!(
            ctx.location(EVELYNN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_effect_moves_the_pick_to_her_battlefield_and_a_pick_that_left_moves_nothing() {
        let mut fixture = lair(0);
        let mut ctx = fixture.ctx();
        let mut item = Item::new(
            7,
            ItemKind::Trigger {
                source: EVELYNN,
                index: 0,
            },
            0,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
        );
        item.targets = vec![TargetRef::Card(THEIR_SCOUT)];
        item.spec_counts = vec![1];
        assert_eq!(allure(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(
            ctx.location(THEIR_SCOUT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.bounce(fixtures::SPRITE));
        item.targets = vec![TargetRef::Card(fixtures::SPRITE)];
        let moves = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::Moved { .. }))
            .count();
        assert_eq!(allure(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Moved { .. }))
                .count(),
            moves,
            "a target that left the board is not moved"
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                crate::engine::ctx::MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(ctx.fault.is_none());
    }
}

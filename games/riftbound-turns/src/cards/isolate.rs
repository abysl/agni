use super::emperors_divide::send_home;
use super::prelude::{a_card, alone_there, card_target, done, draw, play, spell, Location};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::MovableToBase,
    Filter::Movable,
]);

pub fn an_enemy_stands_alone_at(ctx: &Ctx, seat: u8, zone: u16) -> bool {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .any(|unit| ctx.controller(unit) != seat && alone_there(ctx, unit))
}

fn isolate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(Location::Battlefield(zone)) = ctx.location(unit) else {
        return done();
    };
    send_home(ctx, item, unit);
    let seat = item.controller;
    if an_enemy_stands_alone_at(ctx, seat, zone) {
        let drawn = draw(ctx, seat, DRAWS);
        ctx.narrate(format!(
            "an enemy unit is alone at {{zone {zone}}} · {{seat {seat}}} draws {drawn}"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Isolate",
    &[],
    &[play(
        &[a_card(
            ENEMY_UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME,
            "an enemy unit at a battlefield",
        )],
        isolate,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const ISOLATE: u32 = 90;
    const THEIR_ISOLATE: u32 = 91;
    const SECOND: u32 = 92;
    const MINE: u32 = 93;

    fn isolate_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Isolate", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn crowded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(isolate_card(ISOLATE, 0));
        fixture.table.cards.push(isolate_card(THEIR_ISOLATE, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF2, 1, "Brute", 4));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn hand_size(ctx: &Ctx, seat: u8) -> usize {
        ctx.hand_of(seat).len()
    }

    #[test]
    fn the_script_is_a_plain_spell_over_one_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Isolate").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty(), "no Action, no Reaction");
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(
            ability.targets[0].filter,
            ENEMY_UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME
        );
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_enemy_goes_home_and_the_one_it_leaves_behind_alone_draws_you_a_card() {
        let mut fixture = crowded();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ISOLATE).unwrap();
        let before = hand_size(&ctx, 0);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "the enemy units at battlefields; Jinx in their base is out of reach"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SECOND)]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(SECOND), Some(Location::Base(1)));
        assert!(ctx.events.contains(&Event::Moved {
            card: SECOND,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} moves to their base".to_string()));
        assert_eq!(
            hand_size(&ctx, 0),
            before + 1,
            "the Sprite is alone there now"
        );
        assert!(ctx.blob.log.contains(&format!(
            "an enemy unit is alone at {{zone {}}} · {{seat 0}} draws 1",
            fixtures::BF2
        )));
        assert_eq!(ctx.card(ISOLATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_last_enemy_at_a_battlefield_goes_home_and_nothing_is_drawn() {
        let mut fixture = crowded();
        fixture.table.cards.retain(|card| card.id != SECOND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ISOLATE).unwrap();
        let before = hand_size(&ctx, 0);
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert_eq!(hand_size(&ctx, 0), before, "nobody is left behind");
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_friendly_unit_beside_the_remaining_enemy_still_leaves_that_enemy_alone() {
        let mut fixture = crowded();
        fixture
            .table
            .cards
            .push(fixtures::unit(MINE, fixtures::BF2, 0, "Vanguard", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!an_enemy_stands_alone_at(&ctx, 0, fixtures::BF2));
        fixtures::play_from_hand(&mut ctx, 0, ISOLATE).unwrap();
        let before = hand_size(&ctx, 0);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            an_enemy_stands_alone_at(&ctx, 0, fixtures::BF2),
            "740.2.a · alone means no other friendly unit, so your Vanguard does not keep the Sprite company"
        );
        assert_eq!(hand_size(&ctx, 0), before + 1);
    }

    #[test]
    fn a_target_that_went_home_before_resolution_is_left_alone_and_draws_nothing() {
        let mut fixture = crowded();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ISOLATE).unwrap();
        let before = hand_size(&ctx, 0);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        ctx.actor = 1;
        ctx.move_unit(SECOND, Location::Base(1), MoveCause::Effect);
        ctx.actor = 0;
        let moves = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::Moved { .. }))
            .count();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Moved { .. }))
                .count(),
            moves,
            "356.3.e · a unit in its base is no longer a legal target"
        );
        assert_eq!(hand_size(&ctx, 0), before);
        assert_eq!(ctx.card(ISOLATE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_friendly_units_and_enemies_in_their_base() {
        let mut fixture = crowded();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ISOLATE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, ISOLATE).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ISOLATE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}

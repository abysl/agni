use super::prelude::{
    a_card, card_target, charm_destination, deal, done, move_unit, play, spell, target,
    CHARM_DESTINATION, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const FLOW: Cost = Cost {
    energy: 3,
    power: &[Power::Rainbow],
};

const MOVED: usize = 0;
const DESTINATION: usize = 1;
const STRUCK: usize = 2;

pub const UP_TO_ONE_ENEMY_AT_A_BATTLEFIELD: TargetSpec = target(
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]),
    0,
    1,
    TargetKind::Card,
    "up to one enemy unit at a battlefield to deal 2",
);

fn flip(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(enemy) = card_target(ctx, item, STRUCK) {
        if deal(ctx, item, enemy, DAMAGE) {
            ctx.narrate(format!("{{card {enemy}}} takes {DAMAGE}"));
        }
    }
    if let Some(unit) = card_target(ctx, item, MOVED) {
        if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
            move_unit(ctx, item, unit, to);
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Shuriken Flip",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[
            a_card(MOVABLE_FRIENDLY_UNIT, "a friendly unit to move"),
            CHARM_DESTINATION,
            UP_TO_ONE_ENEMY_AT_A_BATTLEFIELD,
        ],
        flip,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const FLIP: u32 = 90;
    const THEIR_FLIP: u32 = 91;
    const BRUTE: u32 = 92;
    const SPARE_RUNE: u32 = 100;

    fn flip_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Shuriken Flip", 1, 1);
        card.domain = vec!["Fury".into(), "Calm".into()];
        card
    }

    fn dojo(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(flip_card(FLIP, zone, 0));
        fixture
            .table
            .cards
            .push(flip_card(THEIR_FLIP, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Fury", false));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
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

    #[test]
    fn the_script_is_a_flow_sorcery_over_a_friendly_mover_its_destination_and_up_to_one_enemy() {
        assert!(std::ptr::eq(script_of("Shuriken Flip").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 3);
        assert_eq!(ability.targets[MOVED].filter, MOVABLE_FRIENDLY_UNIT);
        assert_eq!(ability.targets[DESTINATION], CHARM_DESTINATION);
        assert_eq!(ability.targets[STRUCK], UP_TO_ONE_ENEMY_AT_A_BATTLEFIELD);
        assert_eq!(
            (ability.targets[STRUCK].min, ability.targets[STRUCK].max),
            (0, 1)
        );
        assert_eq!(DAMAGE, 2);
    }

    #[test]
    fn two_land_on_the_enemy_at_a_battlefield_and_then_the_friendly_unit_moves() {
        let mut fixture = dojo(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "my units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 60}".to_string(),
                format!("{{card {BRUTE}}}"),
                "skip".to_string(),
                "cancel".to_string()
            ],
            "enemy units at battlefields; Jinx in her base is out, and the strike is a may"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Card(BRUTE)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(ctx.on_board(BRUTE), "4 Might survives 2");
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::VI,
            from: Some(Location::Base(0)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        let takes = ctx
            .blob
            .log
            .iter()
            .position(|line| line == &format!("{{card {BRUTE}}} takes 2"))
            .unwrap();
        let moves = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{card 50} moves to {zone 9}")
            .unwrap();
        assert!(takes < moves, "the damage lands before the move");
        assert_eq!(ctx.card(FLIP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_strike_may_be_skipped_and_the_move_still_happens() {
        let mut fixture = dojo(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLIP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF2)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "Vi contests the Sprite's battlefield"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn two_kill_a_small_enemy_and_from_the_trash_the_spell_is_banished() {
        let mut fixture = dojo(fixtures::TRASH);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            FLIP,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "the 2-Might Jinx dies to 2"
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: true, .. } if *card == fixtures::THEIR_UNIT)
        ));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.banished_of(0), [FLIP]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn an_enemy_mover_a_base_enemy_and_the_other_seats_turn_are_refused() {
        let mut fixture = dojo(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_FLIP)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, FLIP).unwrap();
        for wrong in [fixtures::THEIR_UNIT, BRUTE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::VI] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 2, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or friendly"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FLIP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}

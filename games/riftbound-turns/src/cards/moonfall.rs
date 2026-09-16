use super::prelude::{
    a_battlefield_with_your_units, card_target, done, might_this_turn, move_destinations,
    move_unit, play, spell, target, zone_target, Location,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = -2;
const BATTLEFIELD: usize = 0;
const PULLED: usize = 1;

pub const UP_TO_ONE_ENEMY_UNIT_TO_PULL: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Enemy,
        Filter::Movable,
        Filter::DifferentLocationFrom(0),
    ]),
    0,
    1,
    TargetKind::Card,
    "up to one enemy unit to move there",
);

pub fn you_have_units_at(ctx: &Ctx, seat: u8, zone: u16) -> bool {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .any(|unit| ctx.controller(unit) == seat)
}

pub fn enemies_at(ctx: &Ctx, seat: u8, zone: u16) -> Vec<u32> {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn moonfall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let Some(zone) = zone_target(item, BATTLEFIELD).filter(|zone| ctx.zones.is_battlefield(*zone))
    else {
        return done();
    };
    if !you_have_units_at(ctx, seat, zone) {
        ctx.narrate(format!(
            "{{card {}}} · {{seat {seat}}} has no unit at {{zone {zone}}} any more",
            item.kind.source()
        ));
        return done();
    }
    let to = Location::Battlefield(zone);
    if let Some(unit) = card_target(ctx, item, PULLED) {
        if move_destinations(ctx, unit).contains(&to) {
            move_unit(ctx, item, unit, to);
        }
    }
    let enemies = enemies_at(ctx, seat, zone);
    if enemies.is_empty() {
        ctx.narrate(format!("no enemy unit at {{zone {zone}}}"));
    }
    for unit in enemies {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets {MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Moonfall",
    &[Keyword::Action],
    &[play(
        &[
            a_battlefield_with_your_units("a battlefield where you have units"),
            UP_TO_ONE_ENEMY_UNIT_TO_PULL,
        ],
        moonfall,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::BATTLEFIELD_WITH_YOUR_UNITS;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const MOONFALL: u32 = 90;
    const THEIR_MOONFALL: u32 = 91;
    const ALLY: u32 = 92;
    const FOE_THERE: u32 = 93;
    const MIND_RUNE: u32 = 46;

    fn moonfall_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Moonfall", 3, 1);
        card.domain = vec!["Mind".into(), "Chaos".into()];
        card
    }

    fn night() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(moonfall_card(MOONFALL, 0));
        fixture.table.cards.push(moonfall_card(THEIR_MOONFALL, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Vanguard", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(FOE_THERE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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
    fn the_script_is_an_action_over_a_battlefield_and_up_to_one_enemy_unit_to_pull_there() {
        assert!(std::ptr::eq(script_of("Moonfall").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.targets[0].filter, BATTLEFIELD_WITH_YOUR_UNITS);
        assert_eq!(ability.targets[1], UP_TO_ONE_ENEMY_UNIT_TO_PULL);
        assert_eq!((ability.targets[1].min, ability.targets[1].max), (0, 1));
        assert_eq!(MIGHT, -2);
        let mut fixture = night();
        let ctx = fixture.ctx();
        assert!(you_have_units_at(&ctx, 0, fixtures::BF1));
        assert!(!you_have_units_at(&ctx, 0, fixtures::BF2));
        assert!(you_have_units_at(&ctx, 1, fixtures::BF2));
        assert_eq!(enemies_at(&ctx, 0, fixtures::BF1), [FOE_THERE]);
        assert_eq!(enemies_at(&ctx, 1, fixtures::BF1), [ALLY]);
    }

    #[test]
    fn an_enemy_is_pulled_to_your_battlefield_and_every_enemy_there_loses_two_might() {
        let mut fixture = night();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MOONFALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "skip", "cancel"],
            "the enemies elsewhere; the Brute already there is not a move, and the pull is a may"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0, "2 - 2");
        assert_eq!(ctx.current_might(FOE_THERE), 2, "4 - 2");
        assert_eq!(ctx.current_might(ALLY), 2, "yours is untouched");
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3, "elsewhere");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FOE_THERE}}} gets -2 might this turn")));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "the pulled unit contests your battlefield"
        );
        assert_eq!(ctx.card(MOONFALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_pull_may_be_skipped_and_the_enemies_already_there_still_shrink() {
        let mut fixture = night();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MOONFALL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF1)]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(FOE_THERE), 2);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_battlefield_your_units_left_before_resolution_gets_nothing() {
        let mut fixture = night();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MOONFALL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.move_unit(ALLY, Location::Base(0), MoveCause::Effect);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(ctx.current_might(FOE_THERE), 4);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MOONFALL}}} · {{seat 0}} has no unit at {{zone {}}} any more",
            fixtures::BF1
        )));
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_a_base_a_friendly_unit_or_a_unit_already_there() {
        let mut fixture = night();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_MOONFALL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, MOONFALL).unwrap();
        for wrong in [fixtures::BASE, fixtures::TRASH] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        for wrong in [fixtures::VI, ALLY, FOE_THERE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit elsewhere"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MOONFALL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn only_battlefields_where_you_have_units_are_offered() {
        let mut fixture = night();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MOONFALL).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{zone {}}}", fixtures::BF1), "cancel".to_string()],
            "no unit of yours stands at the second battlefield"
        );
    }
}

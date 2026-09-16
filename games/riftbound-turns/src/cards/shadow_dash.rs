use super::moonfall::you_have_units_at;
use super::prelude::{
    a_card, card_target, charm_destination, done, might_this_turn, move_unit, play, spell, target,
    zone_target, Location, MOVABLE_ENEMY_UNIT,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Power, Rel, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const EXACTLY: usize = 2;
pub const FLOW: Cost = Cost {
    energy: 5,
    power: &[Power::Rainbow, Power::Rainbow],
};

const PULLED: usize = 0;
const BATTLEFIELD: usize = 1;

pub const A_BATTLEFIELD_ELSEWHERE: TargetSpec = target(
    Filter::And(&[
        Filter::AtBattlefield,
        Filter::ZoneWithUnits(Rel::Friendly),
        Filter::DifferentLocationFrom(0),
    ]),
    1,
    1,
    TargetKind::Zone,
    "a battlefield where you have units",
);

pub fn your_units_at(ctx: &Ctx, seat: u8, zone: u16) -> Vec<u32> {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == seat)
        .collect()
}

fn dash(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let Some(unit) = card_target(ctx, item, PULLED) else {
        return done();
    };
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
    let Some(to) = charm_destination(ctx, item, unit, BATTLEFIELD) else {
        return done();
    };
    move_unit(ctx, item, unit, to);
    let yours = your_units_at(ctx, seat, zone);
    if yours.len() != EXACTLY {
        ctx.narrate(format!(
            "{{seat {seat}}} has {} unit(s) at {{zone {zone}}}, not exactly {EXACTLY}",
            yours.len()
        ));
        return done();
    }
    for unit in yours {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Shadow Dash",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[
            a_card(MOVABLE_ENEMY_UNIT, "an enemy unit to move"),
            A_BATTLEFIELD_ELSEWHERE,
        ],
        dash,
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
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DASH: u32 = 90;
    const THEIR_DASH: u32 = 91;
    const ALLY: u32 = 92;
    const SECOND: u32 = 93;
    const CALM_RUNE: u32 = 100;
    const ORDER_RUNE: u32 = 101;

    fn dash_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Shadow Dash", 2, 1);
        card.domain = vec!["Calm".into(), "Order".into()];
        card
    }

    fn dusk(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dash_card(DASH, zone, 0));
        fixture
            .table
            .cards
            .push(dash_card(THEIR_DASH, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Vanguard", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Recruit", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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
    fn the_script_is_a_flow_sorcery_over_an_enemy_unit_and_a_battlefield_elsewhere() {
        assert!(std::ptr::eq(script_of("Shadow Dash").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[PULLED].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(ability.targets[BATTLEFIELD], A_BATTLEFIELD_ELSEWHERE);
        assert_eq!(ability.targets[BATTLEFIELD].kind, TargetKind::Zone);
        assert_eq!((MIGHT, EXACTLY), (1, 2));
        let mut fixture = dusk(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(your_units_at(&ctx, 0, fixtures::BF1), [ALLY, SECOND]);
        assert!(your_units_at(&ctx, 0, fixtures::BF2).is_empty());
        assert_eq!(your_units_at(&ctx, 1, fixtures::BF2), [fixtures::SPRITE]);
    }

    #[test]
    fn the_enemy_is_pulled_to_your_pair_and_both_of_yours_get_one_this_turn() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "enemy units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "battlefields where you have units only; Jinx's base and the empty battlefield are out"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Zone(fixtures::BF1)
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
        assert_eq!(ctx.current_might(ALLY), 3, "2 + 1");
        assert_eq!(ctx.current_might(SECOND), 2, "1 + 1");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "the pulled enemy gets nothing"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the unit in base gets nothing"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} gets +1 this turn")));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "the pulled unit contests your battlefield"
        );
        assert_eq!(ctx.card(DASH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_one_or_three_of_yours_there_the_move_happens_and_nobody_grows() {
        let mut fixture = dusk(fixtures::HAND);
        fixture.table.card_mut(SECOND).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has 1 unit(s) at {zone 9}, not exactly 2".to_string()));
        drop(ctx);

        let mut crowded = dusk(fixtures::HAND);
        crowded.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        crowded.resolve();
        let mut ctx = crowded.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(ALLY), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has 3 unit(s) at {zone 9}, not exactly 2".to_string()));
    }

    #[test]
    fn a_battlefield_without_your_units_at_resolution_moves_nothing() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        for unit in [ALLY, SECOND] {
            ctx.move_unit(unit, Location::Base(0), MoveCause::Effect);
        }
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DASH}}} · {{seat 0}} has no unit at {{zone {}}} any more",
            fixtures::BF1
        )));
        assert_eq!(ctx.card(DASH).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn from_the_trash_the_flow_play_pulls_the_enemy_and_is_banished() {
        let mut fixture = dusk(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            DASH,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(ALLY), 3);
        assert_eq!(ctx.banished_of(0), [DASH]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn a_friendly_unit_a_base_and_the_other_seats_turn_are_refused() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DASH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        for wrong in [fixtures::VI, ALLY, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        for wrong in [fixtures::BASE, fixtures::BF2, fixtures::TRASH] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is a base, where the Sprite stands, or no battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DASH).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn only_battlefields_where_you_have_units_are_offered() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DASH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "no unit of yours stands at the second battlefield"
        );
    }
}

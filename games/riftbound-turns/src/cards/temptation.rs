use super::prelude::{
    a_card, card_target, charm_destination, done, move_destinations, move_unit, play, spell,
    target, zone_target, Location, MOVABLE_ENEMY_UNIT,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Rel, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};
const UNIT: usize = 0;
const DESTINATION: usize = 1;

pub const A_LOCATION_WITH_A_UNIT_OF_THE_SAME_CONTROLLER: TargetSpec = target(
    Filter::And(&[
        Filter::DifferentLocationFrom(UNIT as u8),
        Filter::ZoneWithUnits(Rel::SameControllerAs(UNIT as u8)),
    ]),
    1,
    1,
    TargetKind::Zone,
    "a location where there's a unit with the same controller",
);

pub fn a_unit_of_the_same_controller_stands_at(ctx: &Ctx, unit: u32, at: Location) -> bool {
    let controller = ctx.controller(unit);
    ctx.units_at(at)
        .into_iter()
        .any(|other| other != unit && ctx.controller(other) == controller)
}

pub fn destinations_with_a_unit_of_the_same_controller(ctx: &Ctx, unit: u32) -> Vec<Location> {
    move_destinations(ctx, unit)
        .into_iter()
        .filter(|to| a_unit_of_the_same_controller_stands_at(ctx, unit, *to))
        .collect()
}

fn tempt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    let Some(to) = zone_target(item, DESTINATION)
        .and_then(|zone| Location::of_zone(zone, ctx.controller(unit), &ctx.zones))
    else {
        return done();
    };
    if ctx.location(unit) == Some(to) {
        ctx.narrate(format!(
            "{{card {unit}}} already stands at {} · it stays",
            describe(to)
        ));
        return done();
    }
    if !a_unit_of_the_same_controller_stands_at(ctx, unit, to) {
        ctx.narrate(format!(
            "{{card {unit}}} stays · no unit with the same controller at {}",
            describe(to)
        ));
        return done();
    }
    if charm_destination(ctx, item, unit, DESTINATION) != Some(to) {
        return done();
    }
    move_unit(ctx, item, unit, to);
    done()
}

pub static CARD: Card = spell(
    "Temptation",
    &[Keyword::Repeat(REPEAT)],
    &[play(
        &[
            a_card(MOVABLE_ENEMY_UNIT, "an enemy unit"),
            A_LOCATION_WITH_A_UNIT_OF_THE_SAME_CONTROLLER,
        ],
        tempt,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const TEMPTATION: u32 = 90;
    const THEIR_TEMPTATION: u32 = 91;
    const MY_EXTRA: [u32; 2] = [46, 47];

    fn temptation(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Temptation", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(temptation(TEMPTATION, 0));
        fixture.table.cards.push(temptation(THEIR_TEMPTATION, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
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
    fn the_script_is_a_repeatable_plain_spell_over_an_enemy_unit_and_where_it_goes() {
        assert!(std::ptr::eq(script_of("Temptation").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(
            ability.targets[1],
            A_LOCATION_WITH_A_UNIT_OF_THE_SAME_CONTROLLER
        );
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
    }

    #[test]
    fn the_seam_reads_only_locations_holding_a_unit_of_the_moved_units_controller() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            destinations_with_a_unit_of_the_same_controller(&ctx, fixtures::SPRITE),
            [Location::Base(1)],
            "Jinx sits in the enemy base"
        );
        assert_eq!(
            destinations_with_a_unit_of_the_same_controller(&ctx, fixtures::THEIR_UNIT),
            [Location::Battlefield(fixtures::BF2)],
            "the Sprite holds the battlefield"
        );
        assert!(
            destinations_with_a_unit_of_the_same_controller(&ctx, fixtures::VI).is_empty(),
            "Vi is my only unit"
        );
    }

    #[test]
    fn an_enemy_unit_is_pulled_to_where_its_controller_already_has_a_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "only the other seat's units"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "nothing moves until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::SPRITE
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} moves to their base".to_string()));
        assert_eq!(ctx.card(TEMPTATION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_destination_without_a_unit_of_the_same_controller_leaves_the_unit_where_it_is() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF2),
            MoveCause::Effect,
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, .. } if *card == fixtures::SPRITE
        )));
        assert!(ctx.blob.log.contains(
            &"{card 60} stays · no unit with the same controller at their base".to_string()
        ));
        assert_eq!(ctx.card(TEMPTATION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_moved_onto_its_destination_in_response_stays_and_says_why() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        ctx.move_unit(fixtures::SPRITE, Location::Base(1), MoveCause::Effect);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Moved { card, .. } if *card == fixtures::SPRITE
                ))
                .count(),
            1,
            "the response moved it; the spell does not move it again"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} already stands at their base · it stays".to_string()));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("no unit with the same controller")));
        assert_eq!(ctx.card(TEMPTATION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_moves_two_enemy_units_and_the_second_may_follow_the_first() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 3 }));
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1, 1, 1]);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "the Sprite joins Jinx in the base"
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Base(1)),
            "the second execution reads the board after the first: the Sprite has left {{zone 10}}"
        );
        assert!(ctx.blob.log.contains(
            &"{card 81} stays · no unit with the same controller at {zone 10}".to_string()
        ));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_unit_is_refused_and_the_spell_has_no_window_off_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_TEMPTATION)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(fixtures::BF2)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "where it already is"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(TEMPTATION).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_second_groups_destinations_are_measured_from_that_groups_own_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 10}", "cancel"],
            "Jinx sits in her base, so the Sprite's battlefield is her destination"
        );
    }

    #[test]
    fn only_locations_holding_a_unit_of_the_same_controller_are_offered() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TEMPTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "cancel"]);
    }
}

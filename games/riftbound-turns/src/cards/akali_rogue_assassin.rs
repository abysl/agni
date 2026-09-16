use super::prelude::{
    a_card, activated, card_target, chosen_card, done, empower, exhausting_self, is_empowered,
    legend, move_unit, named, ready, Location, Moved,
};
use super::syndra_transcendent::in_a_showdown;
use super::{Card, Cost, Filter, Flow, Item, Keyword, Power, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[Power::Rainbow],
};

pub const FRIENDLY_UNIT_IN_A_SHOWDOWN_TO_BASE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::InShowdown,
    Filter::MovableToBase,
]);

pub fn slip_away(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if !ctx.blob.is_turn_player(seat) {
        ctx.narrate(format!(
            "{{card {me}}} · it is not {{seat {seat}}}'s turn · nothing moves"
        ));
        return done();
    }
    let Some(unit) = chosen_card(item, 0).filter(|unit| ctx.is_unit(*unit)) else {
        return done();
    };
    if !in_a_showdown(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is not in a showdown · it stays"));
        return done();
    }
    if card_target(ctx, item, 0) != Some(unit) {
        return done();
    }
    let home = Location::Base(ctx.controller(unit));
    if move_unit(ctx, item, unit, home).is_none_or(|moved| moved == Moved::NotAUnit) {
        return done();
    }
    if is_empowered(ctx, me) && ready(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} readies"));
    }
    done()
}

pub static CARD: Card = legend(
    "Akali - Rogue Assassin",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        named(
            exhausting_self(activated(
                Timing::Action,
                Cost::FREE,
                &[a_card(
                    FRIENDLY_UNIT_IN_A_SHOWDOWN_TO_BASE,
                    "a friendly unit in a showdown to move to base",
                )],
                slip_away,
            )),
            "move a friendly unit in a showdown to base",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, MoveCause, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const AKALI: u32 = fixtures::LEGEND_CARD;
    const SCOUT: u32 = 90;
    const THIRD_FIELD: u32 = 91;
    const LAIR: u32 = 92;

    fn contested(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(AKALI).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(fixtures::card(
            THIRD_FIELD,
            fixtures::BF3,
            0,
            "Aspirant's Climb",
            "Battlefield",
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF3, 0, "Scout", 1));
        for unit in [fixtures::VI, SCOUT] {
            fixture.table.apply(&Effect::exhaust(unit), 0).unwrap();
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(AKALI),
                counter: COUNTER_EMPOWERED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert_eq!(
            ctx.blob
                .showdown
                .as_ref()
                .map(|held| (held.zone, held.focus())),
            Some((fixtures::BF1, 0)),
            "the contested battlefield opens a showdown with the attacker's focus"
        );
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_prints_empower_and_one_free_action_exhaust_choosing_a_friendly_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].label, Some("empower"));
        assert_eq!(CARD.abilities[0].cost, Some(EMPOWER));
        let slip = &CARD.abilities[1];
        assert_eq!(slip.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(slip.cost, Some(Cost::FREE));
        assert_eq!(slip.self_cost, SelfCost::Exhaust);
        assert!(slip.usable.is_none(), "the if is part of the effect");
        assert_eq!(slip.targets[0].filter, FRIENDLY_UNIT_IN_A_SHOWDOWN_TO_BASE);
    }

    #[test]
    fn in_a_showdown_on_her_turn_she_moves_the_chosen_unit_home_still_exhausted() {
        let mut fixture = contested(false);
        let mut ctx = fixture.ctx();
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "before: {:?}",
            ctx.card(fixtures::VI)
        );
        open_showdown(&mut ctx);
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "after open: {:?}",
            ctx.effects
        );
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            ctx.card(AKALI).unwrap().exhausted,
            "the exhaust is her cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing until it resolves"
        );
        resolve_all(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Base(0), .. } if *card == fixtures::VI
        )));
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "unempowered · the move keeps the unit exhausted: {:?} {:?}",
            ctx.effects,
            ctx.blob.log
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empowered_she_readies_the_unit_she_moved_home() {
        let mut fixture = contested(true);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == fixtures::VI
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} readies", fixtures::VI)));
    }

    #[test]
    fn on_the_opponents_turn_the_ability_resolves_into_nothing_and_a_unit_outside_the_showdown_stays(
    ) {
        let mut fixture = contested(true);
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "if it's your turn · it is not"
        );
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {AKALI}}} · it is not {{seat 0}}'s turn · nothing moves"
        )));
        drop(ctx);
        let mut fixture = contested(true);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF3),
            MoveCause::Effect,
        );
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF3)),
            "Vi left for the Scout's battlefield, where no showdown is open"
        );
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is not in a showdown · it stays",
            fixtures::VI
        )));
    }

    #[test]
    fn a_unit_that_can_no_longer_move_to_base_at_resolution_stays_in_the_showdown() {
        let mut fixture = contested(true);
        fixture.table.cards.push(fixtures::card(
            LAIR,
            fixtures::BF2,
            0,
            "Vilemaw's Lair",
            "Battlefield",
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        ctx.table.card_mut(LAIR).unwrap().zone = Some(fixtures::BF1);
        assert!(!ctx.movable_to_base(fixtures::VI));
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "an illegal target is unaffected as the item resolves (359.3.e.5)"
        );
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, .. } if *card == fixtures::VI
        )));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("is not in a showdown")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_akali_the_wrong_seat_and_a_closed_chain_are_refused() {
        let mut spent = contested(false);
        spent.table.card_mut(AKALI).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        open_showdown(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, AKALI, 1),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, AKALI, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut closed = contested(false);
        let mut ctx = closed.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, AKALI, 1),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
    }

    #[test]
    fn only_units_at_the_showdowns_battlefield_are_offered() {
        let mut fixture = contested(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, AKALI, 1),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "no showdown is open"
        );
        open_showdown(&mut ctx);
        activate::activate(&mut ctx, 0, AKALI, 1).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()]
        );
    }

    #[test]
    fn resolving_empower_makes_her_empowered() {
        let mut fixture = contested(false);
        fixture.blob.set_contested(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AKALI, 0).unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(AKALI));
        assert_eq!(
            activate::activate(&mut ctx, 0, AKALI, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
    }
}

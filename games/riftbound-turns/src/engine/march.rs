use crate::cards::{Keyword, Static};
use crate::engine::ctx::{is_unit_face, Ctx, Location, MoveCause, Moved};
use crate::engine::legal::Reason;
use crate::engine::{attach, cleanup};
use crate::state::{ChainItem, PromptWhy, FLAG_NO_MOVE_BY_OWNER};
use crate::Refusal;
use agni_plugin_sdk::decide::{Effect, TOP};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swapped {
    Swapped,
    SameLocation,
    NotUnits,
    Capped,
}

pub fn legal_destination(
    ctx: &Ctx,
    unit: u32,
    from: Location,
    to: Location,
) -> Result<(), Refusal> {
    route(ctx, unit, from, to)?;
    if ctx.destination_capped(unit, to) {
        return Err(Refusal::Illegal(Reason::TwoOtherSeats));
    }
    Ok(())
}

fn route(ctx: &Ctx, unit: u32, from: Location, to: Location) -> Result<(), Refusal> {
    if from == to {
        return Err(Refusal::Illegal(Reason::SameLocation));
    }
    if from.battlefield().is_some()
        && to
            .battlefield()
            .is_some_and(|zone| !ctx.moves_here_from_anywhere(zone))
        && !ctx.has_keyword(unit, Keyword::Ganking)
    {
        return Err(Refusal::Illegal(Reason::NeedsGanking));
    }
    if base_closed(ctx, unit, from, to) {
        return Err(Refusal::Illegal(Reason::NoMoveToBase));
    }
    Ok(())
}

pub fn base_closed(ctx: &Ctx, unit: u32, from: Location, to: Location) -> bool {
    match (from, to) {
        (Location::Battlefield(zone), Location::Base(_)) => {
            !ctx.moves_to_base_from(zone) || !ctx.unit_moves_to_base(unit)
        }
        _ => false,
    }
}

pub fn item_cannot_move(ctx: &Ctx, item: &ChainItem, unit: u32) -> bool {
    item.controller != ctx.controller(unit) && ctx.has_static(unit, Static::NoMoveByEnemy)
}

pub fn effect_move(ctx: &mut Ctx, item: &ChainItem, unit: u32, to: Location) -> Option<Moved> {
    let from = ctx.location(unit)?;
    if item_cannot_move(ctx, item, unit) {
        ctx.narrate(format!(
            "{{card {unit}}} can't be moved by {{card {}}}",
            item.kind.source()
        ));
        return None;
    }
    if base_closed(ctx, unit, from, to) {
        ctx.narrate(format!(
            "{{card {unit}}} can't move from {} to base",
            describe(from)
        ));
        return None;
    }
    let moved = ctx.move_unit(unit, to, MoveCause::Effect);
    match moved {
        Moved::Moved => ctx.narrate(format!("{{card {unit}}} moves to {}", describe(to))),
        Moved::Recalled => ctx.narrate(format!("{{card {unit}}} is recalled instead")),
        Moved::NotAUnit => {}
    }
    Some(moved)
}

pub fn standard_move(ctx: &mut Ctx, seat: u8, unit: u32, from: Location, to: Location) {
    ctx.exhaust(unit);
    ctx.arrived(unit, Some(from), to, MoveCause::Standard);
    attach::follow_unit(ctx, unit);
    ctx.narrate(format!(
        "{{seat {seat}}} moves {{card {unit}}} to {}",
        describe(to)
    ));
    let others = companions(ctx, seat, unit, to);
    if others.is_empty() {
        cleanup::run(ctx, None);
        return;
    }
    let Some((zone, _)) = ctx.zone_of(to) else {
        cleanup::run(ctx, None);
        return;
    };
    ctx.ask(
        seat,
        0,
        others.len() as u8,
        false,
        PromptWhy::GroupMove { unit, to: zone },
    );
}

pub fn swap_units(ctx: &mut Ctx, first: u32, second: u32) -> Swapped {
    if first == second || !ctx.is_unit(first) || !ctx.is_unit(second) {
        return Swapped::NotUnits;
    }
    let (Some(from_first), Some(from_second)) = (ctx.location(first), ctx.location(second)) else {
        return Swapped::NotUnits;
    };
    if from_first == from_second {
        return Swapped::SameLocation;
    }
    if ctx.destination_capped(first, from_second) || ctx.destination_capped(second, from_first) {
        return Swapped::Capped;
    }
    let (Some(first_to), Some(second_to)) = (ctx.zone_of(from_second), ctx.zone_of(from_first))
    else {
        return Swapped::NotUnits;
    };
    ctx.emit(Effect::Move {
        card: first,
        zone: first_to.0,
        seat: first_to.1,
        index: TOP,
    });
    ctx.emit(Effect::Move {
        card: second,
        zone: second_to.0,
        seat: second_to.1,
        index: TOP,
    });
    ctx.arrived(first, Some(from_first), from_second, MoveCause::Swap);
    ctx.arrived(second, Some(from_second), from_first, MoveCause::Swap);
    attach::follow_unit(ctx, first);
    attach::follow_unit(ctx, second);
    ctx.narrate(format!(
        "{{card {first}}} and {{card {second}}} swap places · {} and {}",
        describe(from_second),
        describe(from_first)
    ));
    Swapped::Swapped
}

impl Ctx<'_> {
    pub fn swap_units(&mut self, first: u32, second: u32) -> Swapped {
        swap_units(self, first, second)
    }
}

pub fn effect_destinations(ctx: &Ctx, unit: u32) -> Vec<Location> {
    let from = ctx.location(unit);
    let mut open = vec![Location::Base(ctx.controller(unit))];
    open.extend(
        ctx.zones
            .battlefields
            .iter()
            .copied()
            .map(Location::Battlefield),
    );
    open.retain(|to| Some(*to) != from && !ctx.destination_capped(unit, *to));
    open.retain(|to| from.is_none_or(|from| !base_closed(ctx, unit, from, *to)));
    open
}

pub fn describe(at: Location) -> String {
    match at {
        Location::Base(_) => "their base".to_string(),
        Location::Battlefield(zone) => format!("{{zone {zone}}}"),
    }
}

pub fn companions(ctx: &Ctx, seat: u8, unit: u32, to: Location) -> Vec<u32> {
    let capped = match to {
        Location::Battlefield(zone) => ctx.other_seats_at(zone, seat) >= 2,
        Location::Base(_) => false,
    };
    if capped {
        return Vec::new();
    }
    ctx.table
        .cards
        .iter()
        .filter(|card| card.id != unit && ctx.controller(card.id) == seat && !card.exhausted)
        .filter(|card| is_unit_face(card) && !card.is_hidden())
        .filter(|card| !ctx.has_flag(card.id, FLAG_NO_MOVE_BY_OWNER))
        .filter(|card| !ctx.is_pending_play(card.id) && !ctx.is_facedown(card.id))
        .filter(|card| {
            card.zone
                .and_then(|zone| Location::of_zone(zone, card.seat, &ctx.zones))
                .is_some_and(|from| route(ctx, card.id, from, to).is_ok())
        })
        .map(|card| card.id)
        .collect()
}

pub fn group_move(ctx: &mut Ctx, seat: u8, to_zone: u16, picked: &[u32]) {
    let Some(to) = Location::of_zone(to_zone, seat, &ctx.zones) else {
        cleanup::run(ctx, None);
        return;
    };
    for unit in picked {
        if ctx.controller(*unit) != seat || !ctx.is_unit(*unit) {
            continue;
        }
        let Some(from) = ctx.location(*unit) else {
            continue;
        };
        if legal_destination(ctx, *unit, from, to).is_err() {
            continue;
        }
        if ctx.move_unit(*unit, to, MoveCause::Standard) == Moved::Moved {
            ctx.exhaust(*unit);
            ctx.narrate(format!(
                "{{seat {seat}}} moves {{card {unit}}} to {}",
                describe(to)
            ));
        }
    }
    cleanup::run(ctx, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, showdown, triggers};
    use crate::state::{GameBlob, Staged};
    use agni_plugin_sdk::decide::{Effect, TOP};

    #[test]
    fn a_standard_move_exhausts_the_unit_marks_the_contest_and_opens_the_showdown() {
        let mut fixture = Fixture::enforced();
        let action = fixtures::move_action(fixtures::VI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(ctx.effects, [Effect::exhaust(fixtures::VI)]);
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        assert_eq!(
            ctx.blob.staged.len(),
            1,
            "staged until the Moved event is collected"
        );
        assert!(ctx.blob.showdown.is_none());
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        assert!(ctx.blob.staged.is_empty());
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the single contest opens at once");
        assert_eq!(
            (
                showdown.zone,
                showdown.attacker,
                showdown.defender,
                showdown.combat
            ),
            (fixtures::BF1, 0, 1, false)
        );
        assert_eq!(showdown.focus(), 0);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.log,
            [
                "{seat 0} moves {card 50} to {zone 9}",
                "showdown at {zone 9} · {seat 0} against {seat 1}"
            ]
        );
        let mut fixture = Fixture::enforced();
        let held = fixtures::move_action(fixtures::VI, fixtures::BF2, 0);
        let mut ctx = fixture.ctx_for(0, &held);
        standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF2),
        );
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert!(showdown.combat);
        assert_eq!(showdown.defender, 1);
        assert!(ctx
            .blob
            .log
            .last()
            .unwrap()
            .starts_with("combat at {zone 10}"));
    }

    #[test]
    fn other_ready_units_are_offered_a_group_move_before_the_showdown_stages() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BASE, 0, "Jinx", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(91, fixtures::BF1, 0, "Caitlyn", 2));
        let mut tired = fixtures::unit(92, fixtures::BASE, 0, "Ekko", 2);
        tired.exhausted = true;
        fixture.table.cards.push(tired);
        fixture.resolve();
        let action = fixtures::move_action(fixtures::VI, fixtures::BF2, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF2),
        );
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove {
                unit: fixtures::VI,
                to: fixtures::BF2
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.min, prompt.max, prompt.cancel), (0, 1, false));
        let options = prompts::offered(&ctx);
        assert_eq!(
            options
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 90}", "done"]
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::GroupMove {
                    unit: fixtures::VI,
                    to: fixtures::BF2
                }
            ),
            "move others to {zone 10} too?"
        );
        ctx.blob.close_prompt();
        group_move(&mut ctx, 0, fixtures::BF2, &[90, 91, fixtures::THEIR_UNIT]);
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(fixtures::VI),
                Effect::Move {
                    card: 90,
                    zone: fixtures::BF2,
                    seat: 0,
                    index: TOP
                },
                Effect::exhaust(90),
                Effect::Annotate {
                    card: fixtures::SPRITE,
                    key: "defender".into(),
                    value: Some(vec![1])
                },
                Effect::Annotate {
                    card: fixtures::VI,
                    key: "attacker".into(),
                    value: Some(vec![1])
                },
                Effect::Annotate {
                    card: 90,
                    key: "attacker".into(),
                    value: Some(vec![1])
                },
            ],
            "the combat showdown designates every unit at the battlefield"
        );
        assert_eq!(ctx.units_at(Location::Battlefield(fixtures::BF2)).len(), 3);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert!(showdown.combat);
        assert_eq!(showdown.attacker, 0);
        assert!(ctx.is_attacker(fixtures::VI) && ctx.is_attacker(90));
        assert!(ctx.is_defender(fixtures::SPRITE));
        fixture.blob = GameBlob::start(2, 0, crate::state::Mode::Enforced);
        let back = fixtures::move_action(91, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &back);
        standard_move(
            &mut ctx,
            0,
            91,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn an_effect_move_reaches_every_battlefield_and_the_units_own_base_without_ganking() {
        let mut fixture = Fixture::enforced();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            effect_destinations(&ctx, fixtures::VI),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2),
            ],
            "a unit in its base is offered every battlefield and not the base it stands in"
        );
        assert_eq!(
            effect_destinations(&ctx, fixtures::SPRITE),
            [Location::Base(1), Location::Battlefield(fixtures::BF1)],
            "an enemy unit at a battlefield goes home to its OWN base, and battlefield to \
             battlefield needs no Ganking (427.1)"
        );
        assert_eq!(
            route(
                &ctx,
                fixtures::SPRITE,
                Location::Battlefield(fixtures::BF2),
                Location::Battlefield(fixtures::BF1)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "the standard move still refuses that route (143.4)"
        );
        let mut four = Fixture::enforced();
        four.table.players = 4;
        four.blob = crate::state::GameBlob::start(4, 0, crate::state::Mode::Enforced);
        four.table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 2, "Jinx", 2));
        four.table
            .cards
            .push(fixtures::unit(91, fixtures::BF1, 3, "Vex", 2));
        four.resolve();
        let ctx = four.ctx();
        assert!(
            !effect_destinations(&ctx, fixtures::VI)
                .contains(&Location::Battlefield(fixtures::BF1)),
            "427.2 caps a battlefield two other seats already hold units at"
        );
    }

    #[test]
    fn a_swap_contesting_two_battlefields_stages_two_showdowns_and_the_turn_player_picks() {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, crate::state::Mode::Enforced);
        fixture.blob.set_phase(crate::state::Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            swap_units(&mut ctx, fixtures::VI, fixtures::VI),
            Swapped::NotUnits
        );
        assert_eq!(
            swap_units(&mut ctx, fixtures::VI, fixtures::GROUNDS),
            Swapped::NotUnits
        );
        assert_eq!(
            swap_units(&mut ctx, fixtures::VI, 90),
            Swapped::Swapped,
            "two friendly units at different battlefields trade places"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.location(90), Some(Location::Battlefield(fixtures::BF1)));
        assert_eq!(
            ctx.effects,
            [
                Effect::Move {
                    card: fixtures::VI,
                    zone: fixtures::BF2,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 90,
                    zone: fixtures::BF1,
                    seat: 0,
                    index: TOP
                },
            ],
            "one batch: both moves are emitted before either arrival is processed"
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "428 · arriving at the other seat's battlefield contests it"
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            None,
            "the holder's own unit arriving at its held battlefield contests nothing"
        );
        assert_eq!(
            swap_units(&mut ctx, fixtures::VI, 90),
            Swapped::Swapped,
            "and back again"
        );
        assert_eq!(swap_units(&mut ctx, fixtures::VI, 90), Swapped::Swapped);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Moved {
                        cause: MoveCause::Swap,
                        ..
                    }
                ))
                .count(),
            6
        );
        drop(ctx);
        let mut both = Fixture::enforced();
        both.blob = GameBlob::start(2, 0, crate::state::Mode::Enforced);
        both.blob.set_phase(crate::state::Phase::Action);
        both.blob.seats = vec![Default::default(); 2];
        both.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        both.table.cards.retain(|card| card.id != fixtures::SPRITE);
        both.table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Jinx", 2));
        both.blob.set_holder(fixtures::BF1, Some(1));
        both.blob.set_holder(fixtures::BF2, Some(1));
        both.resolve();
        let mut ctx = both.ctx();
        assert_eq!(swap_units(&mut ctx, fixtures::VI, 90), Swapped::Swapped);
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        cleanup::run(&mut ctx, None);
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        assert_eq!(
            ctx.blob.staged,
            [
                Staged {
                    zone: fixtures::BF1,
                    combat: false,
                    contester: 0
                },
                Staged {
                    zone: fixtures::BF2,
                    combat: false,
                    contester: 0
                },
            ],
            "322.12 · both contests are staged as showdowns"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::PickStaged));
        assert_eq!(
            ctx.blob.prompt.as_ref().unwrap().seat,
            0,
            "the turn player picks"
        );
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(labels, ["{zone 9}", "{zone 10}"]);
        ctx.blob.close_prompt();
        showdown::pick(&mut ctx, fixtures::BF2).unwrap();
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().zone, fixtures::BF2);
        assert_eq!(ctx.blob.staged.len(), 1, "the other waits its turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_swap_of_two_fae_fawns_leaves_a_sprite_at_both_origins() {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, crate::state::Mode::Enforced);
        fixture.blob.set_phase(crate::state::Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        for (id, zone) in [(90, fixtures::BASE), (91, fixtures::BF1)] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, zone, 0, "Lillia - Fae Fawn", 3));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(swap_units(&mut ctx, 90, 91), Swapped::Swapped);
        assert_eq!(ctx.location(90), Some(Location::Battlefield(fixtures::BF1)));
        assert_eq!(ctx.location(91), Some(Location::Base(0)));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "376.3.b · one batch, two triggers of one controller, so their order is asked"
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        for option in [0u16, 0] {
            if let Some(answered) = prompts::answer(
                &mut ctx,
                0,
                agni_plugin_sdk::prompt::Pick { prompt, option },
            )
            .unwrap()
            {
                crate::engine::resume(&mut ctx, &answered).unwrap();
            }
        }
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let sprites: Vec<(u32, Option<Location>)> = ctx
            .table
            .cards
            .iter()
            .filter(|card| card.name == "Sprite")
            .map(|card| (card.id, ctx.location(card.id)))
            .collect();
        assert_eq!(
            sprites,
            [
                (200, Some(Location::Battlefield(fixtures::BF1))),
                (201, Some(Location::Base(0))),
            ],
            "each Fawn plants her Sprite where she left from"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_contests_stay_staged_for_the_turn_player_and_an_abandoned_contest_clears() {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, crate::state::Mode::Enforced);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 0, "Jinx", 2));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.table.card_mut(fixtures::VI).unwrap().seat = 0;
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.blob.set_contested(fixtures::BF3, Some(1));
        let mut ctx = fixture.ctx();
        showdown::stage(&mut ctx);
        assert_eq!(
            ctx.blob.staged,
            [
                Staged {
                    zone: fixtures::BF1,
                    combat: false,
                    contester: 0
                },
                Staged {
                    zone: fixtures::BF2,
                    combat: true,
                    contester: 0
                },
            ]
        );
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(ctx.blob.contester(fixtures::BF3), None);
        showdown::open(&mut ctx, 1);
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().zone, fixtures::BF2);
        assert_eq!(ctx.blob.staged.len(), 1);
        showdown::open(&mut ctx, 5);
        showdown::stage(&mut ctx);
        assert_eq!(ctx.blob.staged.len(), 1);
        let mut four = Fixture::enforced();
        four.table.players = 4;
        four.blob = GameBlob::start(4, 0, crate::state::Mode::Enforced);
        four.table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 2, "Jinx", 2));
        four.table
            .cards
            .push(fixtures::unit(91, fixtures::BF1, 3, "Vex", 2));
        let ctx = four.ctx();
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Base(0),
                Location::Battlefield(fixtures::BF1)
            ),
            Err(Refusal::Illegal(Reason::TwoOtherSeats))
        );
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF2),
                Location::Battlefield(fixtures::BF3)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        assert_eq!(
            legal_destination(&ctx, fixtures::VI, Location::Base(0), Location::Base(0)),
            Err(Refusal::Illegal(Reason::SameLocation))
        );
        assert_eq!(
            companions(&ctx, 0, fixtures::VI, Location::Battlefield(fixtures::BF3)),
            Vec::<u32>::new()
        );
    }

    static LAIR: crate::cards::Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("Lair", &[], &[]),
        &[crate::cards::Static::NoMoveToBase],
    );

    fn lair() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &LAIR);
        fixture
    }

    #[test]
    fn a_no_move_to_base_battlefield_refuses_the_march_home_and_drops_the_base_from_effects() {
        let mut fixture = lair();
        let ctx = fixture.ctx();
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Base(0)
            ),
            Err(Refusal::Illegal(Reason::NoMoveToBase))
        );
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "the Lair closes the base only; the other battlefield still wants Ganking"
        );
        assert_eq!(
            effect_destinations(&ctx, fixtures::VI),
            [Location::Battlefield(fixtures::BF2)]
        );
        assert!(!ctx.movable_to_base(fixtures::VI));
        let rows = crate::engine::legal::highlights(&ctx, 0);
        let vi = rows.iter().find(|row| row.card == fixtures::VI);
        assert!(
            vi.is_none_or(|row| !row.zones.contains(&fixtures::BASE)),
            "the base is not lit for a unit at the Lair: {rows:?}"
        );
        assert!(
            companions(&ctx, 0, fixtures::VI, Location::Base(0)).is_empty(),
            "no unit at the Lair joins a group move home"
        );
        drop(ctx);
        let mut ganking = lair();
        let mut ctx = ganking.ctx();
        ctx.grant(
            fixtures::VI,
            Keyword::Ganking,
            crate::state::Expiry::EndOfTurn(1),
        );
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "Lair to another battlefield with Ganking is a legal march"
        );
        assert_eq!(
            effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Base(0)
            ),
            None
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} can't move from {zone 9} to base".to_string()));
        assert_eq!(
            effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Battlefield(fixtures::BF2)
            ),
            Some(Moved::Moved)
        );
        assert_eq!(
            effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Base(0)
            ),
            Some(Moved::Moved),
            "from an ordinary battlefield the base is open again"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }

    #[test]
    fn a_combat_at_the_lair_still_recalls_the_surviving_attackers() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 1, "Tank", 5));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture
            .blob
            .card_state_mut(90)
            .set(crate::state::FLAG_STUNNED, true);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &LAIR);
        let action = fixtures::move_action(fixtures::VI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        standard_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        assert!(ctx.is_attacker(fixtures::VI) && ctx.is_defender(90));
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(
            ctx.effects.contains(&Effect::Move {
                card: fixtures::VI,
                zone: fixtures::BASE,
                seat: 0,
                index: TOP
            }),
            "456 · the step-3d recall is not a move, so the Lair does not bind it"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
    }

    #[test]
    fn a_battlefield_that_units_move_to_from_anywhere_waives_ganking_for_that_destination_only() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        let pit = ctx
            .add_battlefield_token(0, crate::engine::ctx::Token::BaronPit)
            .unwrap();
        assert_eq!(pit, fixtures::BF3);
        assert!(ctx.moves_here_from_anywhere(pit));
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(pit)
            ),
            Ok(())
        );
        assert_eq!(
            legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "the waiver is the pit's, not the unit's"
        );
        assert!(effect_destinations(&ctx, fixtures::VI).contains(&Location::Battlefield(pit)));
        assert_eq!(
            ctx.move_unit(fixtures::VI, Location::Battlefield(pit), MoveCause::Effect),
            Moved::Moved
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Battlefield(pit)));
        assert_eq!(ctx.blob.contester(pit), Some(0));
        assert!(ctx.fault.is_none());
    }
}

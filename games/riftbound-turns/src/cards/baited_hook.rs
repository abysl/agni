use super::prelude::{
    a_friendly_unit, activated, asking, banish_by, card_target, done, exhausting_self,
    forget_revealing, gear, kill, named, with_candidates, Location,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage, Timing, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::play;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const COST: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Order)],
};
pub const LOOK: usize = 5;
pub const REACH: i32 = 1;
pub const QUESTION: &str = "a card from the top five to banish and play if it is a unit within reach, or where the unit enters";
pub const PICK_BASE: u8 = 16;
pub const AWAIT_BASE: u8 = 64;
pub const PLACE: u8 = 112;
pub const LIMIT_CAP: u8 = 47;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Kill,
    Pick(i32),
    Await(i32),
    Place,
}

pub fn step_of(stage: Stage) -> Step {
    match stage.0 {
        PLACE => Step::Place,
        held if held >= AWAIT_BASE => Step::Await(i32::from(held - AWAIT_BASE)),
        held if held >= PICK_BASE => Step::Pick(i32::from(held - PICK_BASE)),
        _ => Step::Kill,
    }
}

fn stage_with(base: u8, limit: i32) -> u8 {
    let limit = u8::try_from(limit.max(0))
        .unwrap_or(LIMIT_CAP)
        .min(LIMIT_CAP);
    base + limit
}

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

pub fn within_reach(ctx: &Ctx, card: u32, limit: i32) -> bool {
    ctx.kind_of(card) == Some(KIND_UNIT) && ctx.printed_might(card) <= limit
}

fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| {
            ctx.owner(*card) == seat
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
}

fn recycle_all(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

fn play_locations(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    match step_of(stage) {
        Step::Pick(_) => looked(ctx, seat).into_iter().map(TargetRef::Card).collect(),
        Step::Place => play_locations(ctx, seat),
        Step::Kill | Step::Await(_) => Vec::new(),
    }
}

fn cast(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let limit = card_target(ctx, item, 0).map(|unit| {
        let limit = ctx.current_might(unit) + REACH;
        kill(ctx, item, unit);
        limit
    });
    let top = looked(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    let Some(limit) = limit else {
        ctx.narrate(format!(
            "{{seat {seat}}} looks at the top {} cards of their deck · the bait is gone, so no unit is within reach",
            top.len()
        ));
        recycle_all(ctx, seat, &top);
        return done();
    };
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck · a unit with Might up to {limit} may be played",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, stage_with(PICK_BASE, limit), 0, 1))
}

fn pick(ctx: &mut Ctx, item: &Item, limit: i32) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let Some(picked) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card))
    else {
        ctx.narrate(format!("{{seat {seat}}} banishes nothing"));
        recycle_all(ctx, seat, &top);
        return done();
    };
    let rest: Vec<u32> = top.into_iter().filter(|card| *card != picked).collect();
    recycle_all(ctx, seat, &rest);
    let Some(chain) = ctx.zones.chain else {
        return done();
    };
    ctx.emit(Effect::Move {
        card: picked,
        zone: chain,
        seat: 0,
        index: TOP,
    });
    ctx.set_flag(picked, FLAG_REVEALING, true);
    ctx.narrate(format!("{{seat {seat}}} reveals {{card {picked}}}"));
    Flow::Ask(ctx.await_faces(item, &[picked], stage_with(AWAIT_BASE, limit)))
}

fn hooked(ctx: &mut Ctx, item: &Item, limit: i32) -> Flow {
    let seat = item.controller;
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    forget_revealing(ctx, card);
    if !within_reach(ctx, card, limit) {
        ctx.narrate(format!(
            "{{card {card}}} is not a unit with Might up to {limit} · it is recycled"
        ));
        ctx.recycle_to_bottom(card);
        return done();
    }
    banish_by(ctx, card, seat);
    let locations = play_locations(ctx, seat);
    match locations.as_slice() {
        [] => done(),
        [TargetRef::Zone(zone)] => {
            let zone = *zone;
            place(ctx, item, card, zone);
            done()
        }
        _ => Flow::Ask(ctx.ask_resume(item, PLACE, 1, 1)),
    }
}

fn placed(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = ctx
        .zones
        .banishment
        .and_then(|zone| ctx.top_of(zone, seat, 1).first().copied())
    else {
        return done();
    };
    let Some(zone) = ctx
        .picks()
        .first()
        .and_then(|zone| u16::try_from(*zone).ok())
        .filter(|zone| play_locations(ctx, seat).contains(&TargetRef::Zone(*zone)))
    else {
        return done();
    };
    place(ctx, item, card, zone);
    done()
}

fn place(ctx: &mut Ctx, item: &Item, card: u32, zone: u16) {
    let seat = item.controller;
    let Some(location) = Location::of_zone(zone, seat, &ctx.zones) else {
        return;
    };
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} plays {{card {card}}} from banishment, ignoring its cost",
        item.kind.source()
    ));
    let _ = play::begin(ctx, seat, card, Origin::Banishment, Some(location));
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match step_of(stage) {
        Step::Kill => cast(ctx, item),
        Step::Pick(limit) => pick(ctx, item, limit),
        Step::Await(limit) => hooked(ctx, item, limit),
        Step::Place => placed(ctx, item),
    }
}

pub static CARD: Card = gear(
    "Baited Hook",
    &[],
    &[named(
        asking(
            with_candidates(
                exhausting_self(activated(
                    Timing::Sorcery,
                    COST,
                    &[a_friendly_unit("a friendly unit to kill")],
                    run,
                )),
                candidates,
            ),
            QUESTION,
        ),
        "kill a friendly unit and play a bigger one from the top five",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger, KIND_SPELL};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, chain, cost, priority, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::PickRefusal;
    use agni_plugin_sdk::table::Face;

    const HOOK: u32 = 90;
    const BAIT: u32 = 91;
    const ORDER_RUNE: u32 = 46;
    const DECK: [u32; 6] = [20, 21, 22, 23, 26, 27];

    fn pier() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut hook = fixtures::gear(HOOK, fixtures::BASE, 0, "Baited Hook", 3);
        hook.domain = vec!["Order".into()];
        fixture.table.cards.push(hook);
        fixture
            .table
            .cards
            .push(fixtures::unit(BAIT, fixtures::BASE, 0, "Bait", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::hidden(26, fixtures::MAIN_DECK, 0));
        fixture
            .table
            .cards
            .push(fixtures::hidden(27, fixtures::MAIN_DECK, 0));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn cast_on_bait(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, HOOK, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BAIT}}}")).unwrap();
        assert!(ctx.card(HOOK).unwrap().exhausted);
        assert!(ctx.on_board(BAIT), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_is_an_order_and_energy_exhaust_activation_with_a_resume_question() {
        let mut fixture = pier();
        assert!(std::ptr::eq(fixture.scripts.of_card(HOOK).unwrap(), &CARD));
        assert_eq!(CARD.name, "Baited Hook");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(COST));
        assert_eq!(
            ability.targets[0].filter,
            crate::cards::prelude::FRIENDLY_UNIT
        );
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_activation(&ctx, HOOK, 0).label(),
            "1 energy and 1 Order power"
        );
        assert_eq!(step_of(Stage(0)), Step::Kill);
        assert_eq!(step_of(Stage(PICK_BASE + 3)), Step::Pick(3));
        assert_eq!(step_of(Stage(AWAIT_BASE + 3)), Step::Await(3));
        assert_eq!(step_of(Stage(PLACE)), Step::Place);
        assert_eq!(stage_with(PICK_BASE, 3), PICK_BASE + 3);
        assert_eq!(stage_with(AWAIT_BASE, 400), AWAIT_BASE + LIMIT_CAP);
    }

    #[test]
    fn the_kill_lands_then_five_peeks_reach_the_controller_and_the_pick_awaits_the_face() {
        let mut fixture = pier();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, HOOK, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BAIT}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(!ctx.on_board(BAIT));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == BAIT
        )));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK_BASE + 3
            }),
            "the stage carries the killed unit's Might plus one"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            peeks(&ctx),
            [(27, 0), (26, 0), (23, 0), (22, 0), (21, 0)],
            "the top five, top first, for the controller alone"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 27}",
                "{card 26}",
                "{card 23}",
                "{card 22}",
                "{card 21}",
                "skip"
            ]
        );
        assert!(ctx.blob.log.contains(
            &"{seat 0} looks at the top 5 cards of their deck · a unit with Might up to 3 may be played".to_string()
        ));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HOOK}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, AWAIT_BASE + 3);
        assert_eq!(parked.awaiting, [23]);
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(23, FLAG_REVEALING));
        assert_eq!(
            deck_of(&ctx, 0),
            [21, 22, 26, 27, 20],
            "the other four are recycled under the deck in the listed order"
        );
        let holder = priority::holder(&ctx).unwrap_or(0);
        let _ = priority::pass(&mut ctx, holder);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "a pass leaves the parked reveal where it is"
        );
        assert_eq!(ctx.blob.chain[0].awaiting, [23]);
    }

    #[test]
    fn a_unit_within_reach_is_banished_and_played_to_the_base_ignoring_its_cost() {
        let mut fixture = pier();
        cast_on_bait(&mut fixture);
        {
            let mut ctx = fixture.ctx();
            fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        let runes = fixture.ctx().ready_runes_of(0).len();
        arrives(
            &mut fixture,
            23,
            Face::named("Big Fish")
                .with_kind(KIND_UNIT)
                .with_might(Some(3)),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "the ability finished");
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.on_board(23));
        assert_eq!(ctx.location(23), Some(Location::Base(0)));
        assert!(
            ctx.card(23).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "ignoring its cost");
        assert!(!ctx.has_flag(23, FLAG_REVEALING));
        assert!(ctx.blob.log.contains(&"{card 23} is banished".to_string()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {HOOK}}} · {{seat 0}} plays {{card 23}} from banishment, ignoring its cost"
        )));
        assert_eq!(ctx.hand_of(0).len(), 4, "a play, not a draw");
    }

    #[test]
    fn a_unit_out_of_reach_or_a_spell_is_recycled_instead_of_played() {
        let mut fixture = pier();
        cast_on_bait(&mut fixture);
        {
            let mut ctx = fixture.ctx();
            fixtures::choose(&mut ctx, 0, "{card 27}").unwrap();
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        arrives(
            &mut fixture,
            27,
            Face::named("Leviathan")
                .with_kind(KIND_UNIT)
                .with_might(Some(4)),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(27).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(
            deck_of(&ctx, 0),
            [27, 21, 22, 23, 26, 20],
            "recycled under the four already there"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 27} is not a unit with Might up to 3 · it is recycled".to_string()));
        drop(ctx);
        let mut spell = pier();
        cast_on_bait(&mut spell);
        {
            let mut ctx = spell.ctx();
            fixtures::choose(&mut ctx, 0, "{card 27}").unwrap();
            let table = ctx.table.clone();
            drop(ctx);
            spell.commit(table);
        }
        arrives(&mut spell, 27, Face::named("Spark").with_kind(KIND_SPELL));
        let ctx = spell.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(27).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(!ctx.on_board(27));
    }

    #[test]
    fn skipping_the_pick_recycles_all_five_and_a_held_battlefield_asks_where_the_unit_enters() {
        let mut fixture = pier();
        cast_on_bait(&mut fixture);
        {
            let mut ctx = fixture.ctx();
            fixtures::choose(&mut ctx, 0, "skip").unwrap();
            assert!(ctx.blob.chain.is_empty());
            assert_eq!(deck_of(&ctx, 0), [21, 22, 23, 26, 27, 20]);
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} banishes nothing".to_string()));
            assert!(ctx.blob.log.contains(&"{seat 0} recycles 5".to_string()));
        }
        let mut held = pier();
        held.blob.set_holder(fixtures::BF1, Some(0));
        cast_on_bait(&mut held);
        {
            let mut ctx = held.ctx();
            fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
            let table = ctx.table.clone();
            drop(ctx);
            held.commit(table);
        }
        arrives(
            &mut held,
            23,
            Face::named("Big Fish")
                .with_kind(KIND_UNIT)
                .with_might(Some(3)),
        );
        let mut ctx = held.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PLACE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        assert!(ctx.in_banishment(23));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(ctx.location(23), Some(Location::Battlefield(fixtures::BF1)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played {
                card: 23,
                controller: 0,
                origin: Origin::Banishment,
                ..
            }
        )));
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card: 23,
                cause: MoveCause::Effect,
                ..
            }
        )));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wrong_seat_and_a_missing_order_power_are_refused_and_a_pick_off_the_top_five_is_a_no_op()
    {
        let mut fixture = pier();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, HOOK, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        fixture.table.cards.retain(|card| card.id != ORDER_RUNE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HOOK, 0),
            Err(Refusal::NoPowerOf),
            "no Order rune to recycle"
        );
        assert!(!ctx.card(HOOK).unwrap().exhausted);
        assert!(ctx.on_board(BAIT));
        drop(ctx);
        let mut stale = pier();
        cast_on_bait(&mut stale);
        let mut ctx = stale.ctx();
        let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
        assert!(matches!(
            prompts::answer(
                &mut ctx,
                1,
                agni_plugin_sdk::prompt::Pick { prompt, option: 0 }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        ));
        assert!(matches!(
            prompts::answer(
                &mut ctx,
                0,
                agni_plugin_sdk::prompt::Pick { prompt, option: 9 }
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption { .. }))
        ));
        assert_eq!(deck_of(&ctx, 0), DECK.to_vec());
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
    }
}

#[cfg(test)]
mod fidelity_probe {
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority};
    use agni_plugin_sdk::decide::Effect;

    const HOOK: u32 = 90;
    const BAIT: u32 = 91;
    const ORDER_RUNE: u32 = 46;

    #[test]
    fn a_bait_that_left_the_board_still_has_the_top_five_looked_at_and_recycled() {
        let mut fixture = Fixture::enforced();
        let mut hook = fixtures::gear(HOOK, fixtures::BASE, 0, "Baited Hook", 3);
        hook.domain = vec!["Order".into()];
        fixture.table.cards.push(hook);
        fixture
            .table
            .cards
            .push(fixtures::unit(BAIT, fixtures::BASE, 0, "Bait", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::hidden(26, fixtures::MAIN_DECK, 0));
        fixture
            .table
            .cards
            .push(fixtures::hidden(27, fixtures::MAIN_DECK, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        activate::activate(&mut ctx, 0, HOOK, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BAIT}}}")).unwrap();
        assert!(ctx.bounce(BAIT), "the bait is returned to hand in response");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let peeked = ctx
            .effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Peek { seat: 0, .. }))
            .count();
        assert_eq!(peeked, 5, "the top five are still looked at");
        assert!(
            ctx.blob.prompt.is_none(),
            "no unit can be chosen with a null Might"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.table.held(fixtures::MAIN_DECK, 0).count(),
            deck,
            "all five went back under the deck"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 5".to_string()));
        assert!(ctx.fault.is_none());
    }
}

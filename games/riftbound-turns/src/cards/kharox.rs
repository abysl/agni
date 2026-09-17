use super::prelude::{
    asking, burn_cards, done, empower, faceless, on_empowered, seat_target, target, unit,
    with_candidates, Location, Price,
};
use super::sabotage::AN_OPPONENT;
use super::the_harrowing::{play_from_trash, playable_units};
use super::{
    Card, Cost, Domain, Filter, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec,
    KIND_UNIT,
};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EMPOWER: Cost = Cost {
    energy: 6,
    power: &[Power::Domain(Domain::Chaos), Power::Domain(Domain::Chaos)],
};
pub const BURN: usize = 3;
pub const STAGE_REVEALED: u8 = 1;
pub const STAGE_PICK: u8 = 2;
pub const STAGE_LOCATE: u8 = 3;
pub const QUESTION: &str =
    "a unit in their trash to play ignoring its cost, then where it is played";
pub const UNIT_IN_THEIR_TRASH: TargetSpec = target(
    Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Enemy]),
    0,
    1,
    TargetKind::Card,
    "a unit in their trash",
);

pub fn their_units(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let Some(opponent) = seat_target(item, 0) else {
        return Vec::new();
    };
    playable_units(ctx, item, &UNIT_IN_THEIR_TRASH, Price::Free)
        .into_iter()
        .filter(|unit| ctx.owner(*unit) == opponent)
        .collect()
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 >= STAGE_LOCATE {
        return location_options(ctx, item.controller);
    }
    if stage.0 != STAGE_PICK {
        return Vec::new();
    }
    their_units(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn play_from_their_trash(ctx: &mut Ctx, item: &Item, unit: u32, at: Location) -> bool {
    let seat = item.controller;
    ctx.set_controller(unit, seat, unit);
    play_from_trash(ctx, item, unit, vec![at], Price::Free)
}

fn harrow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    let Some(opponent) = seat_target(item, 0) else {
        return done();
    };
    if stage.0 >= STAGE_LOCATE {
        let units = their_units(ctx, item);
        let Some(unit) = units.get(usize::from(stage.0 - STAGE_LOCATE)).copied() else {
            return done();
        };
        let Some(at) = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .filter(|at| ctx.play_locations(seat).contains(at))
        else {
            return done();
        };
        play_from_their_trash(ctx, item, unit, at);
        return done();
    }
    if stage.0 == STAGE_PICK {
        let units = their_units(ctx, item);
        let Some(index) = ctx
            .picks()
            .first()
            .and_then(|unit| units.iter().position(|held| held == unit))
        else {
            ctx.narrate(format!("{{card {me}}} · {{seat {seat}}} plays nothing"));
            return done();
        };
        let locations = ctx.play_locations(seat);
        let Ok(next) = u8::try_from(index).map(|index| index.saturating_add(STAGE_LOCATE)) else {
            return done();
        };
        if locations.len() > 1 {
            return Flow::Ask(ctx.ask_resume(item, next, 1, 1));
        }
        play_from_their_trash(ctx, item, units[index], locations[0]);
        return done();
    }
    if stage.0 != STAGE_REVEALED {
        let burned = burn_cards(ctx, opponent, BURN);
        let unseen = faceless(ctx, &burned);
        if !unseen.is_empty() {
            return Flow::Ask(ctx.await_faces(item, &unseen, STAGE_REVEALED));
        }
    }
    if their_units(ctx, item).is_empty() {
        ctx.narrate(format!(
            "{{card {me}}} finds no unit in {{seat {opponent}}}'s trash"
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_PICK, 0, 1))
}

pub static CARD: Card = unit(
    "Kharox",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        asking(
            with_candidates(on_empowered(&[AN_OPPONENT], harrow), candidates),
            QUESTION,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::the_harrowing::tests::trashed;
    use crate::cards::{script_of, SelfCost, Timing, Trigger, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, chain, priority, settle};
    use crate::state::{ItemKind, ItemStatus, Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::{CardInfo, Face};

    const KHAROX: u32 = 90;
    const THEIR_FALLEN: u32 = 91;
    const MY_FALLEN: u32 = 92;
    const THEIR_SPELL: u32 = 93;
    const THEIR_DECK: [u32; 2] = [24, 25];
    const CHAOS_RUNES: [u32; 6] = [46, 47, 48, 49, 50, 51];

    fn kharox() -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(KHAROX, fixtures::BASE, 0, "Kharox", 5)
        }
    }

    fn crypt(with_trash: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kharox());
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        if with_trash {
            fixture
                .table
                .cards
                .push(trashed(THEIR_FALLEN, 1, "Fallen Knight", 7, 3));
            fixture
                .table
                .cards
                .push(trashed(MY_FALLEN, 0, "My Own Knight", 1, 0));
            fixture.table.cards.push(fixtures::spell(
                THEIR_SPELL,
                fixtures::TRASH,
                1,
                "Their Spell",
                1,
                0,
            ));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KHAROX).unwrap(),
            &CARD
        ));
        fixture
    }

    fn empower_him(ctx: &mut Ctx) -> u16 {
        activate::activate(ctx, 0, KHAROX, 0).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(KHAROX));
        assert_eq!(
            ctx.blob.chain.len(),
            0,
            "the trigger waits on its target first"
        );
        let item = ctx
            .blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 1 } if source == KHAROX)
            })
            .map(|pending| pending.item.id)
            .expect("the become-Empowered trigger is pending");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(ctx),
            ["{seat 1}"],
            "an opponent, not yourself"
        );
        fixtures::choose(ctx, 0, "{seat 1}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        item
    }

    fn burn_and_park(fixture: &mut Fixture) -> u16 {
        let mut ctx = fixture.ctx();
        let item = empower_him(&mut ctx);
        let their_trash = ctx.trash_of(1).len();
        assert_eq!(
            ctx.table.held(fixtures::MAIN_DECK, 1).count(),
            2,
            "the fixture deck is short of three"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.trash_of(1).len(), their_trash + 2);
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 1).count(), 0);
        assert!(ctx.blob.log.contains(&"{seat 1} burns 2".to_string()));
        let parked = ctx.blob.chain.last().unwrap();
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, STAGE_REVEALED);
        let mut awaited = parked.awaiting.clone();
        awaited.sort_unstable();
        assert_eq!(awaited, THEIR_DECK, "the burned faces are awaited");
        assert!(ctx.blob.prompt.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        item
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(1, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn faces_arrive(fixture: &mut Fixture) {
        arrives(
            fixture,
            THEIR_DECK[0],
            Face::named("Burned Brute").with_kind(KIND_UNIT),
        );
        assert!(
            fixture.ctx().blob.prompt.is_none(),
            "one face of two is not enough"
        );
        arrives(
            fixture,
            THEIR_DECK[1],
            Face::named("Burned Spark").with_kind(KIND_SPELL),
        );
    }

    #[test]
    fn the_script_prints_empower_for_six_and_two_chaos_and_asks_an_opponent_when_empowered() {
        assert!(std::ptr::eq(script_of("Kharox").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        let harrow = &CARD.abilities[1];
        assert_eq!(harrow.trigger, Trigger::Empowered);
        assert!(!harrow.optional);
        assert_eq!(harrow.targets, [AN_OPPONENT]);
        assert!(harrow.candidates.is_some());
        assert_eq!(harrow.question, Some(QUESTION));
        assert_eq!(BURN, 3);
    }

    #[test]
    fn the_opponent_burns_three_and_a_unit_from_their_trash_is_played_to_your_base_for_nothing() {
        let mut fixture = crypt(true);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        let runes = ctx.runes_of(0).len();
        let item = empower_him(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 6, "six energy");
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 2,
            "two exhausted Chaos runes are recycled for the power"
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx();
        let their_trash = ctx.trash_of(1).len();
        assert_eq!(
            ctx.table.held(fixtures::MAIN_DECK, 1).count(),
            2,
            "the fixture deck is short of three"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.trash_of(1).len(), their_trash + 2);
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 1).count(), 0);
        assert!(ctx.blob.log.contains(&"{seat 1} burns 2".to_string()));
        assert!(
            ctx.blob.prompt.is_none(),
            "the burned faces are awaited first"
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        faces_arrive(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_PICK
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_FALLEN}}}"),
                format!("{{card {}}}", THEIR_DECK[0]),
                "skip".to_string()
            ],
            "their units, the burned one included · not their spells, not your own trash"
        );
        let mine = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_FALLEN}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(THEIR_FALLEN), Some(Location::Base(0)));
        assert!(ctx.card(THEIR_FALLEN).unwrap().exhausted);
        assert_eq!(
            ctx.controller(THEIR_FALLEN),
            0,
            "you played it, you control it"
        );
        assert_eq!(ctx.owner(THEIR_FALLEN), 1, "ownership never changes");
        assert_eq!(ctx.ready_runes_of(0).len(), mine, "ignoring its cost");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == THEIR_FALLEN
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {THEIR_FALLEN}}} from the trash for nothing"
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_burned_unit_alone_is_offered_from_an_empty_trash_once_its_face_arrives() {
        let mut fixture = crypt(false);
        let item = burn_and_park(&mut fixture);
        faces_arrive(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_PICK
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", THEIR_DECK[0]), "skip".to_string()],
            "the unit Kharox just burned"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", THEIR_DECK[0])).unwrap();
        assert_eq!(ctx.location(THEIR_DECK[0]), Some(Location::Base(0)));
        assert_eq!(ctx.controller(THEIR_DECK[0]), 0);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_the_play_leaves_their_trash_alone_and_an_empty_trash_asks_nothing() {
        let mut fixture = crypt(true);
        burn_and_park(&mut fixture);
        faces_arrive(&mut fixture);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_FALLEN).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.controller(THEIR_FALLEN), 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KHAROX}}} · {{seat 0}} plays nothing")));
        drop(ctx);

        let mut fixture = crypt(false);
        burn_and_park(&mut fixture);
        for card in THEIR_DECK {
            arrives(
                &mut fixture,
                card,
                Face::named("Burned Spark").with_kind(KIND_SPELL),
            );
        }
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KHAROX}}} finds no unit in {{seat 1}}'s trash"
        )));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut bare = crypt(false);
        bare.table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 1);
        bare.resolve();
        let mut ctx = bare.ctx();
        empower_him(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing burned, nothing awaited");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KHAROX}}} finds no unit in {{seat 1}}'s trash"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_empower_and_the_other_seat_are_refused() {
        let mut fixture = crypt(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, KHAROX, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        burn_and_park(&mut fixture);
        for card in THEIR_DECK {
            arrives(
                &mut fixture,
                card,
                Face::named("Burned Spark").with_kind(KIND_SPELL),
            );
        }
        let mut ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, KHAROX, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
    }
}

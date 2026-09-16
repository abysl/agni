use super::prelude::{asking, done, forget_revealing, gear, triggered, with_candidates, Location};
use super::{Card, Flow, Item, Stage, Trigger, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::{pay, play};
use crate::state::{Origin, RevealedFrom, TargetRef, FLAG_REVEALING};

pub const REVEALED: u8 = 1;
pub const PLACE: u8 = u8::MAX;
const LAST_REVEAL: u8 = PLACE - 1;
pub const QUESTION: &str = "where the revealed unit enters";

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

fn play_locations(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 == PLACE {
        return play_locations(ctx, item.controller);
    }
    Vec::new()
}

pub fn play_revealed_ignoring_cost(ctx: &mut Ctx, seat: u8, card: u32, at: Location) -> bool {
    ctx.narrate(format!(
        "{{seat {seat}}} plays the revealed {{card {card}}}, ignoring its cost"
    ));
    play::begin(
        ctx,
        seat,
        card,
        Origin::Revealed {
            from: RevealedFrom::Deck,
        },
        Some(at),
    )
    .is_ok()
}

fn reveal_next(ctx: &mut Ctx, item: &Item, recycled: u8) -> Flow {
    let seat = item.controller;
    let stage = REVEALED.saturating_add(recycled);
    if stage > LAST_REVEAL || (recycled > 0 && usize::from(recycled) >= pay::deck_size(ctx, seat)) {
        ctx.narrate(format!(
            "{{seat {seat}}} has revealed the whole deck · no unit was found"
        ));
        return done();
    }
    let Some(top) = ctx.reveal_top(seat) else {
        ctx.narrate(format!(
            "{{seat {seat}}} has no card left to reveal · no unit was found"
        ));
        return done();
    };
    Flow::Ask(ctx.await_faces(item, &[top], stage))
}

fn revealed(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    if ctx.kind_of(card) != Some(KIND_UNIT) {
        forget_revealing(ctx, card);
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!("{{seat {seat}}} recycles {{card {card}}}"));
        return reveal_next(ctx, item, stage.0 - REVEALED + 1);
    }
    let locations = play_locations(ctx, seat);
    match locations.as_slice() {
        [] => {
            forget_revealing(ctx, card);
            done()
        }
        [TargetRef::Zone(zone)] => {
            let zone = *zone;
            place(ctx, seat, card, zone);
            done()
        }
        _ => Flow::Ask(ctx.ask_resume(item, PLACE, 1, 1)),
    }
}

fn placed(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = revealing(ctx, seat) else {
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
    place(ctx, seat, card, zone);
    done()
}

fn place(ctx: &mut Ctx, seat: u8, card: u32, zone: u16) {
    forget_revealing(ctx, card);
    let Some(at) = Location::of_zone(zone, seat, &ctx.zones) else {
        return;
    };
    play_revealed_ignoring_cost(ctx, seat, card, at);
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        PLACE => placed(ctx, item),
        REVEALED..=LAST_REVEAL => revealed(ctx, item, stage),
        _ => reveal_next(ctx, item, 0),
    }
}

pub static CARD: Card = gear(
    "Dazzling Aurora",
    &[],
    &[asking(
        with_candidates(triggered(Trigger::EndOfTurn, &[], run), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::KIND_SPELL;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, phases, priority, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Phase, PromptWhy};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const AURORA: u32 = 90;
    const TOP: u32 = 23;
    const SECOND: u32 = 22;
    const THIRD: u32 = 21;

    fn sky() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut aurora = fixtures::gear(AURORA, fixtures::BASE, 0, "Dazzling Aurora", 9);
        aurora.domain = vec!["Body".into()];
        aurora.power = Some(2);
        fixture.table.cards.push(aurora);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn end_turn(fixture: &mut Fixture) -> u16 {
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.phase(), Some(Phase::Ending));
        let item = ctx
            .blob
            .chain
            .iter()
            .find(|held| matches!(held.kind, ItemKind::Trigger { source, .. } if source == AURORA))
            .map(|held| held.id)
            .expect("the end-of-turn trigger is on the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        item
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) -> Vec<Event> {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        let events = ctx.events.clone();
        drop(ctx);
        fixture.commit(table);
        events
    }

    #[test]
    fn the_script_is_an_end_of_turn_trigger_with_a_location_question() {
        let fixture = sky();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(AURORA).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Dazzling Aurora");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::EndOfTurn);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_ending_step_reveals_the_top_card_and_parks_the_trigger_until_the_host_pays_the_reveal() {
        let mut fixture = sky();
        let item = end_turn(&mut fixture);
        let ctx = fixture.ctx();
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.id, item);
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [TOP]);
        assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(TOP, FLAG_REVEALING));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.phase(), Some(Phase::Ending), "the turn waits");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top card of their deck".to_string()));
    }

    #[test]
    fn non_units_are_recycled_one_by_one_until_a_unit_is_revealed_and_played_to_the_base() {
        let mut fixture = sky();
        end_turn(&mut fixture);
        arrives(
            &mut fixture,
            TOP,
            Face::named("Spark").with_kind(KIND_SPELL),
        );
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert_eq!(deck_of(&ctx, 0), [TOP, 20, THIRD], "recycled under");
            assert!(!ctx.has_flag(TOP, FLAG_REVEALING));
            assert_eq!(
                ctx.blob.chain[0].awaiting,
                [SECOND],
                "the next card is revealed"
            );
            assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::CHAIN));
            assert_eq!(ctx.blob.phase(), Some(Phase::Ending));
        }
        arrives(&mut fixture, SECOND, Face::named("Boots").with_kind("Gear"));
        {
            let ctx = fixture.ctx();
            assert_eq!(deck_of(&ctx, 0), [SECOND, TOP, 20]);
            assert_eq!(ctx.blob.chain[0].awaiting, [THIRD]);
        }
        let runes = fixture.ctx().ready_runes_of(0).len();
        let events = arrives(
            &mut fixture,
            THIRD,
            Face::named("Dawnbringer")
                .with_kind(KIND_UNIT)
                .with_might(Some(6)),
        );
        let ctx = fixture.ctx();
        assert!(ctx.on_board(THIRD));
        assert_eq!(ctx.location(THIRD), Some(Location::Base(0)));
        assert!(
            ctx.card(THIRD).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "ignoring its cost");
        assert!(!ctx.has_flag(THIRD, FLAG_REVEALING));
        assert!(ctx
            .state_of(THIRD)
            .is_none_or(|state| !state.has(FLAG_REVEALING)));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Played {
                card: THIRD,
                controller: 0,
                ..
            }
        )));
        assert_eq!(
            deck_of(&ctx, 0),
            [SECOND, TOP, 20],
            "the rest stayed recycled"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays the revealed {{card {THIRD}}}, ignoring its cost"
        )));
        assert!(ctx.blob.chain.is_empty());
        assert_ne!(
            ctx.blob.phase(),
            Some(Phase::Ending),
            "the turn moved on once the trigger finished"
        );
        assert_eq!(ctx.turn_player(), 1);
    }

    #[test]
    fn with_a_held_battlefield_the_unit_asks_where_it_enters_and_lands_there() {
        let mut fixture = sky();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        end_turn(&mut fixture);
        arrives(
            &mut fixture,
            TOP,
            Face::named("Dawnbringer")
                .with_kind(KIND_UNIT)
                .with_might(Some(6)),
        );
        let mut ctx = fixture.ctx();
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
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {AURORA}}}: choose {QUESTION} (0 of 1)")
        );
        assert_eq!(
            ctx.card(TOP).unwrap().zone,
            Some(fixtures::CHAIN),
            "still revealed while asked"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(TOP),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_played_unit_reports_an_origin_that_is_not_banishment() {
        let mut fixture = sky();
        end_turn(&mut fixture);
        let events = arrives(
            &mut fixture,
            TOP,
            Face::named("Dawnbringer")
                .with_kind(KIND_UNIT)
                .with_might(Some(6)),
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Played { card: TOP, origin, .. } if *origin != Origin::Banishment
        )));
    }

    #[test]
    fn an_empty_deck_ends_the_search_and_the_other_seats_ending_step_is_not_yours() {
        let mut fixture = sky();
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card left to reveal · no unit was found".to_string()));
        assert_eq!(ctx.turn_player(), 1);
        assert!(ctx.fault.is_none());
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the aurora watches its controller's turn alone"
        );
        assert_eq!(ctx.turn_player(), 0);
    }
}

#[cfg(test)]
mod fidelity_probe {
    use crate::cards::KIND_SPELL;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, phases, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const AURORA: u32 = 90;
    const DECK: [u32; 4] = [23, 22, 21, 20];

    fn sky() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut aurora = fixtures::gear(AURORA, fixtures::BASE, 0, "Dazzling Aurora", 9);
        aurora.domain = vec!["Body".into()];
        aurora.power = Some(2);
        fixture.table.cards.push(aurora);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.resolve();
        fixture
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
    fn a_deck_without_a_unit_ends_the_search_instead_of_revealing_the_recycled_cards_again() {
        let mut fixture = sky();
        {
            let mut ctx = fixture.ctx();
            phases::end_turn(&mut ctx).unwrap();
            assert!(ctx.blob.chain.iter().any(
                |held| matches!(held.kind, ItemKind::Trigger { source, .. } if source == AURORA)
            ));
            priority::pass(&mut ctx, 0).unwrap();
            priority::pass(&mut ctx, 1).unwrap();
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        for card in DECK {
            assert_eq!(fixture.ctx().blob.chain[0].awaiting, [card]);
            arrives(
                &mut fixture,
                card,
                Face::named("Spark").with_kind(KIND_SPELL),
            );
        }
        let ctx = fixture.ctx();
        assert!(
            ctx.blob.chain.is_empty(),
            "every card was revealed and none was a unit · the trigger should be done, not awaiting {:?}",
            ctx.blob.chain.first().map(|held| held.awaiting.clone())
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has revealed the whole deck · no unit was found".to_string()));
        let mut deck: Vec<u32> = ctx
            .table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect();
        deck.sort_unstable();
        assert_eq!(deck, [20, 21, 22, 23], "every card went back under");
        assert!(ctx.fault.is_none());
    }
}

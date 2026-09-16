use super::prelude::{
    asking, banish_by, done, forget_revealing, play, spell, with_candidates, Location,
};
use super::whirlwind::seats_from_the_next_player;
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};

pub const QUESTION: &str = "a revealed card to banish and play, then where it is played";
pub const REVEALED: u8 = 1;
pub const CHOSEN: u8 = 2;
pub const LOCATE: u8 = 3;

fn opponents(ctx: &Ctx, item: &Item) -> Vec<u8> {
    seats_from_the_next_player(ctx)
        .into_iter()
        .filter(|seat| *seat != item.controller)
        .collect()
}

pub fn revealing(ctx: &Ctx, item: &Item) -> Vec<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .filter(|card| {
            ctx.owner(*card) != item.controller
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
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
    match stage.0 {
        CHOSEN => revealing(ctx, item)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE => location_options(ctx, item.controller),
        _ => Vec::new(),
    }
}

fn reveal(ctx: &mut Ctx, item: &Item) -> Flow {
    let mut revealed = Vec::new();
    for seat in opponents(ctx, item) {
        match ctx.reveal_top(seat) {
            Some(card) => revealed.push(card),
            None => ctx.narrate(format!("{{seat {seat}}} has no card to reveal")),
        }
    }
    if revealed.is_empty() {
        return done();
    }
    let unseen: Vec<u32> = revealed
        .into_iter()
        .filter(|card| !ctx.table.is_revealed(*card))
        .collect();
    if unseen.is_empty() {
        return offer(ctx, item);
    }
    Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
}

fn offer(ctx: &mut Ctx, item: &Item) -> Flow {
    match revealing(ctx, item).as_slice() {
        [] => done(),
        [only] => take(ctx, item, *only),
        _ => Flow::Ask(ctx.ask_resume(item, CHOSEN, 1, 1)),
    }
}

fn take(ctx: &mut Ctx, item: &Item, card: u32) -> Flow {
    let seat = item.controller;
    for other in revealing(ctx, item) {
        if other == card {
            continue;
        }
        forget_revealing(ctx, other);
        ctx.recycle_to_bottom(other);
        ctx.narrate(format!("{{card {other}}} is recycled"));
    }
    if ctx.is_unit(card) && ctx.play_locations(seat).len() > 1 {
        return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
    }
    let at = ctx.is_unit(card).then_some(Location::Base(seat));
    play_banished(ctx, item, card, at)
}

fn play_banished(ctx: &mut Ctx, item: &Item, card: u32, at: Option<Location>) -> Flow {
    let seat = item.controller;
    forget_revealing(ctx, card);
    if !banish_by(ctx, card, seat) {
        return done();
    }
    if ctx.is_unit(card) || ctx.is_gear(card) {
        ctx.set_controller(card, seat, card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {card}}} ignoring its cost"
    ));
    let _ = play_engine::begin(ctx, seat, card, Origin::Banishment, at);
    done()
}

fn fury(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        REVEALED => offer(ctx, item),
        CHOSEN => {
            let revealed = revealing(ctx, item);
            match ctx.picks().first().copied() {
                Some(card) if revealed.contains(&card) => take(ctx, item, card),
                _ => done(),
            }
        }
        LOCATE => {
            let Some(card) = revealing(ctx, item).first().copied() else {
                return done();
            };
            let at = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
                .filter(|at| ctx.play_locations(seat).contains(at))
                .unwrap_or(Location::Base(seat));
            play_banished(ctx, item, card, Some(at))
        }
        _ => reveal(ctx, item),
    }
}

pub static CARD: Card = spell(
    "Blind Fury",
    &[Keyword::Action],
    &[asking(
        with_candidates(play(&[], fury), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, might_this_turn, unit as unit_card};
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::script_of;
    use crate::cards::the_harrowing::tests::DRAWS;
    use crate::cards::{KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, legal, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Face;

    const FURY: u32 = 90;
    const THEIR_FURY: u32 = 91;
    const THEIR_TOP: u32 = 25;

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = crate::cards::prelude::card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        })],
    );

    static CRAB: Card = unit_card("Crab", &[], &[]);

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        for (id, seat) in [(FURY, 0), (THEIR_FURY, 1)] {
            let mut fury = fixtures::spell(id, fixtures::HAND, seat, "Blind Fury", 0, 0);
            fury.domain = vec!["Fury".into()];
            fixture.table.cards.push(fury);
        }
        fixture.resolve();
        fixture
    }

    fn cast(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FURY).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing is targeted");
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, script: &'static Card) -> Vec<Event> {
        let action = Action::Reveal { card, face };
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        let mut ctx = fixture.ctx_for(1, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        let events = ctx.events.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        events
    }

    #[test]
    fn the_script_is_a_targetless_action_that_parks_on_the_opponents_reveal() {
        assert!(std::ptr::eq(script_of("Blind Fury").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        cast(&mut fixture);
        let ctx = fixture.ctx();
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [THEIR_TOP]);
        assert_eq!(ctx.card(THEIR_TOP).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(THEIR_TOP, FLAG_REVEALING));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} reveals the top card of their deck".to_string()));
    }

    #[test]
    fn a_revealed_unit_is_banished_and_played_under_my_control_for_nothing_with_its_play_trigger() {
        let mut fixture = armed();
        cast(&mut fixture);
        let hand = fixture.ctx().hand_of(0).len();
        let ready = fixture.ctx().ready_runes_of(0).len();
        let events = arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Crab")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(6), Some(2)),
            &DRAWS,
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Banishment, .. } if *card == THEIR_TOP
        )));
        let mut ctx = fixture.ctx();
        assert!(
            ctx.blob.prompt.is_none(),
            "one opponent, one card, one base"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_TOP}}} is banished")));
        assert_eq!(
            ctx.location(THEIR_TOP),
            Some(Location::Base(0)),
            "{:?} {:?} {:?}",
            ctx.blob.log,
            ctx.card(THEIR_TOP),
            ctx.blob.queue
        );
        assert_eq!(ctx.controller(THEIR_TOP), 0);
        assert_eq!(ctx.owner(THEIR_TOP), 1);
        assert!(ctx.card(THEIR_TOP).unwrap().exhausted);
        assert!(!ctx.has_flag(THEIR_TOP, FLAG_REVEALING));
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "ignoring its cost");
        assert!(ctx.banished_of(1).is_empty());
        assert_eq!(ctx.card(FURY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "the unit's own play trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == THEIR_TOP
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "it draws for me");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_revealed_spell_is_played_with_an_uncancellable_target_prompt_and_resolves_for_me() {
        let mut fixture = armed();
        cast(&mut fixture);
        arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Pump")
                .with_kind(KIND_SPELL)
                .with_cost(Some(3), Some(1)),
            &PUMP,
        );
        let mut ctx = fixture.ctx();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "{:?}",
            ctx.blob.why
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert!(!prompt.cancel, "a limited play is not cancellable");
        assert_eq!(ctx.card(THEIR_TOP).unwrap().zone, Some(fixtures::CHAIN));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        let top = ctx.blob.chain.last().unwrap();
        assert!(matches!(top.kind, ItemKind::Spell { card } if card == THEIR_TOP));
        assert_eq!(top.controller, 0);
        assert_eq!(top.origin, Origin::Banishment);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            ctx.card(THEIR_TOP).unwrap().zone,
            Some(fixtures::TRASH),
            "a spell played from Banishment is trashed after it resolves"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_held_battlefield_asks_where_the_unit_lands() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        cast(&mut fixture);
        arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Crab").with_kind(KIND_UNIT).with_might(Some(2)),
            &CRAB,
        );
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(THEIR_TOP),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.controller(THEIR_TOP), 0);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_empty_opponent_deck_reveals_nothing_and_the_opponent_cannot_play_it_on_my_turn() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FURY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no card to reveal".to_string()));
        assert_eq!(ctx.card(FURY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_FURY,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(legal::classify(&ctx, 1, &entry), Err(Refusal::NotYourTurn));
    }

    #[test]
    fn while_the_reveal_is_owed_nobody_can_pass_or_pick() {
        let mut fixture = armed();
        cast(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: 0,
                    option: 0
                }
            ),
            Err(Refusal::NoPrompt)
        );
        let _ = crate::engine::priority::pass(&mut ctx, 0);
        let _ = crate::engine::priority::pass(&mut ctx, 1);
        assert_eq!(
            ctx.blob.chain[0].status,
            ItemStatus::Resolving,
            "a pass changes nothing while the face is owed"
        );
        assert_eq!(ctx.card(THEIR_TOP).unwrap().zone, Some(fixtures::CHAIN));
    }

    #[test]
    fn the_opponent_cannot_answer_my_location_question() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        cast(&mut fixture);
        arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Crab").with_kind(KIND_UNIT).with_might(Some(2)),
            &CRAB,
        );
        let mut ctx = fixture.ctx();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
    }

    #[test]
    #[ignore = "engine gap · play::cancel sends an Origin::Banishment card to the item controller's Banishment and chain::leave to the controller's trash; a stolen play should use the card's owner"]
    fn a_stolen_spell_without_a_target_stays_in_its_owners_banishment() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.tokens.clear();
        fixture.resolve();
        cast(&mut fixture);
        arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Pump")
                .with_kind(KIND_SPELL)
                .with_cost(Some(3), Some(1)),
            &PUMP,
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.banished_of(1), [THEIR_TOP]);
        assert!(ctx.banished_of(0).is_empty());
    }
}

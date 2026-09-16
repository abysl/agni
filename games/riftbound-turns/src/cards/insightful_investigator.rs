use super::mindsplitter::ANY_CARD_IN_HAND;
use super::prelude::{asking, done, draw, play, seat_target, unit, with_candidates, xp_of};
use super::sabotage::{revealed_matching, AN_OPPONENT};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const AWAITED: u8 = 1;
pub const PICKED: u8 = 2;
pub const XP: u8 = 2;
pub const DRAWS: usize = 1;
pub const QUESTION: &str = "a card from their hand to discard for 2 XP";

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    revealed_matching(ctx, item, ANY_CARD_IN_HAND)
}

pub fn can_pay(ctx: &Ctx, seat: u8) -> bool {
    xp_of(ctx, seat) >= i32::from(XP)
}

fn discard(ctx: &mut Ctx, seat: u8, card: u32) {
    ctx.discard(seat, card);
}

fn reveal(ctx: &mut Ctx, item: &Item) -> Flow {
    let Some(opponent) = seat_target(item, 0) else {
        return done();
    };
    let hand = ctx.hand_of(opponent);
    if hand.is_empty() {
        ctx.narrate(format!("{{seat {opponent}}} has no cards in hand"));
        return done();
    }
    ctx.narrate(format!("{{seat {opponent}}} reveals their hand"));
    let unseen: Vec<u32> = hand
        .into_iter()
        .filter(|card| !ctx.table.is_revealed(*card))
        .collect();
    if unseen.is_empty() {
        return offer(ctx, item);
    }
    Flow::Ask(ctx.await_faces(item, &unseen, AWAITED))
}

fn offer(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    if !can_pay(ctx, seat) {
        ctx.narrate(format!(
            "{{seat {seat}}} has {} XP · {XP} XP is the price of a choice",
            xp_of(ctx, seat)
        ));
        return done();
    }
    if revealed_matching(ctx, item, ANY_CARD_IN_HAND).is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds nothing to choose",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICKED, 0, 1))
}

fn investigate(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let offered = revealed_matching(ctx, item, ANY_CARD_IN_HAND);
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| offered.contains(&TargetRef::Card(*card)))
    else {
        ctx.narrate(format!("{{seat {seat}}} keeps their XP"));
        return done();
    };
    if !ctx.spend_xp(seat, XP) {
        return done();
    }
    let owner = ctx.controller(card);
    discard(ctx, owner, card);
    let drawn = draw(ctx, owner, DRAWS);
    ctx.narrate(format!("{{seat {owner}}} draws {drawn}"));
    done()
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        AWAITED => offer(ctx, item),
        PICKED => investigate(ctx, item),
        _ => reveal(ctx, item),
    }
}

pub static CARD: Card = unit(
    "Insightful Investigator",
    &[],
    &[asking(
        with_candidates(play(&[AN_OPPONENT], run), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::{
        face_of, pass_until_parked, reveal, THEIR_GEAR_CARD, THEIR_SPELL_CARD, THEIR_UNIT_CARD,
    };
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const INVESTIGATOR: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    fn investigator(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(INVESTIGATOR, zone, seat, "Insightful Investigator", 3)
        }
    }

    fn precinct(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        for id in [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 1));
        }
        fixture.table.cards.push(investigator(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.set_xp(0, xp);
        fixture.resolve();
        fixture
    }

    fn deploy(fixture: &mut Fixture) -> u16 {
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            INVESTIGATOR,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one opponent answers its own prompt"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == INVESTIGATOR
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Seat(1)]);
        let item = ctx.blob.chain[0].id;
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        item
    }

    fn reveal_all(fixture: &mut Fixture) {
        for card in [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD] {
            assert!(reveal(fixture, card));
        }
    }

    #[test]
    fn the_script_targets_an_opponent_and_asks_for_a_card_of_the_revealed_hand_for_two_xp() {
        assert!(std::ptr::eq(
            script_of("Insightful Investigator").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Insightful Investigator");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the skip of the pick");
        assert_eq!(
            ability.xp, 0,
            "the XP is spent inside the resolution, not as a trigger cost"
        );
        assert_eq!(ability.targets, [AN_OPPONENT]);
        assert_eq!(ability.targets[0].kind, TargetKind::Seat);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((XP, DRAWS), (2, 1));
    }

    #[test]
    fn the_trigger_parks_until_every_face_arrives_then_offers_the_hand_and_the_pick_costs_two_xp_discards_and_draws(
    ) {
        let mut fixture = precinct(3);
        let item = deploy(&mut fixture);
        {
            let ctx = fixture.ctx();
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.id, item);
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, AWAITED);
            assert_eq!(
                parked.awaiting,
                [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD]
            );
            assert!(ctx.blob.prompt.is_none(), "the host's reveals are awaited");
            assert!(ctx
                .blob
                .log
                .iter()
                .any(|line| line == "{seat 1} reveals their hand"));
        }
        reveal_all(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_UNIT_CARD}}}"),
                format!("{{card {THEIR_SPELL_CARD}}}"),
                format!("{{card {THEIR_GEAR_CARD}}}"),
                "skip".to_string()
            ],
            "every card of the hand, and the choice not to pay"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {INVESTIGATOR}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the chooser is the investigator's controller"
        );
        let their_deck = ctx.table.held(fixtures::MAIN_DECK, 1).count();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert_eq!(ctx.xp(0), 1, "two XP spent");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} spends 2 XP"));
        assert_eq!(
            ctx.card(THEIR_SPELL_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_SPELL_CARD,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}} discards {{card {THEIR_SPELL_CARD}}}")));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 1, .. })));
        assert_eq!(ctx.hand_of(1).len(), 3, "two stayed and one was drawn");
        assert_eq!(
            ctx.table.held(fixtures::MAIN_DECK, 1).count(),
            their_deck - 1
        );
        assert!(ctx.blob.log.iter().any(|line| line == "{seat 1} draws 1"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_keeps_the_xp_and_their_hand() {
        let mut fixture = precinct(2);
        deploy(&mut fixture);
        reveal_all(&mut fixture);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(ctx.hand_of(1).len(), 3);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} keeps their XP"));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { .. })));
    }

    #[test]
    fn short_of_two_xp_the_hand_is_revealed_but_no_choice_is_offered() {
        let mut fixture = precinct(1);
        deploy(&mut fixture);
        reveal_all(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} reveals their hand"));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} has 1 XP · 2 XP is the price of a choice"));
        assert_eq!(ctx.xp(0), 1);
        assert_eq!(ctx.hand_of(1).len(), 3);
    }

    #[test]
    fn an_empty_hand_ends_the_trigger_without_a_reveal_and_a_public_face_skips_the_wait() {
        let mut fixture = precinct(4);
        fixture.table.cards.retain(|card| {
            ![THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id)
        });
        fixture.resolve();
        deploy(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert!(ctx.blob.prompt.is_none());
            assert!(ctx.blob.chain.is_empty());
            assert!(ctx
                .blob
                .log
                .iter()
                .any(|line| line == "{seat 1} has no cards in hand"));
            assert_eq!(ctx.xp(0), 4);
        }
        let mut public = precinct(4);
        public
            .table
            .cards
            .retain(|card| ![THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id));
        public
            .table
            .apply_entry(
                &Action::Reveal {
                    card: THEIR_UNIT_CARD,
                    face: face_of(THEIR_UNIT_CARD),
                },
                1,
            )
            .unwrap();
        public.resolve();
        let item = deploy(&mut public);
        let mut ctx = public.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            }),
            "the face is already public, so the pick opens at once"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {THEIR_UNIT_CARD}}}"), "skip".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_UNIT_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(1).len(), 1, "the one they drew");
    }
}

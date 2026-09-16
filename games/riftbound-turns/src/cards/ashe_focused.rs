use super::mindsplitter::ANY_CARD_IN_HAND;
use super::prelude::{
    asking, banish_by, done, play, seat_target, triggered, unit, with_candidates,
};
use super::sabotage::{revealed_matching, AN_OPPONENT};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const AWAITED: u8 = 1;
pub const PICKED: u8 = 2;
pub const GIVE_BACK: u8 = 1;
pub const QUESTION: &str = "a revealed card to banish until they hold";

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    revealed_matching(ctx, item, ANY_CARD_IN_HAND)
}

pub fn return_when_they_hold(ctx: &mut Ctx, item: &Item, opponent: u8, card: u32) {
    let ashe = item.kind.source();
    ctx.narrate(format!(
        "{{card {card}}} returns to {{seat {opponent}}}'s hand when they hold · even if {{card {ashe}}} is gone"
    ));
}

pub fn give_back(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(TargetRef::Card(card)) = item.targets.first().copied() else {
        return done();
    };
    if !ctx.in_banishment(card) {
        ctx.narrate(format!("{{card {card}}} is no longer banished"));
        return done();
    }
    let Some(hand) = ctx.zones.hand else {
        return done();
    };
    let owner = ctx.owner(card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.narrate(format!(
        "{{seat {owner}}} held · {{card {card}}} returns to their hand"
    ));
    done()
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
    if revealed_matching(ctx, item, ANY_CARD_IN_HAND).is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds nothing to banish",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICKED, 1, 1))
}

fn focus(ctx: &mut Ctx, item: &Item) -> Flow {
    let offered = revealed_matching(ctx, item, ANY_CARD_IN_HAND);
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| offered.contains(&TargetRef::Card(*card)))
    else {
        return done();
    };
    let opponent = ctx.controller(card);
    if !banish_by(ctx, card, item.controller) {
        return done();
    }
    ctx.narrate(format!("{{seat {opponent}}}'s {{card {card}}} is banished"));
    return_when_they_hold(ctx, item, opponent, card);
    done()
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        AWAITED => offer(ctx, item),
        PICKED => focus(ctx, item),
        _ => reveal(ctx, item),
    }
}

pub static CARD: Card = unit(
    "Ashe - Focused",
    &[],
    &[
        asking(
            with_candidates(play(&[AN_OPPONENT], run), candidates),
            QUESTION,
        ),
        triggered(Trigger::Reflexive, &[], give_back),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::{
        pass_until_parked, reveal, THEIR_GEAR_CARD, THEIR_SPELL_CARD, THEIR_UNIT_CARD,
    };
    use crate::cards::{script_of, TargetKind};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, play as play_engine, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const ASHE: u32 = 90;
    const ORDER_RUNE: u32 = 46;

    fn ashe(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(ASHE, zone, seat, "Ashe - Focused", 4)
        }
    }

    fn range() -> Fixture {
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
        fixture.table.cards.push(ashe(fixtures::HAND, 0));
        for id in ORDER_RUNE..ORDER_RUNE + 3 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Order", false));
        }
        fixture.resolve();
        fixture
    }

    fn draw_bow(fixture: &mut Fixture) -> u16 {
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, ASHE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one opponent answers its own prompt"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ASHE
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

    fn loose(fixture: &mut Fixture, card: u32) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, &format!("{{card {card}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn give_back_item(card: u32) -> Item {
        let mut item = Item::new(
            7,
            ItemKind::Trigger {
                source: ASHE,
                index: GIVE_BACK,
            },
            0,
            Origin::Board,
        );
        item.targets.push(TargetRef::Card(card));
        item
    }

    #[test]
    fn the_script_targets_an_opponent_asks_for_a_revealed_card_and_carries_the_reflexive_return() {
        assert!(std::ptr::eq(script_of("Ashe - Focused").unwrap(), &CARD));
        assert_eq!(CARD.name, "Ashe - Focused");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let focus = &CARD.abilities[0];
        assert_eq!(focus.trigger, Trigger::Play);
        assert!(!focus.optional);
        assert_eq!(focus.targets, [AN_OPPONENT]);
        assert_eq!(focus.targets[0].kind, TargetKind::Seat);
        assert_eq!(focus.question, Some(QUESTION));
        assert!(focus.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        let back = &CARD.abilities[usize::from(GIVE_BACK)];
        assert_eq!(
            back.trigger,
            Trigger::Reflexive,
            "queued by a delay, never by an event"
        );
        assert!(back.targets.is_empty());
        assert!(back.condition.is_none());
    }

    #[test]
    fn the_trigger_parks_until_every_face_arrives_then_the_pick_is_banished_from_their_hand() {
        let mut fixture = range();
        let item = draw_bow(&mut fixture);
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
                format!("{{card {THEIR_GEAR_CARD}}}")
            ],
            "every card of the hand, and no skip: a card is banished"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {ASHE}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the chooser is Ashe's controller"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert!(ctx.in_banishment(THEIR_SPELL_CARD));
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_SPELL_CARD,
            zone: fixtures::BANISHMENT,
            seat: 1,
            index: TOP
        }));
        assert_eq!(
            ctx.hand_of(1),
            [THEIR_UNIT_CARD, THEIR_GEAR_CARD],
            "the other two stay"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}}'s {{card {THEIR_SPELL_CARD}}} is banished")));
        assert!(ctx.blob.log.iter().any(|line| {
            line == &format!(
                "{{card {THEIR_SPELL_CARD}}} returns to {{seat 1}}'s hand when they hold · even if {{card {ASHE}}} is gone"
            )
        }));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_return_moves_the_banished_card_back_to_its_owners_hand_and_leaves_a_card_elsewhere_alone(
    ) {
        let mut fixture = range();
        draw_bow(&mut fixture);
        reveal_all(&mut fixture);
        loose(&mut fixture, THEIR_UNIT_CARD);
        fixture.table.cards.retain(|card| card.id != ASHE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            ctx.card(ASHE).is_none(),
            "even if I'm no longer on the board"
        );
        let hand = ctx.hand_of(1).len();
        assert_eq!(
            give_back(&mut ctx, &give_back_item(THEIR_UNIT_CARD), Stage(0)),
            Flow::Done
        );
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert_eq!(ctx.hand_of(1).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_UNIT_CARD,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!("{{seat 1}} held · {{card {THEIR_UNIT_CARD}}} returns to their hand")));
        assert_eq!(
            give_back(&mut ctx, &give_back_item(THEIR_GEAR_CARD), Stage(0)),
            Flow::Done
        );
        assert_eq!(
            ctx.card(THEIR_GEAR_CARD).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {THEIR_GEAR_CARD}}} is no longer banished")));
        assert_eq!(
            give_back(
                &mut ctx,
                &Item::new(
                    8,
                    ItemKind::Trigger {
                        source: ASHE,
                        index: GIVE_BACK
                    },
                    0,
                    Origin::Board
                ),
                Stage(0)
            ),
            Flow::Done,
            "a return with no card is nothing"
        );
    }

    #[test]
    fn an_empty_hand_ends_the_trigger_without_a_reveal() {
        let mut fixture = range();
        fixture.table.cards.retain(|card| {
            ![THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id)
        });
        fixture.resolve();
        draw_bow(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} has no cards in hand"));
        assert!(ctx.banished_of(1).is_empty());
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject and a delayed trigger off the board: When has EndOfTurn, BeginningOf and AfterKillsBy only, so return_when_they_hold can only narrate; the wiring is ctx.delay(When::HeldBy(opponent), ashe, seat, GIVE_BACK, vec![card]) and triggers::queue_delayed(When::HeldBy(seat)) from cleanup::score_holds, which needs no source on the board"]
    fn when_they_hold_the_banished_card_returns_to_their_hand_even_with_ashe_gone() {
        let mut fixture = range();
        draw_bow(&mut fixture);
        reveal_all(&mut fixture);
        loose(&mut fixture, THEIR_SPELL_CARD);
        fixture.table.cards.retain(|card| card.id != ASHE);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(THEIR_SPELL_CARD).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx.blob.delayed.is_empty(), "the promise was spent");
    }
}

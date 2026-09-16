use super::prelude::{asking, done, play, seat_target, unit, with_candidates};
use super::sabotage::{revealed_matching, AN_OPPONENT};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const AWAITED: u8 = 1;
pub const PICKED: u8 = 2;
pub const QUESTION: &str = "a card from their hand to discard";
pub const ANY_CARD_IN_HAND: Filter = Filter::And(&[Filter::InHand, Filter::Enemy]);

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    revealed_matching(ctx, item, ANY_CARD_IN_HAND)
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
    if revealed_matching(ctx, item, ANY_CARD_IN_HAND).is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds nothing to discard",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICKED, 1, 1))
}

fn split(ctx: &mut Ctx, item: &Item) -> Flow {
    let offered = revealed_matching(ctx, item, ANY_CARD_IN_HAND);
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| offered.contains(&TargetRef::Card(*card)))
    else {
        return done();
    };
    let owner = ctx.controller(card);
    discard(ctx, owner, card);
    done()
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        AWAITED => offer(ctx, item),
        PICKED => split(ctx, item),
        _ => reveal(ctx, item),
    }
}

pub static CARD: Card = unit(
    "Mindsplitter",
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
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const MINDSPLITTER: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    fn mindsplitter(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(7),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, "Mindsplitter", "Unit")
        }
    }

    fn peak() -> Fixture {
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
        fixture
            .table
            .cards
            .push(mindsplitter(MINDSPLITTER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        fixture
    }

    fn descend(fixture: &mut Fixture) -> u16 {
        let action = fixtures::move_action(MINDSPLITTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(
            &mut ctx,
            0,
            MINDSPLITTER,
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
            ItemKind::Trigger { source, index: 0 } if source == MINDSPLITTER
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [crate::state::TargetRef::Seat(1)]
        );
        let item = ctx.blob.chain[0].id;
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        item
    }

    #[test]
    fn the_script_targets_an_opponent_and_asks_for_any_card_of_the_revealed_hand() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Mindsplitter").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Mindsplitter");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0], AN_OPPONENT);
        assert_eq!(ability.targets[0].kind, TargetKind::Seat);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_trigger_parks_until_every_face_arrives_then_offers_the_whole_hand_and_the_pick_is_discarded(
    ) {
        let mut fixture = peak();
        let item = descend(&mut fixture);
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
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        assert!(reveal(&mut fixture, THEIR_SPELL_CARD));
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.blob.chain[0].awaiting, [THEIR_GEAR_CARD]);
            assert!(ctx.blob.prompt.is_none());
        }
        assert!(reveal(&mut fixture, THEIR_GEAR_CARD));
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
            "every card of the hand, whatever its kind"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {MINDSPLITTER}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the chooser is the mindsplitter's controller, not the opponent"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_UNIT_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_UNIT_CARD,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert_eq!(
            ctx.hand_of(1),
            [THEIR_SPELL_CARD, THEIR_GEAR_CARD],
            "the other two stay"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}} discards {{card {THEIR_UNIT_CARD}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_card_in_hand_is_discarded_without_a_click() {
        let mut fixture = peak();
        fixture
            .table
            .cards
            .retain(|card| ![THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id));
        fixture.resolve();
        descend(&mut fixture);
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none(), "one candidate answers itself");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.hand_of(1).is_empty());
    }

    #[test]
    fn an_empty_hand_ends_the_trigger_without_a_reveal() {
        let mut fixture = peak();
        fixture.table.cards.retain(|card| {
            ![THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id)
        });
        fixture.resolve();
        descend(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} has no cards in hand"));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} reveals their hand"));
    }

    #[test]
    fn a_face_already_public_skips_the_wait_and_a_stray_reveal_answers_nobody() {
        let mut fixture = peak();
        fixture
            .table
            .cards
            .retain(|card| ![THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id));
        fixture
            .table
            .apply_entry(
                &Action::Reveal {
                    card: THEIR_UNIT_CARD,
                    face: face_of(THEIR_UNIT_CARD),
                },
                1,
            )
            .unwrap();
        fixture.resolve();
        {
            let action = Action::Reveal {
                card: THEIR_UNIT_CARD,
                face: face_of(THEIR_UNIT_CARD),
            };
            let mut ctx = fixture.ctx_for(1, &action);
            assert!(
                !crate::engine::chain::face_arrived(&mut ctx, THEIR_UNIT_CARD).unwrap(),
                "no item awaits it yet"
            );
        }
        let item = descend(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            }),
            "every face is already public, so the pick opens at once"
        );
        assert!(ctx.blob.chain[0].awaiting.is_empty());
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {THEIR_UNIT_CARD}}}")]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_UNIT_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
    }
}

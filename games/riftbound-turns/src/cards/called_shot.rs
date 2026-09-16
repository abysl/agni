use super::prelude::{asking, done, draw, play, spell, with_candidates};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const LOOK: usize = 2;
pub const QUESTION: &str = "a card to draw";
pub const REPEAT: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Chaos)],
};
const PICK: u8 = 1;

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

fn on_top(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    looked(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == PICK {
        return shoot(ctx, item);
    }
    let seat = item.controller;
    let top = looked(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, PICK, 1, 1))
}

fn shoot(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let Some(kept) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card))
    else {
        return done();
    };
    let rest: Vec<u32> = top.into_iter().filter(|card| *card != kept).collect();
    for card in &rest {
        ctx.recycle_to_bottom(*card);
    }
    if !rest.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", rest.len()));
    }
    if draw(ctx, seat, 1) == 1 {
        ctx.narrate(format!("{{seat {seat}}} draws the other"));
    }
    done()
}

pub static CARD: Card = spell(
    "Called Shot",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[asking(with_candidates(play(&[], run), on_top), QUESTION)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::{legal, prompts, resume, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const SHOT: u32 = 90;
    const THEIR_SHOT: u32 = 91;
    const CHAOS: [u32; 2] = [46, 47];

    fn shot(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Called Shot", 0, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shot(SHOT, 0));
        fixture.table.cards.push(shot(THEIR_SHOT, 1));
        for rune in CHAOS {
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

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(ctx: &mut Ctx, repeat: bool) {
        fixtures::play_from_hand(ctx, 0, SHOT).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(ctx, 0, if repeat { "yes" } else { "no" }).unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_repeatable_action_with_one_asking_play_ability() {
        assert!(std::ptr::eq(script_of("Called Shot").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn two_peeks_reach_the_controller_the_pick_is_drawn_and_the_other_goes_under() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        cast(&mut ctx, false);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        for card in [23, 22] {
            assert!(ctx.effects.contains(&Effect::Peek { card, seat: 0 }));
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
        }
        assert!(!ctx.effects.contains(&Effect::Peek { card: 21, seat: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}"],
            "the top two, top first"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SHOT}}}: choose {QUESTION} (0 of 1)")
        );
        let hand = ctx.hand_of(0).len();
        let draws = ctx.blob.seat(0).draws;
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, draws + 1);
        assert!(
            ctx.events
                .iter()
                .any(|event| matches!(event, Event::Drew { seat: 0, .. })),
            "the kept card is drawn, not put"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: 23,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(
            deck_of(&ctx, 0),
            [23, 20, 21],
            "23 went under, 22 was drawn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws the other".to_string()));
        assert_eq!(ctx.card(SHOT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_a_chaos_rune_it_looks_and_draws_twice() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, true);
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "two Chaos recycled, nothing exhausted"
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "{card 22}"]);
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            }),
            "the second execution looks again"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 21}", "{card 20}"],
            "22 went under, so the next two are offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 20}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(20).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(deck_of(&ctx, 0), [21, 22]);
        assert_eq!(ctx.blob.seat(0).draws, 2);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thin_deck_offers_what_there_is_and_an_empty_one_does_nothing() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, false);
        assert_eq!(fixtures::labels(&ctx), ["{card 23}"]);
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert!(deck_of(&ctx, 0).is_empty());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("recycles")));
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        cast(&mut ctx, false);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_pick_is_refused_and_an_action_has_no_window_on_their_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SHOT)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx, false);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 2 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 })
            .unwrap()
            .unwrap();
        resume(&mut ctx, &answered).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.fault.is_none());
    }
}

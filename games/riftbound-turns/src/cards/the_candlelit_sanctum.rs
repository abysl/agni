use super::prelude::{asking, battlefield, done, on_conquer, with_candidates};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 2;
pub const QUESTION: &str = "cards from the top two to recycle, then the one to leave on top";
const RECYCLE: u8 = 1;
const ORDER: u8 = 2;

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
    match stage.0 {
        RECYCLE => recycle(ctx, item),
        ORDER => order(ctx, item),
        _ => look(ctx, item),
    }
}

fn look(ctx: &mut Ctx, item: &Item) -> Flow {
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
        "{{seat {seat}}} looks at the top {} card{} of their deck",
        top.len(),
        if top.len() == 1 { "" } else { "s" }
    ));
    Flow::Ask(ctx.ask_resume(item, RECYCLE, 0, top.len() as u8))
}

fn recycle(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let recycled: Vec<u32> = ctx
        .picks()
        .iter()
        .copied()
        .filter(|card| top.contains(card))
        .collect();
    for card in &recycled {
        ctx.recycle_to_bottom(*card);
    }
    if !recycled.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", recycled.len()));
    }
    let kept = top.len() - recycled.len();
    if kept < LOOK {
        if kept == 1 {
            ctx.narrate(format!("{{seat {seat}}} puts the other back on top"));
        }
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, ORDER, 1, 1))
}

fn order(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let Some(first) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card))
    else {
        return done();
    };
    if let Some(deck) = ctx.zones.main_deck {
        ctx.emit(Effect::Move {
            card: first,
            zone: deck,
            seat,
            index: TOP,
        });
    }
    ctx.narrate(format!("{{seat {seat}}} puts both back in their order"));
    done()
}

pub static CARD: Card = battlefield(
    "The Candlelit Sanctum",
    &[],
    &[asking(
        with_candidates(on_conquer(&[], run), on_top),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const SANCTUM: u32 = fixtures::GROUNDS;

    fn sanctum() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SANCTUM).unwrap().name = "The Candlelit Sanctum".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SANCTUM).unwrap(),
            &CARD
        ));
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn conquer_and_resolve(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
        assert!(ctx.blob.chain.iter().any(|item| matches!(
            item.kind,
            ItemKind::Trigger { source, .. } if source == SANCTUM
        )));
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_sanctum_is_a_conquer_trigger_that_asks_in_two_stages() {
        assert!(std::ptr::eq(
            crate::cards::script_of("The Candlelit Sanctum").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn conquering_peeks_the_top_two_and_recycling_one_leaves_the_other_on_top() {
        let mut fixture = sanctum();
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        conquer_and_resolve(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RECYCLE
            })
        );
        for card in [23, 22] {
            assert!(ctx.effects.contains(&Effect::Peek { card, seat: 0 }));
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
        }
        assert!(!ctx.effects.contains(&Effect::Peek { card: 21, seat: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "done", "skip"],
            "the top two, top first"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SANCTUM}}}: choose {QUESTION} (0 of 2)")
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 22}", "done"]);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none(), "one left: nothing to order");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: 23,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(
            deck_of(&ctx, 0),
            [23, 20, 21, 22],
            "23 went under, 22 stays on top"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts the other back on top".to_string()));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Drew { .. })),
            "a look is not a draw"
        );
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_both_sends_both_under_in_the_picked_order() {
        let mut fixture = sanctum();
        let mut ctx = fixture.ctx();
        conquer_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert!(ctx.blob.prompt.is_none(), "two picks fill the prompt");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [23, 22, 20, 21]);
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 2".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn keeping_both_asks_which_stays_on_top_and_reorders_the_deck() {
        let mut fixture = sanctum();
        let mut ctx = fixture.ctx();
        conquer_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ORDER
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "{card 22}"]);
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: 22,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: TOP
        }));
        assert_eq!(
            deck_of(&ctx, 0),
            [20, 21, 23, 22],
            "22 now on top, 23 second"
        );
        assert_eq!(looked(&ctx, 0), [22, 23]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts both back in their order".to_string()));
        drop(ctx);
        let mut same = sanctum();
        let mut ctx = same.ctx();
        conquer_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(
            deck_of(&ctx, 0),
            [20, 21, 22, 23],
            "the same order is a no-op"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_one_card_deck_offers_one_and_an_empty_deck_asks_nothing_and_a_hold_is_silent() {
        let mut fixture = sanctum();
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer_and_resolve(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck_of(&ctx, 0), [23]);
        drop(ctx);
        let mut empty = sanctum();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        conquer_and_resolve(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        drop(ctx);
        let mut held = sanctum();
        held.blob.set_contested(fixtures::BF1, None);
        held.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = held.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.prompt.is_none(), "a hold is not a conquest");
        assert!(ctx.fault.is_none());
    }
}

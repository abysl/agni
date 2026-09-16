use super::prelude::{asking, deathknell, done, unit, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const PREDICT: usize = 2;
pub const QUESTION: &str = "the predicted cards to recycle, then the one to leave on top";
const RECYCLE: u8 = 1;
const ORDER: u8 = 2;

pub fn predicted(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, PREDICT))
        .unwrap_or_default()
}

fn on_top(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    predicted(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn look(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = predicted(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no card left to predict"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} of their deck",
        match top.len() {
            1 => "card".to_string(),
            count => format!("{count} cards"),
        }
    ));
    let count = u8::try_from(top.len()).unwrap_or(u8::MAX);
    Flow::Ask(ctx.ask_resume(item, RECYCLE, 0, count))
}

fn recycle(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = predicted(ctx, seat);
    let recycled: Vec<u32> = ctx
        .picks()
        .iter()
        .copied()
        .filter(|card| top.contains(card))
        .collect();
    for card in &recycled {
        ctx.recycle_to_bottom(*card);
    }
    if recycled.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} keeps the predicted cards"));
    } else {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", recycled.len()));
    }
    if top.len() - recycled.len() < 2 {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, ORDER, 0, 1))
}

fn order(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = predicted(ctx, seat);
    let Some(deck) = ctx.zones.main_deck else {
        return done();
    };
    match ctx.picks().first().copied() {
        Some(card) if top.contains(&card) => {
            ctx.emit(Effect::Move {
                card,
                zone: deck,
                seat,
                index: TOP,
            });
            ctx.narrate(format!("{{seat {seat}}} puts a predicted card on top"));
        }
        _ => ctx.narrate(format!(
            "{{seat {seat}}} leaves the predicted cards as they were"
        )),
    }
    done()
}

fn vision(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        RECYCLE => recycle(ctx, item),
        ORDER => order(ctx, item),
        _ => look(ctx, item),
    }
}

pub static CARD: Card = unit(
    "Dramatic Visionary",
    &[Keyword::Deathknell],
    &[asking(
        with_candidates(deathknell(&[], vision), on_top),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::table::CardInfo;

    const VISIONARY: u32 = 90;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const MY_TOP: u32 = 23;
    const MY_SECOND: u32 = 22;

    fn visionary(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Mind".into()],
            ..fixtures::unit(VISIONARY, zone, seat, "Dramatic Visionary", 4)
        }
    }

    fn stage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(visionary(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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

    fn die_and_look(ctx: &mut Ctx) -> u16 {
        assert_eq!(ctx.kill(VISIONARY, Cause::Rule), Killed::Yes);
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VISIONARY
        ));
        let item = ctx.blob.chain[0].id;
        fixtures::pass_until_open(ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: RECYCLE
            }),
            "the trigger parks on the recycle question"
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        item
    }

    #[test]
    fn the_script_prints_deathknell_with_one_targetless_death_ability_that_asks_a_question() {
        assert!(std::ptr::eq(
            script_of("Dramatic Visionary").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Dramatic Visionary");
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(
            death.targets.is_empty(),
            "436.2 · Predict chooses nothing on the chain"
        );
        assert!(!death.optional);
        assert!(death.condition.is_none());
        assert!(death.candidates.is_some());
        assert_eq!(death.question, Some(QUESTION));
        assert_eq!(PREDICT, 2);
        let mut fixture = stage();
        let ctx = fixture.ctx();
        assert_eq!(predicted(&ctx, 0), [MY_TOP, MY_SECOND]);
        assert_eq!(predicted(&ctx, 1), [25, 24]);
    }

    #[test]
    fn dying_peeks_the_top_two_to_its_controller_alone_and_offers_them_with_done_and_skip() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        die_and_look(&mut ctx);
        assert_eq!(peeks(&ctx), [(MY_TOP, 0), (MY_SECOND, 0)]);
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Reveal { .. })),
            "817.1.b · a look is not a reveal"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_TOP}}}"),
                format!("{{card {MY_SECOND}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top 2 cards of their deck".to_string()));
        assert_eq!(deck_of(&ctx, 0), MY_DECK, "nothing has moved yet");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_both_sends_them_under_the_deck_and_asks_no_order() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        die_and_look(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SECOND}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.prompt.is_none(),
            "one card left on top needs no order"
        );
        assert_eq!(deck_of(&ctx, 0), [MY_SECOND, MY_TOP, 20, 21]);
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 2".to_string()));
        assert_eq!(
            deck_of(&ctx, 1),
            [24, 25],
            "the opponent's deck is untouched"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_one_leaves_the_other_on_top_without_an_order_question() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        die_and_look(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck_of(&ctx, 0), [MY_TOP, 20, 21, MY_SECOND]);
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn keeping_both_asks_which_goes_on_top_and_the_pick_is_put_there() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        let item = die_and_look(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item, stage: ORDER }),
            "436.1.a · the rest go back in any order"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_TOP}}}"),
                format!("{{card {MY_SECOND}}}"),
                "skip".to_string()
            ]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps the predicted cards".to_string()));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SECOND}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck_of(&ctx, 0), [20, 21, MY_TOP, MY_SECOND]);
        assert_eq!(predicted(&ctx, 0), [MY_SECOND, MY_TOP]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts a predicted card on top".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_order_keeps_the_deck_as_it_was() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        die_and_look(&mut ctx);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), MY_DECK);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} leaves the predicted cards as they were".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_one_card_deck_predicts_one_and_an_empty_deck_predicts_nothing() {
        let mut fixture = stage();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.id == MY_TOP);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        die_and_look(&mut ctx);
        assert_eq!(peeks(&ctx), [(MY_TOP, 0)]);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()],
            "436.4 · as many as possible"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top card of their deck".to_string()));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "one card needs no order");
        assert_eq!(deck_of(&ctx, 0), [MY_TOP]);
        drop(ctx);

        let mut empty = stage();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        assert_eq!(ctx.kill(VISIONARY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(peeks(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card left to predict".to_string()));
        assert_eq!(
            deck_of(&ctx, 0).len(),
            0,
            "436.4.a · no Burn Out for predicting too few"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_visionary_predicts_for_the_opponent() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(visionary(fixtures::BASE, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(VISIONARY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain[0].controller, 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: RECYCLE, .. })
        ));
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(peeks(&ctx), [(25, 1), (24, 1)]);
        assert_eq!(
            fixtures::choose(&mut ctx, 0, "{card 25}"),
            Err(crate::Refusal::Pick(
                agni_plugin_sdk::prompt::PickRefusal::NotYourPrompt { seat: 1 }
            )),
            "the question is the opponent's"
        );
        fixtures::choose(&mut ctx, 1, "{card 25}").unwrap();
        fixtures::choose(&mut ctx, 1, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 1), [25, 24]);
        assert_eq!(deck_of(&ctx, 0), MY_DECK);
        assert!(ctx.fault.is_none());
    }
}

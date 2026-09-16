use super::prelude::{
    asking, done, empower, is_empowered, on_empowered, unit, with_candidates, with_statics,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const PREDICT: usize = 2;
pub const MIGHT_WHILE_EMPOWERED: i16 = 1;
pub const QUESTION: &str = "the predicted cards to recycle, then the one to leave on top";
pub const RECYCLE: u8 = 1;
pub const ORDER: u8 = 2;

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
        ctx.narrate(format!("{{seat {seat}}} has no card to predict"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} predicts {} · looks at the top {} of their deck",
        top.len(),
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

fn predict(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        RECYCLE => recycle(ctx, item),
        ORDER => order(ctx, item),
        _ => look(ctx, item),
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Apprentice Mage",
        &[Keyword::Empower(EMPOWER)],
        &[
            empower(EMPOWER),
            asking(
                with_candidates(on_empowered(&[], predict), on_top),
                QUESTION,
            ),
        ],
    ),
    &[Static::While(
        is_empowered,
        &[Grant::Might(MIGHT_WHILE_EMPOWERED)],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::table::CardInfo;

    const MAGE: u32 = 90;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const MY_TOP: u32 = 23;
    const MY_SECOND: u32 = 22;

    fn mage() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::unit(MAGE, fixtures::BASE, 0, "Apprentice Mage", 3)
        }
    }

    fn academy() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mage());
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MAGE).unwrap(), &CARD));
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn empower_her(ctx: &mut Ctx) -> u16 {
        activate::activate(ctx, 0, MAGE, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(MAGE));
        assert!(ctx.events.contains(&Event::Empowered { card: MAGE, by: 0 }));
        assert_eq!(ctx.blob.chain.len(), 1, "the become-Empowered trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == MAGE
        ));
        let item = ctx.blob.chain[0].id;
        fixtures::pass_until_open(ctx);
        item
    }

    #[test]
    fn the_script_prints_empower_predicts_when_empowered_and_grows_by_one_while_empowered() {
        assert!(std::ptr::eq(script_of("Apprentice Mage").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        let predict = &CARD.abilities[1];
        assert_eq!(predict.trigger, Trigger::Empowered);
        assert!(
            predict.targets.is_empty(),
            "436.2 · Predict chooses nothing on the chain"
        );
        assert!(predict.candidates.is_some());
        assert_eq!(predict.question, Some(QUESTION));
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(MIGHT_WHILE_EMPOWERED)])]
        ));
        assert_eq!(PREDICT, 2);
    }

    #[test]
    fn empowering_her_pays_two_predicts_two_and_the_recycle_goes_under_the_deck() {
        let mut fixture = academy();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(MAGE), 3);
        let ready = ctx.ready_runes_of(0).len();
        let item = empower_her(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2, "two energy");
        assert_eq!(ctx.current_might(MAGE), 4, "Empowered · she has +1 Might");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: RECYCLE
            })
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
            .contains(&"{seat 0} predicts 2 · looks at the top 2 cards of their deck".to_string()));
        assert_eq!(deck_of(&ctx, 0), MY_DECK, "nothing has moved yet");
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one card left on top needs no order"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [MY_TOP, 20, 21, MY_SECOND]);
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert_eq!(deck_of(&ctx, 1), [24, 25], "the other deck is untouched");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn keeping_both_asks_which_goes_on_top_and_the_pick_leads_the_deck() {
        let mut fixture = academy();
        let mut ctx = fixture.ctx();
        let item = empower_her(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: ORDER }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_TOP}}}"),
                format!("{{card {MY_SECOND}}}"),
                "skip".to_string()
            ],
            "one pick closes the question"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SECOND}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [20, 21, MY_TOP, MY_SECOND]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts a predicted card on top".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_empower_the_other_seat_and_an_empty_deck_are_refused_or_predict_nothing() {
        let mut fixture = academy();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, MAGE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        empower_her(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, MAGE, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.disempower(MAGE));
        assert_eq!(
            ctx.current_might(MAGE),
            3,
            "the +1 is gone with the Empowered"
        );
        drop(ctx);

        let mut fixture = academy();
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MAGE, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to predict".to_string()));
        assert!(ctx.fault.is_none());
    }
}

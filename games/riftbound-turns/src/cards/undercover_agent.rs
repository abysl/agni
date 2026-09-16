use super::prelude::{ask_discard, deathknell, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DISCARDS: u8 = 2;
pub const DRAWS: usize = 2;

fn burn_the_cover(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 < DISCARDS {
        if let Some(ask) = ask_discard(ctx, item, stage.0 + 1) {
            return Flow::Ask(ask);
        }
        if stage.0 == 0 {
            ctx.narrate(format!("{{seat {seat}}} has nothing to discard"));
        }
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Undercover Agent",
    &[Keyword::Deathknell],
    &[deathknell(&[], burn_the_cover)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const AGENT: u32 = 90;
    const HAND_CARDS: [u32; 4] = [
        fixtures::HAND_UNIT,
        fixtures::HAND_SPELL,
        fixtures::HAND_GEAR,
        fixtures::HAND_HIDDEN,
    ];

    fn agent(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Undercover Agent", 5);
        card.energy = Some(5);
        card.power = Some(1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn undercover() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(agent(AGENT, fixtures::BASE, 0));
        fixture.resolve();
        fixture
    }

    fn with_hand(keep: &[u32]) -> Fixture {
        let mut fixture = undercover();
        fixture
            .table
            .cards
            .retain(|card| !HAND_CARDS.contains(&card.id) || keep.contains(&card.id));
        fixture.resolve();
        fixture
    }

    fn die(ctx: &mut Ctx) {
        assert_eq!(ctx.kill(AGENT, Cause::Rule), Killed::Yes);
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == AGENT
        ));
        fixtures::pass_until_open(ctx);
    }

    fn discard_prompt(ctx: &Ctx, stage: u8) {
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: ctx.blob.chain[0].id,
                stage
            })
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(
            prompts::status(ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
    }

    #[test]
    fn the_script_is_a_deathknell_unit_that_discards_then_draws() {
        assert!(std::ptr::eq(script_of("Undercover Agent").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Death);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!((DISCARDS, DRAWS), (2, 2));
        let fixture = undercover();
        assert!(std::ptr::eq(fixture.scripts.of_card(AGENT).unwrap(), &CARD));
    }

    #[test]
    fn with_a_full_hand_the_trigger_asks_for_two_discards_one_at_a_time_then_draws_two() {
        let mut fixture = with_hand(&[
            fixtures::HAND_UNIT,
            fixtures::HAND_SPELL,
            fixtures::HAND_GEAR,
        ]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hand_of(0).len(), 3);
        die(&mut ctx);
        discard_prompt(&ctx, 1);
        assert_eq!(
            fixtures::labels(&ctx).len(),
            3,
            "every card in hand, no closers"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        discard_prompt(&ctx, 2);
        assert_eq!(fixtures::labels(&ctx).len(), 2, "the hand shrank by one");
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Drew { .. })),
            "no draw before the second discard"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), 3, "one kept, two drawn");
        assert!(ctx.hand_of(0).contains(&fixtures::HAND_SPELL));
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} discards {card 72}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} discards {card 70}".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_one_card_in_hand_the_rest_of_the_discard_is_ignored_and_two_are_still_drawn() {
        let mut fixture = with_hand(&[fixtures::HAND_SPELL]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hand_of(0).len(), 1);
        die(&mut ctx);
        discard_prompt(&ctx, 1);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "422.4 · no second prompt for an empty hand"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), DRAWS);
        assert!(!ctx.hand_of(0).contains(&fixtures::HAND_SPELL));
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_the_whole_discard_is_ignored_and_two_are_drawn_without_a_prompt() {
        let mut fixture = with_hand(&[]);
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        die(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing to discard".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_answer_the_discard_prompt() {
        let mut fixture = with_hand(&[fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
        let mut ctx = fixture.ctx();
        die(&mut ctx);
        discard_prompt(&ctx, 1);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the prompt belongs to the agent's controller"
        );
        assert_eq!(ctx.hand_of(0).len(), 2, "nothing was discarded");
        assert!(ctx.blob.prompt.is_some());
    }
}

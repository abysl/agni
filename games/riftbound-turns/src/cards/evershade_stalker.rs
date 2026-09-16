use super::prelude::{ask_discard, done, draw, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const STAGE_DISCARDED: u8 = 1;

fn stalk(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != STAGE_DISCARDED {
        match ask_discard(ctx, item, STAGE_DISCARDED) {
            Some(ask) => return Flow::Ask(ask),
            None => ctx.narrate(format!("{{seat {seat}}} has no card to discard")),
        }
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = unit("Evershade Stalker", &[], &[play(&[], stalk)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const STALKER: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    fn stalker(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(STALKER, zone, seat, "Evershade Stalker", 3)
        }
    }

    fn shade() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stalker(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx) {
        play_engine::begin(ctx, 0, STALKER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "355.10.d · nothing is chosen at play"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == STALKER
        ));
        fixtures::pass_until_open(ctx);
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_asks_nothing_up_front() {
        assert!(std::ptr::eq(script_of("Evershade Stalker").unwrap(), &CARD));
        assert_eq!(CARD.name, "Evershade Stalker");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert!(
            ability.question.is_none(),
            "the discard is the engine's own prompt"
        );
        assert_eq!((DRAWS, STAGE_DISCARDED), (1, 1));
    }

    #[test]
    fn the_trigger_asks_its_controller_to_discard_then_draws_one_after_the_pick() {
        let mut fixture = shade();
        let mut ctx = fixture.ctx();
        enter(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item,
                stage: STAGE_DISCARDED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        let hand = ctx.hand_of(0);
        assert_eq!(
            fixtures::labels(&ctx),
            hand.iter()
                .map(|card| format!("{{card {card}}}"))
                .collect::<Vec<String>>(),
            "every card in hand, the stalker itself no longer among them"
        );
        assert_eq!(draws(&ctx), 0, "then draw: nothing before the discard");
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the opponent cannot discard for you"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::HAND_SPELL,
            zone: fixtures::TRASH,
            seat: 0,
            index: TOP
        }));
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(draws(&ctx), 1);
        assert_eq!(ctx.hand_of(0).len(), hand.len(), "one out, one in");
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} discards {{card {}}}",
            fixtures::HAND_SPELL
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {STALKER}}} · {{seat 0}} draws 1")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_there_is_nothing_to_discard_and_the_draw_still_happens() {
        let mut fixture = shade();
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.seat != 0 || card.id == STALKER
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        enter(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no hand, no prompt");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to discard".to_string()));
        assert_eq!(draws(&ctx), 1);
        assert_eq!(ctx.hand_of(0).len(), 1);
    }
}

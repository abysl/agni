use super::prelude::{
    asking, battlefield, channel_exhausted, done, optional, triggered, with_candidates,
};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "the top rune of your Rune Deck to channel exhausted";
const RUNES: usize = 1;
const CHANNEL: u8 = 1;

fn top_rune(ctx: &Ctx, seat: u8) -> Option<u32> {
    let deck = ctx.zones.rune_deck?;
    ctx.top_of(deck, seat, RUNES).first().copied()
}

fn the_top_rune(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    top_rune(ctx, item.controller)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn may_channel_one_exhausted(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == CHANNEL {
        match ctx.picks().first().copied() {
            Some(rune) if top_rune(ctx, seat) == Some(rune) => {
                channel_exhausted(ctx, seat, RUNES);
            }
            _ => ctx.narrate(format!("{{seat {seat}}} channels nothing")),
        }
        return done();
    }
    if top_rune(ctx, seat).is_none() {
        ctx.narrate(format!("{{seat {seat}}} has no rune left to channel"));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, CHANNEL, 0, 1))
}

pub static CARD: Card = battlefield(
    "Startipped Peak",
    &[],
    &[asking(
        with_candidates(
            optional(triggered(
                Trigger::Hold(Who::You),
                &[],
                may_channel_one_exhausted,
            )),
            the_top_rune,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const PEAK: u32 = fixtures::GROUNDS;
    const MY_TOP_RUNE: u32 = 32;

    fn peak_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(PEAK).unwrap().name = "Startipped Peak".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PEAK).unwrap(), &CARD));
        fixture
    }

    fn hold_and_resolve(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
        assert!(ctx.blob.chain.iter().any(|item| matches!(
            item.kind,
            ItemKind::Trigger { source, .. } if source == PEAK
        )));
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn runes_in_pool(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    fn rune_deck(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::RUNE_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_peak_is_an_optional_hold_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Startipped Peak").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn holding_offers_the_top_rune_and_taking_it_channels_it_exhausted() {
        let mut fixture = peak_held_by_me();
        let mut ctx = fixture.ctx();
        assert_eq!(rune_deck(&ctx, 0), [30, 31, 32]);
        let pool = runes_in_pool(&ctx, 0);
        hold_and_resolve(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: CHANNEL
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP_RUNE}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {PEAK}}}: choose {QUESTION} (0 of 1)")
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
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP_RUNE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP_RUNE,
            zone: fixtures::RUNE_POOL,
            seat: 0,
            index: TOP
        }));
        assert_eq!(rune_deck(&ctx, 0), [30, 31]);
        assert_eq!(runes_in_pool(&ctx, 0), pool + 1);
        assert!(
            ctx.card(MY_TOP_RUNE).unwrap().exhausted,
            "channelled exhausted"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_channels_nothing_and_an_empty_rune_deck_asks_nothing() {
        let mut fixture = peak_held_by_me();
        let mut ctx = fixture.ctx();
        let pool = runes_in_pool(&ctx, 0);
        hold_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(rune_deck(&ctx, 0), [30, 31, 32]);
        assert_eq!(runes_in_pool(&ctx, 0), pool);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels nothing".to_string()));
        drop(ctx);
        let mut empty = peak_held_by_me();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        hold_and_resolve(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(runes_in_pool(&ctx, 0), pool);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no rune left to channel".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_holding_their_own_battlefield_never_wakes_the_peak() {
        let mut fixture = peak_held_by_me();
        let mut ctx = fixture.ctx();
        let pool = runes_in_pool(&ctx, 1);
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(runes_in_pool(&ctx, 1), pool);
        assert_eq!(rune_deck(&ctx, 1), [33, 34, 35]);
    }
}

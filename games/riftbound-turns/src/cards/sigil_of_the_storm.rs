use super::prelude::{asking, battlefield, done, on_conquer, with_candidates};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const PICK: u8 = 1;

fn runes_of_the_conqueror(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    ctx.runes_of(item.controller)
        .into_iter()
        .map(|rune| TargetRef::Card(rune.id))
        .collect()
}

fn recycle_a_rune(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let runes = ctx.runes_of(seat);
        if let Some(rune) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| runes.iter().any(|rune| rune.id == *picked))
        {
            ctx.recycle_to_bottom(rune);
            ctx.narrate(format!("{{seat {seat}}} recycles {{card {rune}}}"));
        }
        return done();
    }
    if ctx.runes_of(seat).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICK, 1, 1))
}

pub static CARD: Card = battlefield(
    "Sigil of the Storm",
    &[],
    &[asking(
        with_candidates(on_conquer(&[], recycle_a_rune), runes_of_the_conqueror),
        "a rune to recycle",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const SIGIL: u32 = fixtures::GROUNDS;

    fn sigil() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SIGIL).unwrap().name = "Sigil of the Storm".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SIGIL).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_sigil_is_a_battlefield_with_one_conquer_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Sigil of the Storm").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some("a rune to recycle"));
    }

    #[test]
    fn conquering_the_sigil_asks_the_conqueror_for_one_of_their_runes_and_recycles_it() {
        let mut fixture = sigil();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 40}", "{card 41}", "{card 42}", "{card 43}"],
            "every rune of the conqueror, exhausted or ready"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            format!("{{card {SIGIL}}}: choose a rune to recycle (0 of 1)")
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
        fixtures::choose(&mut ctx, 0, "{card 42}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.effects.contains(&Effect::Move {
            card: 42,
            zone: fixtures::RUNE_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(ctx.runes_of(0).len(), 3);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles {card 42}".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conqueror_with_no_runes_is_not_asked() {
        let mut fixture = sigil();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_POOL) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { zone, .. } if *zone == fixtures::RUNE_DECK
        )));
    }
}

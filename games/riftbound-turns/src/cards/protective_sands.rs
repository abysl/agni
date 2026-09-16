use super::prelude::{battlefield, done, draw, on_conquer, optional, when, with_cost, ONE_ENERGY};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const RUNES_AT_MOST: usize = 4;
pub const DRAW: usize = 1;

pub fn few_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() <= RUNES_AT_MOST
}

fn the_conqueror_controls_few_runes(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Conquered { seat, .. } if few_runes(ctx, *seat))
}

fn draw_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAW);
    done()
}

pub static CARD: Card = battlefield(
    "Protective Sands",
    &[],
    &[when(
        optional(with_cost(on_conquer(&[], draw_one), ONE_ENERGY)),
        the_conqueror_controls_few_runes,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle, triggers};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const SANDS: u32 = fixtures::GROUNDS;
    const FIFTH_RUNE: u32 = 46;

    fn sands() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SANDS).unwrap().name = "Protective Sands".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SANDS).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn sands_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == SANDS => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: by, .. } if *by == seat))
            .count()
    }

    #[test]
    fn the_sands_are_a_conditional_paid_may_conquer_trigger() {
        assert!(std::ptr::eq(script_of("Protective Sands").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(RUNES_AT_MOST, 4);
        assert_eq!(DRAW, 1);
    }

    #[test]
    fn conquering_on_four_runes_asks_for_the_energy_and_yes_draws_one() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(few_runes(&ctx, 0));
        let hand = ctx.table.held(fixtures::HAND, 0).count();
        conquer(&mut ctx);
        assert_eq!(sands_items(&ctx), [0]);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 1 energy for the {{card {SANDS}}} trigger · {{zone 9}}?")
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1, "one rune pays");
        assert!(ctx.effects.contains(&Effect::exhaust(41)));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(drew(&ctx, 0), 0, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.table.held(fixtures::HAND, 0).count(), hand + 1);
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_or_lacking_the_energy_removes_the_trigger_and_draws_nothing() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SANDS}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut broke = sands();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        broke.resolve();
        let mut ctx = broke.ctx();
        conquer(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "an unpayable cost is not offered"
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SANDS}}} trigger is removed · its cost can't be paid"
        )));
    }

    #[test]
    fn five_runes_never_trigger_the_sands_and_a_conquer_elsewhere_is_not_here() {
        let mut fixture = sands();
        fixture
            .table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Calm", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 5);
        assert!(!few_runes(&ctx, 0));
        conquer(&mut ctx);
        assert!(sands_items(&ctx).is_empty(), "the if fails at the trigger");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(triggers::find(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF2,
                seat: 1,
                units: vec![],
            },
        )
        .is_empty());
        assert!(few_runes(&ctx, 1), "seat 1's two runes are their own count");
        assert_eq!(
            triggers::find(
                &ctx,
                &Event::Conquered {
                    zone: fixtures::BF1,
                    seat: 1,
                    units: vec![],
                },
            )
            .len(),
            1,
            "the opponent conquering here on few runes is offered the draw"
        );
    }

    #[test]
    fn a_fifth_rune_gained_in_response_does_not_stop_the_paid_draw() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Calm", false));
        assert_eq!(ctx.runes_of(0).len(), 5);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            drew(&ctx, 0),
            1,
            "383.2.a.1 · the if is the trigger Condition; the energy paid at finalization buys the draw"
        );
        assert!(ctx.fault.is_none());
    }
}

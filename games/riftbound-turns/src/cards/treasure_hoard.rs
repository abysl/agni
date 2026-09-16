use super::prelude::{battlefield, done, on_conquer, optional, spawn_gold, with_cost, ONE_ENERGY};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const GOLD_ARRIVES_READY: bool = false;

fn play_a_gold_exhausted(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = battlefield(
    "Treasure Hoard",
    &[],
    &[optional(with_cost(
        on_conquer(&[], play_a_gold_exhausted),
        ONE_ENERGY,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who, TOKEN_GOLD};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const HOARD: u32 = fixtures::GROUNDS;

    fn hoard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(HOARD).unwrap().name = "Treasure Hoard".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(HOARD).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn golds(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn hoard_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == HOARD => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_hoard_is_an_optional_one_energy_conquer_trigger_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Treasure Hoard").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert!(ability.timing().is_none());
    }

    #[test]
    fn paying_one_energy_plays_an_exhausted_gold_to_the_conquerors_base_when_the_trigger_resolves()
    {
        let mut fixture = hoard();
        let mut ctx = fixture.ctx();
        assert!(golds(&ctx, 0).is_empty());
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {HOARD}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.effects.contains(&Effect::exhaust(41)),
            "{:?}",
            ctx.effects
        );
        assert_eq!(hoard_items(&ctx), [0]);
        assert!(golds(&ctx, 0).is_empty(), "not before the trigger resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let minted = golds(&ctx, 0);
        assert_eq!(minted.len(), 1);
        let gold = ctx.card(minted[0]).unwrap();
        assert_eq!(gold.zone, Some(fixtures::BASE));
        assert!(gold.exhausted, "played exhausted");
        assert!(ctx.is_token(minted[0]));
        assert!(golds(&ctx, 1).is_empty());
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == minted[0]
        )));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_mints_nothing_and_an_unaffordable_cost_is_never_offered() {
        let mut fixture = hoard();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(hoard_items(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(golds(&ctx, 0).is_empty());
        assert!(!ctx.effects.contains(&Effect::exhaust(41)));
        drop(ctx);
        let mut broke = hoard();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(golds(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {HOARD}}} trigger is removed · its cost can't be paid"
        )));
    }

    #[test]
    fn a_hold_is_not_a_conquer_and_the_trigger_is_not_an_affordance() {
        let mut fixture = hoard();
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(hoard_items(&ctx).is_empty());
        assert!(golds(&ctx, 0).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, HOARD, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
    }
}

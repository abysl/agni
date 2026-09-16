use super::prelude::{battlefield, done, optional, score_point, triggered, with_cost};
use super::{Card, Cost, Flow, Item, Power, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const FOUR_RAINBOW: Cost = Cost {
    energy: 0,
    power: &[
        Power::Rainbow,
        Power::Rainbow,
        Power::Rainbow,
        Power::Rainbow,
    ],
};
pub const POINTS: u8 = 1;

fn score_one_point(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = battlefield(
    "Power Nexus",
    &[],
    &[optional(with_cost(
        triggered(Trigger::Hold(Who::You), &[], score_one_point),
        FOUR_RAINBOW,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, prompts, settle};
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const NEXUS: u32 = fixtures::GROUNDS;
    const MY_RUNES: [u32; 4] = [fixtures::RUNE_A, 41, 42, 43];

    fn nexus_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(NEXUS).unwrap().name = "Power Nexus".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(NEXUS).unwrap(), &CARD));
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn nexus_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == NEXUS => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn runes_in_pool(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.runes_of(seat).into_iter().map(|rune| rune.id).collect()
    }

    #[test]
    fn the_nexus_is_an_optional_hold_trigger_costing_four_rainbow_power() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Power Nexus").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(ability.cost, Some(FOUR_RAINBOW));
        assert_eq!(FOUR_RAINBOW.energy, 0);
        assert_eq!(FOUR_RAINBOW.power.len(), 4);
        assert!(FOUR_RAINBOW
            .power
            .iter()
            .all(|need| *need == Power::Rainbow));
        assert_eq!(POINTS, 1);
        assert!(ability.timing().is_none());
    }

    #[test]
    fn holding_asks_for_four_power_and_paying_recycles_four_runes_for_a_second_point() {
        let mut fixture = nexus_held_by_me();
        let mut ctx = fixture.ctx();
        assert_eq!(runes_in_pool(&ctx, 0), MY_RUNES);
        hold(&mut ctx);
        assert_eq!(ctx.points(0), 1, "the hold itself scores");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 4 any power for the {{card {NEXUS}}} trigger · {{zone {}}}?",
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
            runes_in_pool(&ctx, 0).is_empty(),
            "four rainbow power recycles all four runes, the exhausted one included"
        );
        assert_eq!(
            ctx.table.held(fixtures::RUNE_DECK, 0).count(),
            3 + 4,
            "recycled to the bottom of the rune deck"
        );
        assert_eq!(nexus_items(&ctx), [0]);
        assert_eq!(ctx.points(0), 1, "not before the trigger resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_point_is_gained_not_conquered_so_the_final_point_needs_no_full_sweep() {
        let mut fixture = nexus_held_by_me();
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 2);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert_eq!(ctx.points(0), DEFAULT_VICTORY_SCORE - 1);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.points(0),
            DEFAULT_VICTORY_SCORE,
            "471.1.a.1 · a point from an effect ignores the final-point restriction"
        );
        assert_eq!(ctx.hand_of(0).len(), 4, "no draw instead of the point");
    }

    #[test]
    fn declining_keeps_the_runes_and_three_runes_are_never_asked() {
        let mut fixture = nexus_held_by_me();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(nexus_items(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(runes_in_pool(&ctx, 0), MY_RUNES);
        assert_eq!(ctx.points(0), 1);
        drop(ctx);
        let mut short = nexus_held_by_me();
        short.table.cards.retain(|card| card.id != 43);
        short.resolve();
        let mut ctx = short.ctx();
        hold(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(runes_in_pool(&ctx, 0).len(), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {NEXUS}}} trigger is removed · its cost can't be paid"
        )));
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_the_trigger_is_not_an_affordance() {
        let mut fixture = nexus_held_by_me();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(nexus_items(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, NEXUS, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        drop(ctx);
        let mut theirs = nexus_held_by_me();
        let mut ctx = theirs.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(nexus_items(&ctx).is_empty(), "a hold elsewhere is not here");
    }
}

use super::prelude::{battlefield, done, on_conquer, optional, ready, when, with_cost, ONE_ENERGY};
use super::{Card, Flow, Item, Source, Stage, KIND_LEGEND};
use crate::engine::ctx::{Ctx, Event};

pub fn legends_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.table
        .cards
        .iter()
        .filter(|held| held.is_kind(KIND_LEGEND) && ctx.controller(held.id) == seat)
        .map(|held| held.id)
        .collect()
}

pub fn the_conquerors_legend_is_exhausted(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Conquered { seat, .. } = event else {
        return false;
    };
    legends_of(ctx, *seat)
        .into_iter()
        .any(|legend| ctx.card(legend).is_some_and(|held| held.exhausted))
}

fn ready_your_legend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for legend in legends_of(ctx, seat) {
        if ready(ctx, legend) {
            ctx.narrate(format!(
                "{{card {}}} readies {{card {legend}}}",
                item.kind.source()
            ));
        }
    }
    done()
}

pub static CARD: Card = battlefield(
    "Hall of Legends",
    &[],
    &[when(
        optional(with_cost(on_conquer(&[], ready_your_legend), ONE_ENERGY)),
        the_conquerors_legend_is_exhausted,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const HALL: u32 = fixtures::GROUNDS;
    const LEGEND: u32 = fixtures::LEGEND_CARD;
    const THEIR_LEGEND: u32 = 90;

    fn hall(legend_exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(HALL).unwrap().name = "Hall of Legends".into();
        fixture.table.card_mut(LEGEND).unwrap().exhausted = legend_exhausted;
        fixture.table.cards.push(fixtures::card(
            THEIR_LEGEND,
            fixtures::LEGEND,
            1,
            "Kha'Zix - Voidreaver",
            KIND_LEGEND,
        ));
        fixture.table.card_mut(THEIR_LEGEND).unwrap().exhausted = true;
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(HALL).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn hall_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == HALL => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_hall_is_an_optional_one_energy_conquer_trigger_gated_on_an_exhausted_legend() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Hall of Legends").unwrap(),
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
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert!(ability.condition.is_some());
        assert!(ability.timing().is_none());
    }

    #[test]
    fn the_legends_of_a_seat_are_read_by_kind_and_controller() {
        let mut fixture = hall(true);
        let ctx = fixture.ctx();
        assert_eq!(legends_of(&ctx, 0), [LEGEND]);
        assert_eq!(legends_of(&ctx, 1), [THEIR_LEGEND]);
        let conquered = Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        };
        let source = Source {
            card: HALL,
            ability: 0,
        };
        assert!(the_conquerors_legend_is_exhausted(&ctx, &conquered, source));
        assert!(!the_conquerors_legend_is_exhausted(
            &ctx,
            &Event::Held {
                zone: fixtures::BF1,
                seat: 0,
                units: vec![fixtures::VI],
            },
            source
        ));
    }

    #[test]
    fn conquering_asks_for_one_energy_and_paying_readies_the_conquerors_legend_when_it_resolves() {
        let mut fixture = hall(true);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {HALL}}} trigger · {{zone {}}}?",
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
        assert_eq!(hall_items(&ctx), [0]);
        assert!(
            ctx.card(LEGEND).unwrap().exhausted,
            "not before the trigger resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        assert!(
            ctx.card(THEIR_LEGEND).unwrap().exhausted,
            "the opponent's legend is not yours"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {HALL}}} readies {{card {LEGEND}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_removes_the_trigger_and_keeps_the_legend_exhausted() {
        let mut fixture = hall(true);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(hall_items(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(LEGEND).unwrap().exhausted);
        assert!(!ctx.effects.contains(&Effect::exhaust(41)));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {HALL}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn a_ready_legend_has_nothing_to_pay_for_and_no_ready_rune_removes_the_trigger_unasked() {
        let mut fixture = hall(false);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(hall_items(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut broke = hall(true);
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(LEGEND).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {HALL}}} trigger is removed · its cost can't be paid"
        )));
    }

    #[test]
    fn a_conquer_elsewhere_never_triggers_the_hall_and_the_trigger_is_not_an_affordance() {
        let mut fixture = hall(true);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF2),
            Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(hall_items(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, HALL, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
    }
}

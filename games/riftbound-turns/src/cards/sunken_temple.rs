use super::prelude::{battlefield, done, draw, is_mighty, triggered, when};
use super::{Ability, Card, Cost, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::{Ctx, Event};

const DRAWS: usize = 1;

const ONE_ENERGY: Cost = Cost {
    energy: 1,
    power: &[],
};

fn conquered_with_a_mighty_unit(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Conquered { seat, units, .. } = event else {
        return false;
    };
    units
        .iter()
        .copied()
        .filter(|unit| ctx.controller(*unit) == *seat)
        .any(|unit| is_mighty(ctx, unit))
}

fn draw_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

const PAY_ONE_ENERGY_TO_DRAW: Ability = Ability {
    cost: Some(ONE_ENERGY),
    viable: None,
    ..when(
        triggered(Trigger::Conquer(Who::You), &[], draw_one),
        conquered_with_a_mighty_unit,
    )
};

pub static CARD: Card = battlefield("Sunken Temple", &[], &[PAY_ONE_ENERGY_TO_DRAW]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::MIGHTY;
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts, resume, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;

    const TEMPLE: u32 = fixtures::GROUNDS;
    const BRAUM: u32 = 93;
    const NEARLY: u32 = 94;

    fn sunken_temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TEMPLE).unwrap().name = "Sunken Temple".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TEMPLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn with_a_mighty_unit() -> Fixture {
        let mut fixture = sunken_temple();
        fixture.table.cards.push(fixtures::unit(
            BRAUM,
            fixtures::BF1,
            0,
            "Braum",
            MIGHTY as u8,
        ));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx, seat: u8) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(seat)
        );
        settle(ctx).unwrap();
    }

    fn temple_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == TEMPLE => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn confirm(ctx: &mut Ctx, seat: u8, option: u16) {
        let prompt = ctx.blob.prompt.as_ref().expect("the cost confirm").id;
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })
            .unwrap()
            .expect("the confirm closes on one pick");
        resume(ctx, &answered).unwrap();
        settle(ctx).unwrap();
    }

    fn spent_runes(ctx: &Ctx, seat: u8) -> usize {
        ctx.runes_of(seat)
            .into_iter()
            .filter(|rune| rune.exhausted)
            .count()
    }

    #[test]
    fn the_temple_is_a_conquer_trigger_gated_on_might_that_carries_an_optional_energy_cost() {
        assert_eq!(CARD.name, "Sunken Temple");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some());
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert!(ability.timing().is_none(), "a trigger, never an affordance");
    }

    #[test]
    fn a_mighty_unit_conquering_the_temple_puts_the_trigger_on_the_chain_and_it_draws_one() {
        let mut fixture = with_a_mighty_unit();
        let mut ctx = fixture.ctx();
        assert!(ctx.current_might(BRAUM) >= MIGHTY);
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        confirm(&mut ctx, 0, 0);
        assert_eq!(temple_items(&ctx), [0]);
        assert_eq!(ctx.hand_of(0).len(), hand, "not before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TEMPLE}}} ability resolves")));
    }

    #[test]
    fn a_conquer_without_a_mighty_unit_never_triggers_the_temple() {
        let mut fixture = sunken_temple();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.current_might(fixtures::VI) < MIGHTY);
        conquer(&mut ctx, 0);
        assert!(temple_items(&ctx).is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn might_is_read_off_the_board_so_a_buff_makes_a_conqueror_mighty() {
        let mut fixture = sunken_temple();
        fixture.table.cards.push(fixtures::unit(
            NEARLY,
            fixtures::BF1,
            0,
            "Braum",
            MIGHTY as u8 - 1,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.current_might(NEARLY) < MIGHTY);
        assert!(ctx.buff(NEARLY));
        assert_eq!(ctx.current_might(NEARLY), MIGHTY);
        conquer(&mut ctx, 0);
        confirm(&mut ctx, 0, 0);
        assert_eq!(temple_items(&ctx), [0], "5 might on the board is Mighty");
    }

    #[test]
    fn the_temples_trigger_is_not_an_affordance_a_seat_can_activate() {
        let mut fixture = with_a_mighty_unit();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, TEMPLE, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, TEMPLE, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
    }

    #[test]
    fn the_temple_asks_before_it_draws_and_a_declined_energy_costs_the_card() {
        let mut fixture = with_a_mighty_unit();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let spent = spent_runes(&ctx, 0);
        conquer(&mut ctx, 0);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost: 1, .. })),
            "{:?}",
            ctx.blob.why
        );
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(labels, ["yes", "no"]);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {TEMPLE}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option: 1 })
            .unwrap()
            .expect("the confirm closes on one pick");
        resume(&mut ctx, &answered).unwrap();
        settle(&mut ctx).unwrap();
        assert!(temple_items(&ctx).is_empty(), "declined, so removed");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(spent_runes(&ctx, 0), spent);

        let mut fixture = with_a_mighty_unit();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, 0);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 })
            .unwrap()
            .expect("the confirm closes on one pick");
        resume(&mut ctx, &answered).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(spent_runes(&ctx, 0), spent + 1, "one rune exhausted");
        assert_eq!(temple_items(&ctx), [0]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }
}

use super::prelude::{at_end_of_turn, battlefield, done, ready_runes, triggered};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const READY_AT_END_OF_TURN: u8 = 1;
const RUNES: usize = 2;

fn schedule_the_readying(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    at_end_of_turn(ctx, item, READY_AT_END_OF_TURN, Vec::new());
    ctx.narrate(format!(
        "{{seat {seat}}} will ready {RUNES} runes at the end of this turn"
    ));
    done()
}

fn ready_two_runes(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let readied = ready_runes(ctx, seat, RUNES);
    if readied > 0 {
        ctx.narrate(format!(
            "{{seat {seat}}} readies {readied} of {RUNES} runes"
        ));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Targon's Peak",
    &[],
    &[
        triggered(Trigger::Conquer(Who::You), &[], schedule_the_readying),
        triggered(Trigger::Reflexive, &[], ready_two_runes),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, settle};
    use crate::state::{ItemKind, When};
    use crate::Refusal;

    const PEAK: u32 = fixtures::GROUNDS;
    const SPENT: [u32; 3] = [fixtures::RUNE_A, 41, 42];
    const READY: u32 = 43;

    fn targons_peak() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(PEAK).unwrap().name = "Targon's Peak".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        for rune in SPENT {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PEAK).unwrap(), &CARD));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = crate::engine::priority::holder(ctx).expect("someone holds priority");
            crate::engine::priority::pass(ctx, holder).unwrap();
        }
    }

    fn exhausted(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.runes_of(seat)
            .into_iter()
            .filter(|rune| rune.exhausted)
            .map(|rune| rune.id)
            .collect()
    }

    #[test]
    fn the_peak_schedules_a_second_ability_of_its_own_that_no_event_can_fire() {
        assert_eq!(CARD.name, "Targon's Peak");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Conquer(Who::You));
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.abilities[0].cost.is_none());
        let delayed = &CARD.abilities[usize::from(READY_AT_END_OF_TURN)];
        assert_eq!(delayed.trigger, Trigger::Reflexive);
        assert!(delayed.targets.is_empty());
        assert!(delayed.condition.is_none());
        assert!(delayed.timing().is_none());
    }

    #[test]
    fn conquering_the_peak_readies_nothing_now_and_two_runes_at_the_ending_step() {
        let mut fixture = targons_peak();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(
            exhausted(&ctx, 0),
            SPENT,
            "the conquer itself readies nothing"
        );
        assert_eq!(ctx.blob.delayed.len(), 1);
        let delayed = ctx.blob.delayed[0].clone();
        assert_eq!(delayed.when, When::EndOfTurn(ctx.turn()));
        assert_eq!((delayed.source, delayed.seat), (PEAK, 0));
        assert_eq!(delayed.ability, READY_AT_END_OF_TURN);
        assert!(delayed.args.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} will ready 2 runes at the end of this turn".to_string()));

        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = crate::engine::priority::holder(&ctx).expect("someone holds priority");
            crate::engine::priority::pass(&mut ctx, holder).unwrap();
        }
        assert_eq!(
            exhausted(&ctx, 0),
            [42],
            "the first two spent runes come back"
        );
        assert!(!ctx.card(READY).unwrap().exhausted);
        assert!(ctx.blob.delayed.is_empty(), "a delayed entry fires once");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} readies 2 of 2 runes".to_string()));
    }

    #[test]
    fn the_readying_lands_even_though_the_peak_is_no_longer_held() {
        let mut fixture = targons_peak();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        ctx.recall(fixtures::VI, false);
        cleanup::run(&mut ctx, None);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the conqueror walked away"
        );
        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = crate::engine::priority::holder(&ctx).expect("someone holds priority");
            crate::engine::priority::pass(&mut ctx, holder).unwrap();
        }
        assert_eq!(exhausted(&ctx, 0), [42]);
    }

    #[test]
    fn a_second_conquer_the_same_turn_never_happens_and_neither_does_a_second_promise() {
        let mut fixture = targons_peak();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            Established::Kept(0),
            "446.1 · one conquer per battlefield per turn"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == PEAK)));
    }

    #[test]
    fn neither_of_the_peaks_abilities_is_an_affordance_a_seat_can_activate() {
        let mut fixture = targons_peak();
        let mut ctx = fixture.ctx();
        for index in [0, READY_AT_END_OF_TURN] {
            assert_eq!(
                activate::activate(&mut ctx, 0, PEAK, index),
                Err(Refusal::Illegal(Reason::NoSuchAbility))
            );
        }
        assert!(ctx.blob.delayed.is_empty() && ctx.blob.chain.is_empty());
    }
}

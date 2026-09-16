use super::prelude::{done, draw, on_hold_me, unit, when};
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Location};

pub const DRAWS: usize = 1;

pub fn other_units_you_control_here(ctx: &Ctx, unit: u32) -> usize {
    let Some(at) = ctx.location(unit) else {
        return 0;
    };
    let seat = ctx.controller(unit);
    ctx.units_at(at)
        .into_iter()
        .filter(|other| *other != unit && ctx.controller(*other) == seat)
        .count()
}

pub fn exactly_one_other_unit_you_control_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let at_the_scored_battlefield = match event {
        Event::Held { zone, .. } | Event::Conquered { zone, .. } => {
            ctx.location(source.card) == Some(Location::Battlefield(*zone))
        }
        _ => true,
    };
    at_the_scored_battlefield && other_units_you_control_here(ctx, source.card) == 1
}

fn scourge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Shen, Scourge of Shadows",
    &[],
    &[when(
        on_hold_me(&[], scourge),
        exactly_one_other_unit_you_control_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const SHEN: u32 = 90;
    const ALLY: u32 = 91;
    const SECOND_ALLY: u32 = 92;
    const INTRUDER: u32 = 93;

    fn shen(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(SHEN, zone, seat, "Shen, Scourge of Shadows", 6)
        }
    }

    fn twilight(company: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shen(fixtures::BF1, 0));
        for id in company {
            let seat = if *id == INTRUDER { 1 } else { 0 };
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BF1, seat, "Company", 2));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SHEN).unwrap(), &CARD));
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn shen_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == SHEN))
            .count()
    }

    #[test]
    fn the_script_is_one_hold_trigger_conditioned_on_exactly_one_other_unit_of_yours_here() {
        assert!(std::ptr::eq(
            script_of("Shen, Scourge of Shadows").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::Me));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_count_reads_your_own_units_here_and_never_him_or_an_enemy() {
        let mut fixture = twilight(&[ALLY, INTRUDER]);
        let ctx = fixture.ctx();
        assert_eq!(other_units_you_control_here(&ctx, SHEN), 1);
        assert_eq!(other_units_you_control_here(&ctx, ALLY), 1);
        assert_eq!(other_units_you_control_here(&ctx, INTRUDER), 0);
        assert_eq!(
            other_units_you_control_here(&ctx, fixtures::VI),
            0,
            "the base holds Vi alone"
        );
        let source = Source {
            card: SHEN,
            ability: 0,
        };
        let held = Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![SHEN, ALLY],
        };
        assert!(exactly_one_other_unit_you_control_here(&ctx, &held, source));
        let elsewhere = Event::Held {
            zone: fixtures::BF2,
            seat: 0,
            units: vec![],
        };
        assert!(
            !exactly_one_other_unit_you_control_here(&ctx, &elsewhere, source),
            "here is the battlefield that scored"
        );
    }

    #[test]
    fn holding_with_exactly_one_ally_draws_one_when_the_trigger_resolves() {
        let mut fixture = twilight(&[ALLY]);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(shen_items(&ctx), 1, "the trigger waits on the chain");
        assert_eq!(ctx.hand_of(0).len(), hand);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn alone_or_with_two_allies_the_hold_is_silent() {
        for company in [&[][..], &[ALLY, SECOND_ALLY][..]] {
            let mut fixture = twilight(company);
            let mut ctx = fixture.ctx();
            let hand = ctx.hand_of(0).len();
            hold(&mut ctx);
            assert_eq!(ctx.points(0), 1, "the hold itself always scores");
            assert_eq!(
                shen_items(&ctx),
                0,
                "383.2.a.1 · exactly one other unit you control is the condition: {company:?}"
            );
            assert_eq!(ctx.hand_of(0).len(), hand);
            assert!(ctx.fault.is_none());
        }
    }

    #[test]
    fn a_hold_elsewhere_and_an_opponents_hold_trigger_nothing() {
        let mut fixture = twilight(&[ALLY]);
        fixture.table.card_mut(SHEN).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert_eq!(shen_items(&ctx), 0, "the ally holds; he sits in the base");
        drop(ctx);

        let mut fixture = twilight(&[ALLY]);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert_eq!(shen_items(&ctx), 0);
    }
}

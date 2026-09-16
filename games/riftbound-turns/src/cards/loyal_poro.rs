use super::prelude::{deathknell, died_alone, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn devotion(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if died_alone(item) {
        ctx.narrate(format!("{{card {me}}} died alone"));
    } else {
        let seat = item.controller;
        let drawn = draw(ctx, seat, DRAWS);
        ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Loyal Poro",
    &[Keyword::Deathknell],
    &[deathknell(&[], devotion)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cleanup;
    use crate::engine::ctx::{Cause, Event, Killed, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ItemKind, Noted, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const PORO: u32 = 90;
    const SECOND_PORO: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn poro(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(id, zone, seat, "Loyal Poro", 3)
        }
    }

    fn damaged(fixture: &mut Fixture, card: u32, damage: i32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            value: damage,
        });
        fixture.table.counters.sort();
    }

    fn at_the_battlefield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(poro(PORO, fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn die_and_resolve(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        while let Some(PromptWhy::OrderTriggers { seat }) = ctx.blob.why {
            let labels = fixtures::labels(ctx);
            let next = labels
                .iter()
                .find(|label| *label == "done")
                .or_else(|| labels.first())
                .cloned()
                .expect("the order prompt offers something");
            fixtures::choose(ctx, seat, &next).unwrap();
        }
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(script_of("Loyal Poro").unwrap(), &CARD));
        assert_eq!(CARD.name, "Loyal Poro");
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(
            death.condition.is_none(),
            "the snapshot is read at resolution"
        );
        assert_eq!(DRAWS, 1);
        let fixture = at_the_battlefield();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
    }

    #[test]
    fn a_poro_dying_beside_a_friendly_unit_draws_one_when_the_deathknell_resolves() {
        let mut fixture = at_the_battlefield();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(!ctx.alone_at(PORO), "Vi keeps it company");
        assert_eq!(ctx.kill(PORO, Cause::Rule), Killed::Yes);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, noted: Noted { alone: false, .. }, .. } if *card == PORO
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PORO
        ));
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(ctx.hand_of(1).len(), 1, "the opponent draws nothing");
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_poro_that_died_alone_draws_nothing_and_an_enemy_beside_it_is_no_company() {
        let mut fixture = at_the_battlefield();
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        damaged(&mut fixture, PORO, 3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.alone_at(PORO));
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, noted: Noted { alone: true, .. }, .. } if *card == PORO
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PORO}}} died alone")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_poros_dying_in_one_batch_each_draw_one() {
        let mut fixture = at_the_battlefield();
        fixture
            .table
            .cards
            .push(poro(SECOND_PORO, fixtures::BF1, 0));
        damaged(&mut fixture, PORO, 3);
        damaged(&mut fixture, SECOND_PORO, 3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::dying(&ctx), [PORO, SECOND_PORO]);
        die_and_resolve(&mut ctx);
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(SECOND_PORO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 2 * DRAWS,
            "808.1.d.3 · each was read while the other still stood beside it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_poro_killed_at_its_base_beside_vi_draws_and_the_last_one_standing_there_does_not() {
        let mut fixture = at_the_battlefield();
        fixture.table.card_mut(PORO).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(!ctx.alone_at(PORO), "Vi stands at the base too");
        ctx.kill(PORO, Cause::Rule);
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        drop(ctx);
        let mut lonely = at_the_battlefield();
        lonely.table.card_mut(PORO).unwrap().zone = Some(fixtures::BASE);
        lonely.table.cards.retain(|card| card.id != fixtures::VI);
        lonely.resolve();
        let mut ctx = lonely.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.kill(PORO, Cause::Rule);
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand);
    }
}

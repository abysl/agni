use super::prelude::{deathknell, died_alone, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;

fn last_words(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if died_alone(item) {
        draw(ctx, item.controller, DRAWS);
    } else {
        ctx.narrate(format!("{{card {me}}} did not die alone"));
    }
    done()
}

pub static CARD: Card = unit(
    "Lonely Poro",
    &[Keyword::Deathknell],
    &[deathknell(&[], last_words)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::cleanup;
    use crate::engine::ctx::{Cause, Event, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, priority, settle};
    use crate::state::{ItemKind, Noted, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const PORO: u32 = 90;
    const SECOND_PORO: u32 = 91;
    const HAND_PORO: u32 = 92;

    fn poro(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Lonely Poro", 2);
        card.domain = vec!["Calm".into()];
        card
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
    fn the_poro_is_a_deathknell_unit_with_one_targetless_death_ability() {
        assert_eq!(CARD.name, "Lonely Poro");
        assert!(CARD.has_keyword(Keyword::Deathknell));
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(
            death.condition.is_none(),
            "the snapshot is read at resolution"
        );
        assert!(CARD.statics.is_empty());
        let fixture = at_the_battlefield();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
    }

    #[test]
    fn a_poro_that_was_the_last_friendly_unit_standing_draws_one() {
        let mut fixture = at_the_battlefield();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND_PORO, fixtures::BF1, 1, "Jinx", 4));
        damaged(&mut fixture, PORO, 2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(
            ctx.alone_at(PORO),
            "an enemy unit here does not keep it company"
        );
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, noted: Noted { alone: true, .. }, .. } if *card == PORO
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PORO
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
    }

    #[test]
    fn two_poros_dying_in_one_batch_draw_nothing() {
        let mut fixture = at_the_battlefield();
        fixture
            .table
            .cards
            .push(poro(SECOND_PORO, fixtures::BF1, 0));
        damaged(&mut fixture, PORO, 2);
        damaged(&mut fixture, SECOND_PORO, 2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::dying(&ctx), [PORO, SECOND_PORO]);
        die_and_resolve(&mut ctx);
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(SECOND_PORO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "808.1.d.3 · each was read while the other still stood beside it"
        );
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.ends_with("did not die alone"))
                .count(),
            2
        );
    }

    #[test]
    fn a_poro_dying_beside_a_sprite_that_survives_draws_nothing() {
        let mut fixture = at_the_battlefield();
        fixture.table.cards.push(CardInfo {
            ..fixtures::unit(SECOND_PORO, fixtures::BF1, 0, "Sprite", 3)
        });
        fixture.table.tokens.push(SECOND_PORO);
        damaged(&mut fixture, PORO, 2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(!ctx.alone_at(PORO), "740.2.a · a token counts as company");
        die_and_resolve(&mut ctx);
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(SECOND_PORO));
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn a_poro_killed_at_its_base_beside_a_friendly_unit_draws_nothing_and_alone_there_draws() {
        let mut fixture = at_the_battlefield();
        fixture.table.card_mut(PORO).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.location(PORO), Some(Location::Base(0)));
        assert!(!ctx.alone_at(PORO), "Vi stands at the base too");
        ctx.kill(PORO, Cause::Rule);
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand);
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
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }

    #[test]
    fn the_other_seat_cannot_play_the_poro_and_its_deathknell_needs_no_target() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(HAND_PORO, fixtures::HAND, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: HAND_PORO,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "seat 1 cannot play a unit on seat 0's turn"
        );
        drop(ctx);
        let mut mine = Fixture::enforced();
        mine.table.cards.push(poro(HAND_PORO, fixtures::HAND, 0));
        mine.resolve();
        let action = fixtures::move_action(HAND_PORO, fixtures::BASE, 0);
        let mut ctx = mine.ctx_for(0, &action);
        play_engine::begin(
            &mut ctx,
            0,
            HAND_PORO,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(HAND_PORO), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none());
    }
}

use super::prelude::{done, draw, friendly_units, is_mighty, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub fn mighty_units(ctx: &Ctx, seat: u8) -> usize {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_mighty(ctx, *unit))
        .count()
}

fn inferno(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let count = mighty_units(ctx, seat);
    if count == 0 {
        ctx.narrate(format!("{{seat {seat}}} has no Mighty units · no draw"));
        return done();
    }
    draw(ctx, seat, count);
    ctx.narrate(format!(
        "{{seat {seat}}} draws {count} · one for each Mighty unit"
    ));
    done()
}

pub static CARD: Card = unit("Kadregrin the Infernal", &[], &[play(&[], inferno)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::MIGHTY;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const KADREGRIN: u32 = 90;
    const BRUTE: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn kadregrin(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(9),
            domain: vec!["Fury".into()],
            ..fixtures::card(id, zone, seat, "Kadregrin the Infernal", "Unit")
        }
    }

    fn lair() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(kadregrin(KADREGRIN, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 0, "Brute", 5));
        fixture.table.cards.push(fixtures::unit(
            THEIR_BRUTE,
            fixtures::BASE,
            1,
            "Their Brute",
            7,
        ));
        for id in 26..30 {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn descend(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, KADREGRIN, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn draws(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_with_a_targetless_play_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Kadregrin the Infernal").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Kadregrin the Infernal");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(MIGHTY, 5);
    }

    #[test]
    fn kadregrin_draws_one_for_each_friendly_mighty_unit_counting_himself() {
        let mut fixture = lair();
        let action = fixtures::move_action(KADREGRIN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let their_hand = ctx.hand_of(1).len();
        descend(&mut ctx).unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KADREGRIN
        ));
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(
            mighty_units(&ctx, 0),
            2,
            "Kadregrin at 9 and the Brute at 5; Vi at 3 is not Mighty"
        );
        assert_eq!(draws(&ctx, 0), 0, "the draw waits for the trigger");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        assert_eq!(draws(&ctx, 0), 2);
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand,
            "the enemy's 7 Might unit is not ours"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws 2 · one for each Mighty unit".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_count_is_read_at_resolution_so_a_this_turn_bonus_adds_a_draw_and_a_death_removes_one() {
        let mut fixture = lair();
        let action = fixtures::move_action(KADREGRIN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        descend(&mut ctx).unwrap();
        let hand = ctx.hand_of(0).len();
        let until = crate::state::Expiry::EndOfTurn(ctx.turn());
        ctx.might(fixtures::VI, 2, until, None, 0);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        ctx.kill(BRUTE, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert_eq!(mighty_units(&ctx, 0), 2, "Vi grew into it, the Brute died");
        resolve_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        assert_eq!(draws(&ctx, 0), 2);
    }

    #[test]
    fn with_kadregrin_dead_before_resolution_and_no_other_mighty_unit_nothing_is_drawn() {
        let mut fixture = lair();
        fixture.table.cards.retain(|card| card.id != BRUTE);
        fixture.resolve();
        let action = fixtures::move_action(KADREGRIN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        descend(&mut ctx).unwrap();
        let hand = ctx.hand_of(0).len();
        ctx.kill(KADREGRIN, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(KADREGRIN));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand, "no Mighty unit, no draw");
        assert_eq!(draws(&ctx, 0), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no Mighty units · no draw".to_string()));
    }
}

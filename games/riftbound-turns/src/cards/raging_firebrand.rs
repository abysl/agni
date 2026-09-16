use super::prelude::{discount_of, done, play, promise_this_turn, unit, PromiseKind};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: u8 = 5;

pub fn kindle_next_spell(ctx: &mut Ctx, seat: u8) {
    promise_this_turn(ctx, seat, PromiseKind::Spell, discount_of(DISCOUNT, 0));
}

fn kindle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    kindle_next_spell(ctx, seat);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}}'s next spell this turn costs {DISCOUNT} energy less",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = unit("Raging Firebrand", &[], &[play(&[], kindle)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{Promise, PromiseEffect};
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use crate::state::{ChainItem, Expiry, ItemKind, Origin, Pool};
    use agni_plugin_sdk::table::CardInfo;

    const FIREBRAND: u32 = 90;
    const BIG_SPELL: u32 = 91;

    fn firebrand() -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            ..fixtures::unit(FIREBRAND, fixtures::HAND, 0, "Raging Firebrand", 4)
        }
    }

    fn forge() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(firebrand());
        fixture.table.cards.push(fixtures::spell(
            BIG_SPELL,
            fixtures::HAND,
            0,
            "Cannon Barrage",
            6,
            1,
        ));
        for id in [46, 47, 48, 49] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    fn spell_item(card: u32) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card }, 0, Origin::Hand)
    }

    fn kindled(turn: u16) -> Promise {
        Promise {
            kind: PromiseKind::Spell,
            effect: PromiseEffect::Discount(Pool {
                energy: DISCOUNT,
                power: Vec::new(),
            }),
            until: Expiry::EndOfTurn(turn),
        }
    }

    #[test]
    fn the_firebrand_is_a_unit_with_one_play_trigger() {
        assert!(std::ptr::eq(script_of("Raging Firebrand").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!(DISCOUNT, 5);
    }

    #[test]
    fn playing_him_arms_a_five_energy_discount_that_the_next_spell_spends() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 7);
        fixtures::play_from_hand(&mut ctx, 0, FIREBRAND).unwrap();
        assert_eq!(ctx.location(FIREBRAND), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "six energy and one Fury");
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "nothing until the trigger resolves"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.seat(0).promises, [kindled(1)]);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FIREBRAND}}} · {{seat 0}}'s next spell this turn costs 5 energy less"
        )));
        let priced = cost::of_item(&ctx, &spell_item(BIG_SPELL), None);
        assert_eq!(priced.energy, 1, "six less five");
        assert_eq!(priced.power.len(), 1, "the power is untouched");
        assert_eq!(priced.promises, [0]);
        assert_eq!(
            cost::of_item(&ctx, &spell_item(fixtures::HAND_SPELL), None).energy,
            0,
            "356.6 · a two-cost spell goes to zero, not below"
        );
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "the last rune pays the one energy and the one power"
        );
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "the discount is spent by the spell"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s discount is used".to_string()));
        assert_eq!(
            cost::of_item(&ctx, &spell_item(fixtures::HAND_SPELL), None).energy,
            2,
            "the next spell after that pays in full"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn two_firebrands_stack_and_a_countered_firebrand_arms_nothing() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        kindle_next_spell(&mut ctx, 0);
        kindle_next_spell(&mut ctx, 0);
        assert_eq!(ctx.blob.seat(0).promises, [kindled(1), kindled(1)]);
        let priced = cost::of_item(&ctx, &spell_item(BIG_SPELL), None);
        assert_eq!(priced.energy, 0, "six less ten stops at zero");
        assert_eq!(
            priced.promises,
            [0, 1],
            "both Firebrands price the same next spell and both are spent by it"
        );
        let mut countered = forge();
        let mut ctx = countered.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIREBRAND).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        let trigger = ctx.blob.chain[0].id;
        assert!(crate::engine::chain::counter(
            &mut ctx,
            trigger,
            crate::engine::ctx::CounterDest::Trash
        ));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "a countered trigger never resolves"
        );
        assert_eq!(
            ctx.location(FIREBRAND),
            Some(Location::Base(0)),
            "the unit itself stays"
        );
    }

    #[test]
    fn the_discount_skips_a_unit_played_before_the_spell() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIREBRAND).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).promises, [kindled(1)]);
        let unit = ChainItem::new(
            2,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(
            cost::of_item(&ctx, &unit, None).energy,
            2,
            "the next spell, not the next card"
        );
        assert_eq!(cost::of_item(&ctx, &spell_item(BIG_SPELL), None).energy, 1);
    }

    #[test]
    fn the_discount_expires_with_the_turn() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIREBRAND).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).promises, [kindled(1)]);
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "this turn, not the next"
        );
        assert_eq!(cost::of_item(&ctx, &spell_item(BIG_SPELL), None).energy, 6);
    }
}

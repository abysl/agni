use super::prelude::{legion, unit, with_statics};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn legion_discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if legion(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit("Noxus Hopeful", &[Keyword::Legion], &[]),
    &[Static::SelfDiscount(legion_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, priority};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;

    const HOPEFUL: u32 = 90;

    fn barracks(legion_on: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut hopeful = fixtures::unit(HOPEFUL, fixtures::HAND, 0, "Noxus Hopeful", 4);
        hopeful.domain = vec!["Fury".into()];
        hopeful.energy = Some(4);
        fixture.table.cards.push(hopeful);
        fixture.blob.seat_mut(0).played_main = legion_on;
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: HOPEFUL }, seat, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: HOPEFUL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_legion_unit_with_no_abilities_and_one_self_discount() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Noxus Hopeful").unwrap(),
            &CARD
        ));
        let fixture = barracks(false);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HOPEFUL).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(legion_discount)));
        assert_eq!(DISCOUNT.energy, 2);
        assert!(DISCOUNT.power.is_empty());
    }

    #[test]
    fn it_costs_two_once_the_seat_has_played_a_card_this_turn_and_four_before() {
        let mut fixture = barracks(false);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 4);
        assert_eq!(cost::total(&ctx, HOPEFUL, false).energy, 4);
        drop(ctx);

        let mut fixture = barracks(true);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 2);
        assert_eq!(cost::total(&ctx, HOPEFUL, false).energy, 2);
        assert!(
            cost::of_item(&ctx, &item(0), None).power.is_empty(),
            "the discount touches only energy"
        );
        drop(ctx);

        let mut fixture = barracks(false);
        fixture.blob.seat_mut(1).played_main = true;
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            4,
            "812.1.c · the opponent's play is not my Legion"
        );
    }

    #[test]
    fn played_after_a_spell_it_takes_two_runes_and_enters_as_a_plain_four_might_unit() {
        let mut fixture = barracks(false);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 5);
        assert!(
            legal::classify(&ctx, 0, &entry(&ctx)).is_ok(),
            "five ready runes pay the printed four before Legion"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(legion(&ctx, 0));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "Spark took two: one rune exhausted and recycled for its power"
        );
        fixtures::play_from_hand(&mut ctx, 0, HOPEFUL).unwrap();
        assert!(ctx.on_board(HOPEFUL));
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy from two runes");
        assert!(ctx.blob.chain.is_empty(), "no trigger of its own");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.current_might(HOPEFUL), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_three_ready_runes_cannot_pay_the_printed_four() {
        let mut fixture = barracks(false);
        let ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
        drop(ctx);

        let mut fixture = barracks(true);
        let mut ctx = fixture.ctx();
        assert!(legal::classify(&ctx, 0, &entry(&ctx)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, HOPEFUL).unwrap();
        assert!(ctx.on_board(HOPEFUL));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "two of the three ready runes"
        );
    }
}

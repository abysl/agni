use super::prelude::{unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;

pub const PER_CARD: u8 = 1;
pub const MINIMUM: u8 = 1;

pub fn cards_played_this_turn(ctx: &Ctx, seat: u8) -> u8 {
    ctx.blob.seat(seat).cards_played
}

pub fn ram_discount(ctx: &Ctx, card: u32, seat: u8) -> u8 {
    let printed = ctx.card(card).and_then(|held| held.energy).unwrap_or(0);
    let room = printed.saturating_sub(MINIMUM);
    cards_played_this_turn(ctx, seat)
        .saturating_mul(PER_CARD)
        .min(room)
}

fn discount(ctx: &Ctx, card: u32, seat: u8) -> Cost {
    Cost {
        energy: ram_discount(ctx, card, seat),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Battering Ram", &[], &[]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, priority};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;

    const RAM: u32 = 90;
    const PRINTED: u8 = 5;

    fn siege(cards_played: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut ram = fixtures::unit(RAM, fixtures::HAND, 0, "Battering Ram", 5);
        ram.energy = Some(PRINTED);
        fixture.table.cards.push(ram);
        fixture.blob.seat_mut(0).cards_played = cards_played;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RAM).unwrap(), &CARD));
        fixture
    }

    fn item() -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: RAM }, 0, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: RAM,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_self_discount() {
        assert!(std::ptr::eq(script_of("Battering Ram").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
    }

    #[test]
    fn it_costs_one_less_per_card_played_this_turn_down_to_one_and_the_power_is_untouched() {
        for (played, expected) in [(0u8, 5u8), (1, 4), (2, 3), (4, 1), (7, 1)] {
            let mut fixture = siege(played);
            let ctx = fixture.ctx();
            assert_eq!(ram_discount(&ctx, RAM, 0), PRINTED - expected);
            assert_eq!(
                cost::of_item(&ctx, &item(), None).energy,
                expected,
                "{played} cards played"
            );
            assert_eq!(cost::total(&ctx, RAM, false).energy, expected);
            assert!(cost::of_item(&ctx, &item(), None).power.is_empty());
        }
        let mut fixture = siege(0);
        fixture.blob.seat_mut(1).cards_played = 3;
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_item(&ctx, &item(), None).energy,
            5,
            "the opponent's plays are not yours"
        );
    }

    #[test]
    fn a_spell_played_first_makes_the_ram_affordable_on_the_runes_left() {
        let mut fixture = siege(0);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 6);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(cards_played_this_turn(&ctx, 0), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "Spark took two: one exhausted, one recycled"
        );
        assert_eq!(cost::total(&ctx, RAM, false).energy, 4);
        assert!(legal::classify(&ctx, 0, &entry(&ctx)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, RAM).unwrap();
        assert!(ctx.on_board(RAM));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy from four runes"
        );
        assert_eq!(cards_played_this_turn(&ctx, 0), 2);
        assert_eq!(ctx.current_might(RAM), 5);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_it_is_refused_on_four_ready_runes() {
        let mut fixture = siege(0);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            })
        );
    }
}

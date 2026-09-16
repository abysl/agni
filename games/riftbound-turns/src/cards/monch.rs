use super::prelude::{enemy_units, unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn an_opponent_controls_a_stunned_unit(ctx: &Ctx, seat: u8) -> bool {
    enemy_units(ctx, seat)
        .into_iter()
        .any(|unit| ctx.is_stunned(unit))
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if an_opponent_controls_a_stunned_unit(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    an_opponent_controls_a_stunned_unit(ctx, ctx.controller(me))
}

pub static CARD: Card = with_statics(
    unit("Monch", &[], &[]),
    &[
        Static::SelfDiscount(discount),
        Static::EntersReady(enters_ready),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MONCH: u32 = 90;
    const ENERGY: u8 = 6;
    const MIGHT: u8 = 6;
    const CALM_RUNES: [u32; 2] = [46, 47];

    fn monch(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Calm".into()],
            ..fixtures::unit(MONCH, zone, seat, "Monch", MIGHT)
        }
    }

    fn bandle(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(monch(zone, 0));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MONCH).unwrap(), &CARD));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: MONCH }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_is_a_unit_with_one_self_discount_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Monch").unwrap(), &CARD));
        assert_eq!(CARD.name, "Monch");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 2);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.additional.is_none());
        assert_eq!(DISCOUNT.energy, 2);
        assert!(DISCOUNT.power.is_empty());
    }

    #[test]
    fn a_stunned_enemy_unit_takes_two_energy_off_and_reads_as_entering_ready() {
        let mut fixture = bandle(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(!an_opponent_controls_a_stunned_unit(&ctx, 0));
        assert!(!enters_ready(&ctx, MONCH));
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY);
        assert!(ctx.stun(fixtures::VI));
        assert!(
            !enters_ready(&ctx, MONCH),
            "your own stunned unit is not an opponent's"
        );
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY);
        assert!(
            an_opponent_controls_a_stunned_unit(&ctx, 1),
            "from the other seat Vi is the opponent's"
        );
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        assert!(an_opponent_controls_a_stunned_unit(&ctx, 0));
        assert!(enters_ready(&ctx, MONCH));
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 2);
        assert_eq!(cost::total(&ctx, MONCH, false).energy, ENERGY - 2);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            ENERGY - 2,
            "priced for the other seat, stunned Vi is their opponent's"
        );
        ctx.unstun(fixtures::VI);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            ENERGY,
            "Jinx is their own"
        );
        ctx.unstun(fixtures::THEIR_UNIT);
        assert!(!enters_ready(&ctx, MONCH), "the stun wore off");
    }

    #[test]
    fn played_for_four_beside_a_stunned_enemy_and_refused_at_six_with_four_runes() {
        let mut fixture = bandle(fixtures::HAND);
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, MONCH),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 2
            })
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut fixture = bandle(fixtures::HAND);
        fixture.table.card_mut(41).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, MONCH),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 4
            }),
            "nobody is stunned: six is the price"
        );
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        fixtures::play_from_hand(&mut ctx, 0, MONCH).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(MONCH), Some(Location::Base(0)));
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "four energy off four runes"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_beside_a_stunned_enemy_it_enters_ready() {
        let mut fixture = bandle(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        fixtures::play_from_hand(&mut ctx, 0, MONCH).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(MONCH));
        assert!(
            !ctx.card(MONCH).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
    }
}

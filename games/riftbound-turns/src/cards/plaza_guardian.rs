use super::prelude::{friendly_gear, unit, with_statics};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const REDUCTION: u8 = 1;

pub fn gear_you_control(ctx: &Ctx, seat: u8) -> u8 {
    u8::try_from(friendly_gear(ctx, seat).len()).unwrap_or(u8::MAX)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: gear_you_control(ctx, seat).saturating_mul(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Plaza Guardian", &[Keyword::Deflect(1)], &[]),
    &[Static::SelfDiscount(discount)],
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

    const GUARDIAN: u32 = 90;
    const FIRST_GEAR: u32 = 100;
    const GOLD: u32 = 120;
    const THEIR_GEAR: u32 = 121;
    const ENERGY: u8 = 10;
    const MIGHT: u8 = 8;

    fn guardian(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Mind".into()],
            ..fixtures::unit(GUARDIAN, zone, seat, "Plaza Guardian", MIGHT)
        }
    }

    fn plaza(gear: u32, gold: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guardian(fixtures::HAND, 0));
        for offset in 0..gear {
            fixture.table.cards.push(fixtures::gear(
                FIRST_GEAR + offset,
                fixtures::BASE,
                0,
                "Cog",
                1,
            ));
        }
        if gold {
            fixture.table.cards.push(fixtures::gold(GOLD, 0, true));
            fixture.table.tokens.push(GOLD);
        }
        fixture.table.cards.push(fixtures::gear(
            THEIR_GEAR,
            fixtures::BASE,
            1,
            "Their Cog",
            1,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GUARDIAN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Permanent { card: GUARDIAN },
            seat,
            Origin::Hand,
        )
    }

    #[test]
    fn the_script_is_a_deflect_unit_with_one_self_discount() {
        assert!(std::ptr::eq(script_of("Plaza Guardian").unwrap(), &CARD));
        assert_eq!(CARD.name, "Plaza Guardian");
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(REDUCTION, 1);
        let mut fixture = plaza(0, false);
        fixture.table.card_mut(GUARDIAN).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(GUARDIAN), 1);
    }

    #[test]
    fn each_gear_you_control_takes_one_energy_off_gold_included_and_never_the_opponents() {
        let mut fixture = plaza(0, false);
        let ctx = fixture.ctx();
        assert_eq!(
            gear_you_control(&ctx, 0),
            0,
            "gear in hand is not controlled"
        );
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            ENERGY - 1,
            "priced for the other seat, their one cog counts"
        );
        drop(ctx);
        let mut fixture = plaza(2, true);
        let mut ctx = fixture.ctx();
        assert_eq!(gear_you_control(&ctx, 0), 3);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 3);
        assert_eq!(cost::total(&ctx, GUARDIAN, false).energy, ENERGY - 3);
        assert!(
            cost::of_item(&ctx, &item(0), None).power.is_empty(),
            "the discount is energy only"
        );
        ctx.trash(FIRST_GEAR);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 2);
        drop(ctx);
        let mut fixture = plaza(12, false);
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            0,
            "twelve gear is free, never negative"
        );
    }

    #[test]
    fn played_for_three_beside_seven_gear_and_refused_at_four_beside_six() {
        let mut fixture = plaza(6, false);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, GUARDIAN),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY - 6,
                ready: 3
            })
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut fixture = plaza(7, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(GUARDIAN), Some(Location::Base(0)));
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "three energy off three runes"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

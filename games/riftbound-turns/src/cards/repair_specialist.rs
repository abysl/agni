use super::prelude::{friendly_gear, unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn gear_you_control(ctx: &Ctx, me: u32) -> usize {
    friendly_gear(ctx, ctx.controller(me)).len()
}

pub fn assault_value(ctx: &Ctx, me: u32) -> u8 {
    u8::try_from(gear_you_control(ctx, me).min(LADDER)).unwrap_or(u8::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32) -> bool {
    gear_you_control(ctx, card) >= N
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Static::While(at_least::<$step>, &[Grant::Keyword(Keyword::Assault(1))])),+]
    };
}

pub static GEAR_COUNT: &[Static] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(unit("Repair Specialist", &[], &[]), GEAR_COUNT);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::table::CardInfo;

    const SPECIALIST: u32 = 90;
    const WRENCH: u32 = 91;
    const HAMMER: u32 = 92;
    const GOLD: u32 = 93;
    const THEIR_GEAR: u32 = 94;
    const MIGHT: u8 = 3;

    fn specialist(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(SPECIALIST, zone, 0, "Repair Specialist", MIGHT)
        }
    }

    fn workshop(gear: &[u32], gold: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(specialist(fixtures::BF1));
        for id in gear {
            fixture
                .table
                .cards
                .push(fixtures::gear(*id, fixtures::BASE, 0, "Wrench", 1));
        }
        if gold {
            fixture.table.cards.push(fixtures::gold(GOLD, 0, false));
            fixture.table.tokens.push(GOLD);
        }
        fixture.table.cards.push(fixtures::gear(
            THEIR_GEAR,
            fixtures::BASE,
            1,
            "Their Wrench",
            1,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPECIALIST).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_keywordless_unit_whose_whiles_are_one_assault_per_gear_you_control() {
        assert!(std::ptr::eq(script_of("Repair Specialist").unwrap(), &CARD));
        assert_eq!(CARD.name, "Repair Specialist");
        assert!(
            CARD.keywords.is_empty(),
            "the Assault is granted, not printed"
        );
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), LADDER);
        assert!(CARD.statics.iter().all(|held| matches!(
            held,
            Static::While(_, [Grant::Keyword(Keyword::Assault(1))])
        )));
    }

    #[test]
    fn his_assault_is_your_gear_count_gold_included_and_reads_as_might_only_while_attacking() {
        let mut fixture = workshop(&[WRENCH, HAMMER], true);
        let mut ctx = fixture.ctx();
        assert_eq!(gear_you_control(&ctx, SPECIALIST), 3);
        assert_eq!(assault_value(&ctx, SPECIALIST), 3);
        assert_eq!(
            statics::grants_on(&ctx, SPECIALIST).len(),
            3,
            "the opponent's gear does not count"
        );
        assert!(ctx.has_keyword(SPECIALIST, Keyword::Assault(1)));
        assert_eq!(
            ctx.current_might(SPECIALIST),
            i32::from(MIGHT),
            "807.1.d · Assault is Might for an attacker only"
        );
        assert!(ctx.mark_defender(SPECIALIST));
        assert_eq!(ctx.current_might(SPECIALIST), i32::from(MIGHT));
        assert!(ctx.mark_attacker(SPECIALIST));
        assert_eq!(ctx.current_might(SPECIALIST), i32::from(MIGHT) + 3);
        ctx.trash(HAMMER);
        assert_eq!(assault_value(&ctx, SPECIALIST), 2);
        assert_eq!(
            ctx.current_might(SPECIALIST),
            i32::from(MIGHT) + 2,
            "one gear fewer"
        );
        assert!(ctx.set_controller(THEIR_GEAR, 0, SPECIALIST));
        assert_eq!(
            assault_value(&ctx, SPECIALIST),
            3,
            "a stolen gear is yours to count"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_gear_he_has_no_assault_and_gear_in_hand_is_not_controlled() {
        let mut fixture = workshop(&[], false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            gear_you_control(&ctx, SPECIALIST),
            0,
            "Boots of Swiftness in hand is not gear you control"
        );
        assert!(statics::grants_on(&ctx, SPECIALIST).is_empty());
        assert!(!ctx.has_keyword(SPECIALIST, Keyword::Assault(1)));
        assert!(ctx.mark_attacker(SPECIALIST));
        assert_eq!(ctx.current_might(SPECIALIST), i32::from(MIGHT));
        drop(ctx);
        let mut fixture = workshop(&[WRENCH], false);
        fixture.table.card_mut(SPECIALIST).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(gear_you_control(&ctx, SPECIALIST), 1);
        assert!(
            statics::grants_on(&ctx, SPECIALIST).is_empty(),
            "365.1 · not on the board, so the passive is inactive"
        );
    }
}

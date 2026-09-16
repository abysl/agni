use super::prelude::{friendly_units, is_mighty, unit, with_statics};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const PER_MIGHTY: u8 = 2;

pub fn mighty_units(ctx: &Ctx, seat: u8) -> u8 {
    let count = friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_mighty(ctx, *unit))
        .count();
    u8::try_from(count).unwrap_or(u8::MAX)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: mighty_units(ctx, seat).saturating_mul(PER_MIGHTY),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Jaull-Fish", &[Keyword::Accelerate], &[]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const JAULL: u32 = 90;
    const BIG: u32 = 91;
    const BIGGER: u32 = 92;
    const THEIRS: u32 = 93;
    const BODY_RUNE: u32 = 46;

    fn jaull() -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(JAULL, fixtures::HAND, 0, "Jaull-Fish", 6)
        }
    }

    fn reef(big: u8, bigger: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jaull());
        fixture
            .table
            .cards
            .push(fixtures::unit(BIG, fixtures::BASE, 0, "Big", big));
        fixture
            .table
            .cards
            .push(fixtures::unit(BIGGER, fixtures::BF1, 0, "Bigger", bigger));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIRS, fixtures::BF1, 1, "Theirs", 9));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(JAULL).unwrap(), &CARD));
        fixture
    }

    fn item() -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: JAULL }, 0, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: JAULL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_an_accelerate_unit_with_one_self_discount() {
        assert!(std::ptr::eq(script_of("Jaull-Fish").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
    }

    #[test]
    fn two_energy_off_per_mighty_friendly_unit_and_the_enemy_giant_counts_for_nothing() {
        let mut fixture = reef(4, 4);
        let ctx = fixture.ctx();
        assert_eq!(mighty_units(&ctx, 0), 0);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 7);
        assert_eq!(
            cost::of_item(&ctx, &item(), None).power,
            [Need::Domain(Domain::Body), Need::Domain(Domain::Body)]
        );
        drop(ctx);

        let mut fixture = reef(5, 4);
        let ctx = fixture.ctx();
        assert_eq!(mighty_units(&ctx, 0), 1);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 5);
        drop(ctx);

        let mut fixture = reef(5, 8);
        let mut ctx = fixture.ctx();
        assert_eq!(mighty_units(&ctx, 0), 2);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 3);
        assert_eq!(cost::total(&ctx, JAULL, false).energy, 3);
        assert_eq!(
            cost::total(&ctx, JAULL, true).energy,
            4,
            "the Accelerate energy is part of the total the discount reads"
        );
        assert!(ctx.buff(BIG), "4 + 1 buff is still Mighty");
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 3);
        assert!(ctx.stun(BIGGER));
        assert_eq!(
            mighty_units(&ctx, 0),
            2,
            "a stun zeroes combat damage, not Might"
        );
    }

    #[test]
    fn with_four_mighty_units_the_fish_is_free_of_energy_and_still_needs_its_two_body() {
        let mut fixture = reef(5, 5);
        fixture
            .table
            .cards
            .push(fixtures::unit(94, fixtures::BASE, 0, "Third", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BF2, 0, "Fourth", 7));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(mighty_units(&ctx, 0), 4);
        let priced = cost::of_item(&ctx, &item(), None);
        assert_eq!(priced.energy, 0, "eight off seven saturates at nothing");
        assert_eq!(priced.power.len(), 2);
    }

    #[test]
    fn it_is_refused_without_the_body_runes_and_played_once_a_mighty_unit_pays_its_way() {
        let mut fixture = reef(5, 5);
        let ctx = fixture.ctx();
        assert_eq!(cost::total(&ctx, JAULL, false).energy, 3);
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NoPowerOf),
            "three ready Fury and Calm runes cannot pay two Body"
        );
        drop(ctx);
        let mut fixture = reef(5, 5);
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE + 1, 0, "Body", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 5);
        assert!(legal::classify(&ctx, 0, &entry(&ctx)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, JAULL).unwrap();
        assert!(ctx.on_board(JAULL));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "three energy from three runes, two of them recycled for Body"
        );
        assert_eq!(ctx.runes_of(0).len(), 4, "two Body runes recycled");
        assert!(ctx.card(JAULL).unwrap().exhausted, "not accelerated");
        assert!(ctx.fault.is_none());
    }
}

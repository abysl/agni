use super::prelude::{unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;

pub const PER_CARD: u8 = 1;

pub fn trash_discount(ctx: &Ctx, seat: u8) -> u8 {
    u8::try_from(ctx.trash_of(seat).len())
        .unwrap_or(u8::MAX)
        .saturating_mul(PER_CARD)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: trash_discount(ctx, seat),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Rhasa the Sunderer", &[], &[]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RHASA: u32 = 90;
    const CHAOS_RUNE: u32 = 46;
    const FIRST_TRASHED: u32 = 100;

    fn rhasa() -> CardInfo {
        CardInfo {
            energy: Some(10),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(RHASA, fixtures::HAND, 0, "Rhasa the Sunderer", 6)
        }
    }

    fn graveyard(mine: usize, theirs: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rhasa());
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for offset in 0..mine {
            let id = FIRST_TRASHED + offset as u32;
            fixture
                .table
                .cards
                .push(fixtures::spell(id, fixtures::TRASH, 0, "Spark", 2, 1));
        }
        for offset in 0..theirs {
            let id = FIRST_TRASHED + 50 + offset as u32;
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::TRASH, 1, "Jinx", 2));
        }
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: RHASA }, seat, Origin::Hand)
    }

    #[test]
    fn rhasa_is_a_unit_with_a_self_discount_and_nothing_else() {
        assert!(std::ptr::eq(
            script_of("Rhasa the Sunderer").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!(CARD.statics.len(), 1);
        assert_eq!(PER_CARD, 1);
    }

    #[test]
    fn each_card_in_the_controllers_trash_takes_one_energy_off_and_the_power_stays() {
        for (mine, theirs, energy) in [(0, 0, 10), (1, 0, 9), (3, 4, 7), (10, 0, 0), (12, 0, 0)] {
            let mut fixture = graveyard(mine, theirs);
            let ctx = fixture.ctx();
            assert_eq!(trash_discount(&ctx, 0), mine as u8);
            let total = cost::total(&ctx, RHASA, false);
            assert_eq!(total.energy, energy, "{mine} of mine, {theirs} of theirs");
            assert_eq!(
                total.power,
                [Need::Domain(Domain::Chaos)],
                "356.6 · the discount is energy only"
            );
            assert_eq!(cost::of_item(&ctx, &item(0), None).energy, energy);
            assert_eq!(
                cost::of_item(&ctx, &item(1), None).energy,
                10u8.saturating_sub(theirs as u8),
                "an opponent playing him would read their own trash"
            );
        }
    }

    #[test]
    fn with_seven_in_the_trash_he_is_played_for_three_energy_and_with_none_the_play_is_refused() {
        let mut fixture = graveyard(7, 0);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        fixtures::play_from_hand(&mut ctx, 0, RHASA).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert_eq!(ctx.location(RHASA), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three runes for the energy, the Chaos one recycled for the power too"
        );
        assert_eq!(
            ctx.card(CHAOS_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Chaos rune is recycled for the power"
        );
        assert_eq!(
            ctx.trash_of(0).len(),
            7,
            "playing him touches no trash card"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let mut empty = graveyard(0, 3);
        let ctx = empty.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: RHASA,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 10,
                ready: 4
            }),
            "the opponent's trash is not yours"
        );
    }
}

use super::prelude::{unit, with_statics};
use super::{base_name, Card, Cost, Static};
use crate::engine::ctx::Ctx;

pub const REDUCTION: u8 = 2;

pub fn namesakes_in_your_trash(ctx: &Ctx, seat: u8) -> u8 {
    let count = ctx
        .trash_of(seat)
        .into_iter()
        .filter(|card| {
            ctx.card(*card)
                .is_some_and(|held| base_name(&held.name) == CARD.name)
        })
        .count();
    u8::try_from(count).unwrap_or(u8::MAX)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: namesakes_in_your_trash(ctx, seat).saturating_mul(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Shadowblade Lurker", &[], &[]),
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

    const LURKER: u32 = 90;
    const FALLEN: u32 = 91;
    const SECOND_FALLEN: u32 = 92;
    const THIRD_FALLEN: u32 = 93;
    const THEIR_FALLEN: u32 = 94;
    const ENERGY: u8 = 5;
    const MIGHT: u8 = 5;

    fn lurker(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Shadowblade Lurker", MIGHT)
        }
    }

    fn shadows(fallen: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lurker(LURKER, fixtures::HAND, 0));
        for id in fallen {
            fixture.table.cards.push(lurker(*id, fixtures::TRASH, 0));
        }
        fixture
            .table
            .cards
            .push(lurker(THEIR_FALLEN, fixtures::TRASH, 1));
        fixture
            .table
            .cards
            .push(fixtures::spell(95, fixtures::TRASH, 0, "Spark", 2, 1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LURKER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: LURKER }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_self_discount() {
        assert!(std::ptr::eq(
            script_of("Shadowblade Lurker").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Shadowblade Lurker");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(REDUCTION, 2);
    }

    #[test]
    fn each_namesake_in_your_trash_takes_two_energy_off_and_the_opponents_trash_is_not_yours() {
        let mut fixture = shadows(&[]);
        let ctx = fixture.ctx();
        assert_eq!(namesakes_in_your_trash(&ctx, 0), 0, "Spark is no Lurker");
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY);
        assert_eq!(namesakes_in_your_trash(&ctx, 1), 1);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            ENERGY - 2,
            "priced for the other seat, their fallen Lurker counts"
        );
        drop(ctx);
        let mut fixture = shadows(&[FALLEN]);
        let mut ctx = fixture.ctx();
        assert_eq!(namesakes_in_your_trash(&ctx, 0), 1);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 2);
        assert_eq!(cost::total(&ctx, LURKER, false).energy, ENERGY - 2);
        assert!(cost::of_item(&ctx, &item(0), None).power.is_empty());
        ctx.trash(fixtures::VI);
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            ENERGY - 2,
            "Vi in the trash is not a namesake"
        );
        drop(ctx);
        let mut fixture = shadows(&[FALLEN, SECOND_FALLEN]);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 4);
        drop(ctx);
        let mut fixture = shadows(&[FALLEN, SECOND_FALLEN, THIRD_FALLEN]);
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            0,
            "three namesakes is free, never negative"
        );
        drop(ctx);
        let mut fixture = shadows(&[FALLEN]);
        fixture.table.card_mut(FALLEN).unwrap().name = "Shadowblade Lurker (Alternate Art)".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            namesakes_in_your_trash(&ctx, 0),
            1,
            "a print name resolves to its base"
        );
    }

    #[test]
    fn played_for_one_over_two_fallen_namesakes_and_refused_at_five_with_three_runes() {
        let mut fixture = shadows(&[]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, LURKER),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 3
            }),
            "an empty trash is the full price"
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut fixture = shadows(&[FALLEN, SECOND_FALLEN]);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LURKER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(LURKER), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy off one rune");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.trash_of(0).len(),
            3,
            "the fallen stay where they are: a discount is not a cost"
        );
        assert!(ctx.fault.is_none());
    }
}

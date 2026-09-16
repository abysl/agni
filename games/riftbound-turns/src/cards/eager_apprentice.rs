use super::prelude::{at_battlefield, unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind};

pub const REDUCTION: u8 = 1;
pub const MINIMUM: u8 = 1;

pub fn tutoring(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    matches!(item.kind, ItemKind::Spell { .. })
        && ctx.controller(me) == item.controller
        && at_battlefield(ctx, me)
}

fn apprentices_before(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me && ctx.face_in_play(held) && !held.is_hidden())
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| tutoring(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

fn discount(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if !tutoring(ctx, item, me) {
        return Cost::FREE;
    }
    let base = cost::base_of_item(ctx, item).energy;
    let room = base
        .saturating_sub(MINIMUM)
        .saturating_sub(apprentices_before(ctx, item, me));
    Cost {
        energy: room.min(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Eager Apprentice", &[], &[]),
    &[Static::PlayDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain};
    use crate::engine::cost::Need;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const APPRENTICE: u32 = 90;
    const SECOND: u32 = 91;
    const THEIRS: u32 = 92;
    const CHEAP: u32 = 93;

    fn apprentice(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, zone, seat, "Eager Apprentice", 3)
        }
    }

    fn classroom(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(apprentice(APPRENTICE, zone, 0));
        fixture.table.cards.push(fixtures::spell(
            CHEAP,
            fixtures::HAND,
            0,
            "Punch First",
            1,
            1,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn spark(seat: u8) -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            seat,
            Origin::Hand,
        )
    }

    #[test]
    fn the_apprentice_is_a_unit_carrying_one_spell_discount() {
        assert!(std::ptr::eq(script_of("Eager Apprentice").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::PlayDiscount(discount)));
        assert_eq!(CARD.statics.len(), 1);
        assert_eq!((REDUCTION, MINIMUM), (1, 1));
    }

    #[test]
    fn at_a_battlefield_her_controllers_spells_cost_one_energy_less_down_to_one() {
        let mut fixture = classroom(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(tutoring(&ctx, &spark(0), APPRENTICE));
        let discounted = cost::of_item(&ctx, &spark(0), None);
        assert_eq!(discounted.energy, 1);
        assert_eq!(
            discounted.power,
            [Need::Domain(Domain::Fury)],
            "the power need is untouched"
        );
        assert_eq!(
            cost::of_item(&ctx, &spark(1), None).energy,
            2,
            "spells you play, not the opponent's"
        );
        let punch = ChainItem::new(2, ItemKind::Spell { card: CHEAP }, 0, Origin::Hand);
        assert_eq!(
            cost::of_item(&ctx, &punch, None).energy,
            1,
            "356.4.e · a one-cost spell stays at the minimum of one"
        );
        let unit = ChainItem::new(
            3,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
            Origin::Hand,
        );
        assert!(!tutoring(&ctx, &unit, APPRENTICE));
        assert_eq!(
            cost::of_item(&ctx, &unit, None).energy,
            2,
            "units are not spells"
        );
        let mut home = classroom(fixtures::BASE);
        let ctx = home.ctx();
        assert!(!tutoring(&ctx, &spark(0), APPRENTICE));
        assert_eq!(
            cost::of_item(&ctx, &spark(0), None).energy,
            2,
            "at the base she teaches nothing"
        );
    }

    #[test]
    fn two_apprentices_take_one_each_but_never_below_one_and_an_enemy_apprentice_takes_nothing() {
        let mut fixture = classroom(fixtures::BF1);
        fixture
            .table
            .cards
            .push(apprentice(SECOND, fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(apprentice(THEIRS, fixtures::BF1, 1));
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().energy = Some(3);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            cost::of_item(&ctx, &spark(0), None).energy,
            1,
            "three less one less one"
        );
        assert_eq!(apprentices_before(&ctx, &spark(0), SECOND), 1);
        assert_eq!(apprentices_before(&ctx, &spark(0), APPRENTICE), 0);
        assert_eq!(discount(&ctx, &spark(0), APPRENTICE).energy, 1);
        assert_eq!(discount(&ctx, &spark(0), SECOND).energy, 1);
        assert_eq!(
            discount(&ctx, &spark(0), THEIRS).energy,
            0,
            "the enemy apprentice reads the spell as not hers"
        );
        let mut two_cost = classroom(fixtures::BF1);
        two_cost
            .table
            .cards
            .push(apprentice(SECOND, fixtures::BF1, 0));
        two_cost.resolve();
        let ctx = two_cost.ctx();
        assert_eq!(discount(&ctx, &spark(0), APPRENTICE).energy, 1);
        assert_eq!(
            discount(&ctx, &spark(0), SECOND).energy,
            0,
            "the second apprentice finds the minimum already reached"
        );
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 1);
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: fixtures::HAND_SPELL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_discount_is_paid_through_the_engine_and_without_it_the_same_runes_are_too_few() {
        let mut fixture = classroom(fixtures::BF1);
        fixture.table.card_mut(42).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "one energy and one Fury power: one rune exhausted and recycled, one left"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let mut home = classroom(fixtures::BASE);
        home.table.card_mut(42).unwrap().exhausted = true;
        let mut ctx = home.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "at the base the full two energy take both runes"
        );
        let mut short = classroom(fixtures::BASE);
        for rune in [42, 43] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = short.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            }),
            "one rune cannot pay the undiscounted two energy"
        );
    }

    #[test]
    fn one_ready_rune_is_enough_for_a_two_cost_spell_when_she_is_at_a_battlefield() {
        let mut fixture = classroom(fixtures::BF1);
        for rune in [42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 1);
        legal::classify(&ctx, 0, &entry(&ctx)).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
    }
}

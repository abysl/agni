use super::prelude::{in_combat, unit, with_statics, RAINBOW};
use super::{Card, Cost, Power, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind};

pub const ENERGY: u8 = 1;
pub const MINIMUM: u8 = 1;
pub const SURCHARGE: Cost = Cost {
    energy: ENERGY,
    power: &[Power::Rainbow],
};

fn a_spell_while_i_fight(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    matches!(item.kind, ItemKind::Spell { .. }) && in_combat(ctx, me)
}

pub fn gloom_applies_to_friendly(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    a_spell_while_i_fight(ctx, item, me) && item.controller == ctx.controller(me)
}

pub fn gloom_applies_to_enemy(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    a_spell_while_i_fight(ctx, item, me) && item.controller != ctx.controller(me)
}

fn earlier_vexes(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me && ctx.face_in_play(held) && !held.is_hidden())
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| gloom_applies_to_friendly(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

pub fn friendly_discount(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if !gloom_applies_to_friendly(ctx, item, me) {
        return Cost::FREE;
    }
    let base = cost::base_of_item(ctx, item).energy;
    let room = base
        .saturating_sub(MINIMUM)
        .saturating_sub(earlier_vexes(ctx, item, me));
    Cost {
        energy: room.min(ENERGY),
        power: RAINBOW.power,
    }
}

pub fn enemy_surcharge(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if gloom_applies_to_enemy(ctx, item, me) {
        SURCHARGE
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit("Vex - Cheerless", &[], &[]),
    &[
        Static::PlayDiscount(friendly_discount),
        Static::Surcharge(enemy_surcharge),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Need;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::state::Origin;
    use agni_plugin_sdk::table::CardInfo;

    const VEX: u32 = 90;
    const SECOND_VEX: u32 = 91;
    const THEIR_SPELL: u32 = 92;
    const CHEAP: u32 = 93;
    const MIND_RUNE: u32 = 46;

    fn vex(id: u32) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, fixtures::BF1, 0, "Vex - Cheerless", 5)
        }
    }

    fn gloom() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vex(VEX));
        let mut theirs = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Their Spark", 2, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture
            .table
            .cards
            .push(fixtures::spell(CHEAP, fixtures::HAND, 0, "Cheap", 1, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(VEX).unwrap(), &CARD));
        fixture
    }

    fn fighting(fixture: &mut Fixture, vex: u32) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(vex));
        ctx
    }

    fn mine(card: u32) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card }, 0, Origin::Hand)
    }

    fn theirs() -> ChainItem {
        ChainItem::new(2, ItemKind::Spell { card: THEIR_SPELL }, 1, Origin::Hand)
    }

    fn entry(ctx: &Ctx, card: u32, seat: u8) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_spell_discount_and_one_surcharge() {
        assert!(std::ptr::eq(script_of("Vex - Cheerless").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 2);
        assert!(CARD.has_static(Static::PlayDiscount(friendly_discount)));
        assert!(CARD.has_static(Static::Surcharge(enemy_surcharge)));
        assert_eq!(SURCHARGE.energy, 1);
        assert_eq!(SURCHARGE.power, [Power::Rainbow]);
    }

    #[test]
    fn out_of_combat_she_changes_no_cost_at_all() {
        let mut fixture = gloom();
        let ctx = fixture.ctx();
        assert!(!in_combat(&ctx, VEX));
        assert!(!gloom_applies_to_friendly(
            &ctx,
            &mine(fixtures::HAND_SPELL),
            VEX
        ));
        assert!(!gloom_applies_to_enemy(&ctx, &theirs(), VEX));
        assert_eq!(
            friendly_discount(&ctx, &mine(fixtures::HAND_SPELL), VEX),
            Cost::FREE
        );
        assert_eq!(enemy_surcharge(&ctx, &theirs(), VEX), Cost::FREE);
        let priced = cost::of_item(&ctx, &mine(fixtures::HAND_SPELL), None);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(crate::cards::Domain::Fury)]);
        assert_eq!(cost::of_item(&ctx, &theirs(), None).energy, 2);
    }

    #[test]
    fn in_combat_a_friendly_spell_is_one_energy_and_its_power_cheaper_down_to_one_energy() {
        let mut fixture = gloom();
        let ctx = fighting(&mut fixture, VEX);
        assert!(in_combat(&ctx, VEX));
        let spark = mine(fixtures::HAND_SPELL);
        assert!(gloom_applies_to_friendly(&ctx, &spark, VEX));
        let discount = friendly_discount(&ctx, &spark, VEX);
        assert_eq!(discount.energy, 1);
        assert_eq!(discount.power, [Power::Rainbow]);
        let priced = cost::of_item(&ctx, &spark, None);
        assert_eq!(priced.energy, 1);
        assert!(priced.power.is_empty(), "the rainbow strikes the Fury need");
        let cheap = mine(CHEAP);
        assert_eq!(
            friendly_discount(&ctx, &cheap, VEX).energy,
            0,
            "one energy is the floor"
        );
        let priced = cost::of_item(&ctx, &cheap, None);
        assert_eq!(priced.energy, 1);
        assert!(priced.power.is_empty(), "the power still goes");
        let unit = ChainItem::new(
            3,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
            Origin::Hand,
        );
        assert!(
            !gloom_applies_to_friendly(&ctx, &unit, VEX),
            "a unit is not a spell"
        );
        assert_eq!(cost::of_item(&ctx, &unit, None).energy, 2);
    }

    #[test]
    fn two_vexes_in_combat_share_the_floor_so_a_three_energy_spell_never_drops_below_one() {
        let mut fixture = gloom();
        fixture.table.cards.push(vex(SECOND_VEX));
        fixture
            .table
            .cards
            .push(fixtures::spell(94, fixtures::HAND, 0, "Three", 3, 0));
        fixture.resolve();
        let mut ctx = fighting(&mut fixture, VEX);
        assert!(ctx.mark_attacker(SECOND_VEX));
        let three = mine(94);
        assert_eq!(friendly_discount(&ctx, &three, VEX).energy, 1);
        assert_eq!(friendly_discount(&ctx, &three, SECOND_VEX).energy, 1);
        assert_eq!(cost::of_item(&ctx, &three, None).energy, 1);
        let spark = mine(fixtures::HAND_SPELL);
        assert_eq!(friendly_discount(&ctx, &spark, VEX).energy, 1);
        assert_eq!(
            friendly_discount(&ctx, &spark, SECOND_VEX).energy,
            0,
            "the first Vex took the only energy above the floor"
        );
        assert_eq!(cost::of_item(&ctx, &spark, None).energy, 1);
    }

    #[test]
    fn while_she_fights_the_spark_takes_one_rune_and_recycles_none_otherwise_two_and_one() {
        let mut fixture = gloom();
        fixture.table.card_mut(43).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        assert!(legal::classify(&ctx, 0, &entry(&ctx, fixtures::HAND_SPELL, 0)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "two energy from both runes");
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "a Fury rune recycled for the power"
        );
        drop(ctx);
        let mut fixture = gloom();
        fixture.table.card_mut(43).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fighting(&mut fixture, VEX);
        assert_eq!(
            cost::of_item(&ctx, &mine(fixtures::HAND_SPELL), None).energy,
            1
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one energy from one rune");
        assert_eq!(
            ctx.runes_of(0).len(),
            4,
            "no rune recycled for the struck power"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spark_affordable_only_through_her_gloom_is_offered_on_one_ready_rune() {
        let mut fixture = gloom();
        fixture.table.card_mut(41).unwrap().exhausted = true;
        fixture.table.card_mut(42).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fighting(&mut fixture, VEX);
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(
            cost::of_item(&ctx, &mine(fixtures::HAND_SPELL), None).energy,
            1
        );
        assert!(legal::classify(&ctx, 0, &entry(&ctx, fixtures::HAND_SPELL, 0)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
    }

    #[test]
    fn in_combat_an_enemy_spell_costs_one_energy_and_one_rainbow_more() {
        let mut fixture = gloom();
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 1, "Mind", false));
        fixture.resolve();
        let ctx = fighting(&mut fixture, VEX);
        assert!(gloom_applies_to_enemy(&ctx, &theirs(), VEX));
        assert_eq!(enemy_surcharge(&ctx, &theirs(), VEX), SURCHARGE);
        let priced = cost::of_item(&ctx, &theirs(), None);
        assert_eq!(priced.energy, 3);
        assert_eq!(
            priced.power,
            [Need::Domain(crate::cards::Domain::Mind), Need::Rainbow]
        );
    }
}

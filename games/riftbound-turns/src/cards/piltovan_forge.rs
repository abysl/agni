use super::prelude::{battlefield, with_statics, ONE_ENERGY};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind};

pub const DISCOUNT: Cost = ONE_ENERGY;

pub fn a_gear_ability_of(ctx: &Ctx, item: &ChainItem, seat: u8) -> bool {
    matches!(item.kind, ItemKind::Ability { source, .. } if ctx.is_gear(source) && ctx.controller(source) == seat)
        && item.controller == seat
}

pub fn gear_abilities_played_this_turn(ctx: &Ctx, seat: u8) -> usize {
    usize::from(ctx.blob.seat(seat).gear_abilities_activated)
}

pub fn holds_the_forge(ctx: &Ctx, forge: u32, seat: u8) -> bool {
    ctx.card(forge)
        .and_then(|held| held.zone)
        .filter(|zone| ctx.zones.is_battlefield(*zone))
        .is_some_and(|zone| ctx.blob.holder(zone) == Some(seat))
}

pub fn the_first_gear_ability_of_the_holder(ctx: &Ctx, item: &ChainItem, forge: u32) -> bool {
    let seat = item.controller;
    holds_the_forge(ctx, forge, seat)
        && a_gear_ability_of(ctx, item, seat)
        && gear_abilities_played_this_turn(ctx, seat) == 0
}

pub fn gear_ability_discount(ctx: &Ctx, item: &ChainItem, forge: u32) -> Cost {
    if the_first_gear_ability_of_the_holder(ctx, item, forge) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    battlefield("Piltovan Forge", &[], &[]),
    &[Static::AbilityDiscount(gear_ability_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, cost, settle};
    use crate::state::Origin;
    use agni_plugin_sdk::table::CardInfo;

    const FORGE: u32 = fixtures::GROUNDS;
    const GRABBER: u32 = 90;
    const SECOND_GRABBER: u32 = 91;
    const THEIR_GRABBER: u32 = 92;

    fn grabber(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::gear(id, fixtures::BASE, seat, "Garbage Grabber", 2)
        }
    }

    fn forge_held_by(seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(FORGE).unwrap().name = "Piltovan Forge".into();
        fixture.table.cards.push(grabber(GRABBER, 0));
        fixture.table.cards.push(grabber(SECOND_GRABBER, 0));
        fixture.table.cards.push(grabber(THEIR_GRABBER, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(seat));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FORGE).unwrap(), &CARD));
        fixture
    }

    fn ability_of(id: u16, gear: u32, seat: u8) -> ChainItem {
        ChainItem::new(
            id,
            ItemKind::Ability {
                source: gear,
                index: 0,
            },
            seat,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_the_pool_name_with_one_ability_discount_of_one_energy() {
        assert!(std::ptr::eq(script_of("Piltovan Forge").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::AbilityDiscount(gear_ability_discount)));
        assert!(CARD.replacement.is_none());
        assert_eq!(DISCOUNT, ONE_ENERGY);
    }

    #[test]
    fn the_holders_first_gear_ability_is_one_energy_cheaper_and_the_second_is_not() {
        let mut fixture = forge_held_by(0);
        let mut ctx = fixture.ctx();
        assert!(holds_the_forge(&ctx, FORGE, 0));
        assert!(!holds_the_forge(&ctx, FORGE, 1));
        let first = ability_of(1, GRABBER, 0);
        assert!(a_gear_ability_of(&ctx, &first, 0));
        assert_eq!(gear_abilities_played_this_turn(&ctx, 0), 0);
        assert!(the_first_gear_ability_of_the_holder(&ctx, &first, FORGE));
        assert_eq!(gear_ability_discount(&ctx, &first, FORGE), DISCOUNT);
        ctx.blob.seat_mut(0).gear_abilities_activated = 1;
        let second = ability_of(2, SECOND_GRABBER, 0);
        assert_eq!(gear_abilities_played_this_turn(&ctx, 0), 1);
        assert!(!the_first_gear_ability_of_the_holder(&ctx, &second, FORGE));
        assert_eq!(gear_ability_discount(&ctx, &second, FORGE), Cost::FREE);
        assert_eq!(
            gear_abilities_played_this_turn(&ctx, 1),
            0,
            "the count is per seat"
        );
        crate::engine::expiry::at_expiration(&mut ctx);
        assert_eq!(
            gear_abilities_played_this_turn(&ctx, 0),
            0,
            "the counter is per turn"
        );
        assert_eq!(gear_ability_discount(&ctx, &second, FORGE), DISCOUNT);
    }

    #[test]
    fn the_other_players_gear_ability_and_a_unit_ability_and_a_spell_take_nothing() {
        let mut fixture = forge_held_by(0);
        let ctx = fixture.ctx();
        let theirs = ability_of(1, THEIR_GRABBER, 1);
        assert!(a_gear_ability_of(&ctx, &theirs, 1));
        assert!(
            !the_first_gear_ability_of_the_holder(&ctx, &theirs, FORGE),
            "seat 1 does not control the forge"
        );
        assert_eq!(gear_ability_discount(&ctx, &theirs, FORGE), Cost::FREE);
        let unit = ability_of(2, fixtures::VI, 0);
        assert!(!a_gear_ability_of(&ctx, &unit, 0));
        assert_eq!(gear_ability_discount(&ctx, &unit, FORGE), Cost::FREE);
        let spell = ChainItem::new(
            3,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(gear_ability_discount(&ctx, &spell, FORGE), Cost::FREE);
        let stolen = ability_of(4, THEIR_GRABBER, 0);
        assert!(
            !a_gear_ability_of(&ctx, &stolen, 0),
            "an enemy gear's ability is not friendly"
        );
    }

    #[test]
    fn an_unheld_forge_or_one_held_by_the_opponent_discounts_nothing_for_seat_zero() {
        let mut fixture = forge_held_by(1);
        let ctx = fixture.ctx();
        let first = ability_of(1, GRABBER, 0);
        assert_eq!(gear_ability_discount(&ctx, &first, FORGE), Cost::FREE);
        let theirs = ability_of(2, THEIR_GRABBER, 1);
        assert_eq!(
            gear_ability_discount(&ctx, &theirs, FORGE),
            DISCOUNT,
            "the holder's first gear ability, whoever holds"
        );
        drop(ctx);
        let mut fixture = forge_held_by(0);
        fixture.blob.set_holder(fixtures::BF1, None);
        let ctx = fixture.ctx();
        assert!(!holds_the_forge(&ctx, FORGE, 0));
        assert_eq!(gear_ability_discount(&ctx, &first, FORGE), Cost::FREE);
    }

    #[test]
    fn unheld_the_engine_prices_the_gear_ability_at_its_printed_cost() {
        let mut fixture = forge_held_by(1);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, GRABBER, 0).energy, 1);
        assert_eq!(cost::of_activation(&ctx, THEIR_GRABBER, 0).energy, 0);
    }

    #[test]
    fn the_holders_first_gear_ability_of_the_turn_is_priced_one_energy_lower() {
        let mut fixture = forge_held_by(0);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, GRABBER, 0).energy, 0);
        assert_eq!(cost::of_activation(&ctx, THEIR_GRABBER, 0).energy, 1);
    }

    #[test]
    fn the_first_activation_pays_nothing_and_the_second_pays_its_energy() {
        let mut fixture = forge_held_by(0);
        for id in [93, 94, 95] {
            fixture
                .table
                .cards
                .push(fixtures::spell(id, fixtures::TRASH, 0, "Spark", 1, 0));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, GRABBER, 0).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "the first is free");
        assert_eq!(gear_abilities_played_this_turn(&ctx, 0), 1);
        assert_eq!(gear_abilities_played_this_turn(&ctx, 1), 0);
        assert_eq!(
            cost::of_activation(&ctx, SECOND_GRABBER, 0).energy,
            1,
            "the second gear ability this turn pays in full"
        );
        crate::engine::expiry::at_expiration(&mut ctx);
        assert_eq!(cost::of_activation(&ctx, SECOND_GRABBER, 0).energy, 0);
    }
}

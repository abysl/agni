use super::herald_of_scales::is_dragon;
use super::prelude::{battlefield, Location};
use super::{Card, Cost, Power};
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind};

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow, Power::Rainbow],
};

pub static CARD: Card = battlefield("Dragon Roost", &[], &[]);

pub fn a_dragon_being_played(ctx: &Ctx, item: &ChainItem) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    ctx.is_unit(card) && is_dragon(ctx, card)
}

pub fn roost_location(ctx: &Ctx, roost: u32) -> Option<Location> {
    match ctx.location(roost) {
        at @ Some(Location::Battlefield(_)) => at,
        _ => None,
    }
}

pub fn roost_additional_cost(ctx: &Ctx, item: &ChainItem, roost: u32) -> Option<Cost> {
    (roost_location(ctx, roost).is_some() && a_dragon_being_played(ctx, item)).then_some(ADDITIONAL)
}

pub fn roost_play_location(ctx: &Ctx, item: &ChainItem, roost: u32) -> Option<Location> {
    if !a_dragon_being_played(ctx, item) || !item.paid_additional() {
        return None;
    }
    roost_location(ctx, roost)
}

pub fn roosts(ctx: &Ctx) -> Vec<u32> {
    let mut found: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| ctx.is_battlefield_card(held.id))
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| roost_location(ctx, held.id).is_some())
        .map(|held| held.id)
        .collect();
    found.sort_unstable();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Origin, PromptWhy, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const ROOST: u32 = fixtures::GROUNDS;
    const DRAKE: u32 = 90;
    const THEIR_DRAKE: u32 = 91;

    fn drake(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(id, fixtures::HAND, seat, "Mountain Drake", 3)
        }
    }

    fn roost() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ROOST).unwrap().name = "Dragon Roost".into();
        fixture.table.cards.push(drake(DRAKE, 0));
        fixture.table.cards.push(drake(THEIR_DRAKE, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ROOST).unwrap(), &CARD));
        fixture
    }

    fn play_of(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card }, seat, Origin::Hand)
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_additional_cost_is_two_rainbows() {
        assert!(std::ptr::eq(script_of("Dragon Roost").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is the played card's own: the roost offers its cost as a seam"
        );
        assert_eq!(ADDITIONAL.energy, 0);
        assert_eq!(ADDITIONAL.power, [Power::Rainbow, Power::Rainbow]);
    }

    #[test]
    fn any_players_dragon_is_offered_the_cost_and_a_non_dragon_is_not() {
        let mut fixture = roost();
        let ctx = fixture.ctx();
        assert_eq!(roosts(&ctx), [ROOST]);
        assert_eq!(
            roost_location(&ctx, ROOST),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let mine = play_of(DRAKE, 0);
        assert!(a_dragon_being_played(&ctx, &mine));
        assert_eq!(roost_additional_cost(&ctx, &mine, ROOST), Some(ADDITIONAL));
        let theirs = play_of(THEIR_DRAKE, 1);
        assert_eq!(
            roost_additional_cost(&ctx, &theirs, ROOST),
            Some(ADDITIONAL),
            "any player, the holder included"
        );
        let disciple = play_of(fixtures::HAND_UNIT, 0);
        assert!(!a_dragon_being_played(&ctx, &disciple));
        assert_eq!(roost_additional_cost(&ctx, &disciple, ROOST), None);
        let spell = ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(roost_additional_cost(&ctx, &spell, ROOST), None);
    }

    #[test]
    fn paying_sends_the_dragon_to_the_roost_and_not_paying_leaves_the_location_free() {
        let mut fixture = roost();
        let ctx = fixture.ctx();
        let mut mine = play_of(DRAKE, 0);
        assert_eq!(
            roost_play_location(&ctx, &mine, ROOST),
            None,
            "the cost is not paid"
        );
        mine.set_slot(SLOT_ADDITIONAL, 1);
        assert!(mine.paid_additional());
        assert_eq!(
            roost_play_location(&ctx, &mine, ROOST),
            Some(Location::Battlefield(fixtures::BF1)),
            "paid: the dragon lands at the roost, whoever holds it"
        );
        let mut disciple = play_of(fixtures::HAND_UNIT, 0);
        disciple.set_slot(SLOT_ADDITIONAL, 1);
        assert_eq!(roost_play_location(&ctx, &disciple, ROOST), None);
    }

    #[test]
    fn a_roost_off_the_battlefields_or_another_battlefield_offers_nothing() {
        let mut fixture = roost();
        fixture.table.card_mut(ROOST).unwrap().zone = Some(fixtures::SIDEBOARD);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(roosts(&ctx).is_empty());
        assert_eq!(roost_location(&ctx, ROOST), None);
        assert_eq!(roost_additional_cost(&ctx, &play_of(DRAKE, 0), ROOST), None);
        drop(ctx);
        let mut fixture = roost();
        fixture.table.card_mut(ROOST).unwrap().name = "Proving Grounds".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(roosts(&ctx).is_empty());
    }

    #[test]
    fn today_a_dragon_plays_without_the_ask_and_lands_in_its_base() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAKE).unwrap();
        assert!(
            !matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "the roost's cost is never offered"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(DRAKE), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · an additional cost offered by another card and the play location it dictates: play::advance reads Card.additional off the played card's own script and Ctx::play_locations knows the base, held battlefields and ambush only, the engine owes a consult of dragon_roost::roost_additional_cost at STAGE_ADDITIONAL for every roost in play and of dragon_roost::roost_play_location in place of the location prompt when the cost is paid (the Sneaky Deckhand play-locations row), so a Dragon of any player may pay two rainbows and land at the roost"]
    fn a_dragon_may_pay_two_rainbows_to_land_at_the_roost() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAKE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_ADDITIONAL as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(DRAKE),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}

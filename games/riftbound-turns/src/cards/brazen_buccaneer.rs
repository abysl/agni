use super::prelude::{unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;

pub const DISCARDS: u8 = 1;
pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn a_discard_can_be_offered_as_the_additional_cost(ctx: &Ctx, seat: u8) -> bool {
    ctx.hand_of(seat).len() >= usize::from(DISCARDS)
}

pub fn discarded_as_the_additional_cost_until_play_offers_a_discard_at_the_additional_stage(
    ctx: &Ctx,
    card: u32,
    seat: u8,
) -> bool {
    let _ = (ctx.owner(card), seat);
    false
}

fn plunder_discount(ctx: &Ctx, card: u32, seat: u8) -> Cost {
    if discarded_as_the_additional_cost_until_play_offers_a_discard_at_the_additional_stage(
        ctx, card, seat,
    ) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit("Brazen Buccaneer", &[], &[]),
    &[Static::SelfDiscount(plunder_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const BUCCANEER: u32 = 90;
    const PRINTED: u8 = 6;

    fn buccaneer() -> CardInfo {
        let mut card = fixtures::unit(BUCCANEER, fixtures::HAND, 0, "Brazen Buccaneer", 5);
        card.energy = Some(PRINTED);
        card
    }

    fn docked() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(buccaneer());
        fixture.resolve();
        fixture
    }

    fn item() -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: BUCCANEER }, 0, Origin::Hand)
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_self_discount_and_no_resource_additional_cost() {
        assert!(std::ptr::eq(script_of("Brazen Buccaneer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is energy and power · a discard is not one"
        );
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(plunder_discount)));
        assert_eq!(DISCOUNT.energy, 2);
        assert!(DISCOUNT.power.is_empty());
        assert_eq!(DISCARDS, 1);
        let fixture = docked();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BUCCANEER).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_offer_needs_a_card_in_hand_to_discard() {
        let mut fixture = docked();
        let ctx = fixture.ctx();
        assert!(a_discard_can_be_offered_as_the_additional_cost(&ctx, 0));
        assert!(
            a_discard_can_be_offered_as_the_additional_cost(&ctx, 1),
            "one hidden card is a card"
        );
        drop(ctx);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!a_discard_can_be_offered_as_the_additional_cost(&ctx, 1));
    }

    #[test]
    fn today_the_discount_never_lands_and_it_costs_its_printed_six() {
        let mut fixture = docked();
        let ctx = fixture.ctx();
        assert!(
            !discarded_as_the_additional_cost_until_play_offers_a_discard_at_the_additional_stage(
                &ctx, BUCCANEER, 0
            )
        );
        assert_eq!(plunder_discount(&ctx, BUCCANEER, 0), Cost::FREE);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, PRINTED);
        assert_eq!(cost::total(&ctx, BUCCANEER, false).energy, PRINTED);
        assert!(cost::of_item(&ctx, &item(), None).power.is_empty());
    }

    #[test]
    #[ignore = "engine gap · play::advance's additional stage offers energy and power only; with a discard-N additional-cost kind the play asks for the discard, records it on the item, and the SelfDiscount reads it as 2 energy off"]
    fn playing_it_offers_a_discard_that_takes_two_off_its_cost() {
        let mut fixture = docked();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BUCCANEER).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            cost::of_item(&ctx, &item(), None).energy,
            PRINTED - DISCOUNT.energy,
            "four for a six-drop"
        );
    }
}

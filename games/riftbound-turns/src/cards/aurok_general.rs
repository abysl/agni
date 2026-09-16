use super::prelude::{empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Scope, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[Power::Domain(Domain::Order)],
};
pub const MIGHT: i16 = 2;
pub const EMPOWER_ABILITY: u8 = 0;

pub fn both_empowered(ctx: &Ctx, me: u32, unit: u32) -> bool {
    is_empowered(ctx, me) && ctx.is_empowered(unit)
}

pub static CARD: Card = with_statics(
    unit(
        "Aurok General",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: both_empowered,
        grants: &[Grant::Might(MIGHT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const GENERAL: u32 = 90;
    const LEGIONNAIRE: u32 = 91;
    const THEIRS: u32 = 92;
    const ORDER_RUNE: u32 = 46;

    fn general() -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Order".into()],
            ..fixtures::unit(GENERAL, fixtures::BASE, 0, "Aurok General", 5)
        }
    }

    fn muster() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(general());
        fixture.table.cards.push(fixtures::unit(
            LEGIONNAIRE,
            fixtures::BF1,
            0,
            "Legionnaire",
            3,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIRS, fixtures::BF2, 1, "Raider", 3));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GENERAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == GENERAL)
            .collect()
    }

    fn empower_him(ctx: &mut Ctx) {
        activate::activate(ctx, 0, GENERAL, EMPOWER_ABILITY).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(GENERAL));
    }

    #[test]
    fn the_unit_prints_empower_and_carries_one_aura_over_empowered_friendly_units() {
        assert!(std::ptr::eq(script_of("Aurok General").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert!(CARD.has_aura());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(MIGHT)],
                ..
            }]
        ));
        assert_eq!(MIGHT, 2);
    }

    #[test]
    fn empower_pays_three_and_an_order_then_he_reads_seven_and_is_refused_a_second_time() {
        let mut fixture = muster();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(GENERAL), 5);
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {GENERAL}}}: empower (3 energy and 1 Order power)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, GENERAL, EMPOWER_ABILITY).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three runes exhausted for the energy, the Order rune among them"
        );
        assert_ne!(
            ctx.card(ORDER_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "the Order rune is recycled for the power"
        );
        assert_eq!(ctx.current_might(GENERAL), 5, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(is_empowered(&ctx, GENERAL));
        assert_eq!(
            ctx.current_might(GENERAL),
            5 + i32::from(MIGHT),
            "including me"
        );
        assert!(his_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, GENERAL, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_order_rune_the_offer_is_greyed_and_the_activation_refused() {
        let mut fixture = muster();
        fixture.table.cards.retain(|card| card.id != ORDER_RUNE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, GENERAL, EMPOWER_ABILITY),
            Err(Refusal::NoPowerOf)
        );
        assert!(!ctx.is_empowered(GENERAL));
        assert_eq!(ctx.current_might(GENERAL), 5);
    }

    #[test]
    fn empowered_friendly_units_get_two_while_he_is_empowered_and_the_rest_read_as_printed() {
        let mut fixture = muster();
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(LEGIONNAIRE));
        assert!(ctx.empower(THEIRS));
        assert!(
            !both_empowered(&ctx, GENERAL, LEGIONNAIRE),
            "he is not empowered yet"
        );
        assert_eq!(ctx.current_might(LEGIONNAIRE), 3);
        assert_eq!(ctx.current_might(GENERAL), 5);
        empower_him(&mut ctx);
        assert!(both_empowered(&ctx, GENERAL, LEGIONNAIRE));
        assert!(both_empowered(&ctx, GENERAL, GENERAL));
        assert!(!both_empowered(&ctx, GENERAL, fixtures::VI));
        assert_eq!(ctx.current_might(GENERAL), 7);
        assert_eq!(
            ctx.current_might(LEGIONNAIRE),
            5,
            "an empowered friendly unit anywhere"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "a friendly unit that is not empowered"
        );
        assert_eq!(
            ctx.current_might(THEIRS),
            3,
            "an empowered enemy unit is not yours"
        );
        assert!(ctx.disempower(LEGIONNAIRE));
        assert_eq!(ctx.current_might(LEGIONNAIRE), 3);
        assert!(ctx.empower(LEGIONNAIRE));
        assert_eq!(ctx.current_might(LEGIONNAIRE), 5);
        assert!(ctx.disempower(GENERAL));
        assert_eq!(
            ctx.current_might(GENERAL),
            5,
            "the aura follows his own state"
        );
        assert_eq!(ctx.current_might(LEGIONNAIRE), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_general_in_hand_or_taken_by_the_opponent_grants_by_his_controller() {
        let mut fixture = muster();
        fixture.table.card_mut(GENERAL).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(LEGIONNAIRE));
        assert!(
            !ctx.empower(GENERAL),
            "a card in hand is not on the board and cannot be empowered"
        );
        assert_eq!(ctx.current_might(LEGIONNAIRE), 3, "no aura from the hand");
        drop(ctx);
        let mut turned = muster();
        let mut ctx = turned.ctx();
        empower_him(&mut ctx);
        assert!(ctx.empower(LEGIONNAIRE));
        assert!(ctx.empower(THEIRS));
        assert_eq!(ctx.current_might(LEGIONNAIRE), 5);
        assert_eq!(ctx.current_might(THEIRS), 3);
        assert!(ctx.set_controller(GENERAL, 1, THEIRS));
        assert_eq!(ctx.current_might(LEGIONNAIRE), 3, "he is theirs now");
        assert_eq!(
            ctx.current_might(THEIRS),
            5,
            "and their empowered units are his friends"
        );
        assert_eq!(ctx.current_might(GENERAL), 7);
        assert!(ctx.fault.is_none());
    }
}

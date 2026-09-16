use super::prelude::{unit, with_statics, Location};
use super::{Card, Cost, Domain, Power, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Order)],
};
pub const UNITS_THERE: usize = 2;

pub fn you_control_a_battlefield_with_exactly_two_units(ctx: &Ctx, seat: u8) -> bool {
    ctx.held_battlefields(seat)
        .into_iter()
        .any(|zone| ctx.units_at(Location::Battlefield(zone)).len() == UNITS_THERE)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if you_control_a_battlefield_with_exactly_two_units(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit("Keeper of Law", &[], &[]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::{Cause, Killed, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const KEEPER: u32 = 90;
    const FIRST: u32 = 91;
    const SECOND: u32 = 92;
    const THIRD: u32 = 93;
    const ENERGY: u8 = 5;
    const MIGHT: u8 = 5;

    fn keeper(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(KEEPER, zone, seat, "Keeper of Law", MIGHT)
        }
    }

    fn court(here: &[(u32, u8)], holder: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(keeper(fixtures::HAND, 0));
        for (id, seat) in here {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BF1, *seat, "Litigant", 2));
        }
        fixture.blob.set_holder(fixtures::BF1, holder);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KEEPER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: KEEPER }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_self_discount_of_two_energy_and_an_order() {
        assert!(std::ptr::eq(script_of("Keeper of Law").unwrap(), &CARD));
        assert_eq!(CARD.name, "Keeper of Law");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(DISCOUNT.energy, 2);
        assert_eq!(DISCOUNT.power, [Power::Domain(Domain::Order)]);
        assert_eq!(UNITS_THERE, 2);
    }

    #[test]
    fn the_predicate_wants_a_battlefield_you_hold_with_exactly_two_units_of_anyone() {
        let mut fixture = court(&[(FIRST, 0), (SECOND, 1)], Some(0));
        let mut ctx = fixture.ctx();
        assert!(
            you_control_a_battlefield_with_exactly_two_units(&ctx, 0),
            "yours and theirs both count as units there"
        );
        assert!(
            !you_control_a_battlefield_with_exactly_two_units(&ctx, 1),
            "the holder controls the battlefield, not the visitor"
        );
        assert_eq!(ctx.kill(SECOND, Cause::Rule), Killed::Yes);
        assert!(
            !you_control_a_battlefield_with_exactly_two_units(&ctx, 0),
            "one unit there"
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(you_control_a_battlefield_with_exactly_two_units(&ctx, 0));
        drop(ctx);
        let mut fixture = court(&[(FIRST, 0), (SECOND, 0), (THIRD, 0)], Some(0));
        let ctx = fixture.ctx();
        assert!(
            !you_control_a_battlefield_with_exactly_two_units(&ctx, 0),
            "three is not exactly two"
        );
        drop(ctx);
        let mut fixture = court(&[(FIRST, 0), (SECOND, 0)], None);
        let ctx = fixture.ctx();
        assert!(
            !you_control_a_battlefield_with_exactly_two_units(&ctx, 0),
            "an unheld battlefield is nobody's"
        );
        assert!(
            !you_control_a_battlefield_with_exactly_two_units(&ctx, 1),
            "Rockfall Path is held by seat 1 with one Sprite there"
        );
    }

    #[test]
    fn with_the_court_in_session_the_price_drops_two_energy_and_the_order_power() {
        let mut fixture = court(&[(FIRST, 0), (SECOND, 0)], Some(0));
        let mut ctx = fixture.ctx();
        let priced = cost::of_item(&ctx, &item(0), None);
        assert_eq!(priced.energy, ENERGY - 2);
        assert!(priced.power.is_empty(), "the Order power is struck");
        assert_eq!(cost::total(&ctx, KEEPER, false).energy, ENERGY - 2);
        let theirs = cost::of_item(&ctx, &item(1), None);
        assert_eq!(
            (theirs.energy, theirs.power.as_slice()),
            (ENERGY, &[Need::Domain(Domain::Order)][..]),
            "priced for the other seat, who holds no such battlefield"
        );
        assert_eq!(ctx.kill(FIRST, Cause::Rule), Killed::Yes);
        let priced = cost::of_item(&ctx, &item(0), None);
        assert_eq!(priced.energy, ENERGY);
        assert_eq!(priced.power, [Need::Domain(Domain::Order)]);
    }

    #[test]
    fn played_for_three_off_three_runes_with_the_discount_and_refused_at_five_without() {
        let mut fixture = court(&[(FIRST, 0), (SECOND, 0), (THIRD, 0)], Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, KEEPER),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 3
            }),
            "three units there: no discount, and five energy off three runes is refused"
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut fixture = court(&[(FIRST, 0), (SECOND, 1)], Some(0));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KEEPER).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PlayLocation { item: 1 }),
            "the held battlefield is a second place to enter"
        );
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(KEEPER), Some(Location::Base(0)));
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "three energy off three runes and no Order rune needed"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

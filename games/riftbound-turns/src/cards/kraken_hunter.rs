use super::prelude::unit;
use super::wallop::buffs_that_could_pay;
use super::{Card, Cost, Domain, Keyword, Power};
use crate::engine::ctx::Ctx;

pub const DISCOUNT_PER_BUFF: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub fn buffs_that_could_discount(ctx: &Ctx, seat: u8) -> Vec<u32> {
    buffs_that_could_pay(ctx, seat)
}

pub static CARD: Card = unit(
    "Kraken Hunter",
    &[Keyword::Accelerate, Keyword::Assault(1)],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::pay;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ACCELERATE};
    use agni_plugin_sdk::table::CardInfo;

    const HUNTER: u32 = 90;

    fn hunter(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(HUNTER, zone, 0, "Kraken Hunter", 5)
        }
    }

    fn harbour(body_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hunter(fixtures::HAND));
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for rune in 0..body_runes {
            fixture
                .table
                .cards
                .push(fixtures::rune(46 + rune as u32, 0, "Body", false));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_accelerate_and_assault_one_and_names_the_body_discount_per_buff() {
        assert!(std::ptr::eq(script_of("Kraken Hunter").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate, Keyword::Assault(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(
            DISCOUNT_PER_BUFF,
            Cost {
                energy: 0,
                power: &[Power::Domain(Domain::Body)]
            }
        );
    }

    #[test]
    fn played_for_its_printed_cost_it_enters_and_reads_assault_only_while_attacking() {
        let mut fixture = harbour(3);
        let mut ctx = fixture.ctx();
        let item = ChainItem::new(1, ItemKind::Permanent { card: HUNTER }, 0, Origin::Hand);
        let printed = cost::of_item(&ctx, &item, None);
        assert_eq!(printed.energy, 3);
        assert_eq!(
            printed.power,
            [Need::Domain(Domain::Body), Need::Domain(Domain::Body)]
        );
        fixtures::play_from_hand(&mut ctx, 0, HUNTER).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ACCELERATE),
            "Accelerate is offered: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(HUNTER), Some(Location::Base(0)));
        assert!(ctx.card(HUNTER).unwrap().exhausted, "not accelerated");
        assert_eq!(ctx.current_might(HUNTER), 5);
        assert!(ctx.has_keyword(HUNTER, Keyword::Assault(1)));
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_two_body_runes_the_printed_cost_cannot_be_paid_even_with_buffs_on_the_board() {
        let mut fixture = harbour(1);
        let mut ctx = fixture.ctx();
        ctx.buff(fixtures::VI);
        assert_eq!(buffs_that_could_discount(&ctx, 0), [fixtures::VI]);
        let item = ChainItem::new(1, ItemKind::Permanent { card: HUNTER }, 0, Origin::Hand);
        assert!(
            !pay::affordable(&ctx, 0, &cost::of_item(&ctx, &item, None)),
            "one Body rune cannot pay two Body power"
        );
        assert!(fixtures::play_from_hand(&mut ctx, 0, HUNTER).is_err());
        assert_eq!(ctx.card(HUNTER).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.is_buffed(fixtures::VI), "nothing was spent");
    }

    #[test]
    #[ignore = "engine gap · spend a buff: an any-number additional cost at the pay stage with a Body power discount per buff spent; one buffed unit must let the Hunter be played with a single Body rune"]
    fn spending_one_buff_as_he_is_played_strikes_one_body_power_off_his_cost() {
        let mut fixture = harbour(1);
        let mut ctx = fixture.ctx();
        ctx.buff(fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, HUNTER).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.location(HUNTER), Some(Location::Base(0)));
    }
}

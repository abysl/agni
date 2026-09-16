use super::prelude::{adding, gear};
use super::{Adds, Card, Cost, Domain, Paying, Power};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub static CARD: Card = adding(gear("Seal of Focus", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, seal: u32) -> Option<Cost> {
    let ready = ctx.card(seal).is_some_and(|held| !held.exhausted);
    (ready && ctx.on_board(seal) && !ctx.is_facedown(seal) && ctx.controller(seal) == seat)
        .then_some(ADDS)
}

fn adds(ctx: &Ctx, seat: u8, seal: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, seal).map(Adds::exhausting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{Cost as Total, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const SEAL: u32 = 90;
    const DOMAIN: Domain = Domain::Calm;
    const OTHER: &str = "Order";

    fn seal(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec![DOMAIN.label().into()],
            exhausted,
            ..fixtures::gear(SEAL, zone, seat, CARD.name, 0)
        }
    }

    fn with_seal(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(seal(zone, seat, exhausted));
        fixture
            .table
            .cards
            .push(fixtures::rune(48, 0, DOMAIN.label(), false));
        fixture.resolve();
        fixture
    }

    fn without_runes_of_its_domain(mut fixture: Fixture) -> Fixture {
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, OTHER, false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, OTHER, false));
        fixture.resolve();
        fixture
    }

    fn power_of(domain: Domain) -> Total {
        Total {
            energy: 0,
            power: vec![Need::Domain(domain)],
            ..Total::default()
        }
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if *zone == fixtures::RUNE_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_seal_is_a_gear_whose_add_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 0);
        assert_eq!(ADDS.power, [Power::Domain(DOMAIN)]);
        let mut fixture = with_seal(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(SEAL).unwrap(), &CARD));
        assert!(ctx.is_gear(SEAL));
        assert!(!ctx.is_token(SEAL));
        assert_eq!(
            activate::legal(&ctx, 0, SEAL, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SEAL));
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), Some(ADDS));
    }

    #[test]
    fn an_exhausted_seal_an_opponents_seal_or_one_still_in_hand_adds_nothing() {
        let mut spent = with_seal(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), None);
        drop(ctx);
        let mut theirs = with_seal(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), None);
        assert_eq!(adds_while_paying(&ctx, 1, SEAL), Some(ADDS));
        drop(ctx);
        let mut held = with_seal(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), None);
    }

    #[test]
    fn the_seal_is_played_for_one_power_of_its_domain_and_lands_ready_in_the_base() {
        let mut fixture = with_seal(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SEAL).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(SEAL).unwrap().zone, Some(fixtures::BASE));
        assert!(!ctx.card(SEAL).unwrap().exhausted);
        let paid = recycled(&ctx);
        assert_eq!(paid.len(), 1, "0E 1P · one rune pays the power");
        assert_eq!(
            ctx.card(paid[0]).map(|rune| rune.name.as_str()),
            Some(format!("{} Rune", DOMAIN.label()).as_str()),
            "and it is a rune of the Seal's domain"
        );
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), Some(ADDS));
    }

    #[test]
    fn a_ready_seal_stands_in_for_a_power_of_its_domain_and_pays_by_exhausting() {
        let mut fixture = without_runes_of_its_domain(with_seal(fixtures::BASE, 0, false));
        let mut ctx = fixture.ctx();
        let cost = power_of(DOMAIN);
        let planned = pay::plan(&ctx, 0, &cost).expect("the Seal adds the missing power");
        pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.card(SEAL).unwrap().exhausted, "the exhaust is the cost");
        assert!(ctx.on_board(SEAL), "unlike a Gold, the Seal stays");
        assert!(recycled(&ctx).is_empty(), "no rune was recycled for it");
        assert_eq!(adds_while_paying(&ctx, 0, SEAL), None);
        assert!(
            pay::plan(&ctx, 0, &cost).is_err(),
            "an exhausted Seal adds nothing more this turn"
        );
    }
}

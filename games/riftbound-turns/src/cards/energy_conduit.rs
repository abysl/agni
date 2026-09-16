use super::prelude::{adding, gear};
use super::{Adds, Card, Cost, Paying};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = Cost {
    energy: 1,
    power: &[],
};

pub static CARD: Card = adding(gear("Energy Conduit", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, conduit: u32) -> Option<Cost> {
    let ready = ctx.card(conduit).is_some_and(|held| !held.exhausted);
    (ready && ctx.on_board(conduit) && !ctx.is_facedown(conduit) && ctx.controller(conduit) == seat)
        .then_some(ADDS)
}

fn adds(ctx: &Ctx, seat: u8, conduit: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, conduit).map(Adds::exhausting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain};
    use crate::engine::cost::Cost as Total;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const CONDUIT: u32 = 90;
    const ENERGY: u8 = 3;

    fn conduit(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec![Domain::Mind.label().into()],
            exhausted,
            ..fixtures::gear(CONDUIT, zone, seat, CARD.name, ENERGY)
        }
    }

    fn with_conduit(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(conduit(zone, seat, exhausted));
        fixture.resolve();
        fixture
    }

    fn with_one_ready_rune(mut fixture: Fixture) -> Fixture {
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture.resolve();
        fixture
    }

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    fn exhausted(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Annotate {
                    card,
                    key,
                    value: Some(_),
                } if key == "exhausted" => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_conduit_is_a_gear_whose_add_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 1);
        assert!(ADDS.power.is_empty());
        let mut fixture = with_conduit(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(CONDUIT).unwrap(), &CARD));
        assert!(ctx.is_gear(CONDUIT));
        assert!(!ctx.is_token(CONDUIT));
        assert_eq!(
            activate::legal(&ctx, 0, CONDUIT, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == CONDUIT));
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), Some(ADDS));
    }

    #[test]
    fn an_exhausted_conduit_an_opponents_conduit_or_one_still_in_hand_adds_nothing() {
        let mut spent = with_conduit(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), None);
        drop(ctx);
        let mut theirs = with_conduit(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), None);
        assert_eq!(adds_while_paying(&ctx, 1, CONDUIT), Some(ADDS));
        drop(ctx);
        let mut held = with_conduit(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), None);
    }

    #[test]
    fn the_conduit_is_played_for_three_energy_and_lands_ready_in_the_base() {
        let mut fixture = with_conduit(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONDUIT).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(CONDUIT).unwrap().zone, Some(fixtures::BASE));
        assert!(!ctx.card(CONDUIT).unwrap().exhausted);
        assert_eq!(exhausted(&ctx), [41, 42, 43], "3E · three runes exhaust");
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), Some(ADDS));
    }

    #[test]
    fn a_ready_conduit_stands_in_for_one_energy_and_pays_by_exhausting() {
        let mut fixture = with_one_ready_rune(with_conduit(fixtures::BASE, 0, false));
        let mut ctx = fixture.ctx();
        let cost = energy(2);
        let planned = pay::plan(&ctx, 0, &cost).expect("the Conduit adds the missing energy");
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(
            exhausted(&ctx),
            [CONDUIT, 46],
            "the Conduit exhausts beside the one rune"
        );
        assert!(ctx.on_board(CONDUIT), "unlike a Gold, the Conduit stays");
        assert_eq!(adds_while_paying(&ctx, 0, CONDUIT), None);
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "an exhausted Conduit adds nothing more this turn"
        );
    }
}

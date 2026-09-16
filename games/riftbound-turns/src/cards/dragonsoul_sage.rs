use super::prelude::{adding, unit};
use super::{Adds, Card, Cost, Paying};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = Cost {
    energy: 1,
    power: &[],
};

pub static CARD: Card = adding(unit("Dragonsoul Sage", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, sage: u32) -> Option<Cost> {
    let ready = ctx.card(sage).is_some_and(|held| !held.exhausted);
    (ready
        && ctx.is_unit(sage)
        && ctx.on_board(sage)
        && !ctx.is_facedown(sage)
        && ctx.controller(sage) == seat)
        .then_some(ADDS)
}

fn adds(ctx: &Ctx, seat: u8, sage: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, sage).map(Adds::exhausting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Cost as Total;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const SAGE: u32 = 90;

    fn sage(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::unit(SAGE, zone, seat, "Dragonsoul Sage", 1)
        }
    }

    fn with_sage(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sage(zone, seat, exhausted));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SAGE).unwrap(), &CARD));
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
            .push(fixtures::rune(46, 0, "Body", false));
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
    fn the_sage_is_a_unit_whose_add_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of("Dragonsoul Sage").unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 1);
        assert!(ADDS.power.is_empty());
        let mut fixture = with_sage(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(ctx.is_unit(SAGE));
        assert_eq!(
            activate::legal(&ctx, 0, SAGE, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SAGE));
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), Some(ADDS));
    }

    #[test]
    fn an_exhausted_sage_an_opponents_sage_or_one_still_in_hand_adds_nothing() {
        let mut spent = with_sage(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), None);
        drop(ctx);
        let mut theirs = with_sage(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), None);
        assert_eq!(adds_while_paying(&ctx, 1, SAGE), Some(ADDS));
        drop(ctx);
        let mut held = with_sage(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), None);
        drop(ctx);
        let mut afield = with_sage(fixtures::BF1, 0, false);
        let ctx = afield.ctx();
        assert_eq!(
            adds_while_paying(&ctx, 0, SAGE),
            Some(ADDS),
            "a ready Sage adds from a battlefield as well as from the base"
        );
    }

    #[test]
    fn played_for_two_the_sage_enters_exhausted_and_adds_nothing_until_it_readies() {
        let mut fixture = with_sage(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SAGE).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(SAGE), Some(Location::Base(0)));
        assert!(
            ctx.card(SAGE).unwrap().exhausted,
            "369.3 · units enter exhausted"
        );
        assert_eq!(exhausted(&ctx).len(), 2 + 1, "2E · two runes and the Sage");
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), None);
        assert!(ctx.ready(SAGE));
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), Some(ADDS));
    }

    #[test]
    fn a_ready_sage_stands_in_for_one_energy_and_pays_by_exhausting() {
        let mut fixture = with_one_ready_rune(with_sage(fixtures::BASE, 0, false));
        let mut ctx = fixture.ctx();
        let cost = energy(2);
        let planned = pay::plan(&ctx, 0, &cost).expect("the Sage adds the missing energy");
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(
            exhausted(&ctx),
            [SAGE, 46],
            "the Sage exhausts beside the one rune"
        );
        assert!(ctx.on_board(SAGE), "unlike a Gold, the Sage stays");
        assert_eq!(adds_while_paying(&ctx, 0, SAGE), None);
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "an exhausted Sage adds nothing more this turn"
        );
    }
}

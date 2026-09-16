use super::prelude::{adding, gear, with_statics, xp_of, RAINBOW};
use super::{Adds, Card, Cost, Paying, Power, Static};
use crate::engine::ctx::Ctx;

pub const LEVEL: i32 = 6;
pub const ADDS: Cost = RAINBOW;
pub const ADDS_AT_LEVEL: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};

pub static CARD: Card = adding(
    with_statics(gear("Honeyfruit", &[], &[]), &[Static::EntersExhausted]),
    adds,
);

pub fn leveled(ctx: &Ctx, seat: u8) -> bool {
    xp_of(ctx, seat) >= LEVEL
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, fruit: u32) -> Option<Cost> {
    let ready = ctx.card(fruit).is_some_and(|held| !held.exhausted);
    let mine = ctx.on_board(fruit) && !ctx.is_facedown(fruit) && ctx.controller(fruit) == seat;
    if !(ready && mine) {
        return None;
    }
    Some(if leveled(ctx, seat) {
        ADDS_AT_LEVEL
    } else {
        ADDS
    })
}

fn adds(ctx: &Ctx, seat: u8, fruit: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, fruit).map(Adds::exhausting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{Cost as Total, Need};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FRUIT: u32 = 90;

    fn fruit(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            exhausted,
            ..fixtures::gear(FRUIT, zone, seat, CARD.name, 2)
        }
    }

    fn orchard(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fruit(zone, seat, exhausted));
        fixture.resolve();
        fixture
    }

    fn rainbow() -> Total {
        Total {
            energy: 0,
            power: vec![Need::Rainbow],
            ..Total::default()
        }
    }

    #[test]
    fn the_fruit_is_a_gear_that_enters_exhausted_whose_adds_are_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of("Honeyfruit").unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersExhausted));
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 0);
        assert_eq!(ADDS.power, [Power::Rainbow]);
        assert_eq!(ADDS_AT_LEVEL.energy, 1);
        assert_eq!(ADDS_AT_LEVEL.power, [Power::Rainbow]);
        assert_eq!(LEVEL, 6);
        let mut fixture = orchard(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(ctx.is_gear(FRUIT));
        assert_eq!(
            activate::legal(&ctx, 0, FRUIT, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == FRUIT));
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), Some(ADDS));
    }

    #[test]
    fn at_six_xp_the_fruit_adds_one_energy_with_the_rainbow_and_below_it_the_rainbow_alone() {
        let mut fixture = orchard(fixtures::BASE, 0, false);
        fixture.set_xp(0, LEVEL - 1);
        let ctx = fixture.ctx();
        assert!(!leveled(&ctx, 0));
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), Some(ADDS));
        drop(ctx);
        let mut fixture = orchard(fixtures::BASE, 0, false);
        fixture.set_xp(0, LEVEL);
        let ctx = fixture.ctx();
        assert!(leveled(&ctx, 0));
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), Some(ADDS_AT_LEVEL));
        assert!(!leveled(&ctx, 1), "the other seat's XP is their own");
    }

    #[test]
    fn a_spent_fruit_an_opponents_fruit_or_one_still_in_hand_adds_nothing() {
        let mut spent = orchard(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), None);
        drop(ctx);
        let mut theirs = orchard(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), None);
        assert_eq!(adds_while_paying(&ctx, 1, FRUIT), Some(ADDS));
        drop(ctx);
        let mut held = orchard(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), None);
    }

    #[test]
    fn playing_it_from_hand_lands_it_exhausted_so_it_adds_nothing_this_turn() {
        let mut fixture = orchard(fixtures::HAND, 0, false);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRUIT).unwrap();
        assert_eq!(ctx.card(FRUIT).unwrap().zone, Some(fixtures::BASE));
        assert!(
            ctx.card(FRUIT).unwrap().exhausted,
            "369.3 · the entry itself is replaced"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, .. } if *card == FRUIT
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FRUIT}}} enters exhausted")));
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_fruit_stands_in_for_a_rainbow_and_pays_by_exhausting() {
        let mut fixture = orchard(fixtures::BASE, 0, false);
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &rainbow()).expect("the fruit adds the rainbow");
        pay::pay(&mut ctx, 0, &planned);
        assert!(
            ctx.card(FRUIT).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert!(ctx.on_board(FRUIT), "unlike a Gold, the fruit stays");
        assert_eq!(adds_while_paying(&ctx, 0, FRUIT), None);
        assert!(pay::plan(&ctx, 0, &rainbow()).is_err());
    }
}

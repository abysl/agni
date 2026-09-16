use super::prelude::{adding, empower, gear, paying_with, with_statics};
use super::{Adds, Card, Cost, Keyword, Paying, SelfCost, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 1,
    power: &[],
};
pub const ADDS: Cost = Cost {
    energy: 1,
    power: &[],
};
pub const ADDS_WHILE_EMPOWERED: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn adds_while_paying(ctx: &Ctx, seat: u8, egg: u32) -> Option<Cost> {
    let ready = ctx.card(egg).is_some_and(|held| !held.exhausted);
    let mine = ctx.on_board(egg) && !ctx.is_facedown(egg) && ctx.controller(egg) == seat;
    if !(ready && mine) {
        return None;
    }
    Some(if ctx.is_empowered(egg) {
        ADDS_WHILE_EMPOWERED
    } else {
        ADDS
    })
}

fn adds(ctx: &Ctx, seat: u8, egg: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, egg).map(Adds::exhausting)
}

pub static CARD: Card = adding(
    with_statics(
        gear(
            "Platewyrm Egg",
            &[Keyword::Empower(EMPOWER)],
            &[paying_with(empower(EMPOWER), SelfCost::Exhaust)],
        ),
        &[Static::EntersExhausted],
    ),
    adds,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Timing, Trigger};
    use crate::engine::cost::Cost as Total;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const EGG: u32 = 90;

    fn egg(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::gear(EGG, zone, seat, "Platewyrm Egg", 3)
        }
    }

    fn nest(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(egg(zone, seat, exhausted));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(EGG).unwrap(), &CARD));
        fixture
    }

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    #[test]
    fn the_script_enters_exhausted_and_empowers_for_one_energy_and_its_exhaust_with_no_add_ability()
    {
        assert!(std::ptr::eq(script_of("Platewyrm Egg").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(CARD.has_static(Static::EntersExhausted));
        assert_eq!(
            CARD.abilities.len(),
            1,
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Exhaust);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        assert_eq!(ADDS.energy, 1);
        assert_eq!(ADDS_WHILE_EMPOWERED.energy, 2);
    }

    #[test]
    fn played_from_hand_it_enters_exhausted_and_adds_nothing_this_turn() {
        let mut fixture = nest(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EGG).unwrap();
        assert_eq!(ctx.card(EGG).unwrap().zone, Some(fixtures::BASE));
        assert!(
            ctx.card(EGG).unwrap().exhausted,
            "369.3 · the entry itself is replaced"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {EGG}}} enters exhausted")));
        assert_eq!(adds_while_paying(&ctx, 0, EGG), None);
        assert_eq!(
            activate::activate(&mut ctx, 0, EGG, 0),
            Err(Refusal::Exhausted),
            "the Empower needs the exhaust it no longer has"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_egg_empowers_for_one_energy_and_its_exhaust_and_then_adds_two_once_readied() {
        let mut fixture = nest(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, EGG), Some(ADDS));
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == EGG)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {EGG}}}: empower (1 energy, exhaust)")
        );
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, EGG, 0).unwrap();
        assert!(ctx.card(EGG).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert_eq!(adds_while_paying(&ctx, 0, EGG), None, "spent");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(EGG));
        assert!(ctx.events.contains(&Event::Empowered { card: EGG, by: 0 }));
        assert_eq!(
            activate::activate(&mut ctx, 0, EGG, 0),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.ready(EGG));
        assert_eq!(adds_while_paying(&ctx, 0, EGG), Some(ADDS_WHILE_EMPOWERED));
        assert_eq!(
            activate::activate(&mut ctx, 0, EGG, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_opponents_egg_or_one_in_hand_adds_nothing_and_the_other_seat_cannot_empower_it() {
        let mut theirs = nest(fixtures::BASE, 1, false);
        let mut ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, EGG), None);
        assert_eq!(adds_while_paying(&ctx, 1, EGG), Some(ADDS));
        assert_eq!(
            activate::activate(&mut ctx, 0, EGG, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut held = nest(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, EGG), None);
    }

    #[test]
    fn a_ready_egg_stands_in_for_one_energy_and_two_while_empowered() {
        let mut fixture = nest(fixtures::BASE, 0, false);
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &energy(1)).expect("the egg adds one energy");
        pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.card(EGG).unwrap().exhausted, "the exhaust is the cost");
        assert!(ctx.on_board(EGG), "unlike a Gold, the egg stays");
        assert!(ctx.ready(EGG));
        assert!(ctx.empower(EGG));
        assert!(
            pay::plan(&ctx, 0, &energy(2)).is_ok(),
            "two while Empowered"
        );
    }
}

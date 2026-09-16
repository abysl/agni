use super::prelude::{
    activated, disempowering_self, done, draw, empower, gear, named, paying_with, ONE_ENERGY,
};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost::FREE;
pub const DRAWS: usize = 1;

fn study(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = gear(
    "Questionable Tome",
    &[Keyword::Empower(EMPOWER)],
    &[
        paying_with(empower(EMPOWER), SelfCost::Exhaust),
        named(
            disempowering_self(activated(Timing::Sorcery, ONE_ENERGY, &[], study)),
            "disempower this: draw 1",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const TOME: u32 = 90;

    fn tome(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(TOME, fixtures::BASE, 0, "Questionable Tome", 3)
        }
    }

    fn library(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tome(exhausted));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TOME).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn labels_of(ctx: &Ctx) -> Vec<String> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == TOME)
            .map(|offer| offer.label)
            .collect()
    }

    #[test]
    fn the_script_empowers_by_exhausting_and_draws_by_disempowering_one_energy_and_an_exhaust() {
        assert!(std::ptr::eq(script_of("Questionable Tome").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(Cost::FREE)]);
        assert_eq!(CARD.empower_cost(), Some(Cost::FREE));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(Cost::FREE));
        assert_eq!(
            empower.self_cost,
            SelfCost::Exhaust,
            "the exhaust is the Empower cost"
        );
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        let study = &CARD.abilities[1];
        assert_eq!(study.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(study.cost, Some(ONE_ENERGY));
        assert_eq!(study.self_cost, SelfCost::Disempower);
        assert!(study.usable.is_none());
        assert!(study.targets.is_empty());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn exhausting_the_tome_empowers_it_for_nothing_and_the_draw_waits_for_a_ready_tome() {
        let mut fixture = library(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            labels_of(&ctx),
            [format!("{{card {TOME}}}: empower (exhaust)")],
            "the draw is not offered while it is not Empowered"
        );
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, TOME, 0).unwrap();
        assert!(ctx.card(TOME).unwrap().exhausted, "exhausted as the cost");
        assert!(!ctx.is_empowered(TOME), "not until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.is_empowered(TOME));
        assert!(ctx.events.contains(&Event::Empowered { card: TOME, by: 0 }));
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "no energy, no power");
        assert_eq!(
            activate::activate(&mut ctx, 0, TOME, 1),
            Err(Refusal::Exhausted),
            "the draw needs the exhaust the Empower spent"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, TOME, 0),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_empowered_tome_pays_one_and_the_exhaust_disempowers_itself_and_draws_one() {
        let mut fixture = library(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(TOME));
        assert_eq!(
            labels_of(&ctx),
            [format!(
                "{{card {TOME}}}: disempower this: draw 1 (1 energy, exhaust)"
            )],
            "a second Empower is not offered"
        );
        let hand = ctx.hand_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, TOME, 1).unwrap();
        assert!(ctx.card(TOME).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing before it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(TOME));
        assert!(ctx.events.contains(&Event::Disempowered { card: TOME }));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_draw_is_refused_while_not_empowered_and_the_other_seat_is_refused_outright() {
        let mut fixture = library(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, TOME, 1),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "the usable gate · nothing to disempower"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, TOME, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut fixture = library(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, TOME, 0),
            Err(Refusal::Exhausted),
            "an exhausted tome cannot pay its Empower"
        );
    }

    #[test]
    fn the_disempower_is_paid_at_activation_before_the_ability_is_on_the_chain() {
        let mut fixture = library(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(TOME));
        activate::activate(&mut ctx, 0, TOME, 1).unwrap();
        assert!(
            !ctx.is_empowered(TOME),
            "355.10.c · the cost is paid as the ability is activated"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}

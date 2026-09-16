use super::ferrous_forerunner::play_mechs;
use super::prelude::{
    activated, disempowering_self, done, empower, gear, named, paying_with, Location, ONE_ENERGY,
};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost::FREE;
pub const MECHS: usize = 1;

fn assemble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_mechs(ctx, seat, Location::Base(seat), MECHS);
    done()
}

pub static CARD: Card = gear(
    "Hextech Disc",
    &[Keyword::Empower(EMPOWER)],
    &[
        paying_with(empower(EMPOWER), SelfCost::Exhaust),
        named(
            disempowering_self(activated(Timing::Sorcery, ONE_ENERGY, &[], assemble)),
            "disempower this: play a 3 Might Mech to your base",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::ferrous_forerunner::tests::mechs_of;
    use crate::cards::ferrous_forerunner::MECH_MIGHT;
    use crate::cards::rumble_mechanized_menace::is_mech;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DISC: u32 = 90;

    fn disc(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            power: Some(1),
            exhausted,
            ..fixtures::gear(DISC, fixtures::BASE, 0, "Hextech Disc", 4)
        }
    }

    fn workshop(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(disc(exhausted));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DISC).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_empowers_by_exhausting_and_builds_a_mech_by_disempowering_one_energy_and_an_exhaust(
    ) {
        assert!(std::ptr::eq(script_of("Hextech Disc").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(Cost::FREE)]);
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(Cost::FREE));
        assert_eq!(empower.self_cost, SelfCost::Exhaust);
        assert_eq!(empower.label, Some("empower"));
        let assemble = &CARD.abilities[1];
        assert_eq!(assemble.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(assemble.cost, Some(ONE_ENERGY));
        assert_eq!(assemble.self_cost, SelfCost::Disempower);
        assert!(assemble.usable.is_none());
        assert!(assemble.targets.is_empty());
        assert_eq!(MECHS, 1);
        assert_eq!(MECH_MIGHT, 3);
    }

    #[test]
    fn exhausting_the_disc_empowers_it_and_the_mech_waits_for_it_to_ready() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, DISC, 0).unwrap();
        assert!(ctx.card(DISC).unwrap().exhausted);
        resolve_chain(&mut ctx);
        assert!(ctx.is_empowered(DISC));
        assert!(ctx.events.contains(&Event::Empowered { card: DISC, by: 0 }));
        assert_eq!(
            activate::activate(&mut ctx, 0, DISC, 1),
            Err(Refusal::Exhausted)
        );
        assert!(mechs_of(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_empowered_disc_pays_one_disempowers_itself_and_plays_a_mech_to_your_base() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(DISC));
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, DISC, 1).unwrap();
        assert!(ctx.card(DISC).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert!(mechs_of(&ctx, 0).is_empty(), "nothing before it resolves");
        resolve_chain(&mut ctx);
        assert!(!ctx.is_empowered(DISC));
        assert!(ctx.events.contains(&Event::Disempowered { card: DISC }));
        let mechs = mechs_of(&ctx, 0);
        assert_eq!(mechs.len(), 1);
        let mech = mechs[0];
        assert!(is_mech(&ctx, mech));
        assert_eq!(ctx.location(mech), Some(Location::Base(0)));
        assert_eq!(ctx.current_might(mech), i32::from(MECH_MIGHT));
        assert!(
            ctx.card(mech).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == mech
        )));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_mech_is_refused_while_not_empowered_and_the_other_seat_outright() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, DISC, 1),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "the usable gate · nothing to disempower"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, DISC, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, DISC, 0),
            Err(Refusal::Exhausted)
        );
    }

    #[test]
    fn the_disempower_is_paid_at_activation_before_the_ability_is_on_the_chain() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(DISC));
        activate::activate(&mut ctx, 0, DISC, 1).unwrap();
        assert!(!ctx.is_empowered(DISC));
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}

use super::prelude::{
    a_unit, activated, card_target, done, exhausting_self, gear, might_this_turn, named,
};
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const PENALTY: i16 = -1;
pub const FLOOR: i32 = 1;

fn regret(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, PENALTY, Some(FLOOR));
        ctx.narrate(format!(
            "{{card {unit}}} gets -1 Might this turn · to a minimum of {FLOOR}"
        ));
    }
    done()
}

pub static CARD: Card = gear(
    "Orb of Regret",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            Cost::FREE,
            &[a_unit("a unit to give -1 Might this turn")],
            regret,
        )),
        "give a unit -1 Might this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::Target;

    const ORB: u32 = 90;
    const WEAKLING: u32 = 91;

    fn with_orb() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut orb = fixtures::gear(ORB, fixtures::BASE, 0, "Orb of Regret", 1);
        orb.domain = vec!["Mind".into()];
        fixture.table.cards.push(orb);
        fixture
            .table
            .cards
            .push(fixtures::unit(WEAKLING, fixtures::BASE, 1, "Weakling", 1));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn orb_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == ORB)
            .map(|offer| (offer.label, offer.enabled))
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_gear_with_one_exhaust_activation_over_any_unit() {
        let fixture = with_orb();
        assert!(std::ptr::eq(fixture.scripts.of_card(ORB).unwrap(), &CARD));
        assert_eq!(CARD.name, "Orb of Regret");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, crate::cards::prelude::UNIT);
        assert_eq!(ability.label, Some("give a unit -1 Might this turn"));
    }

    #[test]
    fn exhausting_the_orb_gives_the_chosen_enemy_unit_minus_one_until_the_turn_ends() {
        let mut fixture = with_orb();
        let mut ctx = fixture.ctx();
        assert_eq!(
            orb_offers(&ctx),
            [(
                format!("{{card {ORB}}}: give a unit -1 Might this turn (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, ORB, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let offered: Vec<Option<u32>> = crate::engine::prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.card)
            .collect();
        assert!(
            offered.contains(&Some(fixtures::VI)),
            "friendly units are units"
        );
        assert!(offered.contains(&Some(fixtures::THEIR_UNIT)));
        assert!(offered.contains(&Some(WEAKLING)));
        assert!(!offered.contains(&Some(ORB)), "the orb is gear");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(ctx.card(ORB).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 1);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -1);
        let mods = &ctx.state_of(fixtures::THEIR_UNIT).unwrap().might;
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].until, Expiry::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets -1 Might this turn · to a minimum of 1",
            fixtures::THEIR_UNIT
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, ORB, 0),
            Err(Refusal::Exhausted),
            "one use per readying"
        );
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "the penalty expired"
        );
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn a_one_might_unit_stays_at_one_because_the_minimum_is_read_at_application() {
        let mut fixture = with_orb();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, ORB, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {WEAKLING}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(WEAKLING), 1, "454.3.b · never below 1");
        assert_eq!(might_counter(&ctx, WEAKLING), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_activate_it_and_an_exhausted_orb_is_refused() {
        let mut fixture = with_orb();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, ORB, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, ORB, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        drop(ctx);
        fixture.table.card_mut(ORB).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ORB, 0),
            Err(Refusal::Exhausted)
        );
        assert!(orb_offers(&ctx).is_empty());
        assert!(ctx.blob.queue.is_empty());
    }
}

use super::prelude::{
    a_card, activated, card_target, done, exhausting_self, gear, move_unit, named, Location,
    ONE_ENERGY,
};
use super::{Card, Filter, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD_FOR_BASE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::AtBattlefield,
    Filter::MovableToBase,
]);

fn sing_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        move_unit(ctx, item, unit, Location::Base(item.controller));
    }
    done()
}

pub static CARD: Card = gear(
    "The Syren",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            ONE_ENERGY,
            &[a_card(
                FRIENDLY_UNIT_AT_A_BATTLEFIELD_FOR_BASE,
                "a friendly unit at a battlefield to move to your base",
            )],
            sing_home,
        )),
        "move a friendly unit to your base",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;

    const SYREN: u32 = 90;
    const SAILOR: u32 = 91;

    fn harbour() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut syren = fixtures::gear(SYREN, fixtures::BASE, 0, "The Syren", 2);
        syren.domain = vec!["Chaos".into()];
        fixture.table.cards.push(syren);
        let mut sailor = fixtures::unit(SAILOR, fixtures::BF1, 0, "Sailor", 2);
        sailor.exhausted = true;
        fixture.table.cards.push(sailor);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn syren_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == SYREN)
            .map(|offer| (offer.label, offer.enabled))
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_costs_one_energy_and_the_exhaust_over_a_friendly_unit_at_a_battlefield() {
        let mut fixture = harbour();
        assert!(std::ptr::eq(fixture.scripts.of_card(SYREN).unwrap(), &CARD));
        assert_eq!(CARD.name, "The Syren");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(
            ability.targets[0].filter,
            FRIENDLY_UNIT_AT_A_BATTLEFIELD_FOR_BASE
        );
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, SYREN, 0).label(), "1 energy");
        assert_eq!(
            syren_offers(&ctx),
            [(
                format!("{{card {SYREN}}}: move a friendly unit to your base (1 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn one_energy_and_the_exhaust_move_the_sailor_home_as_an_effect_move() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let ready_runes = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, SYREN, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            prompts::offered(&ctx)
                .iter()
                .map(|opt| opt.card)
                .collect::<Vec<_>>(),
            [Some(SAILOR), None],
            "Vi sits in the base and the Sprite is an enemy"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SAILOR}}}")).unwrap();
        assert!(ctx.card(SYREN).unwrap().exhausted);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready_runes - 1,
            "one rune pays"
        );
        assert_eq!(
            ctx.location(SAILOR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        resolve_chain(&mut ctx);
        assert_eq!(ctx.location(SAILOR), Some(Location::Base(0)));
        assert!(
            ctx.card(SAILOR).unwrap().exhausted,
            "a move keeps its state"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, from: Some(Location::Battlefield(fixtures::BF1)), to: Location::Base(0), cause: MoveCause::Effect, .. } if *card == SAILOR
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SAILOR}}} moves to their base")));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_friendly_unit_at_a_battlefield_or_without_energy_the_song_is_refused() {
        let mut fixture = harbour();
        fixture.table.card_mut(SAILOR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SYREN, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert!(syren_offers(&ctx).is_empty());
        drop(ctx);
        let mut broke = harbour();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            syren_offers(&ctx),
            [(
                format!("{{card {SYREN}}}: move a friendly unit to your base (1 energy, exhaust)"),
                false
            )],
            "listed but greyed"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, SYREN, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, SYREN, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(SYREN).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
    }
}

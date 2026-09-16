use super::faithful_manufactor::play_recruits;
use super::prelude::{
    a_play_location, activated, done, exhausting_self, legend, named, zone_target, Location,
    ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const WHERE: &str = "where the Recruit is played";

fn herald(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    play_recruits(ctx, seat, at, 1);
    done()
}

pub static CARD: Card = legend(
    "Viktor - Herald of the Arcane",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            ONE_ENERGY,
            &[a_play_location(WHERE)],
            herald,
        )),
        "play a Recruit",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::{script_of, SelfCost, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const VIKTOR: u32 = fixtures::LEGEND_CARD;

    fn laboratory() -> Fixture {
        let mut fixture = Fixture::enforced();
        let legend = fixture.table.card_mut(VIKTOR).unwrap();
        legend.name = "Viktor - Herald of the Arcane".into();
        legend.domain = vec!["Mind".into(), "Order".into()];
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VIKTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn labels(offers: &[activate::Offer]) -> Vec<(String, bool)> {
        offers
            .iter()
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_one_paid_exhaust_ability_that_picks_a_play_location() {
        assert!(std::ptr::eq(
            script_of("Viktor - Herald of the Arcane").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.xp, 0);
        assert_eq!(ability.label, Some("play a Recruit"));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let mut fixture = laboratory();
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, VIKTOR, 0).label(), "1 energy");
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [(
                format!("{{card {VIKTOR}}}: play a Recruit (1 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn one_energy_and_the_exhaust_play_an_exhausted_recruit_at_the_chosen_held_location() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, VIKTOR, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "the base and the held battlefield, never the other seat's"
        );
        assert!(
            !ctx.card(VIKTOR).unwrap().exhausted,
            "358.5 · nothing is spent before the plan"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.card(VIKTOR).unwrap().exhausted,
            "the legend is exhausted"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one ready rune pays the energy"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF1)]);
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for resolution"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recruits_of(&ctx, 0), [next]);
        assert_eq!(
            ctx.location(next),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_token(next));
        assert!(
            ctx.card(next).unwrap().exhausted,
            "185.2.d · it enters exhausted"
        );
        assert_eq!(ctx.current_might(next), 1);
        assert_eq!(ctx.controller(next), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {next}}} to {{zone 9}}")));
        assert_eq!(
            activate::activate(&mut ctx, 0, VIKTOR, 0),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_base_is_a_play_location_too_and_a_cancelled_activation_spends_nothing() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, VIKTOR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        resolve_chain(&mut ctx);
        let recruit = *recruits_of(&ctx, 0).first().expect("the Herald's Recruit");
        assert_eq!(ctx.location(recruit), Some(Location::Base(0)));
        drop(ctx);

        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, VIKTOR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(!ctx.card(VIKTOR).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(ctx.effects.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
    }

    #[test]
    fn an_exhausted_legend_an_empty_rune_pool_and_the_other_seat_are_refused() {
        let mut fixture = laboratory();
        fixture.table.card_mut(VIKTOR).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert!(
            activate::offers(&ctx, 0).is_empty(),
            "an exhausted legend offers nothing"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, VIKTOR, 0),
            Err(Refusal::Exhausted)
        );
        drop(ctx);

        let mut fixture = laboratory();
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, VIKTOR, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, VIKTOR, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.effects.is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
    }
}

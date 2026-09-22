use super::prelude::{
    a_card, activated, card_target, done, named, on_conquer_me, paying_with, spawn, swap_units,
    unit, Location, Swapped, Token,
};
use super::{
    Card, Cost, Domain, Filter, Flow, Item, Power, SelfCost, Stage, Timing, TOKEN_SHADOW_CLONE,
};
use crate::engine::ctx::Ctx;

pub const SWAP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Chaos)],
};
const CLONE_ARRIVES_READY: bool = false;
pub const SHADOW_CLONE_ELSEWHERE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Named(TOKEN_SHADOW_CLONE),
    Filter::Not(&Filter::Here),
]);

fn conjure(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(clone) = spawn(
        ctx,
        seat,
        Token::ShadowClone,
        Location::Base(seat),
        CLONE_ARRIVES_READY,
    ) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {clone}}} to their base"
        ));
    }
    done()
}

fn shadow_swap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(clone) = card_target(ctx, item, 0) else {
        return done();
    };
    if swap_units(ctx, me, clone) == Swapped::Swapped {
        ctx.narrate(format!("{{card {me}}} and {{card {clone}}} trade places"));
    }
    done()
}

pub static CARD: Card = unit(
    "Zed, Without a Sound",
    &[],
    &[
        on_conquer_me(&[], conjure),
        named(
            paying_with(
                activated(
                    Timing::Action,
                    SWAP,
                    &[a_card(
                        SHADOW_CLONE_ELSEWHERE,
                        "a Shadow Clone to trade places with",
                    )],
                    shadow_swap,
                ),
                SelfCost::Free,
            ),
            "swap places with a Shadow Clone",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, chain, cleanup, triggers};
    use crate::state::PromptWhy;

    const ZED: u32 = 90;
    const CLONE: u32 = 91;
    const NEAR_CLONE: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut zed = fixtures::unit(ZED, fixtures::BF1, 0, "Zed, Without a Sound", 5);
        zed.domain = vec!["Chaos".into()];
        fixture.table.cards.push(zed);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Chaos", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn clone_at(id: u32, zone: u16) -> agni_plugin_sdk::table::CardInfo {
        agni_plugin_sdk::table::CardInfo {
            might: Some(0),
            ..fixtures::card(id, zone, 0, "Shadow Clone", "Unit")
        }
    }

    fn swap(ctx: &mut Ctx, clone: u32) {
        activate::activate(ctx, 0, ZED, 1).unwrap();
        crate::engine::settle(ctx).unwrap();
        fixtures::choose(ctx, 0, &format!("{{card {clone}}}")).unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn conquering_plays_an_exhausted_shadow_clone_to_the_base() {
        assert!(std::ptr::eq(
            script_of("Zed, Without a Sound").unwrap(),
            &CARD
        ));
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert!(triggers::collect(&mut ctx) >= 1);
        chain::proceed(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        let clone = ctx
            .table
            .cards
            .iter()
            .find(|card| card.name == TOKEN_SHADOW_CLONE)
            .expect("a Shadow Clone");
        assert_eq!(clone.zone, Some(fixtures::BASE));
        assert_eq!(clone.owner, 0);
        assert!(clone.exhausted);
        assert_eq!(clone.might, Some(0));
        assert!(ctx.is_token(clone.id));
        assert!(std::ptr::eq(
            ctx.script(clone.id).unwrap(),
            &crate::cards::shadow_clone::CARD
        ));
    }

    #[test]
    fn the_swap_charges_only_its_printed_cost_and_keeps_zed_ready() {
        assert_eq!(CARD.abilities[1].cost, Some(SWAP));
        assert_eq!(CARD.abilities[1].self_cost, SelfCost::Free);
        let mut fixture = armed();
        fixture.table.cards.push(clone_at(CLONE, fixtures::BF2));
        fixture
            .table
            .cards
            .push(clone_at(NEAR_CLONE, fixtures::BF1));
        fixture.table.tokens.extend([CLONE, NEAR_CLONE]);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        let offers = activate::offers(&ctx, 0);
        let offer = offers
            .iter()
            .find(|offer| offer.source == ZED)
            .expect("the swap is offered");
        assert!(offer.enabled);
        assert!(offer.label.contains("1 energy and 1 Chaos power"));
        assert!(!offer.label.contains("exhaust"));
        activate::activate(&mut ctx, 0, ZED, 1).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 91}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(!ctx.card(ZED).unwrap().exhausted);
        assert_eq!(ctx.runes_of(0).len(), runes - 1);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert_eq!(
            ctx.blob.chain.last().unwrap().targets,
            [crate::state::TargetRef::Card(CLONE)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.location(CLONE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
    }

    #[test]
    fn an_exhausted_zed_can_swap_with_a_clone() {
        let mut fixture = armed();
        fixture.table.cards.push(clone_at(CLONE, fixtures::BF2));
        fixture.table.tokens.push(CLONE);
        fixture.resolve();
        fixture.table.card_mut(ZED).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ZED && offer.enabled));
        swap(&mut ctx, CLONE);
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn the_swap_can_be_repeated_with_sufficient_resources() {
        let mut fixture = armed();
        fixture.table.cards.push(clone_at(CLONE, fixtures::BF2));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Chaos", false));
        fixture.table.tokens.push(CLONE);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        swap(&mut ctx, CLONE);
        assert!(!ctx.card(ZED).unwrap().exhausted);
        swap(&mut ctx, CLONE);
        assert!(!ctx.card(ZED).unwrap().exhausted);
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(CLONE),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn the_swap_requires_chaos_power_and_cancels_without_costs() {
        let mut fixture = armed();
        fixture.table.cards.push(clone_at(CLONE, fixtures::BF2));
        fixture.table.tokens.push(CLONE);
        fixture.table.cards.retain(|card| card.id != 46);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let runes = ctx
            .runes_of(0)
            .into_iter()
            .map(|rune| (rune.id, rune.exhausted))
            .collect::<Vec<_>>();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ZED && !offer.enabled));
        assert!(activate::activate(&mut ctx, 0, ZED, 1).is_err());
        assert_eq!(
            ctx.runes_of(0)
                .into_iter()
                .map(|rune| (rune.id, rune.exhausted))
                .collect::<Vec<_>>(),
            runes
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(CLONE),
            Some(Location::Battlefield(fixtures::BF2))
        );

        drop(ctx);
        let mut fixture = armed();
        fixture.table.cards.push(clone_at(CLONE, fixtures::BF2));
        fixture.table.tokens.push(CLONE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let runes = ctx
            .runes_of(0)
            .into_iter()
            .map(|rune| (rune.id, rune.exhausted))
            .collect::<Vec<_>>();
        activate::activate(&mut ctx, 0, ZED, 1).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(
            ctx.runes_of(0)
                .into_iter()
                .map(|rune| (rune.id, rune.exhausted))
                .collect::<Vec<_>>(),
            runes
        );
        assert!(!ctx.card(ZED).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(CLONE),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn without_a_clone_elsewhere_the_action_is_not_offered_and_is_refused() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(clone_at(NEAR_CLONE, fixtures::BF1));
        fixture.table.tokens.push(NEAR_CLONE);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            !activate::offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == ZED && offer.index == 1),
            "402.3: no legal Clone, no offer"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, ZED, 1),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::NoLegalTargets
            ))
        );
        assert!(!ctx.card(ZED).unwrap().exhausted);
        assert_eq!(
            ctx.location(ZED),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}

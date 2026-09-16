use super::prelude::{
    a_card, activated, card_target, done, named, on_conquer_me, spawn, swap_units, unit, Location,
    Swapped, Token,
};
use super::{Card, Cost, Domain, Filter, Flow, Item, Power, Stage, Timing, TOKEN_SHADOW_CLONE};
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
            activated(
                Timing::Action,
                SWAP,
                &[a_card(
                    SHADOW_CLONE_ELSEWHERE,
                    "a Shadow Clone to trade places with",
                )],
                shadow_swap,
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
    fn the_action_swaps_zed_with_a_clone_elsewhere_and_marks_the_contest() {
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
        let offers = activate::offers(&ctx, 0);
        let offer = offers
            .iter()
            .find(|offer| offer.source == ZED)
            .expect("the swap is offered");
        assert!(offer.enabled);
        assert!(offer.label.contains("swap places with a Shadow Clone"));
        activate::activate(&mut ctx, 0, ZED, 1).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 }),
            "355: the Clone is chosen as the ability is finalized"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "cancel"],
            "the Clone beside Zed is not a candidate"
        );
        assert!(!ctx.card(ZED).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(ctx.card(ZED).unwrap().exhausted, "the arrow is an exhaust");
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

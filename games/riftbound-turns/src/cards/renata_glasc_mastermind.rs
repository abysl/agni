use super::prelude::{
    activated, at_battlefield, done, draw, exhausting_self, named, paying_with, score_point, unit,
    usable_if,
};
use super::{Card, Cost, Domain, Flow, Item, Power, SelfCost, Source, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const DRAW: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Mind)],
};
pub const SCORE: Cost = Cost {
    energy: 4,
    power: &[
        Power::Domain(Domain::Mind),
        Power::Domain(Domain::Mind),
        Power::Domain(Domain::Mind),
        Power::Domain(Domain::Mind),
    ],
};
pub const DRAWS: usize = 1;

pub fn at_a_battlefield(ctx: &Ctx, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn scheme(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

fn masterstroke(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Renata Glasc - Mastermind",
    &[],
    &[
        named(
            usable_if(
                paying_with(
                    activated(Timing::Sorcery, DRAW, &[], scheme),
                    SelfCost::Free,
                ),
                at_a_battlefield,
            ),
            "draw 1",
        ),
        named(
            usable_if(
                exhausting_self(activated(Timing::Sorcery, SCORE, &[], masterstroke)),
                at_a_battlefield,
            ),
            "score 1 point",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::rules::COUNTER_POINTS;
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const RENATA: u32 = 90;
    const MIND_RUNES: [u32; 5] = [46, 47, 48, 49, 55];

    fn renata(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: None,
            domain: vec!["Mind".into()],
            ..fixtures::unit(RENATA, zone, 0, "Renata Glasc - Mastermind", 4)
        }
    }

    fn boardroom(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(renata(zone));
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == RENATA)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn points(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), COUNTER_POINTS)
            .unwrap_or(0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_two_sorcery_activations_usable_only_at_a_battlefield() {
        assert!(std::ptr::eq(
            script_of("Renata Glasc - Mastermind").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let scheme = &CARD.abilities[0];
        assert_eq!(scheme.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(scheme.cost, Some(DRAW));
        assert_eq!(
            scheme.self_cost,
            SelfCost::Free,
            "the draw prints no exhaust"
        );
        assert!(scheme.usable.is_some());
        assert_eq!(scheme.label, Some("draw 1"));
        let masterstroke = &CARD.abilities[1];
        assert_eq!(masterstroke.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(masterstroke.cost, Some(SCORE));
        assert_eq!(masterstroke.self_cost, SelfCost::Exhaust);
        assert!(masterstroke.usable.is_some());
        assert_eq!(masterstroke.label, Some("score 1 point"));
        assert_eq!(SCORE.power.len(), 4);
        let mut fixture = boardroom(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(at_a_battlefield(
            &ctx,
            Source {
                card: RENATA,
                ability: 0
            }
        ));
    }

    #[test]
    fn at_a_battlefield_the_draw_costs_one_and_a_mind_without_exhausting_her() {
        let mut fixture = boardroom(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            her_offers(&ctx),
            [
                (
                    format!("{{card {RENATA}}}: draw 1 (1 energy and 1 Mind power)"),
                    true
                ),
                (
                    format!(
                        "{{card {RENATA}}}: score 1 point (4 energy and 4 Mind power, exhaust)"
                    ),
                    true
                ),
            ]
        );
        let hand = ctx.hand_of(0).len();
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, RENATA, 0).unwrap();
        assert!(
            !ctx.card(RENATA).unwrap().exhausted,
            "no exhaust in the cost"
        );
        assert_eq!(ctx.runes_of(0).len(), runes - 1, "one Mind recycled");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "164.2 · the same rune is exhausted for the energy, then recycled for the Mind"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RENATA}}} · {{seat 0}} draws 1")));
        activate::activate(&mut ctx, 0, RENATA, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 2 * DRAWS,
            "no once-per-turn on the draw"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_masterstroke_exhausts_her_for_four_and_four_mind_and_scores_a_point() {
        let mut fixture = boardroom(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        activate::activate(&mut ctx, 0, RENATA, 1).unwrap();
        assert!(
            ctx.card(RENATA).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.runes_of(0).len(), runes - 4, "four Mind recycled");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(points(&ctx, 0), 0, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(points(&ctx, 0), 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert_eq!(
            activate::activate(&mut ctx, 0, RENATA, 1),
            Err(Refusal::Exhausted),
            "one masterstroke per ready"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn at_the_base_neither_ability_is_offered_nor_usable() {
        let mut fixture = boardroom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(
            her_offers(&ctx).is_empty(),
            "use my abilities only while I'm at a battlefield"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, RENATA, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, RENATA, 1),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, RENATA, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn without_four_mind_runes_the_masterstroke_is_greyed_and_refused() {
        let mut fixture = boardroom(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| !MIND_RUNES[3..].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            7,
            "three Mind, three Fury, one Calm"
        );
        assert_eq!(
            her_offers(&ctx)
                .iter()
                .map(|(_, enabled)| *enabled)
                .collect::<Vec<bool>>(),
            [true, false],
            "three Mind runes cannot pay four Mind power"
        );
        assert!(activate::activate(&mut ctx, 0, RENATA, 1).is_err());
        assert!(!ctx.card(RENATA).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
    }
}

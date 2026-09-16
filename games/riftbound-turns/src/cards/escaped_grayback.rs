use super::prelude::{
    a_friendly_unit, activated, card_target, done, is_empowered, named, paying_with, unit,
    usable_if, with_statics,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, SelfCost, Source, Stage, Static, Timing};
use crate::cards::cruel_patron;
use crate::engine::ctx::{Ctx, Killed};

pub const EMPOWER: Cost = Cost::FREE;
pub const MIGHT: i16 = 2;

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    cruel_patron::kill_candidates(ctx, seat)
}

pub fn killed_as_the_empower_cost_until_the_pay_stage_asks_which(
    ctx: &Ctx,
    source: Source,
) -> bool {
    !ctx.is_empowered(source.card)
        && cruel_patron::kill_cost_payable(ctx, ctx.controller(source.card))
}

fn break_free(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let paid = card_target(ctx, item, 0)
        .is_some_and(|unit| cruel_patron::pay_kill_cost(ctx, unit) != Killed::NotOnBoard);
    if !paid {
        ctx.narrate(format!(
            "the chosen unit is no longer there to kill · {{card {me}}} is not Empowered"
        ));
        return done();
    }
    ctx.empower_by(me, item.controller);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Escaped Grayback",
        &[Keyword::Empower(EMPOWER)],
        &[usable_if(
            named(
                paying_with(
                    activated(
                        Timing::Sorcery,
                        EMPOWER,
                        &[a_friendly_unit("a friendly unit to kill")],
                        break_free,
                    ),
                    SelfCost::Free,
                ),
                "empower",
            ),
            killed_as_the_empower_cost_until_the_pay_stage_asks_which,
        )],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const GRAYBACK: u32 = 90;
    const SCOUT: u32 = 91;

    fn grayback() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(GRAYBACK, fixtures::BASE, 0, "Escaped Grayback", 3)
        }
    }

    fn pen(with_scout: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(grayback());
        if with_scout {
            fixture
                .table
                .cards
                .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
            fixture.blob.set_holder(fixtures::BF1, Some(0));
        }
        fixture.resolve();
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == GRAYBACK)
            .collect()
    }

    #[test]
    fn the_script_prints_a_free_empower_whose_kill_is_the_seam_and_grants_two_might() {
        assert!(std::ptr::eq(script_of("Escaped Grayback").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(Cost::FREE)]);
        assert_eq!(
            CARD.empower_cost(),
            Some(Cost::FREE),
            "a kill is no resource · cards::Cost is energy and power"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(Cost::FREE));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert_eq!(empower.targets.len(), 1);
        assert_eq!(empower.targets[0].filter, FRIENDLY_UNIT);
        assert!(empower.usable.is_some());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(2)])]
        ));
    }

    #[test]
    fn the_candidates_are_his_controllers_units_himself_included() {
        let mut fixture = pen(true);
        let ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [fixtures::VI, GRAYBACK, SCOUT]);
        assert_eq!(
            kill_candidates(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
    }

    #[test]
    fn empowering_him_kills_the_chosen_friendly_unit_at_resolution_and_he_stands_at_five() {
        let mut fixture = pen(true);
        let mut ctx = fixture.ctx();
        let runes = ctx.ready_runes_of(0).len();
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].label, format!("{{card {GRAYBACK}}}: empower"));
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, GRAYBACK, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {GRAYBACK}}}"),
                format!("{{card {SCOUT}}}"),
                "cancel".to_string(),
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == GRAYBACK
        ));
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "no rune is spent");
        assert!(!ctx.card(GRAYBACK).unwrap().exhausted);
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::BF1),
            "the kill waits for resolution"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == SCOUT
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SCOUT}}} is killed as an additional cost")));
        assert!(ctx.is_empowered(GRAYBACK));
        assert!(matches!(
            statics::grants_on(&ctx, GRAYBACK).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.current_might(GRAYBACK), 5);
        fixtures::pass_until_open(&mut ctx);
        assert!(his_offers(&ctx).is_empty(), "377.2.b · the offer is gone");
        assert_eq!(
            activate::activate(&mut ctx, 0, GRAYBACK, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_chosen_unit_that_left_before_resolution_pays_nothing_and_he_stays_unempowered() {
        let mut fixture = pen(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GRAYBACK, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(
            ctx.kill(SCOUT, crate::engine::ctx::Cause::Rule),
            Killed::Yes
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(!ctx.is_empowered(GRAYBACK));
        assert_eq!(ctx.current_might(GRAYBACK), 3);
        assert!(ctx.blob.log.contains(&format!(
            "the chosen unit is no longer there to kill · {{card {GRAYBACK}}} is not Empowered"
        )));
    }

    #[test]
    fn alone_on_the_board_he_may_only_kill_himself_and_without_any_unit_the_offer_is_withheld() {
        let mut fixture = pen(false);
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [GRAYBACK]);
        activate::activate(&mut ctx, 0, GRAYBACK, 0).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {GRAYBACK}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {GRAYBACK}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(GRAYBACK).unwrap().zone, Some(fixtures::TRASH));
        assert!(!ctx.is_empowered(GRAYBACK), "a dead unit is not Empowered");
        assert!(kill_candidates(&ctx, 0).is_empty());
        assert!(
            !killed_as_the_empower_cost_until_the_pay_stage_asks_which(
                &ctx,
                Source {
                    card: GRAYBACK,
                    ability: 0
                }
            ),
            "356.2.a.1 · without a friendly unit the cost cannot be paid"
        );
        assert!(his_offers(&ctx).is_empty());
        assert!(activate::activate(&mut ctx, 0, GRAYBACK, 0).is_err());
    }

    #[test]
    #[ignore = "engine gap · a kill as an Empower cost is a pick at the pay stage (355.10.c, 357) that raises no Chosen and cannot be undone by a response; today the unit is a target killed at resolution"]
    fn the_kill_is_paid_before_the_empower_reaches_the_chain_and_raises_no_chosen() {
        let mut fixture = pen(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GRAYBACK, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::TRASH),
            "357.2 · the kill is paid before the ability exists"
        );
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Chosen { card, .. } if *card == SCOUT)),
            "a cost is not a choice"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.is_empowered(GRAYBACK));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(GRAYBACK));
    }
}

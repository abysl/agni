use super::prelude::{
    a_unit_at_a_battlefield, activated, card_target, deal, done, grant_this_turn, named, ready,
    stun, unit,
};
use super::sett_brawler::{spend_buff, spending_my_buff};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const DEAL: u8 = 0;
pub const STUN: u8 = 1;
pub const READY: u8 = 2;
pub const GANKING: u8 = 3;

fn claw(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if spend_buff(ctx, me) && deal(ctx, item, unit, DAMAGE) {
        ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
    }
    done()
}

fn pounce(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if spend_buff(ctx, me) {
        stun(ctx, unit);
    }
    done()
}

fn shake_off(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if spend_buff(ctx, me) {
        ready(ctx, me);
    }
    done()
}

fn prowl(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if spend_buff(ctx, me) && grant_this_turn(ctx, me, Keyword::Ganking) {
        ctx.narrate(format!("{{card {me}}} has Ganking this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Udyr - Wildman",
    &[],
    &[
        named(
            spending_my_buff(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit_at_a_battlefield("a unit at a battlefield to deal 2")],
                claw,
            )),
            "spend my buff: deal 2 to a unit at a battlefield",
        ),
        named(
            spending_my_buff(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit_at_a_battlefield("a unit at a battlefield to stun")],
                pounce,
            )),
            "spend my buff: stun a unit at a battlefield",
        ),
        named(
            spending_my_buff(activated(Timing::Sorcery, Cost::FREE, &[], shake_off)),
            "spend my buff: ready me",
        ),
        named(
            spending_my_buff(activated(Timing::Sorcery, Cost::FREE, &[], prowl)),
            "spend my buff: Ganking this turn",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, COUNTER_BUFFED, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const UDYR: u32 = 90;
    const RAIDER: u32 = 91;

    fn udyr(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(UDYR, zone, 0, "Udyr - Wildman", 6)
        }
    }

    fn wilds(zone: u16, buffed: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(udyr(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        if buffed {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(UDYR),
                counter: COUNTER_BUFFED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn his_offers(ctx: &Ctx) -> Vec<String> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == UDYR)
            .map(|offer| offer.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_four_free_sorcery_modes_each_gated_on_his_buff() {
        assert!(std::ptr::eq(script_of("Udyr - Wildman").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 4);
        for ability in CARD.abilities {
            assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
            assert_eq!(ability.cost, Some(Cost::FREE));
            assert_eq!(ability.self_cost, SelfCost::Free);
            assert!(ability.usable.is_some());
        }
        assert_eq!(
            CARD.abilities[usize::from(DEAL)].targets[0].filter,
            UNIT_AT_BATTLEFIELD
        );
        assert_eq!(
            CARD.abilities[usize::from(STUN)].targets[0].filter,
            UNIT_AT_BATTLEFIELD
        );
        assert!(CARD.abilities[usize::from(READY)].targets.is_empty());
        assert!(CARD.abilities[usize::from(GANKING)].targets.is_empty());
        assert_eq!(DAMAGE, 2);
    }

    #[test]
    fn buffed_he_offers_all_four_modes_and_the_claw_spends_the_buff_for_two_damage() {
        let mut fixture = wilds(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [
                format!("{{card {UDYR}}}: spend my buff: deal 2 to a unit at a battlefield"),
                format!("{{card {UDYR}}}: spend my buff: stun a unit at a battlefield"),
                format!("{{card {UDYR}}}: spend my buff: ready me"),
                format!("{{card {UDYR}}}: spend my buff: Ganking this turn"),
            ]
        );
        activate::activate(&mut ctx, 0, UDYR, DEAL).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {RAIDER}}}"),
                "cancel".to_string()
            ],
            "units at battlefields; Udyr and Vi in the base are not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert!(ctx.is_buffed(UDYR), "the buff goes as the mode resolves");
        assert_eq!(damage(&ctx, RAIDER), 0);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(UDYR));
        assert_eq!(damage(&ctx, RAIDER), 2);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 2, .. } if *card == RAIDER
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {UDYR}}} deals 2 to {{card {RAIDER}}}")));
        assert!(his_offers(&ctx).is_empty(), "spent, nothing is offered");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_pounce_stuns_and_the_stun_lands_only_if_the_buff_was_there_to_spend() {
        let mut fixture = wilds(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, UDYR, STUN).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(spend_buff(&mut ctx, UDYR));
        assert_eq!(pounce(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(
            !ctx.is_stunned(RAIDER),
            "the buff was already gone when the mode resolved"
        );
        ctx.buff(UDYR);
        assert_eq!(pounce(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx.is_stunned(RAIDER));
        assert!(!ctx.is_buffed(UDYR));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RAIDER}}} is stunned")));
    }

    #[test]
    fn readying_himself_and_ganking_are_the_two_untargeted_modes() {
        let mut fixture = wilds(fixtures::BASE, true);
        fixture.table.card_mut(UDYR).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, UDYR, READY).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.card(UDYR).unwrap().exhausted,
            "a free self cost never exhausts him"
        );
        resolve_chain(&mut ctx);
        assert!(!ctx.card(UDYR).unwrap().exhausted);
        assert!(!ctx.is_buffed(UDYR));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == UDYR
        )));
        ctx.buff(UDYR);
        assert!(!ctx.has_keyword(UDYR, Keyword::Ganking));
        activate::activate(&mut ctx, 0, UDYR, GANKING).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.has_keyword(UDYR, Keyword::Ganking));
        assert!(!ctx.is_buffed(UDYR));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {UDYR}}} has Ganking this turn")));
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(!ctx.has_keyword(UDYR, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unbuffed_he_offers_nothing_and_every_mode_is_refused() {
        let mut fixture = wilds(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert!(his_offers(&ctx).is_empty());
        for mode in [DEAL, STUN, READY, GANKING] {
            assert_eq!(
                activate::activate(&mut ctx, 0, UDYR, mode),
                Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
                "mode {mode} needs his buff"
            );
        }
        assert_eq!(
            activate::activate(&mut ctx, 1, UDYR, READY),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: the modes a card has used this turn; a mode chosen this turn must be refused even after he is buffed again"]
    fn a_mode_chosen_this_turn_is_refused_when_he_is_buffed_again() {
        let mut fixture = wilds(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, UDYR, READY).unwrap();
        resolve_chain(&mut ctx);
        ctx.buff(UDYR);
        assert_eq!(
            activate::activate(&mut ctx, 0, UDYR, READY),
            Err(Refusal::Illegal(Reason::AlreadyActivated))
        );
        assert!(activate::activate(&mut ctx, 0, UDYR, GANKING).is_ok());
    }
}

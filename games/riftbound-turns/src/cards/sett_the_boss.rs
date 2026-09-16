use super::prelude::{
    done, legend, on_conquer, ready, recall, replaces, with_replacement, RAINBOW,
};
use super::sett_brawler::spend_buff;
use super::{Card, Flow, Item, Source, Stage, WouldDie};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::pay;

pub fn rainbow_for(ctx: &Ctx, legend: u32) -> cost::Cost {
    cost::of_script(&RAINBOW, &ctx.domains_of(legend))
}

pub fn a_buffed_friendly_unit_would_die_and_the_boss_can_pay(
    ctx: &Ctx,
    would: &WouldDie,
    source: Source,
) -> bool {
    let seat = ctx.controller(source.card);
    ctx.is_unit(would.unit)
        && ctx.on_board(would.unit)
        && ctx.controller(would.unit) == seat
        && ctx.is_buffed(would.unit)
        && ctx.card(source.card).is_some_and(|held| !held.exhausted)
        && pay::affordable(ctx, seat, &rainbow_for(ctx, source.card))
}

pub fn pay_and_recall_it_exhausted(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    let seat = ctx.controller(source.card);
    let Ok(plan) = pay::plan(ctx, seat, &rainbow_for(ctx, source.card)) else {
        return;
    };
    if !ctx.exhaust(source.card) {
        return;
    }
    pay::pay(ctx, seat, &plan);
    spend_buff(ctx, would.unit);
    recall(ctx, would.unit, true);
    ctx.narrate(format!(
        "{{card {}}} is recalled exhausted instead of dying",
        would.unit
    ));
}

fn ready_me(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = with_replacement(
    legend("Sett - The Boss", &[], &[on_conquer(&[], ready_me)]),
    replaces(
        a_buffed_friendly_unit_would_die_and_the_boss_can_pay,
        pay_and_recall_it_exhausted,
    ),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::{Cause, Event, Killed, Location, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{kill, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const BOSS: u32 = fixtures::LEGEND_CARD;
    const THUG: u32 = 90;

    fn source() -> Source {
        Source {
            card: BOSS,
            ability: 0,
        }
    }

    fn would(unit: u32) -> WouldDie {
        WouldDie {
            unit,
            cause: Cause::Combat,
        }
    }

    fn arena(buffed: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let boss = fixture.table.card_mut(BOSS).unwrap();
        boss.name = "Sett - The Boss".into();
        boss.domain = vec!["Body".into(), "Order".into()];
        fixture
            .table
            .cards
            .push(fixtures::unit(THUG, fixtures::BF1, 0, "Pit Rookie", 2));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BOSS).unwrap(), &CARD));
        if buffed {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(THUG),
                counter: COUNTER_BUFFED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture
    }

    #[test]
    fn the_script_is_a_conquer_trigger_that_readies_him_and_a_priced_replacement() {
        assert!(std::ptr::eq(script_of("Sett - The Boss").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_some());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::You));
        assert!(conquer.targets.is_empty());
        assert!(!conquer.optional);
        assert!(conquer.cost.is_none());
    }

    #[test]
    fn the_replacement_applies_to_a_buffed_friendly_unit_while_he_is_ready_and_a_rune_can_pay() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        assert!(a_buffed_friendly_unit_would_die_and_the_boss_can_pay(
            &ctx,
            &would(THUG),
            source()
        ));
        assert!(
            !a_buffed_friendly_unit_would_die_and_the_boss_can_pay(
                &ctx,
                &would(fixtures::VI),
                source()
            ),
            "Vi is not buffed"
        );
        ctx.buff(fixtures::THEIR_UNIT);
        assert!(
            !a_buffed_friendly_unit_would_die_and_the_boss_can_pay(
                &ctx,
                &would(fixtures::THEIR_UNIT),
                source()
            ),
            "not a unit he controls"
        );
        ctx.exhaust(BOSS);
        assert!(
            !a_buffed_friendly_unit_would_die_and_the_boss_can_pay(&ctx, &would(THUG), source()),
            "exhausting him is part of the cost"
        );
        drop(ctx);
        let mut broke = arena(true);
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            broke.table.cards.retain(|card| card.id != id);
        }
        let ctx = broke.ctx();
        assert!(
            !a_buffed_friendly_unit_would_die_and_the_boss_can_pay(&ctx, &would(THUG), source()),
            "no rune, no rainbow"
        );
    }

    #[test]
    fn the_run_pays_a_rune_exhausts_him_spends_the_buff_and_recalls_the_unit_exhausted() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        pay_and_recall_it_exhausted(&mut ctx, &would(THUG), source());
        assert!(ctx.card(BOSS).unwrap().exhausted);
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "one rune recycled for the rainbow"
        );
        assert!(!ctx.is_buffed(THUG));
        assert_eq!(ctx.location(THUG), Some(Location::Base(0)));
        assert!(ctx.card(THUG).unwrap().exhausted);
        assert!(ctx.on_board(THUG), "recalled, not dead");
        assert!(ctx.effects.contains(&Effect::Move {
            card: THUG,
            zone: fixtures::BASE,
            seat: 0,
            index: agni_plugin_sdk::decide::TOP
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {THUG}}} is recalled exhausted instead of dying"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn conquering_readies_him_when_the_trigger_resolves() {
        let mut fixture = arena(false);
        fixture.table.card_mut(BOSS).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BOSS
        ));
        assert!(ctx.card(BOSS).unwrap().exhausted, "not before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(BOSS).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == BOSS
        )));
        assert!(ctx.blob.log.contains(&format!("{{card {BOSS}}} readies")));
        drop(ctx);
        let mut theirs = arena(false);
        theirs.table.card_mut(THUG).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_holder(fixtures::BF1, None);
        theirs.blob.set_contested(fixtures::BF1, Some(1));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            Established::Conquered(1)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the enemy's conquer is not his");
    }

    #[test]
    #[ignore = "engine gap · kill::applicable lists faces on board only, so a legend's replacement is never consulted; and the may (pay a rune and exhaust him) needs a prompt the kill path lacks"]
    fn a_buffed_friendly_unit_dying_is_offered_the_recall_for_a_rune_and_his_exhaust() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill::applicable(&ctx, &would(THUG)),
            [source()],
            "his replacement is on the list"
        );
        assert_eq!(ctx.kill(THUG, Cause::Combat), Killed::Replaced);
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.on_board(THUG));
        assert!(ctx.card(BOSS).unwrap().exhausted);
    }
}

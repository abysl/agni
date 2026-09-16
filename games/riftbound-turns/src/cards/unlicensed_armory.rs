use super::prelude::{
    a_friendly_unit, activated, ask_discard, at_end_of_turn, card_target, done, exhausting_self,
    gear, named, recall, replaces, triggered, usable_if, with_replacement,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage, Timing, Trigger, WouldDie};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay};
use crate::state::{TargetRef, When};

pub const FURY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};
pub const ARM: u8 = 0;
pub const LAPSE: u8 = 1;
pub const DISCARDED: u8 = 1;

pub fn can_discard_one(ctx: &Ctx, source: Source) -> bool {
    !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

pub fn watches(ctx: &Ctx, armory: u32, unit: u32) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == armory
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit)
    })
}

fn spend_watch(ctx: &mut Ctx, armory: u32, unit: u32) {
    let turn = ctx.turn();
    let mut spent = false;
    ctx.blob.delayed.retain(|delayed| {
        let hit = !spent
            && delayed.source == armory
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit);
        if hit {
            spent = true;
        }
        !hit
    });
}

pub fn fury_payable(ctx: &Ctx, seat: u8) -> bool {
    pay::affordable(ctx, seat, &cost::of_script(&FURY, &[]))
}

pub fn may_pay_fury_to_recall(ctx: &mut Ctx, seat: u8, unit: u32) -> bool {
    let total = cost::of_script(&FURY, &[]);
    let Ok(plan) = pay::plan(ctx, seat, &total) else {
        return false;
    };
    pay::pay(ctx, seat, &plan);
    recall(ctx, unit, true);
    ctx.narrate(format!(
        "{{seat {seat}}} pays a Fury power · {{card {unit}}} is recalled exhausted instead of dying"
    ));
    true
}

fn arm(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 != DISCARDED {
        return match ask_discard(ctx, item, DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {}}} has nothing to discard · no unit is chosen",
                    item.kind.source()
                ));
                done()
            }
        };
    }
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    at_end_of_turn(ctx, item, LAPSE, vec![unit]);
    ctx.narrate(format!(
        "the next time {{card {unit}}} dies this turn, {{seat {}}} may pay a Fury power to recall it exhausted instead",
        item.controller
    ));
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(TargetRef::Card(unit)) = item.targets.first() {
        ctx.narrate(format!("the watch on {{card {unit}}} lapses"));
    }
    done()
}

fn watched_unit_would_die(ctx: &Ctx, would: &WouldDie, source: Source) -> bool {
    ctx.is_unit(would.unit)
        && ctx.on_board(would.unit)
        && watches(ctx, source.card, would.unit)
        && fury_payable(ctx, ctx.controller(source.card))
}

fn recall_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    let seat = ctx.controller(source.card);
    spend_watch(ctx, source.card, would.unit);
    may_pay_fury_to_recall(ctx, seat, would.unit);
}

pub static CARD: Card = with_replacement(
    gear(
        "Unlicensed Armory",
        &[],
        &[
            named(
                usable_if(
                    exhausting_self(activated(
                        Timing::Sorcery,
                        Cost::FREE,
                        &[a_friendly_unit("a friendly unit to watch this turn")],
                        arm,
                    )),
                    can_discard_one,
                ),
                "discard 1: watch a friendly unit this turn",
            ),
            triggered(Trigger::Reflexive, &[], lapse),
        ],
    ),
    replaces(watched_unit_would_die, recall_instead),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, phases, priority, settle, showdown};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;

    const ARMORY: u32 = 90;
    const RECRUIT: u32 = 91;
    const JINX: u32 = 92;

    fn forge() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut armory = fixtures::gear(ARMORY, fixtures::BASE, 0, "Unlicensed Armory", 2);
        armory.domain = vec!["Fury".into()];
        fixture.table.cards.push(armory);
        fixture
            .table
            .cards
            .push(fixtures::unit(RECRUIT, fixtures::BF1, 0, "Recruit", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn arm_recruit(ctx: &mut Ctx) {
        activate::activate(ctx, 0, ARMORY, ARM).unwrap();
        fixtures::choose(ctx, 0, &format!("{{card {RECRUIT}}}")).unwrap();
        resolve_chain(ctx);
        fixtures::choose(ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
    }

    #[test]
    fn the_script_is_a_gated_exhaust_activation_a_lapse_trigger_and_a_replacement() {
        let fixture = forge();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ARMORY).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Unlicensed Armory");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let arm = &CARD.abilities[usize::from(ARM)];
        assert_eq!(arm.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(arm.self_cost, SelfCost::Exhaust);
        assert_eq!(arm.cost, Some(Cost::FREE));
        assert!(
            arm.usable.is_some(),
            "no card in hand, no discard, no activation"
        );
        assert_eq!(arm.targets[0].filter, crate::cards::prelude::FRIENDLY_UNIT);
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
        assert!(CARD.replacement.is_some());
    }

    #[test]
    fn arming_discards_one_at_resolution_and_the_watch_lives_in_the_delayed_list_until_the_turn_ends(
    ) {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, ARMORY, ARM).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {RECRUIT}}}")).unwrap();
        assert!(ctx.card(ARMORY).unwrap().exhausted);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the discard waits for resolution"
        );
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: DISCARDED
            })
        );
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        assert!(
            !watches(&ctx, ARMORY, RECRUIT),
            "nothing before the discard"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(watches(&ctx, ARMORY, RECRUIT));
        assert!(!watches(&ctx, ARMORY, fixtures::VI));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(&format!(
            "the next time {{card {RECRUIT}}} dies this turn, {{seat 0}} may pay a Fury power to recall it exhausted instead"
        )));
        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(
            ctx.blob.delayed.is_empty(),
            "the watch lapses with the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("the watch on {{card {RECRUIT}}} lapses")));
        assert!(!watches(&ctx, ARMORY, RECRUIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_watched_units_next_death_pays_a_fury_power_and_recalls_it_exhausted_once() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        arm_recruit(&mut ctx);
        let runes = ctx.runes_of(0).len();
        assert_eq!(ctx.kill(RECRUIT, Cause::Item(9)), Killed::Replaced);
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(RECRUIT));
        assert_eq!(ctx.location(RECRUIT), Some(Location::Base(0)));
        assert!(ctx.card(RECRUIT).unwrap().exhausted, "recalled exhausted");
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "the Fury power recycles a rune"
        );
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == RECRUIT
        )));
        assert!(!watches(&ctx, ARMORY, RECRUIT), "the next time only");
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} pays a Fury power · {{card {RECRUIT}}} is recalled exhausted instead of dying"
        )));
        assert_eq!(ctx.kill(RECRUIT, Cause::Item(10)), Killed::Yes);
        assert!(!ctx.on_board(RECRUIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_combat_death_of_the_watched_unit_is_replaced_and_the_recalled_unit_is_healed_with_the_rest(
    ) {
        let mut fixture = forge();
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 1, "Jinx", 3));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        arm_recruit(&mut ctx);
        ctx.blob.set_contested(fixtures::BF1, Some(1));
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none(), "the combat resolved");
        assert_eq!(
            ctx.location(RECRUIT),
            Some(Location::Base(0)),
            "the 2 would die to the 3 and is recalled instead"
        );
        assert!(ctx.card(RECRUIT).unwrap().exhausted);
        assert_eq!(ctx.damage_on(RECRUIT), 0, "3c heals every unit");
        assert!(ctx.on_board(JINX), "the 3 took 2 and lived");
        assert!(!watches(&ctx, ARMORY, RECRUIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_fury_power_to_pay_the_watched_unit_simply_dies_and_an_unwatched_unit_is_never_saved(
    ) {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        arm_recruit(&mut ctx);
        assert_eq!(
            ctx.kill(fixtures::VI, Cause::Item(9)),
            Killed::Yes,
            "Vi was not chosen"
        );
        for rune in [fixtures::RUNE_A, 41, 43] {
            ctx.recycle_to_bottom(rune);
        }
        assert!(!fury_payable(&ctx, 0), "only the Calm rune is left");
        assert_eq!(ctx.kill(RECRUIT, Cause::Item(9)), Killed::Yes);
        assert!(!ctx.on_board(RECRUIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_the_armory_cannot_be_activated_and_the_other_seat_is_refused() {
        let mut fixture = forge();
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::HAND) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!can_discard_one(
            &ctx,
            Source {
                card: ARMORY,
                ability: ARM
            }
        ));
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMORY, ARM),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses with the engine's one gate reason"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, ARMORY, ARM),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(ARMORY).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "the discard is a base cost paid at the pay stage (204.1.b) and the Fury payment is a may on the kill path, which has no prompt; both are the non-resource-cost and kill-path-prompt engine gaps"]
    fn the_discard_is_paid_before_the_ability_reaches_the_chain_and_the_fury_is_asked_for() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, ARMORY, ARM).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {RECRUIT}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "paid with the exhaust");
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        ctx.kill(RECRUIT, Cause::Item(9));
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!ctx.on_board(RECRUIT), "declined, it dies");
    }
}

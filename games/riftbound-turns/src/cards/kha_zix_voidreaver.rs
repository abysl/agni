use super::prelude::{
    a_card, a_unit, activated, buff, card_target, done, exhausting_self, gain_xp, legend,
    move_unit, named, on_combat_won, spending_xp, Location,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const WIN_XP: u8 = 1;
pub const BUFF_XP: u8 = 1;
pub const RECALL_XP: u8 = 2;

pub const EXHAUSTED_FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Exhausted,
    Filter::AtBattlefield,
    Filter::MovableToBase,
]);

fn won(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, WIN_XP);
    done()
}

fn buff_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        buff(ctx, unit);
    }
    done()
}

fn send_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let home = Location::Base(ctx.controller(unit));
        move_unit(ctx, item, unit, home);
    }
    done()
}

pub static CARD: Card = legend(
    "Kha'Zix - Voidreaver",
    &[],
    &[
        on_combat_won(&[], won),
        named(
            spending_xp(
                exhausting_self(activated(
                    Timing::Sorcery,
                    Cost::FREE,
                    &[a_unit("a unit to buff")],
                    buff_unit,
                )),
                BUFF_XP,
            ),
            "buff a unit",
        ),
        named(
            spending_xp(
                exhausting_self(activated(
                    Timing::Sorcery,
                    Cost::FREE,
                    &[a_card(
                        EXHAUSTED_FRIENDLY_UNIT_AT_A_BATTLEFIELD,
                        "an exhausted friendly unit to move to its base",
                    )],
                    send_home,
                )),
                RECALL_XP,
            ),
            "move an exhausted unit to its base",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play, priority, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;

    const KHA: u32 = fixtures::LEGEND_CARD;
    const SCOUT: u32 = 90;

    fn hive(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        let mut scout = fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2);
        scout.exhausted = true;
        fixture.table.cards.push(scout);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(KHA, &CARD);
        fixture
    }

    fn labels(offers: &[activate::Offer]) -> Vec<(String, bool)> {
        offers
            .iter()
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn the_legend_wins_combats_for_xp_and_spends_it_on_two_exhaust_abilities() {
        assert_eq!(CARD.name, "Kha'Zix - Voidreaver");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::CombatWon(Who::You));
        assert_eq!(CARD.abilities[0].xp, 0);
        let buff = &CARD.abilities[1];
        assert_eq!(buff.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(buff.xp, BUFF_XP);
        assert_eq!(buff.self_cost, SelfCost::Exhaust);
        assert_eq!(buff.cost, Some(Cost::FREE));
        assert_eq!(buff.label, Some("buff a unit"));
        let home = &CARD.abilities[2];
        assert_eq!(home.xp, RECALL_XP);
        assert_eq!(home.self_cost, SelfCost::Exhaust);
        assert_eq!(
            home.targets[0].filter,
            EXHAUSTED_FRIENDLY_UNIT_AT_A_BATTLEFIELD
        );
        let mut fixture = hive(0);
        let ctx = fixture.ctx();
        let cost = cost::of_activation(&ctx, KHA, 1);
        assert_eq!(cost.xp, 1);
        assert_eq!(cost.label(), "1 XP");
        assert_eq!(cost::of_activation(&ctx, KHA, 2).label(), "2 XP");
    }

    #[test]
    fn the_offers_are_greyed_without_xp_and_a_direct_activation_is_refused() {
        let mut fixture = hive(0);
        let mut ctx = fixture.ctx();
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [
                (
                    format!("{{card {KHA}}}: buff a unit (1 XP, exhaust)"),
                    false
                ),
                (
                    format!("{{card {KHA}}}: move an exhausted unit to its base (2 XP, exhaust)"),
                    false
                ),
            ],
            "the strip lists both, greyed"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, KHA, 1),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, KHA, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        drop(ctx);
        let mut one = hive(1);
        let ctx = one.ctx();
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [
                (format!("{{card {KHA}}}: buff a unit (1 XP, exhaust)"), true),
                (
                    format!("{{card {KHA}}}: move an exhausted unit to its base (2 XP, exhaust)"),
                    false
                ),
            ],
            "one XP enables the buff alone"
        );
    }

    #[test]
    fn the_buff_spends_one_xp_at_finalization_and_buffs_the_unit_when_it_resolves() {
        let mut fixture = hive(1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, KHA, 1).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 })
        ));
        assert_eq!(ctx.xp(0), 1, "358.5 · nothing is spent before the plan");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.xp(0), 0, "the XP lands with the rest of the cost");
        assert!(ctx.card(KHA).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.is_buffed(fixtures::VI), "nothing until it resolves");
        assert!(ctx.blob.log.contains(&"{seat 0} spends 1 XP".to_string()));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, KHA, 1),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
    }

    #[test]
    fn a_cancelled_activation_leaves_the_xp_untouched() {
        let mut fixture = hive(1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, KHA, 1).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert_eq!(ctx.xp(0), 1);
        assert!(!ctx.card(KHA).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn two_xp_move_an_exhausted_friendly_unit_from_a_battlefield_to_its_base() {
        let mut fixture = hive(2);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, KHA, 2).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {SCOUT}}}"), "cancel".to_string()],
            "Vi is ready and at the base, the Scout is the one candidate"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(ctx.xp(0), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.location(SCOUT), Some(Location::Base(0)));
        assert!(
            ctx.card(SCOUT).unwrap().exhausted,
            "a move keeps the unit exhausted"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Base(0), .. } if *card == SCOUT
        )));
    }

    #[test]
    fn winning_a_combat_gains_one_xp_when_the_trigger_resolves() {
        let mut fixture = hive(0);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger waits on the chain");
        assert_eq!(ctx.xp(0), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 1,
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the other seat's win is not the legend's"
        );
        assert_eq!(ctx.xp(0), 1);
    }
}

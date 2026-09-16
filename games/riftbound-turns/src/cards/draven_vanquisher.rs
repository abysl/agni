use super::prelude::{
    done, might_this_turn, on_attack, on_defend, optional, spawn_gold, triggered, unit, with_cost,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;
pub const GOLD_ENTERS_READY: bool = false;
pub const FURY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

fn spoils(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ENTERS_READY);
    done()
}

fn spin(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Draven - Vanquisher",
    &[],
    &[
        triggered(Trigger::CombatWon(Who::Me), &[], spoils),
        optional(with_cost(on_attack(&[], spin), FURY)),
        optional(with_cost(on_defend(&[], spin), FURY)),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, prompts, settle, triggers};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::Effect;

    const DRAVEN: u32 = 90;

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut draven = fixtures::unit(DRAVEN, fixtures::BF1, 0, "Draven - Vanquisher", 4);
        draven.domain = vec!["Fury".into()];
        draven.energy = Some(4);
        fixture.table.cards.push(draven);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRAVEN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn fury_runes_recycled(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        card,
                        zone: fixtures::RUNE_DECK,
                        seat: 0,
                        ..
                    } if [fixtures::RUNE_A, 41, 43].contains(card)
                )
            })
            .count()
    }

    #[test]
    fn the_script_is_a_unit_with_a_combat_won_gold_and_a_fury_costed_pump_on_attack_and_on_defend()
    {
        assert!(std::ptr::eq(
            script_of("Draven - Vanquisher").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 3);
        let gold = &CARD.abilities[0];
        assert_eq!(gold.trigger, Trigger::CombatWon(Who::Me));
        assert!(!gold.optional && gold.cost.is_none());
        let attack = &CARD.abilities[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        assert!(attack.optional);
        assert_eq!(attack.cost, Some(FURY));
        let defend = &CARD.abilities[2];
        assert_eq!(defend.trigger, Trigger::Defends(Who::Me));
        assert!(defend.optional);
        assert_eq!(defend.cost, Some(FURY));
        assert!(CARD
            .abilities
            .iter()
            .all(|ability| ability.targets.is_empty()));
    }

    #[test]
    fn winning_a_combat_where_he_stands_plays_an_exhausted_gold_at_his_base() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAVEN
        ));
        let next = ctx.table.next_id;
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(next).unwrap().name, "Gold");
        assert_eq!(ctx.location(next), Some(Location::Base(0)));
        assert!(ctx.is_token(next));
        assert!(ctx.card(next).unwrap().exhausted, "played exhausted");
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF2,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "466.3.c · not his combat");
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 1,
        });
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "not his win");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn attacking_asks_for_the_fury_and_paying_it_gives_him_two_might_this_turn() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: DRAVEN });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "pay 1 Fury power for the {card 90} trigger?"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(fury_runes_recycled(&ctx), 1, "{:?}", ctx.effects);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == DRAVEN
        ));
        assert_eq!(ctx.current_might(DRAVEN), 4, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(DRAVEN), 6);
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(DRAVEN), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn defending_asks_the_same_and_declining_or_lacking_fury_pumps_nothing() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Defends { card: DRAVEN });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, .. })
        ));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        assert_eq!(fury_runes_recycled(&ctx), 0);
        assert_eq!(ctx.current_might(DRAVEN), 4);
        drop(ctx);

        let mut poor = arena();
        for rune in [fixtures::RUNE_A, 41, 43] {
            poor.table.card_mut(rune).unwrap().domain = vec!["Calm".into()];
        }
        poor.resolve();
        let mut ctx = poor.ctx();
        ctx.raise(Event::Defends { card: DRAVEN });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        assert_eq!(ctx.current_might(DRAVEN), 4);
    }
}

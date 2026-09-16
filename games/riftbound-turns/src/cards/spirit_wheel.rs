use super::prelude::{
    done, draw, exhausting_self, gear, on_friendly_unit_chosen, optional, with_cost, ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn spin(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = gear(
    "Spirit Wheel",
    &[],
    &[optional(exhausting_self(with_cost(
        on_friendly_unit_chosen(&[], spin),
        ONE_ENERGY,
    )))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, play, spell};
    use crate::cards::{script_of, Keyword, SelfCost, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use agni_plugin_sdk::table::CardInfo;

    const WHEEL: u32 = 90;
    const BUFF: u32 = 91;
    const THEIR_BUFF: u32 = 92;

    static BUFF_CARD: Card = spell(
        "Buff",
        &[Keyword::Action],
        &[play(&[a_unit("a unit")], |_, _, _| Flow::Done)],
    );

    fn wheel(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(WHEEL, fixtures::BASE, 0, "Spirit Wheel", 2)
        }
    }

    fn shrine(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wheel(exhausted));
        fixture
            .table
            .cards
            .push(fixtures::spell(BUFF, fixtures::HAND, 0, "Buff", 1, 0));
        let mut theirs = fixtures::spell(THEIR_BUFF, fixtures::HAND, 1, "Buff", 1, 0);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BUFF, &BUFF_CARD)
            .with_script(THEIR_BUFF, &BUFF_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(WHEEL).unwrap(), &CARD));
        fixture
    }

    fn wheel_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == WHEEL))
            .map(|item| item.id)
            .collect()
    }

    fn cast_at(ctx: &mut Ctx, seat: u8, spell: u32, target: u32) {
        fixtures::play_from_hand(ctx, seat, spell).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "{:?} {:?} {:?} {:?} {:?}",
            ctx.blob.why,
            ctx.blob.log,
            ctx.blob.chain,
            ctx.blob.queue,
            ctx.blob.prompt
        );
        fixtures::choose(ctx, seat, &format!("{{card {target}}}")).unwrap();
    }

    #[test]
    fn the_script_is_an_optional_one_energy_exhaust_trigger_on_choosing_a_friendly_unit() {
        assert!(std::ptr::eq(script_of("Spirit Wheel").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::ChosenFriendly(Who::Friendly));
        assert!(ability.optional, "you may pay");
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn choosing_your_own_unit_with_a_spell_asks_for_the_energy_and_the_exhaust_then_draws_one() {
        let mut fixture = shrine(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, 0, BUFF, fixtures::VI);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == fixtures::VI
        )));
        assert_eq!(wheel_items(&ctx).len(), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: wheel_items(&ctx)[0],
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert!(!ctx.card(WHEEL).unwrap().exhausted, "nothing until yes");
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(WHEEL).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "one ready rune pays the energy"
        );
        assert_eq!(ctx.blob.chain.len(), 2, "the spell and the Wheel's trigger");
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "not before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand, "the spell left, a card came");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_cost_leaves_the_wheel_ready_and_draws_nothing() {
        let mut fixture = shrine(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, 0, BUFF, fixtures::VI);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.card(WHEEL).unwrap().exhausted);
        assert!(wheel_items(&ctx).is_empty());
        assert_eq!(ctx.blob.chain.len(), 1, "only the spell");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {WHEEL}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn an_enemy_unit_chosen_by_you_and_your_unit_chosen_by_the_opponent_wake_nothing() {
        let mut fixture = shrine(false);
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, 0, BUFF, fixtures::THEIR_UNIT);
        assert!(
            wheel_items(&ctx).is_empty(),
            "an enemy unit is not friendly"
        );
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(&mut ctx);
        drop(ctx);
        let mut fixture = shrine(false);
        fixture.blob.core_mut().unwrap().advance();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        cast_at(&mut ctx, 1, THEIR_BUFF, fixtures::VI);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 1, .. } if *card == fixtures::VI
        )));
        assert!(
            wheel_items(&ctx).is_empty(),
            "the opponent choosing your unit is not you choosing"
        );
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn a_spent_wheel_and_an_empty_rune_pool_are_declined_without_asking() {
        let mut fixture = shrine(true);
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, 0, BUFF, fixtures::VI);
        assert!(
            ctx.blob.prompt.is_none(),
            "a spent Wheel has nothing to offer"
        );
        assert!(wheel_items(&ctx).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {WHEEL}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut fixture = shrine(false);
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BUFF, &BUFF_CARD)
            .with_script(THEIR_BUFF, &BUFF_CARD);
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, 0, BUFF, fixtures::VI);
        assert!(
            ctx.blob.prompt.is_none(),
            "the spell took the last ready rune: no energy for the Wheel"
        );
        assert!(wheel_items(&ctx).is_empty());
        assert!(!ctx.card(WHEEL).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {WHEEL}}} trigger is removed · its cost can't be paid"
        )));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.fault.is_none());
    }
}

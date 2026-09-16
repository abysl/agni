use super::prelude::{done, exhausting_self, gear, optional, spawn_gold, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub const GOLD_ARRIVES_READY: bool = false;

pub fn on_an_opponents_turn(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(event, Event::PlayedSpell { .. }) && ctx.turn_player() != ctx.controller(source.card)
}

fn brew(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = gear(
    "Chemtech Cask",
    &[],
    &[when(
        optional(exhausting_self(triggered(Trigger::YouPlaySpell, &[], brew))),
        on_an_opponents_turn,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::spell;
    use crate::cards::{script_of, Keyword, SelfCost, TOKEN_GOLD};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use agni_plugin_sdk::table::CardInfo;

    const CASK: u32 = 90;
    const REPLY: u32 = 91;
    const THEIR_SPELL: u32 = 92;

    static REPLY_CARD: Card = spell("Reply", &[Keyword::Reaction], &[]);
    static NUDGE_CARD: Card = spell("Nudge", &[], &[]);

    fn cask(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(CASK, zone, 0, "Chemtech Cask", 1)
        }
    }

    fn laboratory(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cask(fixtures::BASE, exhausted));
        fixture
            .table
            .cards
            .push(fixtures::spell(REPLY, fixtures::HAND, 0, "Reply", 1, 0));
        let mut theirs = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Nudge", 1, 0);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(REPLY, &REPLY_CARD)
            .with_script(THEIR_SPELL, &NUDGE_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(CASK).unwrap(), &CARD));
        fixture
    }

    fn on_their_turn(exhausted: bool) -> Fixture {
        let mut fixture = laboratory(exhausted);
        fixture.blob.core_mut().unwrap().advance();
        fixture
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn cask_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == CASK))
            .map(|item| item.id)
            .collect()
    }

    fn reply_on_their_turn(ctx: &mut Ctx) {
        assert_eq!(ctx.turn_player(), 1);
        fixtures::play_from_hand(ctx, 1, THEIR_SPELL).unwrap();
        priority::pass(ctx, 1).unwrap();
        fixtures::play_from_hand(ctx, 0, REPLY).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_optional_exhaust_trigger_on_your_spells_gated_on_the_turn() {
        assert!(std::ptr::eq(script_of("Chemtech Cask").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlaySpell);
        assert!(ability.optional, "you may exhaust me");
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn the_condition_reads_the_turn_player_not_the_spell() {
        let mut fixture = laboratory(false);
        let ctx = fixture.ctx();
        let source = Source {
            card: CASK,
            ability: 0,
        };
        let played = Event::PlayedSpell {
            item: 3,
            controller: 0,
            nth: 1,
        };
        assert!(
            !on_an_opponents_turn(&ctx, &played, source),
            "the Cask's controller's own turn"
        );
        assert!(!on_an_opponents_turn(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source
        ));
        drop(ctx);
        let mut fixture = on_their_turn(false);
        let ctx = fixture.ctx();
        assert!(on_an_opponents_turn(&ctx, &played, source));
        assert!(!on_an_opponents_turn(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source
        ));
    }

    #[test]
    fn a_reaction_on_the_opponents_turn_asks_to_exhaust_the_cask_and_yes_brews_an_exhausted_gold() {
        let mut fixture = on_their_turn(false);
        let mut ctx = fixture.ctx();
        reply_on_their_turn(&mut ctx);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the reply resolved, their spell waits"
        );
        assert_eq!(cask_items(&ctx).len(), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: cask_items(&ctx)[0],
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert!(!ctx.card(CASK).unwrap().exhausted, "nothing until yes");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(CASK).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the trigger sits above their spell"
        );
        assert!(golds_of(&ctx, 0).is_empty(), "the Gold waits for the chain");
        let next = ctx.table.next_id;
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(golds_of(&ctx, 0), [next]);
        assert!(ctx.is_token(next));
        assert!(ctx.is_gear(next));
        assert!(ctx.card(next).unwrap().exhausted, "played exhausted");
        assert_eq!(ctx.card(next).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.controller(next), 0);
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(ctx.blob.chain.len(), 1, "their spell is still there");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_keeps_the_cask_ready_and_brews_nothing() {
        let mut fixture = on_their_turn(false);
        let mut ctx = fixture.ctx();
        reply_on_their_turn(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.card(CASK).unwrap().exhausted);
        assert!(cask_items(&ctx).is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CASK}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn a_spell_on_your_own_turn_and_an_exhausted_cask_brew_nothing() {
        let mut fixture = laboratory(false);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 0);
        fixtures::play_from_hand(&mut ctx, 0, REPLY).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            cask_items(&ctx).is_empty(),
            "your own turn is not an opponent's"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(golds_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = on_their_turn(true);
        let mut ctx = fixture.ctx();
        reply_on_their_turn(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "a spent Cask has nothing to offer"
        );
        assert!(cask_items(&ctx).is_empty());
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CASK}}} trigger is removed · its source is exhausted"
        )));
        assert!(ctx.fault.is_none());
    }
}

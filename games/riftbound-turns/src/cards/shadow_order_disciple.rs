use super::prelude::{burning, done, might_this_turn, on_move, optional, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const BURN: u8 = 1;
pub const MIGHT: i16 = 1;

fn sharpen(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
    }
    done()
}

pub static CARD: Card = unit(
    "Shadow Order Disciple",
    &[],
    &[burning(optional(on_move(&[], sharpen)), BURN)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger, Where, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use agni_plugin_sdk::table::CardInfo;

    const DISCIPLE: u32 = 90;
    const DECK_TOP: u32 = 23;

    fn disciple() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(DISCIPLE, fixtures::BASE, 0, "Shadow Order Disciple", 2)
        }
    }

    fn dojo(deck: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(disciple());
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        if !deck {
            fixture
                .table
                .cards
                .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        }
        fixture.resolve();
        fixture
    }

    fn march(ctx: &mut Ctx) {
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(ctx, 0, &entry).unwrap();
        act(ctx, 0, intent).unwrap();
        settle(ctx).unwrap();
    }

    fn burned(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Burned { seat: 0, card } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_disciple_watches_its_own_moves_with_an_optional_burn_cost() {
        let fixture = dojo(true);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DISCIPLE).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.optional);
        assert_eq!(ability.burn, BURN);
        assert_eq!(ability.xp, 0);
        assert_eq!(ability.self_cost, SelfCost::Auto);
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn a_march_asks_to_burn_and_yes_burns_the_top_card_then_grants_the_might() {
        let mut fixture = dojo(true);
        let action = fixtures::move_action(DISCIPLE, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("burn 1 for the {{card {DISCIPLE}}} trigger?")
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert!(burned(&ctx).is_empty(), "nothing burns before the answer");
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            burned(&ctx),
            [DECK_TOP],
            "the top card burns at finalization"
        );
        assert_eq!(ctx.card(DECK_TOP).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), deck - 1);
        assert!(ctx.blob.log.contains(&"{seat 0} burns 1".to_string()));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DISCIPLE
        ));
        assert_eq!(
            ctx.current_might(DISCIPLE),
            2,
            "the Might waits for the chain"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(DISCIPLE), 3);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn no_removes_the_trigger_and_burns_nothing() {
        let mut fixture = dojo(true);
        let action = fixtures::move_action(DISCIPLE, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(burned(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.current_might(DISCIPLE), 2);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DISCIPLE}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn an_empty_deck_removes_the_trigger_without_asking() {
        let mut fixture = dojo(false);
        let action = fixtures::move_action(DISCIPLE, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "440.4 · a Burn that cannot be paid is not offered"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(burned(&ctx).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DISCIPLE}}} trigger is removed · its cost can't be paid"
        )));
    }
}

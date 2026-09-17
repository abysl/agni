use super::prelude::{done, draw, on_move, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit("Stellacorn Herder", &[], &[on_move(&[], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::ctx::{Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, legal, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Needs, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const HERDER: u32 = 90;
    const SECOND_HERDER: u32 = 91;
    const THEIR_HERDER: u32 = 92;
    const TOP_OF_DECK: u32 = 23;
    const NEXT_CARD: u32 = 22;

    fn herder(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Stellacorn Herder", 3);
        card.domain = vec!["Calm".into()];
        card.energy = Some(4);
        card
    }

    fn pasture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herder(HERDER, fixtures::BASE, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn ranged() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herder(HERDER, fixtures::BF1, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn herd() -> Fixture {
        let mut fixture = pasture();
        fixture
            .table
            .cards
            .push(herder(SECOND_HERDER, fixtures::BASE, 0));
        fixture.resolve();
        fixture
    }

    fn march(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
        let entry = ctx.entry.ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
        let intent = legal::classify(ctx, seat, &entry)?;
        act(ctx, seat, intent)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn refuse(fixture: &mut Fixture, seat: u8, card: u32, to: u16) -> Refusal {
        let action = fixtures::move_action(card, to, seat);
        let ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        legal::classify(&ctx, seat, &entry).expect_err("the march is refused")
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn sources_on_chain(ctx: &Ctx) -> Vec<u32> {
        ctx.blob
            .chain
            .iter()
            .map(|item| item.kind.source())
            .collect()
    }

    fn drawn(ctx: &Ctx, card: u32, seat: u8) -> bool {
        ctx.effects.contains(&Effect::Move {
            card,
            zone: fixtures::HAND,
            seat,
            index: TOP,
        })
    }

    fn moves(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Moved { .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_only_ability_triggers_on_any_move_of_itself() {
        assert_eq!(CARD.name, "Stellacorn Herder");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(std::ptr::eq(
            super::super::script_of("Stellacorn Herder").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn a_standard_move_to_a_battlefield_puts_the_trigger_on_the_chain_and_draws_one_when_it_resolves(
    ) {
        let mut fixture = pasture();
        let action = fixtures::move_action(HERDER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no other ready unit, so no group move prompt"
        );
        assert_eq!(
            ctx.location(HERDER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.effects.contains(&Effect::exhaust(HERDER)));
        assert_eq!(sources_on_chain(&ctx), [HERDER]);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HERDER
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(ctx.blob.prompt.is_none(), "the herder asks nothing");
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the draw waits for the trigger to resolve"
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(drawn(&ctx, TOP_OF_DECK, 0));
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(ctx.blob.seat(1).draws, 0, "only the herder's seat draws");
    }

    #[test]
    fn walking_home_is_a_move_and_draws_but_a_recall_is_not() {
        let mut fixture = ranged();
        let action = fixtures::move_action(HERDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        assert_eq!(ctx.location(HERDER), Some(Location::Base(0)));
        assert_eq!(
            sources_on_chain(&ctx),
            [HERDER],
            "Where::Any covers the walk home"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(drawn(&ctx, TOP_OF_DECK, 0));

        let mut fixture = ranged();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.recall(HERDER, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(HERDER), Some(Location::Base(0)));
        assert_eq!(moves(&ctx), 0, "434.1: a recall is not a move");
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.blob.seat(0).draws, 0);
    }

    #[test]
    fn two_herders_taken_along_by_one_group_move_each_draw_a_card() {
        let mut fixture = herd();
        let action = fixtures::move_action(HERDER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove {
                unit: HERDER,
                to: fixtures::BF1
            })
        );
        assert_eq!(labels(&ctx), ["{card 91}", "done"]);
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::GroupMove {
                    unit: HERDER,
                    to: fixtures::BF1
                }
            ),
            "move others to {zone 9} too?"
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.location(SECOND_HERDER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(moves(&ctx), 2);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "143.3 · one declared move, one batch of triggers"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "nothing reaches the chain unordered"
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(sources_on_chain(&ctx), [HERDER, SECOND_HERDER]);
        assert_eq!(ctx.hand_of(0).len(), hand);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(sources_on_chain(&ctx), [HERDER], "the chain resolves LIFO");
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        assert!(drawn(&ctx, TOP_OF_DECK, 0));
        assert!(drawn(&ctx, NEXT_CARD, 0));
        assert_eq!(ctx.blob.seat(0).draws, 2);
    }

    #[test]
    fn two_herders_moved_together_are_one_batch_their_controller_orders() {
        let mut fixture = herd();
        let action = fixtures::move_action(HERDER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove { unit, .. }) if unit == HERDER
        ));
        assert_eq!(
            ctx.blob.queue.len(),
            1,
            "the leader's trigger is collected while the group is still being declared"
        );
        assert_eq!(
            ctx.blob.queue[0].needs,
            Needs::Order,
            "and its batch is held open until the group has moved"
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "both Moved events land in one batch, so the seat picks which draw resolves first"
        );
        assert_eq!(labels(&ctx), ["{card 90} trigger", "{card 91} trigger",]);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            prompts::status(&ctx, PromptWhy::OrderTriggers { seat: 0 }),
            "order your triggers (last placed resolves first)"
        );
        assert_eq!(
            priority::pass(&mut ctx, 0),
            Err(Refusal::PromptOpen),
            "no passing around an open order"
        );
        answer(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            sources_on_chain(&ctx),
            [SECOND_HERDER, HERDER],
            "the seat placed the second herder first, so the leader's draw resolves first"
        );
        assert!(ctx
            .blob
            .queue
            .iter()
            .all(|pending| pending.needs != Needs::Order));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(sources_on_chain(&ctx), [SECOND_HERDER]);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        assert_eq!(ctx.blob.seat(0).draws, 2);
    }

    #[test]
    fn a_group_move_declined_leaves_the_leaders_trigger_alone_and_it_asks_no_order() {
        let mut fixture = herd();
        let action = fixtures::move_action(HERDER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        let done = labels(&ctx)
            .iter()
            .position(|label| label == "done")
            .unwrap() as u16;
        answer(&mut ctx, 0, done).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one trigger orders itself");
        assert_eq!(ctx.location(SECOND_HERDER), Some(Location::Base(0)));
        assert_eq!(sources_on_chain(&ctx), [HERDER]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1);
    }

    #[test]
    fn an_enemy_effect_that_moves_the_herder_still_draws_for_the_herders_controller() {
        let mut fixture = pasture();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        ctx.actor = 1;
        assert_eq!(
            ctx.move_unit(
                HERDER,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert_eq!(sources_on_chain(&ctx), [HERDER]);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.hand_of(1).len(), theirs);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert_eq!(ctx.blob.seat(1).draws, 0);
    }

    #[test]
    fn a_herder_that_left_the_board_before_its_trigger_resolves_still_draws() {
        let mut fixture = pasture();
        let action = fixtures::move_action(HERDER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march(&mut ctx, 0).unwrap();
        assert_eq!(sources_on_chain(&ctx), [HERDER]);
        assert!(ctx.bounce(HERDER));
        assert_eq!(ctx.card(HERDER).unwrap().zone, Some(fixtures::HAND));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(drawn(&ctx, TOP_OF_DECK, 0));
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 2,
            "356.3.e.5: the draw names no source to check"
        );
    }

    #[test]
    fn a_refused_march_moves_nothing_and_draws_nothing() {
        let mut fixture = ranged();
        assert_eq!(
            refuse(&mut fixture, 0, HERDER, fixtures::BF2),
            Refusal::Illegal(Reason::NeedsGanking),
            "battlefield to battlefield without Ganking"
        );
        assert_eq!(
            refuse(&mut fixture, 1, HERDER, fixtures::BASE),
            Refusal::Illegal(Reason::NotYourCard)
        );
        assert_eq!(
            fixture.table.card(HERDER).unwrap().zone,
            Some(fixtures::BF1)
        );
        assert_eq!(fixture.blob.seat(0).draws, 0);
        assert!(fixture.blob.queue.is_empty());
        assert!(fixture.blob.chain.is_empty());

        let mut fixture = pasture();
        fixture.table.card_mut(HERDER).unwrap().exhausted = true;
        fixture.resolve();
        assert_eq!(
            refuse(&mut fixture, 0, HERDER, fixtures::BF1),
            Refusal::Exhausted
        );
        assert_eq!(fixture.blob.seat(0).draws, 0);

        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(herder(THEIR_HERDER, fixtures::BASE, 1));
        fixture.resolve();
        assert_eq!(
            refuse(&mut fixture, 1, THEIR_HERDER, fixtures::BF1),
            Refusal::NotYourTurn,
            "the other seat's herder waits for its own action phase"
        );
        assert_eq!(fixture.blob.seat(1).draws, 0);
        assert!(fixture.blob.queue.is_empty());
    }
}

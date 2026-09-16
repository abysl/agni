use super::prelude::{battlefield, done, triggered};
use super::temporal_portal::next_spell_repeats_for_its_cost;
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

fn study(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    next_spell_repeats_for_its_cost(ctx, item.controller);
    done()
}

pub static CARD: Card = battlefield(
    "The Academy",
    &[],
    &[triggered(Trigger::Hold(Who::You), &[], study)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{play, spell, Promise, PromiseEffect, PromiseKind};
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{Expiry, ItemKind, PromptWhy, SLOT_PROMISED_REPEAT};

    const ACADEMY: u32 = fixtures::GROUNDS;
    const ECHO: u32 = 90;

    static ECHO_CARD: Card = spell("Echo", &[], &[play(&[], |_, _, _| Flow::Done)]);

    fn academy_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ACADEMY).unwrap().name = "The Academy".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::spell(ECHO, fixtures::HAND, 0, "Echo", 1, 0));
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ECHO, &ECHO_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ACADEMY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn academy_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == ACADEMY => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        assert!(cleanup::score_holds(ctx, seat).contains(&fixtures::BF1));
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_academy_is_one_untargeted_hold_trigger_sharing_the_portals_promise() {
        assert!(std::ptr::eq(script_of("The Academy").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(!ability.optional);
    }

    #[test]
    fn holding_puts_the_trigger_on_the_chain_for_the_holder_and_resolving_it_records_the_promise() {
        let mut fixture = academy_held_by(Some(0));
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert_eq!(academy_items(&ctx), [0]);
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.points(0), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(
            &"the next spell {seat 0} plays this turn has Repeat equal to its cost".to_string()
        ));
        assert_eq!(
            ctx.blob.seat(0).promises,
            [Promise {
                kind: PromiseKind::Spell,
                effect: PromiseEffect::RepeatForCost,
                until: Expiry::EndOfTurn(1),
            }]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_holding_gets_their_own_promise_and_a_conquer_is_not_a_hold() {
        let mut theirs = academy_held_by(Some(1));
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.resolve();
        theirs.scripts = theirs.scripts.clone().with_script(ECHO, &ECHO_CARD);
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert_eq!(academy_items(&ctx), [1]);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.log.contains(
            &"the next spell {seat 1} plays this turn has Repeat equal to its cost".to_string()
        ));
        assert_eq!(ctx.blob.seat(1).promises.len(), 1);
        assert!(ctx.blob.seat(0).promises.is_empty());
        drop(ctx);
        let mut taken = academy_held_by(None);
        taken.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = taken.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(academy_items(&ctx).is_empty());
        assert_eq!(
            ctx.points(0),
            1,
            "the conquer scores, the Academy stays quiet"
        );
    }

    #[test]
    fn declining_the_repeat_pays_the_printed_cost_and_still_spends_the_promise() {
        let mut fixture = academy_held_by(Some(0));
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        resolve_chain(&mut ctx);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ECHO).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["yes", "no", "cancel"],
            "the Repeat is optional"
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "Echo was the next spell whether or not its Repeat was paid"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s Repeat is used".to_string()));
        let mut unheld = academy_held_by(None);
        let mut ctx = unheld.ctx();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ECHO).unwrap();
        assert!(
            !matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "no hold, no Repeat ask: {:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
    }

    #[test]
    fn the_next_spell_this_turn_after_a_hold_offers_a_repeat_equal_to_its_cost() {
        let mut fixture = academy_held_by(Some(0));
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, ECHO).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_PROMISED_REPEAT as u8
            }),
            "Echo costs one energy, so its Repeat costs one energy"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "the spell and its repeat exhaust one rune each"
        );
    }
}

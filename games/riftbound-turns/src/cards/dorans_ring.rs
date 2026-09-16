use super::prelude::{
    ask_discard, done, draw, equip, gear, on_conquer_me, while_attached, with_statics,
};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Chaos)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const DRAWS: usize = 1;
pub const ON_CONQUER: u8 = GRANTED;
pub const STAGE_DISCARDED: u8 = 1;

fn cycle(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != STAGE_DISCARDED {
        if let Some(ask) = ask_discard(ctx, item, STAGE_DISCARDED) {
            return Flow::Ask(ask);
        }
        ctx.narrate(format!("{{seat {seat}}} has nothing to discard"));
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_conquer_me(&[], cycle)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Doran's Ring", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, queue_granted, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle, triggers};
    use crate::state::{PromptWhy, TargetRef};

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Doran's Ring", 1, "Chaos"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_chaos_equipment_with_one_might_and_a_wearer_conquer_listener() {
        assert!(std::ptr::eq(script_of("Doran's Ring").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::Conquer(Who::Me));
        assert!(listener.condition.is_none());
        assert!(listener.targets.is_empty());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_conquer_asks_for_one_discard_then_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let hand = ctx.hand_of(0).len();
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DISCARDED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx).len(),
            hand,
            "every card in hand is offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand, "one out, one in");
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), deck - 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} discards {card 71}".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_hand_skips_the_discard_and_still_draws_and_the_engine_hears_it() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to discard, nothing asked"
        );
        assert_eq!(ctx.hand_of(0).len(), 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing to discard".to_string()));
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_conquer_by_the_wearer_cycles_a_card_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
    }
}

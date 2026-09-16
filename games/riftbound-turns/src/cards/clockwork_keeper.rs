use super::prelude::{done, draw, play, unit, when, with_additional};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};
pub const DRAWS: usize = 1;

fn paid_additional_on_entry(_: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played {
            card,
            paid_additional: true,
            ..
        } if *card == source.card
    )
}

fn keep(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Clockwork Keeper",
        &[],
        &[when(play(&[], keep), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::pay;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const KEEPER: u32 = 90;
    const CALM_RUNE: u32 = 42;

    fn workshop(calm_rune: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut keeper = fixtures::unit(KEEPER, fixtures::HAND, 0, "Clockwork Keeper", 2);
        keeper.domain = vec!["Calm".into()];
        keeper.energy = Some(2);
        fixture.table.cards.push(keeper);
        if !calm_rune {
            let held = fixture.table.card_mut(CALM_RUNE).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        fixture.resolve();
        fixture
    }

    fn item() -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: KEEPER }, 0, Origin::Hand)
    }

    fn additional_confirm(ctx: &Ctx) -> bool {
        matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        )
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_prints_an_optional_calm_cost_and_a_play_trigger_gated_on_paying_it() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Clockwork Keeper").unwrap(),
            &CARD
        ));
        let fixture = workshop(true);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KEEPER).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional, "the may is the cost, not the draw");
        assert!(ability.condition.is_some());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_additional_cost_adds_a_calm_need_only_when_the_slot_says_paid() {
        let mut fixture = workshop(true);
        let ctx = fixture.ctx();
        let mut held = item();
        let plain = cost::of_item(&ctx, &held, None);
        assert_eq!(plain.energy, 2);
        assert!(plain.power.is_empty());
        held.set_slot(SLOT_ADDITIONAL, 0);
        assert!(cost::of_item(&ctx, &held, None).power.is_empty());
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 2);
        assert_eq!(paid.power, [Need::Domain(Domain::Calm)]);
        assert_eq!(paid.label(), "2 energy and 1 Calm power");
        assert!(held.paid_additional());
    }

    #[test]
    fn paying_the_calm_rune_draws_one_when_the_trigger_resolves() {
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KEEPER).unwrap();
        assert!(
            additional_confirm(&ctx),
            "355.1.a · the additional cost is asked as you play: {:?}",
            ctx.blob.why
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.on_board(KEEPER));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.card(CALM_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Calm rune is recycled for the additional cost"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "the Calm rune paid one energy as it was exhausted and the power as it was recycled; one more rune paid the other energy"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KEEPER
        ));
        assert_eq!(draws(&ctx), 0, "the draw waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 1);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_calm_rune_plays_the_keeper_for_two_and_draws_nothing() {
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KEEPER).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.on_board(KEEPER));
        assert_eq!(
            ctx.card(CALM_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "no additional cost, no Calm rune spent"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(draws(&ctx), 0);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_calm_rune_the_confirm_is_skipped_and_the_plain_play_goes_through() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        let mut paid = item();
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!pay::affordable(&ctx, 0, &cost::of_item(&ctx, &paid, None)));
        fixtures::play_from_hand(&mut ctx, 0, KEEPER).unwrap();
        assert!(
            !additional_confirm(&ctx),
            "no Calm rune, so the confirm is not offered: {:?}",
            ctx.blob.why
        );
        assert!(ctx.on_board(KEEPER));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 0);
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
    }

    #[test]
    fn the_other_seat_cannot_answer_the_confirm_and_cancel_takes_the_play_back() {
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KEEPER).unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            crate::engine::prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(!ctx.on_board(KEEPER));
        assert_eq!(ctx.card(KEEPER).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "a cancelled play pays nothing"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 0);
    }
}

use super::prelude::{card_target, done, draw, exhaust, play, spell, target};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const FRIENDLY_READY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Ready]);
pub const EXHAUST_COST: TargetSpec = target(
    FRIENDLY_READY_UNIT,
    0,
    1,
    TargetKind::Card,
    "a friendly unit to exhaust as an additional cost",
);
pub const DRAWS_PAID: usize = 2;
pub const DRAWS_UNPAID: usize = 1;

pub fn exhausted_as_additional_cost(ctx: &mut Ctx, item: &Item) -> bool {
    let Some(unit) = card_target(ctx, item, 0) else {
        return false;
    };
    if !exhaust(ctx, unit) {
        return false;
    }
    ctx.narrate(format!(
        "{{card {}}} · {{card {unit}}} is exhausted as the additional cost",
        item.kind.source()
    ));
    true
}

fn meditate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let draws = if exhausted_as_additional_cost(ctx, item) {
        DRAWS_PAID
    } else {
        DRAWS_UNPAID
    };
    draw(ctx, item.controller, draws);
    done()
}

pub static CARD: Card = spell(
    "Meditation",
    &[Keyword::Reaction],
    &[play(&[EXHAUST_COST], meditate)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const MEDITATION: u32 = 90;

    fn meditation(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(MEDITATION, fixtures::HAND, seat, "Meditation", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(meditation(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MEDITATION).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_a_reaction_with_one_optional_friendly_ready_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Meditation").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [EXHAUST_COST]);
        assert_eq!((EXHAUST_COST.min, EXHAUST_COST.max), (0, 1));
        assert_eq!(EXHAUST_COST.filter, FRIENDLY_READY_UNIT);
        assert_eq!((DRAWS_PAID, DRAWS_UNPAID), (2, 1));
    }

    #[test]
    fn exhausting_a_friendly_unit_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, MEDITATION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "skip", "cancel"],
            "only Vi is a ready friendly unit: Jinx and the Sprite are the other seat's"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit to exhaust as an additional cost (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        resolve(&mut ctx);
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · {card 50} is exhausted as the additional cost".to_string()));
        assert_eq!(drew(&ctx, 0), DRAWS_PAID);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "one played, two drawn");
        assert_eq!(ctx.card(MEDITATION).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn skipping_the_cost_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, MEDITATION).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(drew(&ctx, 0), DRAWS_UNPAID);
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn an_exhausted_unit_is_not_offered_and_one_exhausted_in_response_pays_nothing() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MEDITATION).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no ready friendly unit: the optional pick is skipped without asking"
        );
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve(&mut ctx);
        assert_eq!(drew(&ctx, 0), DRAWS_UNPAID);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MEDITATION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.exhaust(fixtures::VI);
        resolve(&mut ctx);
        assert_eq!(
            drew(&ctx, 0),
            DRAWS_UNPAID,
            "the exhaust could not be paid on resolution"
        );
    }

    #[test]
    #[ignore = "engine gap · non-resource additional costs; the exhaust belongs to the pay stage (355.10.c, 357), raises no Chosen and cannot be undone by a response"]
    fn the_exhaust_is_paid_before_the_spell_is_on_the_chain_and_is_not_a_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MEDITATION).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "the may-exhaust is a cost confirm, not a target prompt"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "paid while playing, before priority"
        );
        assert!(
            ctx.blob.chain[0].targets.is_empty(),
            "a cost choice is not a target"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Chosen { card, .. } if *card == fixtures::VI)));
        ctx.kill(fixtures::VI, crate::engine::ctx::Cause::Rule);
        resolve(&mut ctx);
        assert_eq!(drew(&ctx, 0), DRAWS_PAID, "the cost was paid: two cards");
    }
}

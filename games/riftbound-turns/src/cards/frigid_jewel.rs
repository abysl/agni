use super::prelude::{self, a_friendly_unit, gear, triggered};
use super::{Card, Flow, Trigger};

pub static CARD: Card = gear(
    "Frigid Jewel",
    &[],
    &[triggered(
        Trigger::Draw { nth: 2 },
        &[a_friendly_unit("a friendly unit")],
        |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                prelude::might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        },
    )],
);

#[cfg(test)]
mod tests {
    use super::CARD;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Ctx, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play, priority, prompts, settle};
    use crate::state::{Expiry, MightMod, Phase, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::Target;

    const JEWEL: u32 = 90;
    const POPPY: u32 = 91;

    fn with_jewel(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::gear(JEWEL, zone, 0, "Frigid Jewel", 2));
        fixture.resolve();
        fixture
    }

    fn two_units() -> Fixture {
        let mut fixture = with_jewel(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(POPPY, fixtures::BASE, 0, "Poppy", 2));
        fixture.resolve();
        fixture
    }

    fn draw(ctx: &mut Ctx, seat: u8, count: usize) {
        ctx.draw(seat, count);
        settle(ctx).unwrap();
    }

    fn quiet(ctx: &Ctx) -> bool {
        ctx.blob.prompt.is_none() && ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty()
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map_or(0, |prompt| prompt.id);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return Ok(());
        };
        if let PromptWhy::Target { item, spec } = answered.why {
            play::choose_targets(ctx, item, spec, &answered.prompt.picked)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_registry_resolves_the_pool_name_to_this_script() {
        let fixture = with_jewel(fixtures::BASE);
        assert!(std::ptr::eq(fixture.scripts.of_card(JEWEL).unwrap(), &CARD));
        assert!(matches!(
            CARD.abilities[0].trigger,
            Trigger::Draw { nth: 2 }
        ));
        assert!(!CARD.abilities[0].optional);
    }

    #[test]
    fn the_second_draw_asks_for_a_friendly_unit_and_gives_it_two_might_this_turn() {
        let mut fixture = two_units();
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 0, 1);
        assert!(quiet(&ctx), "the first draw is not the second");
        draw(&mut ctx, 0, 1);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(!prompt.cancel, "a trigger cannot be canceled");
        assert_eq!(
            prompts::offered(&ctx)
                .iter()
                .map(|opt| opt.card)
                .collect::<Vec<_>>(),
            [Some(fixtures::VI), Some(POPPY)],
            "only friendly units are offered"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit (0 of 1)"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), JEWEL);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(POPPY)]);
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            might_counter(&ctx, POPPY),
            0,
            "nothing happens until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(quiet(&ctx));
        assert_eq!(might_counter(&ctx, POPPY), 2);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(
            ctx.blob.card_state(POPPY).unwrap().might,
            [MightMod {
                delta: 2,
                until: Expiry::EndOfTurn(1),
                src: 1
            }]
        );
        assert!(ctx.blob.is_neutral_open());
        draw(&mut ctx, 0, 1);
        assert!(quiet(&ctx), "the third draw is not the second");
        assert_eq!(might_counter(&ctx, POPPY), 2);
    }

    #[test]
    fn a_single_friendly_unit_is_chosen_without_a_prompt() {
        let mut fixture = with_jewel(fixtures::BASE);
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 0, 2);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        resolve_chain(&mut ctx);
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert!(quiet(&ctx));
    }

    #[test]
    fn a_wrong_target_is_refused_and_the_prompt_stays_open() {
        let mut fixture = two_units();
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 0, 2);
        assert!(ctx.blob.prompt.is_some());
        assert!(
            matches!(pick(&mut ctx, 0, 2), Err(Refusal::Pick(_))),
            "there is no third option"
        );
        assert!(
            matches!(pick(&mut ctx, 1, 0), Err(Refusal::Pick(_))),
            "the other seat does not answer"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the enemy sprite at a battlefield is not friendly"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[JEWEL]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the jewel itself is gear, not a unit"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the choice is mandatory"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn without_a_friendly_unit_the_trigger_fizzles() {
        let mut fixture = with_jewel(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 0, 2);
        assert!(quiet(&ctx));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn the_jewel_only_watches_its_controller_and_only_from_the_board() {
        let mut fixture = with_jewel(fixtures::BASE);
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 1, 2);
        assert!(quiet(&ctx), "the other seat's second draw is not yours");
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        let mut in_hand = with_jewel(fixtures::HAND);
        let mut ctx = in_hand.ctx();
        draw(&mut ctx, 0, 2);
        assert!(quiet(&ctx), "a jewel in hand has no abilities");
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
    }

    #[test]
    fn the_count_restarts_each_turn_and_the_beginning_draw_is_the_first() {
        let mut fixture = with_jewel(fixtures::BASE);
        for id in 26..30 {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        draw(&mut ctx, 0, 2);
        resolve_chain(&mut ctx);
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        draw(&mut ctx, 0, 2);
        assert!(quiet(&ctx), "the fourth draw of a turn is not the second");
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(might_counter(&ctx, fixtures::VI), 0, "the bonus expired");
        assert!(ctx
            .blob
            .card_state(fixtures::VI)
            .is_none_or(|state| state.might.is_empty()));
        assert!(quiet(&ctx), "seat 1's beginning draw is not seat 0's");
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (3, 0));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(
            quiet(&ctx),
            "the beginning-phase draw is the first of the turn"
        );
        draw(&mut ctx, 0, 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), JEWEL);
        resolve_chain(&mut ctx);
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(
            ctx.blob.card_state(fixtures::VI).unwrap().might[0].until,
            Expiry::EndOfTurn(3)
        );
    }
}

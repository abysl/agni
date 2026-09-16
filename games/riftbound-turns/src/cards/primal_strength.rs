use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 7;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Primal Strength",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const PRIMAL: u32 = 90;
    const BODY_RUNE: u32 = 46;

    fn primal() -> CardInfo {
        let mut card = fixtures::spell(PRIMAL, fixtures::HAND, 0, "Primal Strength", 4, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(primal());
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_an_action_with_one_unit_target() {
        assert!(std::ptr::eq(script_of("Primal Strength").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(
            (
                CARD.abilities[0].targets[0].min,
                CARD.abilities[0].targets[0].max
            ),
            (1, 1)
        );
    }

    #[test]
    fn primal_strength_gives_seven_might_for_the_turn_to_any_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRIMAL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Body rune is recycled for the power"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "four energy exhausted, the Body rune among them before it is recycled"
        );
        assert_eq!(
            might_counter(&ctx, fixtures::SPRITE),
            0,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::SPRITE), 10, "3 + 7");
        assert_eq!(might_counter(&ctx, fixtures::SPRITE), 7);
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::SPRITE),
            counter: COUNTER_MIGHT,
            delta: 7
        }));
        let row = ctx.state_of(fixtures::SPRITE).unwrap();
        assert_eq!(row.might.len(), 1);
        assert_eq!(row.might[0].until, Expiry::EndOfTurn(1));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} gets +7 might this turn".to_string()));
        assert_eq!(ctx.card(PRIMAL).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert_eq!(might_counter(&ctx, fixtures::SPRITE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_left_the_board_gets_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRIMAL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(!ctx.blob.log.iter().any(|line| line.contains("+7 might")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn without_a_body_rune_the_play_is_refused_and_a_non_unit_is_not_a_target() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| card.id != BODY_RUNE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, PRIMAL),
            Err(Refusal::NoPowerOf),
            "no Body power to pay"
        );
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRIMAL).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::LEGEND_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a legend is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one unit, not two"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(PRIMAL).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "nothing was paid"
        );
    }
}

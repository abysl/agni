use super::prelude::{a_unit, card_target, done, play, spell, stun};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Rune Prison",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::ANNOTATION_STUNNED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{expiry, play as play_engine};
    use crate::state::{PromptWhy, TargetRef, FLAG_STUNNED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const PRISON: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn prison() -> CardInfo {
        let mut card = fixtures::spell(PRISON, fixtures::HAND, 0, "Rune Prison", 2, 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(prison());
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.resolve();
        fixture
    }

    fn stun_lines(ctx: &Ctx) -> usize {
        ctx.blob
            .log
            .iter()
            .filter(|line| line.ends_with("is stunned"))
            .count()
    }

    #[test]
    fn the_script_is_an_action_with_one_unit_target() {
        assert!(std::ptr::eq(script_of("Rune Prison").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Hidden));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let spec = CARD.abilities[0].targets[0];
        assert_eq!(spec.filter, crate::cards::prelude::UNIT);
        assert_eq!((spec.min, spec.max), (1, 1));
    }

    #[test]
    fn rune_prison_stuns_the_chosen_unit_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRISON).unwrap();
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
            [42, CALM_RUNE]
                .iter()
                .filter(|rune| ctx.card(**rune).unwrap().zone == Some(fixtures::RUNE_DECK))
                .count(),
            1,
            "one Calm rune pays the power"
        );
        assert!(
            !ctx.is_stunned(fixtures::SPRITE),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::SPRITE));
        assert!(ctx.has_flag(fixtures::SPRITE, FLAG_STUNNED));
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: fixtures::SPRITE,
            key: ANNOTATION_STUNNED.into(),
            value: Some(vec![1])
        }));
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "410.1.c · the printed might is untouched"
        );
        assert_eq!(
            ctx.combat_might(fixtures::SPRITE),
            0,
            "410.1.b · it deals no combat damage this turn"
        );
        assert!(ctx.blob.log.contains(&"{card 60} is stunned".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(PRISON).unwrap().zone, Some(fixtures::TRASH));
        expiry::clear_stuns(&mut ctx);
        assert!(
            !ctx.is_stunned(fixtures::SPRITE),
            "the stun ends with the turn"
        );
        assert_eq!(ctx.combat_might(fixtures::SPRITE), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_left_the_board_is_not_stunned_and_a_stunned_one_is_not_stunned_twice() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRISON).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(stun_lines(&ctx), 0);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        fixtures::play_from_hand(&mut ctx, 0, PRISON).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(stun_lines(&ctx), 0, "already stunned, nothing to narrate");
    }

    #[test]
    fn a_non_unit_is_refused_and_without_a_calm_rune_the_play_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PRISON).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::LEGEND_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a legend is not a unit"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(PRISON).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(CALM_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        let mut broke = armed();
        broke.table.cards.retain(|card| card.id != CALM_RUNE);
        broke.table.card_mut(42).unwrap().domain = vec!["Fury".into()];
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, PRISON),
            Err(Refusal::NoPowerOf),
            "no Calm power to pay"
        );
    }
}

use super::prelude::{a_card, card_target, done, play, spell, stun};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

const FRIENDLY: usize = 0;
const ENEMY: usize = 1;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::AtBattlefield]);
pub const ENEMY_UNIT_AT_THE_SAME_BATTLEFIELD: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::SameLocationAs(FRIENDLY as u8),
]);

pub const FRIENDLY_TARGET: TargetSpec = a_card(
    FRIENDLY_UNIT_AT_A_BATTLEFIELD,
    "a friendly unit at a battlefield",
);
pub const ENEMY_TARGET: TargetSpec = a_card(
    ENEMY_UNIT_AT_THE_SAME_BATTLEFIELD,
    "an enemy unit at the same battlefield",
);

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, FRIENDLY) {
        stun(ctx, unit);
    }
    if let Some(unit) = card_target(ctx, item, ENEMY) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Facebreaker",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[FRIENDLY_TARGET, ENEMY_TARGET], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const FACEBREAKER: u32 = 90;
    const ALLY: u32 = 91;
    const FOE: u32 = 92;

    fn facebreaker() -> CardInfo {
        let mut card = fixtures::spell(FACEBREAKER, fixtures::HAND, 0, "Facebreaker", 2, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(facebreaker());
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            FOE,
            fixtures::BF1,
            1,
            "Vanguard Sergeant",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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
    fn the_script_is_a_hidden_action_choosing_a_friendly_then_an_enemy_unit_there() {
        assert!(std::ptr::eq(script_of("Facebreaker").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets, [FRIENDLY_TARGET, ENEMY_TARGET]);
        assert_eq!((ENEMY_TARGET.min, ENEMY_TARGET.max), (1, 1));
    }

    #[test]
    fn both_units_at_the_battlefield_are_stunned_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FACEBREAKER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "cancel"],
            "Vi in the base is not at a battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit at a battlefield (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "cancel"],
            "the Sprite at the other battlefield and Jinx in the base are elsewhere"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(ALLY), TargetRef::Card(FOE)]
        );
        assert_eq!(
            ctx.effects,
            [Effect::exhaust(41), Effect::exhaust(42)],
            "two energy, no power"
        );
        assert!(!ctx.is_stunned(ALLY), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(ALLY));
        assert!(ctx.is_stunned(FOE));
        assert_eq!(ctx.combat_might(ALLY), 0);
        assert_eq!(ctx.combat_might(FOE), 0);
        assert_eq!(ctx.current_might(FOE), 2, "the printed might is untouched");
        assert!(!ctx.is_stunned(fixtures::VI));
        assert_eq!(stun_lines(&ctx), 2);
        assert!(ctx.blob.log.contains(&"{card 91} is stunned".to_string()));
        assert!(ctx.blob.log.contains(&"{card 92} is stunned".to_string()));
        assert_eq!(ctx.card(FACEBREAKER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_that_moved_away_before_resolution_is_no_longer_at_the_same_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FACEBREAKER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(FOE, fixtures::BF2, 0), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.is_stunned(ALLY),
            "the friendly unit is still a legal target"
        );
        assert!(
            !ctx.is_stunned(FOE),
            "355.9.b · the enemy left the restriction behind"
        );
        assert_eq!(stun_lines(&ctx), 1);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FACEBREAKER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(ALLY, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.is_stunned(FOE),
            "without the friendly anchor the enemy is not at its battlefield"
        );
        assert_eq!(stun_lines(&ctx), 0);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn played_from_facedown_it_is_free_and_still_needs_both_sides_at_the_hiding_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(FACEBREAKER).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(FACEBREAKER).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(FACEBREAKER, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            FACEBREAKER,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 91}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 92}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Annotate { key, .. } if key == "exhausted")),
            "a hidden card reacts for free"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(ALLY));
        assert!(ctx.is_stunned(FOE));
        assert_eq!(ctx.card(FACEBREAKER).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_unit_in_a_base_an_enemy_elsewhere_and_a_lone_friendly_unit_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FACEBREAKER).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit in the base is not at a battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[FOE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the first pick must be friendly"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy at another battlefield is not at the same one"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy in its base is not at a battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[ALLY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the friendly unit is not an enemy"
        );
        assert!(ctx.blob.prompt.is_some());
        assert!(ctx.effects.is_empty(), "nothing was paid");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FACEBREAKER).unwrap().zone, Some(fixtures::HAND));
        let mut alone = armed();
        alone.table.cards.retain(|card| card.id != FOE);
        alone.resolve();
        let mut ctx = alone.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FACEBREAKER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "no enemy shares the battlefield"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(FACEBREAKER).unwrap().zone, Some(fixtures::HAND));
    }
}

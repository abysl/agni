use super::prelude::{a_friendly_unit, bounce, card_target, channel_exhausted, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const RUNES: usize = 1;

fn return_and_channel(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(owner) = ctx.card(unit).map(|held| held.owner) else {
        return done();
    };
    if bounce(ctx, unit) {
        channel_exhausted(ctx, owner, RUNES);
    }
    done()
}

pub static CARD: Card = spell(
    "Retreat",
    &[Keyword::Reaction],
    &[play(
        &[a_friendly_unit("a friendly unit to return")],
        return_and_channel,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const RETREAT: u32 = 90;
    const THEIR_RETREAT: u32 = 91;
    const TOP_RUNE: u32 = 32;

    fn retreat(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Retreat", 1, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(retreat(RETREAT, 0));
        fixture.table.cards.push(retreat(THEIR_RETREAT, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RETREAT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pool_size(ctx: &Ctx) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, 0).count()
    }

    fn pass_both(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_reaction_over_one_friendly_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Retreat").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn retreat_returns_the_unit_to_its_owners_hand_and_channels_one_rune_exhausted() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RETREAT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "only the seat's own units: the Sprite and Jinx are the other seat's"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        let pool_before = pool_size(&ctx);
        pass_both(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::VI,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.hand_of(0).contains(&fixtures::VI));
        assert!(ctx.effects.contains(&Effect::Move {
            card: TOP_RUNE,
            zone: fixtures::RUNE_POOL,
            seat: 0,
            index: TOP
        }));
        assert!(
            ctx.effects.contains(&Effect::exhaust(TOP_RUNE)),
            "the rune enters the pool exhausted: {:?}",
            ctx.effects
        );
        assert_eq!(pool_size(&ctx), pool_before + 1);
        assert!(ctx.card(TOP_RUNE).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the battlefield empties with the unit gone"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_gone_before_resolution_returns_nothing_and_channels_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RETREAT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.bounce(fixtures::VI);
        let effects = ctx.effects.len();
        let pool_before = pool_size(&ctx);
        pass_both(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_size(&ctx), pool_before);
        assert!(!ctx.effects[effects..].iter().any(|effect| matches!(
            effect,
            Effect::Move { zone, .. } if *zone == fixtures::RUNE_POOL
        )));
    }
}

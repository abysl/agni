use super::prelude::{a_unit, card_target, done, play, stun, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn shield(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit(
    "Solari Shieldbearer",
    &[],
    &[play(&[a_unit("a unit to stun")], shield)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Location, ANNOTATION_STUNNED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef, FLAG_STUNNED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const SHIELDBEARER: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn shieldbearer(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            might: Some(2),
            domain: vec!["Calm".into()],
            ..fixtures::card(id, zone, seat, "Solari Shieldbearer", "Unit")
        }
    }

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(shieldbearer(SHIELDBEARER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.resolve();
        fixture
    }

    fn raise(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, SHIELDBEARER, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_any_one_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Solari Shieldbearer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Solari Shieldbearer");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = &ability.targets[0];
        assert_eq!((spec.min, spec.max), (1, 1));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.filter, UNIT);
    }

    #[test]
    fn playing_the_shieldbearer_offers_every_unit_and_stuns_the_pick_until_end_of_turn() {
        let mut fixture = temple();
        let action = fixtures::move_action(SHIELDBEARER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        raise(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {SHIELDBEARER}}}")
            ],
            "friend and foe alike, the shieldbearer itself included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SHIELDBEARER
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert!(
            !ctx.is_stunned(fixtures::SPRITE),
            "the stun waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::SPRITE));
        assert!(ctx.has_flag(fixtures::SPRITE, FLAG_STUNNED));
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: fixtures::SPRITE,
            key: ANNOTATION_STUNNED.into(),
            value: Some(vec![1])
        }));
        assert!(
            !ctx.deals_combat_damage(fixtures::SPRITE),
            "423.1.b · a stunned unit deals no combat damage"
        );
        assert!(!ctx.is_stunned(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is stunned", fixtures::SPRITE)));
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            !ctx.is_stunned(fixtures::SPRITE),
            "423.1.a.2 · the stun clears at the end of the turn"
        );
    }

    #[test]
    fn a_unit_already_stunned_is_a_legal_pick_that_changes_nothing() {
        let mut fixture = temple();
        fixture
            .blob
            .card_state_mut(fixtures::VI)
            .set(FLAG_STUNNED, true);
        let action = fixtures::move_action(SHIELDBEARER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(ctx.is_stunned(fixtures::VI));
        raise(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::VI));
        assert!(
            !ctx.blob
                .log
                .contains(&format!("{{card {}}} is stunned", fixtures::VI)),
            "423.1.a.1 · a stunned unit can not be stunned again"
        );
    }

    #[test]
    fn alone_on_the_board_the_shieldbearer_is_its_own_only_target_and_stuns_itself() {
        let mut fixture = temple();
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.resolve();
        let action = fixtures::move_action(SHIELDBEARER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        raise(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "a single legal target is chosen without asking"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the mandatory target keeps the trigger"
        );
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(SHIELDBEARER)],
            "a unit is a unit, itself included"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(SHIELDBEARER));
        assert_eq!(ctx.location(SHIELDBEARER), Some(Location::Base(0)));
    }
}

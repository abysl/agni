use super::prelude::{an_enemy_unit, card_target, done, kill, play, stun, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Ctx, Killed};

fn judge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.is_stunned(unit) {
        if kill(ctx, item, unit) == Killed::Yes {
            ctx.narrate(format!("{{card {unit}}} was stunned · it dies"));
        }
    } else {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit(
    "Solari Chief",
    &[],
    &[play(
        &[an_enemy_unit(
            "an enemy unit to stun, or to kill if it is stunned",
        )],
        judge,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::ENEMY_UNIT;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef, FLAG_STUNNED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const CHIEF: u32 = 90;
    const ORDER_RUNE: u32 = 46;

    fn chief(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(4),
            domain: vec!["Order".into()],
            ..fixtures::card(id, zone, seat, "Solari Chief", "Unit")
        }
    }

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(chief(CHIEF, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.resolve();
        fixture
    }

    fn rally(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, CHIEF, Origin::Hand, Some(Location::Base(0)))?;
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
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_enemy_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Solari Chief").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Solari Chief");
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
        assert_eq!(spec.filter, ENEMY_UNIT);
    }

    #[test]
    fn an_unstunned_enemy_pick_is_stunned_and_lives() {
        let mut fixture = temple();
        let action = fixtures::move_action(CHIEF, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        rally(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT)
            ],
            "enemy units only"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CHIEF
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "otherwise, stun it");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is stunned", fixtures::THEIR_UNIT)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_stunned_enemy_pick_is_killed_instead() {
        let mut fixture = temple();
        fixture
            .blob
            .card_state_mut(fixtures::THEIR_UNIT)
            .set(FLAG_STUNNED, true);
        let action = fixtures::move_action(CHIEF, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        rally(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "the kill waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} was stunned · it dies",
            fixtures::THEIR_UNIT
        )));
        assert!(ctx.on_board(fixtures::SPRITE), "only the pick dies");
    }

    #[test]
    fn the_stun_is_read_at_resolution_so_a_stun_in_response_turns_the_stun_into_a_kill() {
        let mut fixture = temple();
        let action = fixtures::move_action(CHIEF, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        rally(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert!(ctx.stun(fixtures::SPRITE));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token dies");
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
    }

    #[test]
    fn with_no_enemy_unit_on_the_board_the_trigger_fizzles() {
        let mut fixture = temple();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(CHIEF, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        rally(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CHIEF}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(CHIEF), Some(Location::Base(0)));
        assert!(!ctx.is_stunned(fixtures::VI));
    }
}

use super::prelude::{an_enemy_unit, card_target, done, kill, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Ctx, Killed};

fn breathe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, unit) == Killed::Yes {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Harnessed Dragon",
    &[],
    &[play(&[an_enemy_unit("an enemy unit to kill")], breathe)],
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
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DRAGON: u32 = 90;
    const ORDER_RUNE: u32 = 46;

    fn dragon(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(6),
            domain: vec!["Order".into()],
            ..fixtures::card(id, zone, seat, "Harnessed Dragon", "Unit")
        }
    }

    fn roost() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dragon(DRAGON, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.resolve();
        fixture
    }

    fn unleash(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, DRAGON, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)
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
            crate::cards::script_of("Harnessed Dragon").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Harnessed Dragon");
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
    fn playing_the_dragon_offers_only_enemy_units_and_kills_the_pick_into_its_owners_trash() {
        let mut fixture = roost();
        let action = fixtures::move_action(DRAGON, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        unleash(&mut ctx).unwrap();
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT)
            ],
            "at a battlefield or in their base, but never a friendly unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[DRAGON]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "and so is the dragon itself"
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
            ItemKind::Trigger { source, index: 0 } if source == DRAGON
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
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
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} dies", fixtures::THEIR_UNIT)));
        assert!(ctx.on_board(fixtures::SPRITE), "only the pick dies");
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_token_pick_dies_and_ceases_to_exist() {
        let mut fixture = roost();
        let action = fixtures::move_action(DRAGON, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        unleash(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
    }

    #[test]
    fn with_no_enemy_unit_on_the_board_the_trigger_fizzles_and_the_dragon_still_lands() {
        let mut fixture = roost();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(DRAGON, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        unleash(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DRAGON}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
        assert!(
            ctx.on_board(fixtures::VI),
            "a friendly unit is never in danger"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
    }
}

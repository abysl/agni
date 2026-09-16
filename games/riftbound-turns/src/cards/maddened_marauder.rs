use super::prelude::{a_card, card_target, done, move_unit, play, unit, Location};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const UNIT_AT_A_BATTLEFIELD_MOVABLE_TO_BASE: Filter =
    Filter::And(&[Filter::Unit, Filter::AtBattlefield, Filter::MovableToBase]);

const SENT_HOME: TargetSpec = a_card(
    UNIT_AT_A_BATTLEFIELD_MOVABLE_TO_BASE,
    "a unit at a battlefield to move to its base",
);

fn scatter(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let home = Location::Base(ctx.controller(unit));
        move_unit(ctx, item, unit, home);
    }
    done()
}

pub static CARD: Card = unit(
    "Maddened Marauder",
    &[Keyword::Tank],
    &[play(&[SENT_HOME], scatter)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::equipment;
    use crate::cards::prelude::attach_gear;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const MARAUDER: u32 = 90;
    const ALLY: u32 = 91;

    fn marauder(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            might: Some(4),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, "Maddened Marauder", "Unit")
        }
    }

    fn raid() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(marauder(MARAUDER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn charge(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, MARAUDER, Origin::Hand, Some(Location::Base(0)))?;
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
    fn the_script_is_a_tank_whose_play_trigger_targets_one_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Maddened Marauder").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Maddened Marauder");
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(CARD.has_keyword(Keyword::Tank));
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
        assert_eq!(spec.filter, UNIT_AT_A_BATTLEFIELD_MOVABLE_TO_BASE);
    }

    #[test]
    fn the_marauder_offers_units_at_battlefields_of_either_side_and_sends_the_pick_to_its_own_base()
    {
        let mut fixture = raid();
        let action = fixtures::move_action(MARAUDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        charge(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {ALLY}}}")
            ],
            "the enemy token at BF2 and the friendly unit at BF1; the base units stay out"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit already in its base is refused"
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
            ItemKind::Trigger { source, index: 0 } if source == MARAUDER
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "the move waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "an enemy unit goes to its controller's base, not ours"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::SPRITE,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::SPRITE,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with(&format!("{{card {}}} moves to", fixtures::SPRITE))));
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF1)),
            "only the pick moves"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the emptied battlefield is left uncontrolled, not contested"
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_pick_goes_home_to_our_base() {
        let mut fixture = raid();
        let action = fixtures::move_action(MARAUDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        charge(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(ALLY), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn with_no_unit_at_a_battlefield_the_trigger_fizzles_and_the_marauder_still_lands() {
        let mut fixture = raid();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, ALLY].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(MARAUDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        charge(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MARAUDER}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(MARAUDER), Some(Location::Base(0)));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
    }

    #[test]
    fn an_enemy_jagged_cutlass_wearer_stays_while_a_friendly_one_still_goes_home() {
        const THEIR_CUTLASS: u32 = 92;
        const MY_CUTLASS: u32 = 93;
        let mut fixture = raid();
        fixture
            .table
            .cards
            .push(equipment(THEIR_CUTLASS, 1, "Jagged Cutlass", 3, "Body"));
        fixture
            .table
            .cards
            .push(equipment(MY_CUTLASS, 0, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let action = fixtures::move_action(MARAUDER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        attach_gear(&mut ctx, THEIR_CUTLASS, fixtures::SPRITE);
        attach_gear(&mut ctx, MY_CUTLASS, ALLY);
        charge(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {ALLY}}}")
            ],
            "both wearers are offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "an enemy trigger can't move the wearer"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} can't be moved by {{card {MARAUDER}}}",
            fixtures::SPRITE
        )));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        drop(ctx);

        let mut fixture = raid();
        fixture
            .table
            .cards
            .push(equipment(MY_CUTLASS, 0, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let mut ctx = fixture.ctx_for(0, &action);
        attach_gear(&mut ctx, MY_CUTLASS, ALLY);
        charge(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Base(0)),
            "your own trigger moves your own wearer"
        );
        assert!(ctx.fault.is_none());
    }
}

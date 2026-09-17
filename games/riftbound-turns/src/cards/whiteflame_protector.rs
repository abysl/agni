use super::prelude::{a_unit, card_target, done, might_this_turn, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 8;

fn protect(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
    }
    done()
}

pub static CARD: Card = unit(
    "Whiteflame Protector",
    &[],
    &[play(&[a_unit("a unit to protect")], protect)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{Expiry, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const PROTECTOR: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn protector(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(8),
            domain: vec!["Calm".into()],
            ..fixtures::card(id, zone, seat, "Whiteflame Protector", "Unit")
        }
    }

    fn peak() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(protector(PROTECTOR, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.resolve();
        fixture
    }

    fn summon(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, PROTECTOR, Origin::Hand, Some(Location::Base(0)))?;
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

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_any_one_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Whiteflame Protector").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Whiteflame Protector");
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
        assert_eq!(MIGHT, 8);
    }

    #[test]
    fn the_protector_gives_the_chosen_unit_eight_might_until_the_end_of_the_turn() {
        let mut fixture = peak();
        let action = fixtures::move_action(PROTECTOR, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        summon(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {PROTECTOR}}}")
            ],
            "any unit, the protector itself included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PROTECTOR
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 11);
        assert_eq!(might_counter(&ctx, fixtures::VI), 8);
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().might[0].until,
            Expiry::EndOfTurn(ctx.turn())
        );
        assert_eq!(ctx.current_might(PROTECTOR), 8, "only the pick grows");
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
    }

    #[test]
    fn a_pick_that_died_before_resolution_gets_nothing() {
        let mut fixture = peak();
        let action = fixtures::move_action(PROTECTOR, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        summon(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        ctx.kill(
            fixtures::THEIR_UNIT,
            crate::engine::ctx::Cause::Cleanup { last_item: None },
        );
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.state_of(fixtures::THEIR_UNIT)
                .is_none_or(|row| row.might.is_empty()),
            "356.3.e · a unit in the trash is no longer a target"
        );
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
    }
}

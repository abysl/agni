use super::prelude::{a_unit, card_target, done, might_this_turn, play, unit, HIDDEN};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const SHRINK: i16 = -2;
pub const FLOOR: i32 = 1;

fn blast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, SHRINK, Some(FLOOR));
    }
    done()
}

pub static CARD: Card = unit(
    "Blastcone Fae",
    HIDDEN,
    &[play(&[a_unit("a unit to shrink")], blast)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{Expiry, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const FAE: u32 = 90;
    const MIND_RUNE: u32 = 46;

    fn fae(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(2),
            domain: vec!["Mind".into()],
            ..fixtures::card(id, zone, seat, "Blastcone Fae", "Unit")
        }
    }

    fn glade() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fae(FAE, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.resolve();
        fixture
    }

    fn release(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, FAE, Origin::Hand, Some(Location::Base(0)))?;
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
    fn the_script_is_a_hidden_unit_whose_play_trigger_targets_any_one_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Blastcone Fae").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Blastcone Fae");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
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
        assert_eq!((SHRINK, FLOOR), (-2, 1));
    }

    #[test]
    fn the_fae_shrinks_the_chosen_unit_by_two_this_turn_and_the_mod_expires() {
        let mut fixture = glade();
        let action = fixtures::move_action(FAE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        release(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {FAE}}}")
            ],
            "any unit, friendly or enemy, the fae itself included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::ROCKFALL]),
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
            ItemKind::Trigger { source, index: 0 } if source == FAE
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::SPRITE), 1);
        assert_eq!(might_counter(&ctx, fixtures::SPRITE), -2);
        assert_eq!(
            ctx.state_of(fixtures::SPRITE).unwrap().might[0].until,
            Expiry::EndOfTurn(ctx.turn())
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "only the pick shrinks");
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert_eq!(might_counter(&ctx, fixtures::SPRITE), 0);
    }

    #[test]
    fn the_minimum_of_one_clamps_the_shrink_of_a_two_might_unit_to_minus_one() {
        let mut fixture = glade();
        let action = fixtures::move_action(FAE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        release(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 1);
        assert_eq!(
            might_counter(&ctx, fixtures::THEIR_UNIT),
            -1,
            "2 - 2 floors at 1, so the mod is remembered as -1"
        );
        let until = Expiry::EndOfTurn(ctx.turn());
        ctx.might(fixtures::THEIR_UNIT, 3, until, None, 0);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            4,
            "the clamped mod stays -1 under a later bonus"
        );
    }
}

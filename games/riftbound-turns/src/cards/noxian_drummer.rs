use super::faithful_manufactor::play_recruits_here;
use super::prelude::{done, on_move_to_battlefield, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn drum(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_recruits_here(ctx, item, 1);
    done()
}

pub static CARD: Card = unit("Noxian Drummer", &[], &[on_move_to_battlefield(&[], drum)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::{order_unit, recruits_of};
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;

    const DRUMMER: u32 = 90;

    fn camp(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(order_unit(DRUMMER, zone, 0, "Noxian Drummer", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRUMMER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march_to(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, DRUMMER, from, to);
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_move_to_battlefield_trigger() {
        assert!(std::ptr::eq(script_of("Noxian Drummer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
    }

    #[test]
    fn marching_to_a_battlefield_plays_an_exhausted_recruit_beside_the_drummer() {
        let mut fixture = camp(fixtures::BASE);
        let action = fixtures::move_action(DRUMMER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the move trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRUMMER
        ));
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits, [next]);
        let recruit = recruits[0];
        assert_eq!(
            ctx.location(recruit),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · it is also at the battlefield"
        );
        assert!(ctx.is_token(recruit));
        assert_eq!(ctx.current_might(recruit), i32::from(RECRUIT_MIGHT));
        assert!(ctx.card(recruit).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::exhaust(recruit)));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [DRUMMER, recruit]
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_move_to_the_base_is_not_a_move_to_a_battlefield() {
        let mut fixture = camp(fixtures::BF1);
        let action = fixtures::move_action(DRUMMER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert_eq!(ctx.location(DRUMMER), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "no battlefield, no drumroll");
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
    }

    #[test]
    fn a_drummer_killed_before_the_trigger_resolves_has_no_here_and_plays_nothing() {
        let mut fixture = camp(fixtures::BASE);
        let action = fixtures::move_action(DRUMMER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.kill(DRUMMER, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(DRUMMER));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DRUMMER}}} has left the board · no Recruit"
        )));
        assert!(ctx.fault.is_none());
    }
}

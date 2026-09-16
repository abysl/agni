use super::faithful_manufactor::play_recruits_here;
use super::prelude::{done, on_move_to_battlefield, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RECRUITS: usize = 3;

fn escort(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_recruits_here(ctx, item, RECRUITS);
    done()
}

pub static CARD: Card = unit(
    "Corina Veraza",
    &[Keyword::Accelerate],
    &[on_move_to_battlefield(&[], escort)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::{Cause, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const CORINA: u32 = 90;

    fn corina(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(CORINA, zone, 0, "Corina Veraza", 6);
        card.domain = vec!["Order".into()];
        card.energy = Some(7);
        card.power = Some(1);
        card
    }

    fn boardroom() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(corina(fixtures::BASE));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CORINA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_accelerate_and_one_trigger_on_her_own_move_to_a_battlefield() {
        assert!(std::ptr::eq(script_of("Corina Veraza").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
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
        assert_eq!(RECRUITS, 3);
    }

    #[test]
    fn marching_to_a_battlefield_plays_three_exhausted_recruits_there_when_the_trigger_resolves() {
        let mut fixture = boardroom();
        let action = fixtures::move_action(CORINA, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(CORINA),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CORINA
        ));
        assert!(ctx.blob.prompt.is_none(), "she asks nothing");
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruits wait for the chain"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits, [next, next + 1, next + 2]);
        for recruit in &recruits {
            assert_eq!(
                ctx.location(*recruit),
                Some(Location::Battlefield(fixtures::BF1)),
                "here · where she moved to"
            );
            assert!(ctx.is_token(*recruit));
            assert_eq!(ctx.current_might(*recruit), i32::from(RECRUIT_MIGHT));
            assert!(ctx.card(*recruit).unwrap().exhausted);
            assert!(ctx.effects.contains(&Effect::exhaust(*recruit)));
        }
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [CORINA, recruits[0], recruits[1], recruits[2]]
        );
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_effect_move_to_a_battlefield_counts_and_a_move_home_or_a_recall_does_not() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                CORINA,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            recruits_of(&ctx, 0).len(),
            RECRUITS,
            "Charm's move is still a move"
        );
        assert_eq!(
            ctx.move_unit(CORINA, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "a move to the base is no battlefield"
        );
        assert_eq!(recruits_of(&ctx, 0).len(), RECRUITS);
        ctx.move_unit(
            CORINA,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(recruits_of(&ctx, 0).len(), 2 * RECRUITS);
        ctx.recall(CORINA, true);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
        assert_eq!(
            recruits_of(&ctx, 0).len(),
            2 * RECRUITS,
            "a recall is not a move"
        );
    }

    #[test]
    fn corina_gone_before_the_trigger_resolves_plays_no_recruit() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            CORINA,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.kill(CORINA, Cause::Rule);
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CORINA}}} has left the board · no Recruit"
        )));
    }
}

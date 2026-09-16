use super::faithful_manufactor::play_recruits;
use super::prelude::{battlefield, done, on_hold_me, Location};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn unify(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), 1);
    done()
}

pub static CARD: Card = battlefield("Altar to Unity", &[], &[on_hold_me(&[], unify)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cleanup;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;

    const ALTAR: u32 = fixtures::GROUNDS;

    fn altar(holder: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ALTAR).unwrap().name = "Altar to Unity".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, holder);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ALTAR).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_altar_is_a_battlefield_with_one_targetless_hold_trigger() {
        assert!(std::ptr::eq(script_of("Altar to Unity").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
    }

    #[test]
    fn holding_the_altar_scores_the_point_and_plays_an_exhausted_recruit_in_the_holders_base() {
        let mut fixture = altar(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ALTAR
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0, "the holder's trigger");
        assert!(ctx.blob.prompt.is_none(), "the Altar asks nothing");
        assert_eq!(ctx.points(0), 1, "the hold itself, the trigger waits");
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for the chain"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recruits_of(&ctx, 0), [next]);
        assert_eq!(
            ctx.location(next),
            Some(Location::Base(0)),
            "in your base, not at the Altar"
        );
        assert!(ctx.is_token(next));
        assert!(ctx.card(next).unwrap().exhausted);
        assert_eq!(ctx.current_might(next), 1);
        assert_eq!(ctx.controller(next), 0);
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [fixtures::VI]
        );
        assert_eq!(ctx.points(0), 1, "the Recruit is no second point");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_a_hold_elsewhere_is_not_the_altars() {
        let mut fixture = altar(None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "469.2 · conquering is not holding"
        );
        assert!(recruits_of(&ctx, 0).is_empty());
        assert_eq!(ctx.points(0), 1);
        drop(ctx);

        let mut fixture = altar(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the Sprite holds the Rockfall Path, not the Altar"
        );
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert_eq!(ctx.points(1), 1);
    }

    #[test]
    fn the_opponent_holding_the_altar_recruits_into_their_own_base() {
        let mut fixture = altar(Some(1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].controller, 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 1);
        assert_eq!(recruits.len(), 1);
        assert_eq!(ctx.location(recruits[0]), Some(Location::Base(1)));
        assert_eq!(ctx.controller(recruits[0]), 1);
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }
}

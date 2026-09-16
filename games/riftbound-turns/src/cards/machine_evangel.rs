use super::faithful_manufactor::play_recruits;
use super::prelude::{deathknell, done, unit, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RECRUITS: usize = 3;

fn evangelize(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), RECRUITS);
    done()
}

pub static CARD: Card = unit(
    "Machine Evangel",
    &[Keyword::Deathknell],
    &[deathknell(&[], evangelize)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::{order_unit, recruits_of};
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;

    const EVANGEL: u32 = 90;

    fn pulpit(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut evangel = order_unit(EVANGEL, zone, 0, "Machine Evangel", 4);
        evangel.energy = Some(5);
        evangel.power = Some(1);
        fixture.table.cards.push(evangel);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EVANGEL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(script_of("Machine Evangel").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(death.cost.is_none() && death.condition.is_none());
        assert_eq!(RECRUITS, 3);
    }

    #[test]
    fn dying_at_a_battlefield_plays_three_exhausted_recruits_into_the_base() {
        let mut fixture = pulpit(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.kill(EVANGEL, Cause::Rule);
        assert_eq!(ctx.card(EVANGEL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == EVANGEL
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == EVANGEL
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "the Deathknell chooses nothing");
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
                Some(Location::Base(0)),
                "into your base, not where he fell"
            );
            assert!(ctx.is_token(*recruit));
            assert_eq!(ctx.current_might(*recruit), i32::from(RECRUIT_MIGHT));
            assert!(ctx.card(*recruit).unwrap().exhausted);
        }
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(
                    effect,
                    Effect::Spawn { zone, seat: 0, owner: Some(0), .. } if *zone == fixtures::BASE
                ))
                .count(),
            3
        );
        assert_eq!(ctx.units_at(Location::Base(0)).len(), 4, "Vi and the three");
        assert!(ctx
            .units_at(Location::Battlefield(fixtures::BF1))
            .is_empty());
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_bounce_is_not_a_death_and_an_enemy_evangel_recruits_for_its_own_controller() {
        let mut fixture = pulpit(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.bounce(EVANGEL));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(EVANGEL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty(), "no death, no Deathknell");
        assert!(recruits_of(&ctx, 0).is_empty());
        drop(ctx);

        let mut theirs = Fixture::enforced();
        theirs
            .table
            .cards
            .push(order_unit(EVANGEL, fixtures::BASE, 1, "Machine Evangel", 4));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        ctx.kill(EVANGEL, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].controller, 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recruits_of(&ctx, 1).len(), 3);
        assert!(recruits_of(&ctx, 0).is_empty());
        for recruit in recruits_of(&ctx, 1) {
            assert_eq!(ctx.location(recruit), Some(Location::Base(1)));
            assert_eq!(ctx.controller(recruit), 1);
        }
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_recruit_of_his_dies_without_a_deathknell_of_its_own() {
        let mut fixture = pulpit(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.kill(EVANGEL, Cause::Rule);
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits.len(), 3);
        ctx.kill(recruits[0], Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Despawn { card: recruits[0] }));
        assert_eq!(recruits_of(&ctx, 0), [recruits[1], recruits[2]]);
        assert!(ctx.fault.is_none());
    }
}

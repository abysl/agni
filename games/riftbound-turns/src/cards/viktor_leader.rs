use super::faithful_manufactor::{play_recruits, RECRUIT};
use super::prelude::{done, on_friendly_unit_dies, unit, when, Location};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub fn was_a_recruit(ctx: &Ctx, card: u32) -> bool {
    ctx.last_face(card)
        .is_some_and(|held| held.name == RECRUIT && held.is_kind(KIND_UNIT))
}

pub fn another_non_recruit_unit_of_yours_died(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Died {
        card,
        controller,
        unit: true,
        ..
    } = event
    else {
        return false;
    };
    *card != source.card && *controller == ctx.controller(source.card) && !was_a_recruit(ctx, *card)
}

pub fn when_another_non_recruit_unit_of_yours_dies(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), 1);
    done()
}

pub static CARD: Card = unit(
    "Viktor - Leader",
    &[],
    &[when(
        on_friendly_unit_dies(&[], when_another_non_recruit_unit_of_yours_dies),
        another_non_recruit_unit_of_yours_died,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::spawn_recruit;
    use crate::cards::faithful_manufactor::tests::{order_unit, recruits_of};
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::{ItemKind, Noted, Origin};

    const VIKTOR: u32 = 90;

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut viktor = order_unit(VIKTOR, fixtures::BASE, 0, "Viktor - Leader", 4);
        viktor.energy = Some(4);
        viktor.power = Some(1);
        fixture.table.cards.push(viktor);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VIKTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn source() -> Source {
        Source {
            card: VIKTOR,
            ability: 0,
        }
    }

    fn died(ctx: &Ctx, card: u32, controller: u8) -> Event {
        Event::Died {
            card,
            controller,
            unit: true,
            noted: Noted {
                zone: fixtures::BASE,
                might: ctx
                    .origin
                    .card(card)
                    .and_then(|held| held.might)
                    .map(i32::from)
                    .unwrap_or(1),
                controller,
                alone: false,
                buffed: false,
            },
        }
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: VIKTOR,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_registered_and_the_condition_reads_another_friendly_non_recruit_death() {
        assert!(std::ptr::eq(script_of("Viktor - Leader").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::UnitDies(Who::Friendly));
        assert!(CARD.abilities[0].condition.is_some());
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        let recruit = spawn_recruit(&mut ctx, 0, Location::Base(0)).unwrap();
        assert!(another_non_recruit_unit_of_yours_died(
            &ctx,
            &died(&ctx, fixtures::VI, 0),
            source()
        ));
        assert!(
            !another_non_recruit_unit_of_yours_died(&ctx, &died(&ctx, VIKTOR, 0), source()),
            "another · not himself"
        );
        assert!(
            !another_non_recruit_unit_of_yours_died(
                &ctx,
                &died(&ctx, fixtures::THEIR_UNIT, 1),
                source()
            ),
            "you control · not the enemy's"
        );
        assert!(
            !another_non_recruit_unit_of_yours_died(&ctx, &died(&ctx, recruit, 0), source()),
            "non-Recruit · a Recruit of his own does not chain"
        );
        assert!(was_a_recruit(&ctx, recruit));
        assert!(!was_a_recruit(&ctx, fixtures::VI));
        assert!(
            !another_non_recruit_unit_of_yours_died(
                &ctx,
                &Event::Died {
                    card: fixtures::HAND_GEAR,
                    controller: 0,
                    unit: false,
                    noted: Noted {
                        zone: fixtures::BASE,
                        might: 0,
                        controller: 0,
                        alone: false,
                        buffed: false
                    }
                },
                source()
            ),
            "a gear dying is not a unit dying"
        );
    }

    #[test]
    fn the_owed_body_plays_one_exhausted_recruit_into_his_controllers_base() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        let next = ctx.table.next_id;
        assert_eq!(
            when_another_non_recruit_unit_of_yours_dies(&mut ctx, &trigger_item(7), Stage(0)),
            Flow::Done
        );
        assert_eq!(recruits_of(&ctx, 0), [next]);
        assert_eq!(ctx.location(next), Some(Location::Base(0)));
        assert!(ctx.card(next).unwrap().exhausted);
        assert_eq!(ctx.current_might(next), 1);
        assert_eq!(
            when_another_non_recruit_unit_of_yours_dies(&mut ctx, &trigger_item(8), Stage(0)),
            Flow::Done
        );
        assert_eq!(
            recruits_of(&ctx, 0).len(),
            2,
            "every death is its own trigger"
        );
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_recruit_that_died_this_action_is_still_told_apart_by_its_first_face() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        let recruit = spawn_recruit(&mut ctx, 0, Location::Base(0)).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx();
        ctx.kill(recruit, Cause::Rule);
        assert!(ctx.card(recruit).is_none(), "the token despawned");
        assert!(
            was_a_recruit(&ctx, recruit),
            "the origin snapshot still names it"
        );
        assert!(!another_non_recruit_unit_of_yours_died(
            &ctx,
            &died(&ctx, recruit, 0),
            source()
        ));
    }

    #[test]
    fn a_friendly_units_death_queues_his_trigger_and_his_own_recruits_death_does_not() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, .. } if source == VIKTOR
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(recruits_of(&ctx, 0).len(), 1);
        let recruit = recruits_of(&ctx, 0)[0];
        ctx.kill(recruit, Cause::Rule);
        assert_eq!(triggers::collect(&mut ctx), 0);
    }
}

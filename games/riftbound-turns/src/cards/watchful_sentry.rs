use super::prelude::{deathknell, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn last_watch(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Watchful Sentry",
    &[Keyword::Deathknell],
    &[deathknell(&[], last_watch)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, legal, priority, settle};
    use crate::state::{ItemKind, Noted};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const SENTRY: u32 = 90;
    const THEIR_SENTRY: u32 = 91;

    fn sentry(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Watchful Sentry", 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn posted(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sentry(SENTRY, zone, 0));
        if zone == fixtures::BF1 {
            fixture.blob.set_holder(fixtures::BF1, Some(0));
        }
        fixture.resolve();
        fixture
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_untargeted_death_ability() {
        assert!(std::ptr::eq(script_of("Watchful Sentry").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Death);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(CARD.statics.is_empty());
        assert_eq!(DRAWS, 1);
        let fixture = posted(fixtures::BASE);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SENTRY).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn a_killed_sentry_queues_its_deathknell_and_its_controller_draws_one_on_resolution() {
        let mut fixture = posted(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let top = ctx.top_of(fixtures::MAIN_DECK, 0, 1)[0];
        assert_eq!(ctx.kill(SENTRY, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.card(SENTRY).unwrap().zone, Some(fixtures::TRASH));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SENTRY
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.blob.chain[0].noted,
            Some(Noted {
                zone: fixtures::BASE,
                might: 1,
                controller: 0,
                alone: false,
                buffed: false
            })
        );
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx
            .effects
            .contains(&agni_plugin_sdk::decide::Effect::Move {
                card: top,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }));
        assert_eq!(ctx.hand_of(1).len(), 1, "only its controller draws");
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn lethal_damage_at_a_battlefield_walks_the_same_deathknell() {
        let mut fixture = posted(fixtures::BF1);
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(SENTRY),
            counter: COUNTER_DAMAGE,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::dying(&ctx), [SENTRY]);
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.log.contains(&format!("{{card {SENTRY}}} dies")));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.zone),
            Some(fixtures::BF1)
        );
        resolve(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.location(SENTRY), None, "the body is in the trash");
    }

    #[test]
    fn a_living_sentry_draws_nothing_and_the_other_seat_cannot_play_one_on_this_turn() {
        let mut fixture = posted(fixtures::BASE);
        fixture
            .table
            .cards
            .push(sentry(THEIR_SENTRY, fixtures::HAND, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "a living sentry has no Deathknell"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_SENTRY,
            from: Some(fixtures::HAND),
            from_seat: 1,
            to: Some(fixtures::BASE),
            to_seat: 1,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "seat 1 cannot play its sentry on seat 0's turn"
        );
    }
}

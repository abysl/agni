use super::prelude::{deathknell, done, draw, unit, was_mighty};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 2;

fn eulogy(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if !was_mighty(item) {
        return done();
    }
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Unsung Hero",
    &[Keyword::Deathknell],
    &[deathknell(&[], eulogy)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::MIGHTY;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, EntryMove, Event, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, legal, play as play_engine, priority, settle};
    use crate::state::{Expiry, ItemKind, Noted, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const HERO: u32 = 90;
    const THEIR_HERO: u32 = 91;
    const HAND_HERO: u32 = 92;
    const PRINTED: u8 = 2;
    const DECK_TOP: [u32; 2] = [23, 22];

    fn hero(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Unsung Hero", PRINTED);
        card.domain = vec!["Order".into()];
        card
    }

    fn standing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hero(HERO, fixtures::BASE, 0));
        fixture.resolve();
        fixture
    }

    fn in_hand() -> Fixture {
        let mut fixture = standing();
        fixture.table.cards.push(hero(HAND_HERO, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(hero(THEIR_HERO, fixtures::HAND, 1));
        fixture.resolve();
        fixture
    }

    fn wounded(damage: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hero(HERO, fixtures::BF1, 0));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(HERO),
            counter: crate::engine::ctx::COUNTER_DAMAGE,
            value: damage,
        });
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn entry(seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: Some(fixtures::HAND),
            from_seat: seat,
            to: Some(fixtures::BASE),
            to_seat: seat,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_untargeted_death_ability() {
        assert_eq!(CARD.name, "Unsung Hero");
        assert!(CARD.has_keyword(Keyword::Deathknell));
        assert_eq!(CARD.keywords, &[Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Death);
        assert!(ability.targets.is_empty(), "it chooses nothing");
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(
            ability.condition.is_none(),
            "the Mighty clause is read off the snapshot when the item resolves"
        );
        assert_eq!(DRAWS, 2);
        assert_eq!(MIGHTY, 5);
        let fixture = standing();
        assert!(std::ptr::eq(fixture.scripts.of_card(HERO).unwrap(), &CARD));
    }

    #[test]
    fn a_hero_that_was_mighty_when_it_died_draws_two_off_its_noted_snapshot() {
        let mut fixture = standing();
        let mut ctx = fixture.ctx();
        let turn = ctx.turn();
        ctx.might(HERO, 3, Expiry::EndOfTurn(turn), None, 0);
        assert_eq!(ctx.current_might(HERO), MIGHTY);
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(HERO, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.deaths.len(), 1, "queued before the body left");
        assert_eq!(ctx.card(HERO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.printed_might(HERO),
            i32::from(PRINTED),
            "the board would say 2 now: only the snapshot remembers 5"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HERO
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.blob.chain[0].noted,
            Some(Noted {
                zone: fixtures::BASE,
                might: MIGHTY,
                controller: 0,
                alone: false,
                buffed: false
            })
        );
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        for card in DECK_TOP {
            assert!(ctx.effects.contains(&Effect::Move {
                card,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }));
        }
        assert_eq!(ctx.hand_of(1).len(), 1, "only its controller draws");
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
    }

    #[test]
    fn a_hero_one_point_short_of_mighty_still_triggers_and_draws_nothing() {
        let mut fixture = standing();
        let mut ctx = fixture.ctx();
        let turn = ctx.turn();
        ctx.might(HERO, 2, Expiry::EndOfTurn(turn), None, 0);
        assert_eq!(ctx.current_might(HERO), MIGHTY - 1);
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(HERO, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Deathknell triggers either way; the if is in the effect"
        );
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.might),
            Some(MIGHTY - 1)
        );
        resolve(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand, "4 might is not Mighty");
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { .. })));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("draws")));
    }

    #[test]
    fn lethal_damage_at_a_battlefield_walks_the_same_deathknell() {
        let mut fixture = wounded(MIGHTY);
        let mut ctx = fixture.ctx();
        let turn = ctx.turn();
        ctx.might(HERO, 3, Expiry::EndOfTurn(turn), None, 0);
        assert_eq!(ctx.current_might(HERO), MIGHTY);
        assert_eq!(cleanup::dying(&ctx), [HERO]);
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::lethal_kills(&mut ctx), [HERO]);
        assert!(ctx.blob.log.contains(&format!("{{card {HERO}}} dies")));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain[0].noted,
            Some(Noted {
                zone: fixtures::BF1,
                might: MIGHTY,
                controller: 0,
                alone: true,
                buffed: false
            })
        );
        resolve(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.location(HERO), None, "the body is in the trash");
    }

    #[test]
    fn a_hero_nobody_killed_draws_nothing_and_the_other_seat_cannot_play_one() {
        let mut fixture = in_hand();
        let ctx = fixture.ctx();
        assert!(ctx.on_board(HERO));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Died { .. })),
            "a living hero has no Deathknell"
        );
        assert_eq!(
            legal::classify(&ctx, 1, &entry(1, THEIR_HERO)),
            Err(Refusal::NotYourTurn),
            "seat 1 cannot play its hero on seat 0's turn"
        );
        drop(ctx);
        let mut broke = in_hand();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        broke.resolve();
        let action = fixtures::move_action(HAND_HERO, fixtures::BASE, 0);
        let mut ctx = broke.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            play_engine::begin(
                &mut ctx,
                0,
                HAND_HERO,
                Origin::Hand,
                Some(Location::Base(0))
            ),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
    }
}

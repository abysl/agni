use super::prelude::{deathknell, done, draw, play, seat_target, target, unit};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 1;

pub const DEATHKNELL_XP: u8 = 1;

fn scuttle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

fn gain_xp(ctx: &mut Ctx, seat: u8) {
    ctx.score_xp(seat, i32::from(DEATHKNELL_XP));
    ctx.narrate(format!("{{seat {seat}}} gains {DEATHKNELL_XP} XP"));
}

fn last_words(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(opponent) = seat_target(item, 0) else {
        return done();
    };
    ctx.reveal_hand(opponent);
    ctx.look_at_facedown(item.controller, opponent);
    gain_xp(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Scuttle Crab",
    &[Keyword::Deathknell],
    &[
        play(&[], scuttle),
        deathknell(
            &[target(Filter::Enemy, 1, 1, TargetKind::Seat, "an opponent")],
            last_words,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::cleanup;
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, march, play as play_engine, priority, settle, showdown, triggers};
    use crate::rules::COUNTER_POINTS;
    use crate::rules::COUNTER_XP;
    use crate::state::{ItemKind, Origin, Phase, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const CRAB: u32 = 90;
    const THEIR_CRAB: u32 = 91;
    const DECK_TOP: u32 = 23;

    fn crab(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Scuttle Crab", 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn in_hand() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(crab(CRAB, fixtures::HAND, 0));
        fixture.resolve();
        fixture
    }

    fn alone_at_base() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.table.cards.push(crab(CRAB, fixtures::BASE, 0));
        for id in 26..32 {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture.resolve();
        fixture
    }

    fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), COUNTER_XP)
            .unwrap_or(0)
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_crab_is_a_deathknell_unit_with_a_play_draw_and_a_death_ability() {
        assert_eq!(CARD.name, "Scuttle Crab");
        assert!(CARD.has_keyword(Keyword::Deathknell));
        assert_eq!(CARD.abilities.len(), 2);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        let death = &CARD.abilities[1];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(!death.optional);
        assert!(death.cost.is_none());
        assert!(death.condition.is_none());
        assert_eq!(death.targets.len(), 1, "it chooses an opponent");
        let spec = death.targets[0];
        assert_eq!((spec.min, spec.max), (1, 1));
        assert_eq!(spec.kind, crate::cards::TargetKind::Seat);
        assert_eq!(spec.filter, Filter::Enemy);
        assert_eq!(spec.label, "an opponent");
        assert_eq!(DEATHKNELL_XP, 1);
        assert_eq!(COUNTER_XP, 1);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        let fixture = in_hand();
        assert!(std::ptr::eq(fixture.scripts.of_card(CRAB).unwrap(), &CARD));
    }

    #[test]
    fn playing_the_crab_puts_its_trigger_on_the_chain_and_draws_one_when_that_resolves() {
        let mut fixture = in_hand();
        let action = fixtures::move_action(CRAB, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        play_engine::begin(&mut ctx, 0, CRAB, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(CRAB), Some(Location::Base(0)));
        assert!(ctx.card(CRAB).unwrap().exhausted);
        assert_eq!(ctx.current_might(CRAB), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == CRAB
        )));
        assert!(ctx.blob.prompt.is_none(), "the crab asks nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CRAB
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the draw waits for the trigger to resolve"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: DECK_TOP,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.hand_of(1).len(), 1, "only its controller draws");
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
    }

    #[test]
    fn a_zero_might_crab_marches_alone_conquers_the_empty_battlefield_and_holds_it_next_turn() {
        let mut fixture = alone_at_base();
        let action = fixtures::move_action(CRAB, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.points(0), 0);
        march::standard_move(
            &mut ctx,
            0,
            CRAB,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert!(ctx.blob.prompt.is_none(), "no companion to bring along");
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        let showdown = ctx.blob.showdown.clone().expect("the crab contests alone");
        assert_eq!(
            (showdown.zone, showdown.attacker, showdown.combat),
            (fixtures::BF1, 0, false)
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.showdown.is_none(),
            "both seats passed the showdown"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.blob.contester(fixtures::BF1), None);
        assert_eq!(ctx.points(0), 1, "0 might still conquers");
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} conquers {{zone {}}}", fixtures::BF1)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![CRAB]
        )));
        crate::engine::phases::end_turn(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        crate::engine::phases::end_turn(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (3, 0));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(ctx.points(0), 2, "0 might still holds");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![CRAB]
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} holds {{zone {}}}", fixtures::BF1)));
    }

    #[test]
    fn a_dying_crab_chooses_the_only_opponent_lets_its_controller_look_and_gains_one_xp() {
        let mut fixture = alone_at_base();
        let mut ctx = fixture.ctx();
        assert_eq!(xp_of(&ctx, 0), 0);
        assert_eq!(ctx.blob.seat(0).looks_facedown_of, 0);
        ctx.kill(CRAB, crate::engine::ctx::Cause::Cleanup { last_item: None });
        assert_eq!(ctx.deaths.len(), 1, "queued before the body left");
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one opponent answers its own prompt"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == CRAB
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Seat(1)]);
        assert_eq!(xp_of(&ctx, 0), 0, "the effects wait for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "the table declares an xp counter");
        assert_eq!(xp_of(&ctx, 0), i32::from(DEATHKNELL_XP));
        assert_eq!(xp_of(&ctx, 1), 0, "only its controller gains it");
        assert!(ctx
            .effects
            .contains(&Effect::score(0, COUNTER_XP, i32::from(DEATHKNELL_XP))));
        assert_eq!(
            ctx.blob.seat(0).looks_facedown_of,
            0b10,
            "seat 0 may look at seat 1's facedown cards"
        );
        assert_eq!(ctx.blob.seat(1).looks_facedown_of, 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} may look at {seat 1}'s facedown cards this turn".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        let mut blob = ctx.blob.clone();
        blob.seat_mut(0).reset_turn();
        assert_eq!(
            blob.seat(0).looks_facedown_of,
            0,
            "the permission is this turn only"
        );
    }

    #[test]
    fn a_dying_crab_reveals_the_chosen_opponents_hand() {
        let mut fixture = alone_at_base();
        let mut ctx = fixture.ctx();
        let theirs = ctx.hand_of(1);
        assert_eq!(theirs, [fixtures::THEIR_HAND_CARD]);
        let mine = ctx.hand_of(0);
        ctx.kill(CRAB, crate::engine::ctx::Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Reveal { .. })),
            "the reveal waits for the chain"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert!(
            ctx.effects.contains(&Effect::Reveal {
                card: fixtures::THEIR_HAND_CARD
            }),
            "they reveal their hand: {:?}",
            ctx.effects
        );
        assert!(
            !ctx.table.is_revealed(fixtures::THEIR_HAND_CARD),
            "411.1.a · a reveal is a debt the host pays, not a zone change"
        );
        assert_eq!(
            ctx.card(fixtures::THEIR_HAND_CARD).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(
            !mine
                .iter()
                .any(|card| ctx.effects.contains(&Effect::Reveal { card: *card })),
            "only the chosen opponent reveals"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} reveals their hand".to_string()));
        let reveal = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 1} reveals their hand")
            .unwrap();
        let look = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 0} may look at {seat 1}'s facedown cards this turn")
            .unwrap();
        let xp = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 0} gains 1 XP")
            .unwrap();
        assert!(reveal < look && look < xp, "the text's order");
    }

    #[test]
    fn a_dying_crab_shows_its_controller_the_opponents_facedown_card() {
        let mut fixture = alone_at_base();
        {
            let hidden = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
            hidden.zone = Some(fixtures::BF2);
            hidden.name.clear();
            hidden.kind = None;
            hidden.might = None;
            hidden.energy = None;
        }
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.card_state_mut(fixtures::THEIR_UNIT).hidden_at = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.is_facedown(fixtures::THEIR_UNIT));
        ctx.kill(CRAB, crate::engine::ctx::Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert_eq!(ctx.blob.seat(0).looks_facedown_of, 0b10);
        assert!(
            ctx.effects.contains(&Effect::Peek {
                card: fixtures::THEIR_UNIT,
                seat: 0
            }),
            "the looker is handed the facedown face: {:?}",
            ctx.effects
        );
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Peek { seat: 1, .. }
                    | Effect::Reveal {
                        card: fixtures::THEIR_UNIT
                    }
            )),
            "a look is not a reveal and reaches one seat"
        );
        assert!(
            ctx.is_facedown(fixtures::THEIR_UNIT),
            "the card stays facedown on the board"
        );
    }

    #[test]
    fn a_lone_crab_left_at_a_battlefield_keeps_control_without_scoring_twice() {
        let mut fixture = alone_at_base();
        fixture.table.card_mut(CRAB).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        assert_eq!(ctx.points(0), 1);
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Kept(0)
        );
        assert_eq!(ctx.points(0), 1, "once per battlefield per turn");
    }

    #[test]
    fn the_other_seat_cannot_play_the_crab_and_an_empty_rune_pool_refuses_it() {
        let mut fixture = in_hand();
        fixture
            .table
            .cards
            .push(crab(THEIR_CRAB, fixtures::HAND, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CRAB)),
            Err(Refusal::NotYourTurn),
            "seat 1 cannot play its crab on seat 0's turn"
        );
        let mut broke = in_hand();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let action = fixtures::move_action(CRAB, fixtures::BASE, 0);
        let mut ctx = broke.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            play_engine::begin(&mut ctx, 0, CRAB, Origin::Hand, Some(Location::Base(0))),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
        assert!(ctx.blob.chain.is_empty(), "nothing triggered");
        assert!(
            ctx.blob.pending(1).is_some(),
            "the play never left its pending stage"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Played { card, .. } if *card == CRAB)));
        assert_eq!(ctx.hand_of(0).len(), hand, "no draw from a refused play");
    }
}

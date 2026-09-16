use super::possession::ENEMY_UNIT_AT_A_BATTLEFIELD;
use super::prelude::{a_card, at_end_of_turn, card_target, done, play, ready, spell, triggered};
use super::{Card, Flow, Item, Keyword, Stage, Trigger};
use crate::engine::control;
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const RELEASE_AT_END_OF_TURN: u8 = 1;

pub fn takes_control_where_it_stands(ctx: &mut Ctx, item: &Item, unit: u32) -> bool {
    ctx.set_controller_in_place(unit, item.controller, unit)
}

pub fn release(ctx: &mut Ctx, unit: u32) -> bool {
    if !control::revert(ctx, unit) {
        return false;
    }
    ctx.narrate(format!("{{card {unit}}} is recalled"));
    true
}

fn take_over(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !takes_control_where_it_stands(ctx, item, unit) {
        return done();
    }
    ready(ctx, unit);
    at_end_of_turn(ctx, item, RELEASE_AT_END_OF_TURN, vec![unit]);
    ctx.narrate(format!(
        "{{card {unit}}} returns to its owner at the end of the turn"
    ));
    done()
}

fn let_go(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(TargetRef::Card(unit)) = item.targets.first() {
        release(ctx, *unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Hostile Takeover",
    &[Keyword::Hidden],
    &[
        play(
            &[a_card(
                ENEMY_UNIT_AT_A_BATTLEFIELD,
                "an enemy unit at a battlefield",
            )],
            take_over,
        ),
        triggered(Trigger::Reflexive, &[], let_go),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{cleanup, phases, priority, settle, showdown};
    use crate::state::{ItemKind, Priority, PromptWhy, When};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const TAKEOVER: u32 = 90;
    const MIND_ORDER_RUNES: [u32; 6] = [100, 101, 102, 103, 104, 105];

    fn takeover(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into(), "Order".into()],
            ..fixtures::spell(TAKEOVER, fixtures::HAND, seat, "Hostile Takeover", 5, 2)
        }
    }

    fn boardroom() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(takeover(0));
        for (index, rune) in MIND_ORDER_RUNES.iter().enumerate() {
            let domain = if index % 2 == 0 { "Mind" } else { "Order" };
            fixture
                .table
                .cards
                .push(fixtures::rune(*rune, 0, domain, false));
        }
        let jinx = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
        jinx.zone = Some(fixtures::BF1);
        jinx.exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TAKEOVER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn take_jinx(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, TAKEOVER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "enemy units at battlefields only"
        );
        fixtures::choose(ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.controller(fixtures::THEIR_UNIT),
            1,
            "nothing before it resolves"
        );
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        close_showdown(ctx);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "alone there it conquers"
        );
    }

    fn close_showdown(ctx: &mut Ctx) {
        for _ in 0..6 {
            let Some(held) = ctx.blob.showdown.clone() else {
                return;
            };
            assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
            if !ctx.blob.chain.is_empty() {
                let holder = priority::holder(ctx).unwrap();
                priority::pass(ctx, holder).unwrap();
                continue;
            }
            showdown::pass(ctx, held.focus()).unwrap();
        }
    }

    #[test]
    fn the_script_is_a_hidden_spell_over_an_enemy_unit_at_a_battlefield_with_a_delayed_release() {
        assert!(std::ptr::eq(script_of("Hostile Takeover").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(
            CARD.abilities[0].targets[0].filter,
            ENEMY_UNIT_AT_A_BATTLEFIELD
        );
        assert_eq!(
            CARD.abilities[usize::from(RELEASE_AT_END_OF_TURN)].trigger,
            Trigger::Reflexive
        );
    }

    #[test]
    fn the_caster_takes_control_readies_the_unit_and_schedules_the_release() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        take_jinx(&mut ctx);
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            ctx.owner(fixtures::THEIR_UNIT),
            1,
            "ownership never changes"
        );
        assert!(
            !ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "readied"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == fixtures::THEIR_UNIT
        )));
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().control_source,
            Some(fixtures::THEIR_UNIT),
            "the unit is its own control source, so control survives the spell leaving"
        );
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert_eq!(ctx.blob.delayed[0].source, TAKEOVER);
        assert_eq!(ctx.blob.delayed[0].ability, RELEASE_AT_END_OF_TURN);
        assert_eq!(ctx.blob.delayed[0].args, [fixtures::THEIR_UNIT]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} takes control of {card 81}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} returns to its owner at the end of the turn".to_string()));
        assert_eq!(ctx.card(TAKEOVER).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn at_the_end_of_the_turn_control_reverts_and_the_unit_is_recalled_to_its_owners_base() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        take_jinx(&mut ctx);
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.controller(fixtures::THEIR_UNIT),
            0,
            "a cleanup in the same turn leaves the theft alone"
        );
        let before = ctx.effects.len();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the release is a trigger on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger {
                source: TAKEOVER,
                index: RELEASE_AT_END_OF_TURN
            }
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.controller(fixtures::THEIR_UNIT),
            0,
            "not until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx.effects[before..].contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(
            !ctx.events[..].iter().any(
                |event| matches!(event, Event::Moved { card, .. } if *card == fixtures::THEIR_UNIT)
            ),
            "456 · a recall is not a move"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} returns to {seat 1}'s control".to_string()));
        assert!(ctx.blob.log.contains(&"{card 81} is recalled".to_string()));
        assert!(ctx.blob.delayed.is_empty());
        phases::finish_turn(&mut ctx);
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_stolen_unit_that_died_before_the_end_of_the_turn_needs_no_release() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        take_jinx(&mut ctx);
        ctx.kill(fixtures::THEIR_UNIT, crate::engine::ctx::Cause::Rule);
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(
            ctx.blob.delayed.is_empty(),
            "the release ran and found nothing to do"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_the_battlefield_is_not_taken_and_no_release_is_scheduled() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TAKEOVER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Base(1),
            crate::engine::ctx::MoveCause::Effect,
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert!(ctx.blob.delayed.is_empty());
        assert_eq!(ctx.card(TAKEOVER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_unit_and_an_enemy_in_its_base_are_refused_and_the_hand_play_is_a_sorcery() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TAKEOVER).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(TAKEOVER).unwrap().zone, Some(fixtures::HAND));
        drop(ctx);

        let mut fixture = boardroom();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TAKEOVER).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "enemy units in base are out of reach"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        drop(ctx);

        let mut fixture = boardroom();
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: TAKEOVER,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "Hidden alone is not Reaction from hand"
        );
    }

    #[test]
    fn the_taken_unit_stays_at_its_battlefield_and_contests_it_for_the_caster() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TAKEOVER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1)),
            "taken where it stands"
        );
        assert!(
            ctx.blob
                .staged
                .iter()
                .any(|staged| staged.zone == fixtures::BF1)
                || ctx
                    .blob
                    .showdown
                    .as_ref()
                    .is_some_and(|showdown| showdown.zone == fixtures::BF1)
                || ctx.blob.holder(fixtures::BF1) == Some(0),
            "alone there it conquers, otherwise a combat opens"
        );
    }

    #[test]
    fn a_stolen_unit_returned_to_its_owners_hand_goes_to_the_owner_not_the_thief() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        take_jinx(&mut ctx);
        assert_eq!(ctx.owner(fixtures::THEIR_UNIT), 1);
        assert!(crate::cards::prelude::bounce(
            &mut ctx,
            fixtures::THEIR_UNIT
        ));
        let moved = ctx.effects.iter().rev().find_map(|effect| match effect {
            Effect::Move {
                card, zone, seat, ..
            } if *card == fixtures::THEIR_UNIT => Some((*zone, *seat)),
            _ => None,
        });
        assert_eq!(moved, Some((fixtures::HAND, 1)), "to its owner's hand");
    }

    #[test]
    fn a_stolen_unit_that_dies_lands_in_its_owners_trash() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        take_jinx(&mut ctx);
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        ctx.trash(fixtures::THEIR_UNIT);
        let moved = ctx.effects.iter().rev().find_map(|effect| match effect {
            Effect::Move {
                card, zone, seat, ..
            } if *card == fixtures::THEIR_UNIT => Some((*zone, *seat)),
            _ => None,
        });
        assert_eq!(
            moved,
            Some((fixtures::TRASH, 1)),
            "056.2 · a card entering a player's zone enters its owner's"
        );
    }
}

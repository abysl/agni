use super::prelude::{
    a_card, asking, card_target, done, friendly_units, move_destinations, move_unit, play,
    seat_target, spell, target, with_candidates, zone_target, Location, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const MARCHER: usize = 0;
const BATTLEFIELD: usize = 1;
const OPPONENT: usize = 2;
pub const ANSWER: u8 = 1;
pub const QUESTION: &str = "a unit you control to move to the same battlefield";

pub const A_BATTLEFIELD_YOU_CONTROL: TargetSpec = target(
    Filter::And(&[
        Filter::AtBattlefield,
        Filter::Friendly,
        Filter::DifferentLocationFrom(0),
    ]),
    1,
    1,
    TargetKind::Zone,
    "a battlefield you control",
);

pub const AN_OPPONENT: TargetSpec = target(Filter::Enemy, 1, 1, TargetKind::Seat, "an opponent");

pub fn answerers(ctx: &Ctx, seat: u8, zone: u16) -> Vec<u32> {
    let to = Location::Battlefield(zone);
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| move_destinations(ctx, *unit).contains(&to))
        .collect()
}

fn their_units(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != ANSWER {
        return Vec::new();
    }
    let (Some(opponent), Some(zone)) =
        (seat_target(item, OPPONENT), zone_target(item, BATTLEFIELD))
    else {
        return Vec::new();
    };
    answerers(ctx, opponent, zone)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn call(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(zone) = zone_target(item, BATTLEFIELD).filter(|zone| ctx.zones.is_battlefield(*zone))
    else {
        return done();
    };
    let to = Location::Battlefield(zone);
    if stage.0 == ANSWER {
        let Some(opponent) = seat_target(item, OPPONENT) else {
            return done();
        };
        match ctx.picks().first().copied() {
            Some(unit) if answerers(ctx, opponent, zone).contains(&unit) => {
                ctx.narrate(format!(
                    "{{seat {opponent}}} answers the call with {{card {unit}}}"
                ));
                move_unit(ctx, item, unit, to);
            }
            _ => ctx.narrate(format!("{{seat {opponent}}} answers the call with nothing")),
        }
        return done();
    }
    if let Some(unit) = card_target(ctx, item, MARCHER) {
        if move_destinations(ctx, unit).contains(&to) {
            move_unit(ctx, item, unit, to);
        }
    }
    let Some(opponent) = seat_target(item, OPPONENT) else {
        return done();
    };
    match answerers(ctx, opponent, zone).as_slice() {
        [] => {
            ctx.narrate(format!(
                "{{seat {opponent}}} has no unit to move to {{zone {zone}}}"
            ));
            done()
        }
        [only] => {
            let only = *only;
            ctx.narrate(format!(
                "{{seat {opponent}}} answers the call with {{card {only}}}"
            ));
            move_unit(ctx, item, only, to);
            done()
        }
        _ => Flow::Ask(ctx.ask_seat_resume(item, opponent, ANSWER, 1, 1)),
    }
}

pub static CARD: Card = spell(
    "Call to Battle",
    &[],
    &[asking(
        with_candidates(
            play(
                &[
                    a_card(MOVABLE_FRIENDLY_UNIT, "a unit you control"),
                    A_BATTLEFIELD_YOU_CONTROL,
                    AN_OPPONENT,
                ],
                call,
            ),
            their_units,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const CALL: u32 = 90;
    const THEIR_CALL: u32 = 91;
    const THEIR_SECOND: u32 = 92;

    fn call_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Call to Battle", 3, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn muster() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(call_card(CALL, 0));
        fixture.table.cards.push(call_card(THEIR_CALL, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BASE, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
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
    fn the_script_is_a_plain_spell_over_your_unit_a_held_battlefield_and_an_opponent() {
        assert!(std::ptr::eq(script_of("Call to Battle").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 3);
        assert_eq!(ability.targets[0].filter, MOVABLE_FRIENDLY_UNIT);
        assert_eq!(ability.targets[1], A_BATTLEFIELD_YOU_CONTROL);
        assert_eq!(ability.targets[2], AN_OPPONENT);
        assert_eq!(AN_OPPONENT.kind, TargetKind::Seat);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = muster();
        let ctx = fixture.ctx();
        assert_eq!(
            answerers(&ctx, 1, fixtures::BF1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, THEIR_SECOND]
        );
        assert_eq!(
            answerers(&ctx, 1, fixtures::BF2),
            [fixtures::THEIR_UNIT, THEIR_SECOND],
            "the Sprite already stands there"
        );
        assert_eq!(answerers(&ctx, 0, fixtures::BF1), [fixtures::VI]);
    }

    #[test]
    fn your_unit_marches_to_your_battlefield_and_the_opponent_must_send_one_of_theirs_after_it() {
        let mut fixture = muster();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "only the battlefield you control; the other seat holds the second"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{seat 1}", "cancel"],
            "the opponent is chosen with the targets"
        );
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Seat(1)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ANSWER
            })
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(1),
            "the opponent picks"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}"],
            "every unit they control that can move there · no skip, the move is not optional"
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the caster cannot answer for them"
        );
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} answers the call with {card 81}".to_string()));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "450 · their unit contests your battlefield"
        );
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_opponent_with_one_unit_that_can_come_sends_it_without_being_asked() {
        let mut fixture = muster();
        fixture
            .table
            .cards
            .retain(|card| card.id != THEIR_SECOND && card.id != fixtures::SPRITE);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_no_unit_of_theirs_able_to_come_the_call_goes_unanswered() {
        let mut fixture = muster();
        fixture
            .table
            .cards
            .retain(|card| card.owner != 1 || !card.is_kind("Unit"));
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} has no unit to move to {{zone {}}}",
            fixtures::BF1
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_marcher_that_left_the_board_still_leaves_the_opponent_answering_the_call() {
        let mut fixture = muster();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ANSWER
            }),
            "356.3.e · the battlefield and the opponent are still legal, so the second sentence runs"
        );
        fixtures::choose(&mut ctx, 1, "{card 92}").unwrap();
        assert_eq!(
            ctx.location(THEIR_SECOND),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_enemy_units_unheld_battlefields_and_yourself() {
        let mut fixture = muster();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CALL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        for wrong in [fixtures::SPRITE, fixtures::THEIR_UNIT, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit you control on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for wrong in [fixtures::BF2, fixtures::BASE, fixtures::TRASH] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a battlefield you control"
            );
        }
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 2, &[0]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "you are no opponent of yours"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        let mut landless = muster();
        landless.blob.set_holder(fixtures::BF1, None);
        let mut ctx = landless.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "you control no battlefield: only the way out"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::HAND));
    }
}

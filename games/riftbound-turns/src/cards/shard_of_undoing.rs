use super::prelude::{
    asking, done, friendly_units, gear, on_friendly_unit_dies, once_each_turn, remember_card,
    remembered_cards, when, with_candidates,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Cause, Ctx, Event};
use crate::engine::kill;
use crate::state::{Phase, TargetRef};

pub const KILLS_EACH: usize = 1;
pub const QUESTION: &str = "one of your units to kill";

pub fn a_friendly_unit_died_during_your_beginning_phase(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    let me = ctx.controller(source.card);
    let Event::Died {
        controller,
        unit: true,
        ..
    } = event
    else {
        return false;
    };
    *controller == me && ctx.turn_player() == me && ctx.blob.phase() == Some(Phase::Beginning)
}

pub fn opponents_in_turn_order(ctx: &Ctx, me: u8) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = order.next_seat(me);
    for _ in 1..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

fn units_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit))
        .collect()
}

fn seat_at(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    opponents_in_turn_order(ctx, item.controller)
        .get(index)
        .copied()
}

pub fn their_units(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    seat_at(ctx, item, stage)
        .map(|seat| units_of(ctx, seat))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn each_opponent_kills_one(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = opponents_in_turn_order(ctx, item.controller);
    let mut next = usize::from(stage.0);
    let mut chosen = remembered_cards(item);
    if let Some(chooser) = seat_at(ctx, item, stage) {
        if let Some(unit) = ctx.picks().first().copied() {
            if units_of(ctx, chooser).contains(&unit) {
                ctx.narrate(format!("{{seat {chooser}}} chooses {{card {unit}}}"));
                remember_card(ctx, unit);
                chosen.push(unit);
            }
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        if units_of(ctx, seat).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no unit to kill"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1));
    }
    if !chosen.is_empty() {
        ctx.narrate("the chosen units are killed together".to_string());
        kill::batch(ctx, &chosen, Cause::Item(item.id));
    }
    done()
}

pub static CARD: Card = gear(
    "Shard of Undoing",
    &[],
    &[once_each_turn(when(
        asking(
            with_candidates(
                on_friendly_unit_dies(&[], each_opponent_kills_one),
                their_units,
            ),
            QUESTION,
        ),
        a_friendly_unit_died_during_your_beginning_phase,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::activated;
    use crate::cards::{script_of, Cost, Once, Timing, Trigger, Who};
    use crate::engine::ctx::Killed;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, prompts, settle};
    use crate::state::{GameBlob, Mode, Noted, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const SHARD: u32 = 90;
    const ALLY: u32 = 91;

    static UNDOING_BY_HAND: Card = gear(
        "Shard of Undoing",
        &[],
        &[asking(
            with_candidates(
                activated(Timing::Sorcery, Cost::FREE, &[], each_opponent_kills_one),
                their_units,
            ),
            QUESTION,
        )],
    );

    fn shard() -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            ..fixtures::gear(SHARD, fixtures::BASE, 0, CARD.name, 6)
        }
    }

    fn reliquary() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shard());
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.resolve();
        fixture
    }

    fn wired() -> Fixture {
        let mut fixture = reliquary();
        fixture.scripts = fixture.scripts.clone().with_script(SHARD, &UNDOING_BY_HAND);
        fixture
    }

    fn died(card: u32, controller: u8, unit: bool) -> Event {
        Event::Died {
            card,
            controller,
            unit,
            noted: Noted {
                zone: fixtures::BASE,
                might: 2,
                controller,
                alone: false,
                buffed: false,
            },
        }
    }

    fn source() -> Source {
        Source {
            card: SHARD,
            ability: 0,
        }
    }

    #[test]
    fn the_script_is_the_pool_name_with_a_once_each_turn_conditioned_friendly_death_trigger() {
        assert!(std::ptr::eq(script_of("Shard of Undoing").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert_eq!(ability.once, Once::PerTurn);
        assert!(ability.condition.is_some());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert_eq!(KILLS_EACH, 1);
        let fixture = reliquary();
        assert!(std::ptr::eq(fixture.scripts.of_card(SHARD).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_reads_a_friendly_unit_death_in_your_own_beginning_phase_only() {
        let mut fixture = reliquary();
        let ctx = fixture.ctx();
        let seam = a_friendly_unit_died_during_your_beginning_phase;
        assert!(
            !seam(&ctx, &died(ALLY, 0, true), source()),
            "the Action Phase is not the Beginning Phase"
        );
        ctx.blob.set_phase(Phase::Beginning);
        assert!(seam(&ctx, &died(ALLY, 0, true), source()));
        assert!(
            !seam(&ctx, &died(fixtures::THEIR_UNIT, 1, true), source()),
            "an enemy unit is not friendly"
        );
        assert!(
            !seam(&ctx, &died(fixtures::HAND_GEAR, 0, false), source()),
            "gear is not a unit"
        );
        assert!(!seam(&ctx, &Event::Drew { seat: 0, nth: 1 }, source()));
        drop(ctx);
        let mut theirs = reliquary();
        theirs.blob = GameBlob::start(2, 1, Mode::Enforced);
        theirs.blob.set_phase(Phase::Beginning);
        let ctx = theirs.ctx();
        assert_eq!(ctx.turn_player(), 1);
        assert!(
            !seam(&ctx, &died(ALLY, 0, true), source()),
            "their Beginning Phase is not yours"
        );
    }

    #[test]
    fn the_opponents_are_every_other_seat_after_you_in_turn_order() {
        let mut fixture = reliquary();
        let ctx = fixture.ctx();
        assert_eq!(opponents_in_turn_order(&ctx, 0), [1]);
        assert_eq!(opponents_in_turn_order(&ctx, 1), [0]);
    }

    #[test]
    fn each_opponent_picks_one_of_their_units_and_the_picks_are_killed_together() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SHARD, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.seat, prompt.min, prompt.max),
            (1, 1, 1),
            "the opponent chooses"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{seat 1}: choose one of your units to kill (0 of 1)"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT)
            ],
            "their units only"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(fixtures::SPRITE), "one, not all");
        assert!(
            ctx.on_board(ALLY) && ctx.on_board(fixtures::VI),
            "yours are safe"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} chooses {{card {}}}",
            fixtures::THEIR_UNIT
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_opponent_without_units_is_skipped_and_nothing_is_killed() {
        let mut fixture = wired();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(SHARD, &UNDOING_BY_HAND);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SHARD, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit to kill".to_string()));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx.on_board(ALLY));
    }

    #[test]
    fn a_friendly_death_outside_your_beginning_phase_queues_nothing_for_the_shard() {
        let mut fixture = reliquary();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(
            !ctx.blob
                .chain
                .iter()
                .any(|item| item.kind.source() == SHARD),
            "the Action Phase is not the Beginning Phase"
        );
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
    }

    #[test]
    fn the_first_friendly_death_in_your_beginning_phase_makes_each_opponent_kill_one_of_theirs() {
        let mut fixture = reliquary();
        let mut ctx = fixture.ctx();
        ctx.blob.set_phase(Phase::Beginning);
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the first time each turn");
        assert!(ctx.on_board(fixtures::SPRITE));
    }
}

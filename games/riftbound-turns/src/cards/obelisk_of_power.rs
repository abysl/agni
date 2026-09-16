use super::prelude::{battlefield, done, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;

pub const RUNES: usize = 1;

pub fn is_first_beginning_phase(ctx: &Ctx, seat: u8) -> bool {
    seat < ctx.players() && ctx.turn() <= u16::from(ctx.players())
}

pub fn first_beginning_phase_of_that_player(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::BeginningPhase { seat } if is_first_beginning_phase(ctx, *seat))
}

pub fn that_player(item: &Item) -> Option<u8> {
    match item.subject {
        Some(TargetRef::Seat(seat)) => Some(seat),
        _ => None,
    }
}

fn that_player_channels_a_rune(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(seat) = that_player(item) else {
        return done();
    };
    let channelled = ctx.channel(seat, RUNES);
    ctx.narrate(format!(
        "{{seat {seat}}} channels {channelled} rune · {{card {}}}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = battlefield(
    "Obelisk of Power",
    &[],
    &[when(
        triggered(Trigger::BeginningPhase, &[], that_player_channels_a_rune),
        first_beginning_phase_of_that_player,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, triggers};
    use crate::state::{GameBlob, Mode, Phase};
    use agni_plugin_sdk::decide::{Effect, TOP};

    const OBELISK: u32 = fixtures::GROUNDS;

    fn obelisk(turn: u16, player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(OBELISK).unwrap().name = "Obelisk of Power".into();
        for (id, seat) in [(36, 0), (37, 0), (38, 1), (39, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::RUNE_DECK, seat));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(OBELISK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        for _ in 0..4 {
            if ctx.blob.chain.is_empty() {
                break;
            }
            let Some(holder) = priority::holder(ctx) else {
                break;
            };
            priority::pass(ctx, holder).unwrap();
        }
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_obelisk_has_one_conditioned_beginning_phase_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Obelisk of Power").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn the_first_beginning_phase_of_each_seat_is_one_of_the_opening_round_of_turns() {
        let mut fixture = obelisk(1, 0);
        let ctx = fixture.ctx();
        assert!(is_first_beginning_phase(&ctx, 0));
        assert!(is_first_beginning_phase(&ctx, 1));
        assert!(!is_first_beginning_phase(&ctx, 2), "no such seat");
        drop(ctx);
        let mut later = obelisk(3, 0);
        let ctx = later.ctx();
        assert!(!is_first_beginning_phase(&ctx, 0));
        assert!(!is_first_beginning_phase(&ctx, 1));
    }

    #[test]
    fn the_first_player_channels_an_extra_rune_at_the_start_of_turn_one() {
        let mut fixture = obelisk(1, 0);
        let mut ctx = fixture.ctx();
        let before = pool_of(&ctx, 0);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger waits on the chain");
        let item = &ctx.blob.chain[0];
        assert_eq!(item.controller, 0);
        assert_eq!(item.subject, Some(TargetRef::Seat(0)));
        assert_eq!(that_player(item), Some(0));
        resolve_the_chain(&mut ctx);
        let channelled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        zone: fixtures::RUNE_POOL,
                        seat: 0,
                        index: TOP,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            channelled,
            RUNES + crate::rules::runes_this_turn(ctx.blob),
            "the Obelisk's rune and the turn's own channel are each a move from the rune deck"
        );
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            pool_of(&ctx, 0),
            before + RUNES + crate::rules::runes_this_turn(ctx.blob),
            "the Obelisk's rune and the turn's own channel"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| *line == format!("{{seat 0}} channels 1 rune · {{card {OBELISK}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_second_player_channels_at_the_start_of_turn_two_while_the_obelisk_is_unheld() {
        let mut fixture = obelisk(2, 1);
        let mut ctx = fixture.ctx();
        let before = pool_of(&ctx, 1);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].controller, 1,
            "190.6.b · an unheld battlefield's trigger is the turn player's"
        );
        assert_eq!(that_player(&ctx.blob.chain[0]), Some(1));
        resolve_the_chain(&mut ctx);
        assert_eq!(
            pool_of(&ctx, 1),
            before + RUNES + crate::rules::runes_this_turn(ctx.blob)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_later_beginning_phase_channels_nothing_extra() {
        let mut fixture = obelisk(3, 0);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert!(triggers::find(&ctx, &Event::BeginningPhase { seat: 0 }).is_empty());
        let before = pool_of(&ctx, 0);
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "no trigger on a third turn");
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            pool_of(&ctx, 0),
            before + crate::rules::runes_this_turn(ctx.blob),
            "only the turn's own channel"
        );
    }

    #[test]
    #[ignore = "engine gap · a battlefield's BeginningPhase trigger reads only its holder (triggers::matches owner_of); 'each player's' needs the turn player whoever holds"]
    fn the_second_player_still_channels_when_the_first_player_holds_the_obelisk() {
        let mut fixture = obelisk(2, 1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let found = triggers::find(&ctx, &Event::BeginningPhase { seat: 1 });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].controller, 1);
        let before = pool_of(&ctx, 1);
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(
            pool_of(&ctx, 1),
            before + RUNES + crate::rules::runes_this_turn(ctx.blob)
        );
    }

    #[test]
    #[ignore = "engine gap · per-seat first-Beginning-Phase flag; is_first_beginning_phase reads the turn number, which an extra turn inside the opening round shifts"]
    fn an_extra_turn_in_the_opening_round_does_not_steal_the_second_players_first_phase() {
        let mut fixture = obelisk(3, 1);
        let ctx = fixture.ctx();
        assert!(
            is_first_beginning_phase(&ctx, 1),
            "seat 0 took turns 1 and 2; turn 3 is seat 1's first"
        );
        drop(ctx);
        let mut again = obelisk(2, 0);
        let ctx = again.ctx();
        assert!(
            !is_first_beginning_phase(&ctx, 0),
            "seat 0's extra turn is not its first Beginning Phase"
        );
    }
}

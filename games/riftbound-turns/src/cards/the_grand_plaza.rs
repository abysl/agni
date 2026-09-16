use super::prelude::{battlefield, done, triggered, when, win_the_game, Location};
use super::{Card, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::{Ctx, Event};

pub const UNITS_TO_WIN: usize = 7;

pub fn units_of_seat_here(ctx: &Ctx, plaza: u32, seat: u8) -> usize {
    let Some(here @ Location::Battlefield(_)) = ctx.location(plaza) else {
        return 0;
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == seat)
        .count()
}

pub fn seven_or_more_of_the_holders_units_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Held { seat, .. } if units_of_seat_here(ctx, source.card, *seat) >= UNITS_TO_WIN
    )
}

fn the_holder_wins(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    win_the_game(ctx, item.controller, item.kind.source());
    done()
}

pub static CARD: Card = battlefield(
    "The Grand Plaza",
    &[],
    &[when(
        triggered(Trigger::Hold(Who::You), &[], the_holder_wins),
        seven_or_more_of_the_holders_units_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, triggers};
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::{GameBlob, Mode, Phase};

    const PLAZA: u32 = fixtures::GROUNDS;
    const CROWD: u32 = 90;

    fn plaza(mine: usize, theirs: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = 3;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.table.card_mut(PLAZA).unwrap().name = "The Grand Plaza".into();
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        for offset in 0..mine {
            let id = CROWD + offset as u32;
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BF1, 0, "Recruit", 1));
        }
        for offset in 0..theirs {
            let id = CROWD + 20 + offset as u32;
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BASE, 1, "Recruit", 1));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PLAZA).unwrap(), &CARD));
        fixture
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
    fn the_plaza_has_one_conditioned_hold_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("The Grand Plaza").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn the_count_reads_the_holders_units_here_and_nobody_elses() {
        let mut fixture = plaza(3, 2);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(units_of_seat_here(&ctx, PLAZA, 0), 3);
        assert_eq!(
            units_of_seat_here(&ctx, PLAZA, 1),
            1,
            "Jinx stands here too"
        );
        assert_eq!(
            units_of_seat_here(&ctx, fixtures::ROCKFALL, 1),
            1,
            "the Sprite"
        );
        assert_eq!(
            units_of_seat_here(&ctx, fixtures::VI, 0),
            0,
            "not a battlefield"
        );
        assert!(triggers::find(
            &ctx,
            &Event::Held {
                zone: fixtures::BF1,
                seat: 0,
                units: vec![]
            }
        )
        .is_empty());
    }

    #[test]
    fn holding_with_seven_units_here_wins_the_game_for_the_holder() {
        let mut fixture = plaza(7, 3);
        let mut ctx = fixture.ctx();
        let found = triggers::find(
            &ctx,
            &Event::Held {
                zone: fixtures::BF1,
                seat: 0,
                units: vec![],
            },
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].controller, 0);
        assert_eq!(found[0].source, PLAZA);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.points(0), 1, "the hold itself scores first");
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Plaza's trigger waits on the chain"
        );
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.won, None, "nothing until it resolves");
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.won, Some(0));
        assert_eq!(ctx.winner(), Some(0));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| *line == format!("{{seat 0}} wins the game · {{card {PLAZA}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_with_six_units_here_scores_the_hold_and_nothing_more() {
        let mut fixture = plaza(6, 0);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the condition fails at the trigger"
        );
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.won, None);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn seven_units_of_the_other_seat_in_their_base_count_for_nothing_on_a_hold_here() {
        let mut fixture = plaza(2, 7);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.won, None);
    }

    #[test]
    fn a_unit_leaving_in_reaction_does_not_undo_the_trigger() {
        let mut fixture = plaza(7, 0);
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 3);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.recall(CROWD, false);
        assert_eq!(units_of_seat_here(&ctx, PLAZA, 0), 6);
        resolve_the_chain(&mut ctx);
        assert_eq!(
            ctx.won,
            Some(0),
            "383.2.a.1 · the if is part of the condition, not the effect"
        );
        assert_eq!(ctx.points(0), DEFAULT_VICTORY_SCORE - 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_win_leaves_the_point_counters_alone() {
        let mut fixture = plaza(7, 0);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.won, Some(0));
        assert_eq!(ctx.points(0), 1, "the hold's point only");
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with("points")));
        assert_eq!(ctx.blob.won, Some(0), "the win outlives the request");
        assert!(ctx.blob.log.contains(&"{seat 0} wins the game".to_string()));
        let encoded = ctx.blob.encode();
        drop(ctx);
        let decoded = GameBlob::decode(&encoded).unwrap();
        assert_eq!(decoded.won, Some(0));
    }
}

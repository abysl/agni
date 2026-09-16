use super::obelisk_of_power::{first_beginning_phase_of_that_player, that_player};
use super::prelude::{battlefield, done, score_point, triggered, when};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;

fn that_player_gains_a_point(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(seat) = that_player(item) {
        score_point(ctx, seat);
    }
    done()
}

pub static CARD: Card = battlefield(
    "The Arena's Greatest",
    &[],
    &[when(
        triggered(Trigger::BeginningPhase, &[], that_player_gains_a_point),
        first_beginning_phase_of_that_player,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, triggers};
    use crate::rules::{COUNTER_POINTS, DEFAULT_VICTORY_SCORE};
    use crate::state::{GameBlob, Mode, Phase, TargetRef};
    use agni_plugin_sdk::decide::Effect;

    const ARENA: u32 = fixtures::GROUNDS;

    fn arena(turn: u16, player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(ARENA).unwrap().name = "The Arena's Greatest".into();
        for (id, seat) in [(36, 0), (37, 0), (38, 1), (39, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::RUNE_DECK, seat));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ARENA).unwrap(), &CARD));
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
    fn the_arena_has_one_conditioned_beginning_phase_trigger_shared_with_the_obelisk() {
        assert!(std::ptr::eq(
            crate::cards::script_of("The Arena's Greatest").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(std::ptr::fn_addr_eq(
            ability.condition.unwrap(),
            first_beginning_phase_of_that_player as fn(&Ctx, &Event, crate::cards::Source) -> bool
        ));
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn the_first_player_gains_a_point_at_the_start_of_turn_one() {
        let mut fixture = arena(1, 0);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger waits on the chain");
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.blob.chain[0].subject, Some(TargetRef::Seat(0)));
        assert_eq!(ctx.points(0), 0, "nothing until it resolves");
        resolve_the_chain(&mut ctx);
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.points(1), 0);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} scores 1 point"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_second_player_gains_a_point_at_the_start_of_turn_two_while_the_arena_is_unheld() {
        let mut fixture = arena(2, 1);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].controller, 1,
            "190.6.b · an unheld battlefield's trigger is the turn player's"
        );
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.points(1), 1);
        assert_eq!(
            ctx.points(0),
            0,
            "the first player's point came on their own turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_point_is_a_gain_not_a_conquer_so_the_final_point_rule_does_not_hold_it_back() {
        let mut fixture = arena(2, 1);
        fixture.set_points(1, DEFAULT_VICTORY_SCORE - 1);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(
            ctx.points(1),
            DEFAULT_VICTORY_SCORE,
            "471.1.a.1 · points from sources other than Conquer are unrestricted"
        );
        assert_eq!(ctx.won, Some(1));
    }

    #[test]
    fn a_later_beginning_phase_gains_nothing() {
        let mut fixture = arena(3, 0);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(triggers::find(&ctx, &Event::BeginningPhase { seat: 0 }).is_empty());
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "no trigger on a third turn");
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            ctx.points(0),
            1,
            "the hold scored its point; the Arena added none"
        );
    }

    #[test]
    #[ignore = "engine gap · a battlefield's BeginningPhase trigger reads only its holder (triggers::matches owner_of); 'each player's' needs the turn player whoever holds"]
    fn the_second_player_still_gains_a_point_when_the_first_player_holds_the_arena() {
        let mut fixture = arena(2, 1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let found = triggers::find(&ctx, &Event::BeginningPhase { seat: 1 });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].controller, 1);
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.points(1), 1);
    }
}

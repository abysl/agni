use super::prelude::{battlefield, deal, done, location_of, triggered};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 1;

pub fn units_here(ctx: &Ctx, fortress: u32) -> Vec<u32> {
    let Some(here) = location_of(ctx, fortress) else {
        return Vec::new();
    };
    let mut units = ctx.units_at(here);
    units.sort_unstable();
    units
}

fn deal_one_to_each_unit_here(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let units = units_here(ctx, me);
    if units.is_empty() {
        ctx.narrate(format!("{{card {me}}}: no unit here to freeze"));
        return done();
    }
    for unit in units {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
        }
    }
    done()
}

pub static CARD: Card = battlefield(
    "Frozen Fortress",
    &[],
    &[triggered(
        Trigger::BeginningPhase,
        &[],
        deal_one_to_each_unit_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, triggers};
    use crate::state::{GameBlob, Mode, Phase, TargetRef};

    const FORTRESS: u32 = fixtures::GROUNDS;
    const FRAIL: u32 = 90;
    const THEIR_FRAIL: u32 = 91;

    fn fortress(turn: u16, player: u8, holder: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(FORTRESS).unwrap().name = "Frozen Fortress".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(FRAIL, fixtures::BF1, 0, "Frail", 1));
        for (id, seat) in [(36, 0), (37, 0), (38, 1), (39, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::RUNE_DECK, seat));
        }
        fixture.blob.set_holder(fixtures::BF1, holder);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FORTRESS).unwrap(),
            &CARD
        ));
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
    fn the_fortress_is_one_unconditioned_beginning_phase_trigger() {
        assert!(std::ptr::eq(script_of("Frozen Fortress").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert_eq!(DAMAGE, 1);
    }

    #[test]
    fn units_here_lists_every_unit_at_the_fortress_in_id_order_and_nothing_elsewhere() {
        let mut fixture = fortress(3, 0, Some(0));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            units_here(&ctx, FORTRESS),
            [fixtures::VI, fixtures::THEIR_UNIT, FRAIL]
        );
        assert!(units_here(&ctx, fixtures::ROCKFALL).is_empty());
        assert!(
            units_here(&ctx, fixtures::LEGEND_CARD).is_empty(),
            "a card off the board has no location to read"
        );
    }

    #[test]
    fn at_the_start_of_the_holders_turn_each_unit_here_takes_one_before_the_hold_scores() {
        let mut fixture = fortress(3, 0, Some(0));
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger waits on the chain");
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.blob.chain[0].subject, Some(TargetRef::Seat(0)));
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "nothing until it resolves");
        assert_eq!(ctx.points(0), 0, "the hold has not scored yet");
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::VI), 1);
        assert!(
            !ctx.on_board(FRAIL),
            "one damage on one Might is lethal at cleanup"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 1, .. } if *card == fixtures::VI
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == FRAIL
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FORTRESS}}} deals 1 to {{card {FRAIL}}}")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            ctx.points(0),
            1,
            "Vi survives at the fortress and the hold scores after the damage"
        );
        assert_eq!(
            ctx.damage_on(fixtures::VI),
            1,
            "the damage stays on her for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_frail_holder_dies_before_scoring_and_the_hold_is_lost() {
        let mut fixture = fortress(3, 0, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert!(!ctx.on_board(FRAIL));
        assert_eq!(ctx.points(0), 0, "this happens before scoring");
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }

    #[test]
    fn an_unheld_fortress_freezes_units_of_both_sides_on_the_turn_players_trigger() {
        let mut fixture = fortress(4, 1, None);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(fixtures::unit(
            THEIR_FRAIL,
            fixtures::BF1,
            1,
            "Their Frail",
            1,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let found = triggers::find(&ctx, &Event::BeginningPhase { seat: 1 });
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].controller, 1,
            "190.6.b · an unheld battlefield's trigger is the turn player's"
        );
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::VI), 1);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 1);
        assert!(!ctx.on_board(FRAIL));
        assert!(!ctx.on_board(THEIR_FRAIL));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FORTRESS}}} deals 1 to {{card {}}}",
            fixtures::THEIR_UNIT
        )));
    }

    #[test]
    fn an_empty_fortress_narrates_and_deals_nothing() {
        let mut fixture = fortress(3, 0, None);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(FRAIL).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_the_chain(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FORTRESS}}}: no unit here to freeze")));
        assert!(ctx.on_board(FRAIL));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
    }

    #[test]
    #[ignore = "engine gap · a battlefield's BeginningPhase trigger reads only its holder (triggers::matches owner_of); 'each player's' needs the turn player whoever holds (The Arena's Greatest's row)"]
    fn the_opponents_beginning_phase_freezes_the_units_here_while_i_hold_the_fortress() {
        let mut fixture = fortress(4, 1, Some(0));
        let mut ctx = fixture.ctx();
        let found = triggers::find(&ctx, &Event::BeginningPhase { seat: 1 });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].controller, 1);
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert!(!ctx.on_board(FRAIL));
    }

    #[test]
    fn today_the_opponents_beginning_phase_leaves_the_units_here_alone_while_i_hold_it() {
        let mut fixture = fortress(4, 1, Some(0));
        let mut ctx = fixture.ctx();
        assert!(triggers::find(&ctx, &Event::BeginningPhase { seat: 1 }).is_empty());
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(FRAIL));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
    }
}

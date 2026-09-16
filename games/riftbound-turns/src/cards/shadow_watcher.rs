use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::state::Phase;

pub fn a_friendly_unit_died_during_your_beginning_phase_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.turn_player() == seat
        && ctx
            .deaths_this_turn()
            .iter()
            .any(|death| death.unit && death.controller == seat && death.phase == Phase::Beginning)
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    a_friendly_unit_died_during_your_beginning_phase_this_turn(ctx, ctx.controller(me))
}

pub static CARD: Card = with_statics(
    unit("Shadow Watcher", &[], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WATCHER: u32 = 90;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 5;
    const CALM_RUNE: u32 = 46;

    fn watcher(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(WATCHER, zone, seat, "Shadow Watcher", MIGHT)
        }
    }

    fn shrine(zone: u16, phase: Phase) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(watcher(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_phase(phase);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WATCHER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_entry_rule_is_the_enters_ready_static() {
        assert!(std::ptr::eq(script_of("Shadow Watcher").unwrap(), &CARD));
        assert_eq!(CARD.name, "Shadow Watcher");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn the_seam_reads_a_friendly_units_death_in_your_own_beginning_phase_only() {
        let mut fixture = shrine(fixtures::HAND, Phase::Beginning);
        let mut ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, WATCHER));
        ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
        assert!(
            !enters_ready(&ctx, WATCHER),
            "an enemy unit is not friendly"
        );
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(enters_ready(&ctx, WATCHER));
        assert!(
            statics::enters_ready(&ctx, WATCHER),
            "the static reaches the seam"
        );
        assert!(
            !enters_ready(&ctx, fixtures::SPRITE),
            "read from the entering unit's controller, whose Beginning Phase this is not"
        );
        drop(ctx);
        let mut action = shrine(fixtures::HAND, Phase::Action);
        let mut ctx = action.ctx();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(
            !enters_ready(&ctx, WATCHER),
            "a death in the Action Phase is not one in the Beginning Phase"
        );
        drop(ctx);
        let mut ending = shrine(fixtures::HAND, Phase::Ending);
        let mut ctx = ending.ctx();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(!enters_ready(&ctx, WATCHER));
    }

    #[test]
    fn played_with_no_beginning_phase_death_it_lands_exhausted_and_is_refused_without_the_calm() {
        let mut fixture = shrine(fixtures::HAND, Phase::Action);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WATCHER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(WATCHER), Some(Location::Base(0)));
        assert!(ctx.card(WATCHER).unwrap().exhausted);
        assert_eq!(
            [42, CALM_RUNE]
                .into_iter()
                .filter(|rune| ctx.card(*rune).unwrap().zone == Some(fixtures::RUNE_DECK))
                .count(),
            1,
            "a Calm rune recycles for the power"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut short = shrine(fixtures::HAND, Phase::Action);
        short.table.cards.retain(|card| card.id != CALM_RUNE);
        short.table.card_mut(42).unwrap().domain = vec!["Fury".into()];
        short.resolve();
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, WATCHER),
            Err(Refusal::NoPowerOf),
            "no Calm rune anywhere in the pool"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn played_in_the_action_phase_after_a_friendly_death_in_the_beginning_phase_it_enters_ready() {
        let mut fixture = shrine(fixtures::HAND, Phase::Beginning);
        let table = {
            let mut ctx = fixture.ctx();
            ctx.kill(fixtures::VI, Cause::Rule);
            ctx.table.clone()
        };
        fixture.commit(table);
        fixture.blob.set_phase(Phase::Action);
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request carries no events");
        assert!(
            enters_ready(&ctx, WATCHER),
            "the Beginning Phase death is remembered for the turn"
        );
        fixtures::play_from_hand(&mut ctx, 0, WATCHER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(WATCHER));
        assert!(!ctx.card(WATCHER).unwrap().exhausted);
    }
}

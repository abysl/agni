use super::prelude::{friendly_units, unit};
use super::Card;
use crate::engine::ctx::{Cause, Ctx, Killed};

pub const KILLS: usize = 1;

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut units = friendly_units(ctx, seat);
    units.sort_unstable();
    units
}

pub fn kill_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    kill_candidates(ctx, seat).len() >= KILLS
}

pub fn pay_kill_cost(ctx: &mut Ctx, unit: u32) -> Killed {
    let killed = ctx.kill(unit, Cause::Cost);
    if killed != Killed::NotOnBoard {
        ctx.narrate(format!("{{card {unit}}} is killed as an additional cost"));
    }
    killed
}

pub static CARD: Card = unit("Cruel Patron", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const PATRON: u32 = 90;
    const SCOUT: u32 = 91;

    fn patron() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Order".into()],
            ..fixtures::unit(PATRON, fixtures::HAND, 0, "Cruel Patron", 6)
        }
    }

    fn court(with_scout: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(patron());
        if with_scout {
            fixture
                .table
                .cards
                .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        }
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_patron_is_a_plain_unit_whose_whole_text_is_the_kill_cost() {
        assert!(std::ptr::eq(script_of("Cruel Patron").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; his is mandatory and kills"
        );
        assert_eq!(KILLS, 1);
    }

    #[test]
    fn the_candidates_are_the_controllers_units_on_the_board_and_the_cost_kills_one() {
        let mut fixture = court(true);
        let mut ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [fixtures::VI, SCOUT]);
        assert_eq!(
            kill_candidates(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT],
            "each seat reads its own units"
        );
        assert!(kill_cost_payable(&ctx, 0));
        assert_eq!(pay_kill_cost(&mut ctx, SCOUT), Killed::Yes);
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == SCOUT
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SCOUT}}} is killed as an additional cost")));
        assert_eq!(
            pay_kill_cost(&mut ctx, SCOUT),
            Killed::NotOnBoard,
            "a dead unit cannot pay twice"
        );
        assert_eq!(kill_candidates(&ctx, 0), [fixtures::VI]);
        let mut alone = court(false);
        alone.table.cards.retain(|card| card.id != fixtures::VI);
        alone.resolve();
        let ctx = alone.ctx();
        assert!(kill_candidates(&ctx, 0).is_empty());
        assert!(
            !kill_cost_payable(&ctx, 0),
            "356.2.a.1 · without a friendly unit the mandatory cost cannot be paid"
        );
    }

    #[test]
    #[ignore = "engine gap · a mandatory non-resource additional cost (kill a friendly unit) at the pay stage; play::advance knows optional rune costs only, so the play neither asks which unit dies nor refuses without one"]
    fn playing_him_asks_which_friendly_unit_dies_and_is_refused_without_one() {
        let mut fixture = court(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PATRON).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "which friendly unit pays: {:?}",
            ctx.blob.why
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {SCOUT}}}"),
                "cancel".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::TRASH),
            "357.2 · the kill is paid before he enters"
        );
        assert_eq!(ctx.location(PATRON), Some(Location::Base(0)));
        let mut alone = court(false);
        alone.table.cards.retain(|card| card.id != fixtures::VI);
        alone.resolve();
        let mut ctx = alone.ctx();
        assert!(
            fixtures::play_from_hand(&mut ctx, 0, PATRON).is_err(),
            "no friendly unit, no play"
        );
        assert_eq!(ctx.card(PATRON).unwrap().zone, Some(fixtures::HAND));
    }
}

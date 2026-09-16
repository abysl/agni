use super::prelude::{activated, done, exhausting_self, gear, legion, named};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Timing};
use crate::engine::ctx::Ctx;

pub fn next_unit_enters_ready(ctx: &mut Ctx, seat: u8) {
    ctx.blob.seat_mut(seat).next_unit_enters_ready = true;
    ctx.narrate(format!(
        "{{seat {seat}}}'s next unit this turn enters ready"
    ));
}

fn shine(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    if !legion(ctx, seat) {
        ctx.narrate(format!(
            "{{card {me}}} does nothing · Legion is off, no other card was played this turn"
        ));
        return done();
    }
    next_unit_enters_ready(ctx, seat);
    done()
}

pub static CARD: Card = gear(
    "Sun Disc",
    &[Keyword::Legion],
    &[named(
        exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], shine)),
        "the next unit you play this turn enters ready",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;

    const DISC: u32 = 90;
    const SECOND: u32 = 91;

    fn temple(played: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut disc = fixtures::gear(DISC, fixtures::BASE, 0, "Sun Disc", 2);
        disc.domain = vec!["Fury".into()];
        disc.power = Some(1);
        fixture.table.cards.push(disc);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::HAND, 0, "Vanguard", 2));
        fixture.blob.seat_mut(0).played_main = played;
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_carries_legion_and_one_untargeted_exhaust_activation() {
        let fixture = temple(false);
        assert!(std::ptr::eq(fixture.scripts.of_card(DISC).unwrap(), &CARD));
        assert_eq!(CARD.name, "Sun Disc");
        assert!(
            CARD.has_keyword(Keyword::Legion),
            "812.3 · Legion is a characteristic of the card"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert!(ability.targets.is_empty());
        assert!(
            ability.usable.is_none(),
            "812.1.b.1 · the ability exists whether or not Legion is on"
        );
    }

    #[test]
    fn without_legion_the_disc_exhausts_for_nothing_and_says_so() {
        let mut fixture = temple(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, DISC, 0).unwrap();
        assert!(ctx.card(DISC).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DISC}}} does nothing · Legion is off, no other card was played this turn"
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, DISC, 0),
            Err(Refusal::Exhausted)
        );
    }

    #[test]
    fn with_legion_the_disc_reaches_the_seam_and_the_other_seat_is_refused() {
        let mut fixture = temple(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, DISC, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, DISC, 0).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0}'s next unit this turn enters ready"));
        assert!(ctx.blob.seat(0).next_unit_enters_ready);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_legion_the_next_unit_played_this_turn_enters_ready_and_the_one_after_does_not() {
        let mut fixture = temple(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, DISC, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            !ctx.card(fixtures::HAND_UNIT).unwrap().exhausted,
            "6477 · it enters ready, it does not enter exhausted and then ready"
        );
        assert!(
            !ctx.blob.seat(0).next_unit_enters_ready,
            "the first unit spends the flag"
        );
        fixtures::play_from_hand(&mut ctx, 0, SECOND).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SECOND));
        assert!(
            ctx.card(SECOND).unwrap().exhausted,
            "the next unit only; the one after enters exhausted"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_played_before_the_unit_does_not_spend_the_flag() {
        let mut fixture = temple(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, DISC, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(fixtures::HAND_GEAR));
        assert!(
            ctx.blob.seat(0).next_unit_enters_ready,
            "a gear is not a unit"
        );
        fixtures::play_from_hand(&mut ctx, 0, SECOND).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SECOND));
        assert!(!ctx.card(SECOND).unwrap().exhausted);
        assert!(!ctx.blob.seat(0).next_unit_enters_ready);
    }
}

use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

pub fn units_enter_ready_this_turn(ctx: &mut Ctx, seat: u8) {
    ctx.blob.seat_mut(seat).units_enter_ready_this_turn = true;
    ctx.narrate(format!("{{seat {seat}}}'s units enter ready this turn"));
}

fn confront(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    units_enter_ready_this_turn(ctx, seat);
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell("Confront", &[Keyword::Action], &[play(&[], confront)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::priority;
    use crate::state::Priority;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CONFRONT: u32 = 90;

    fn confront_card(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(CONFRONT, fixtures::HAND, seat, "Confront", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(confront_card(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CONFRONT).unwrap(),
            &CARD
        ));
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

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn cast_and_resolve(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, CONFRONT).unwrap();
        assert!(ctx.blob.prompt.is_none());
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_an_action_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Confront").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn confront_draws_one_and_announces_the_grant() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_and_resolve(&mut ctx);
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s units enter ready this turn".to_string()));
        assert_eq!(ctx.card(CONFRONT).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_action_is_refused_off_turn_and_while_the_chain_is_closed_to_it() {
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(confront_card(1));
        theirs.resolve();
        let ctx = theirs.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, CONFRONT)),
            Err(Refusal::NotYourTurn)
        );
        let mut closed = armed();
        closed.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let ctx = closed.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, CONFRONT)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "an Action is not a Reaction"
        );
    }

    #[test]
    fn a_unit_played_after_confront_enters_ready() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        cast_and_resolve(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::BASE)
        );
        assert!(
            !ctx.card(fixtures::HAND_UNIT).unwrap().exhausted,
            "Confront's grant overrides the exhausted entry"
        );
    }
}

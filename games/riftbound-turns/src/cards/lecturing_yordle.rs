use super::prelude::{done, draw, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const CARDS: usize = 1;

fn lecture(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    draw(ctx, seat, CARDS);
    done()
}

pub static CARD: Card = unit("Lecturing Yordle", &[Keyword::Tank], &[play(&[], lecture)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::Origin;

    const YORDLE: u32 = 90;

    #[test]
    fn the_yordle_is_a_tank_that_draws_one_when_played() {
        assert!(CARD.has_keyword(Keyword::Tank));
        let mut fixture = Fixture::enforced();
        let mut yordle = fixtures::unit(YORDLE, fixtures::HAND, 0, "Lecturing Yordle", 2);
        yordle.energy = Some(3);
        fixture.table.cards.push(yordle);
        fixture.resolve();
        let action = fixtures::move_action(YORDLE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        play_engine::begin(&mut ctx, 0, YORDLE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
    }
}

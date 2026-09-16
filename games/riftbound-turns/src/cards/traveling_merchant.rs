use super::prelude::{ask_discard, done, draw, on_move, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

const STAGE_DRAW: u8 = 1;
const CARDS: usize = 1;

pub fn discard_then_draw(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == STAGE_DRAW {
        draw(ctx, seat, CARDS);
        return done();
    }
    match ask_discard(ctx, item, STAGE_DRAW) {
        Some(ask) => Flow::Ask(ask),
        None => {
            draw(ctx, seat, CARDS);
            done()
        }
    }
}

pub static CARD: Card = unit(
    "Traveling Merchant",
    &[],
    &[on_move(&[], discard_then_draw)],
);

#[cfg(test)]
mod tests {
    use crate::engine::ctx::{Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::PromptWhy;

    const MERCHANT: u32 = 90;

    #[test]
    fn the_merchant_discards_one_then_draws_one_when_it_moves() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            MERCHANT,
            fixtures::BASE,
            0,
            "Traveling Merchant",
            2,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.move_unit(
            MERCHANT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, "{card 70}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand, "one out, one in");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
    }
}

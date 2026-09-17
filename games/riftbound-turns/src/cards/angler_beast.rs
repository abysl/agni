use super::prelude::{bounce, done, play, unit, units_on_board};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT_LIMIT: i32 = 2;

fn sweep(ctx: &mut Ctx, _: &Item, _: Stage) -> Flow {
    let small: Vec<u32> = units_on_board(ctx)
        .into_iter()
        .filter(|unit| ctx.current_might(*unit) <= MIGHT_LIMIT)
        .collect();
    for unit in small {
        bounce(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit("Angler Beast", &[], &[play(&[], sweep)]);

#[cfg(test)]
mod tests {
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::Origin;

    const BEAST: u32 = 90;

    #[test]
    fn the_beast_bounces_every_small_unit_and_tokens_vanish() {
        let mut fixture = Fixture::enforced();
        let mut beast = fixtures::unit(BEAST, fixtures::HAND, 0, "Angler Beast", 5);
        beast.energy = Some(2);
        beast.power = Some(1);
        fixture.table.cards.push(beast);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().might = Some(2);
        fixture.resolve();
        let action = fixtures::move_action(BEAST, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, BEAST, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND),
            "2 Might goes home"
        );
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "a token ceases to exist"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "3 Might stays"
        );
        assert_eq!(ctx.location(BEAST), Some(Location::Base(0)));
    }
}

use super::prelude::{done, enemy_units, might_this_turn, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = -3;
pub const FLOOR: i32 = 1;

fn watch(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for enemy in enemy_units(ctx, item.controller) {
        might_this_turn(ctx, item, enemy, MIGHT, Some(FLOOR));
    }
    done()
}

pub static CARD: Card = unit(
    "Thousand-Tailed Watcher",
    &[Keyword::Accelerate],
    &[play(&[], watch)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Origin, PromptWhy};

    const WATCHER: u32 = 90;

    #[test]
    fn the_watcher_shrinks_enemy_units_to_a_minimum_of_one_and_may_accelerate() {
        assert!(CARD.has_keyword(Keyword::Accelerate));
        let mut fixture = Fixture::enforced();
        let mut watcher = fixtures::unit(WATCHER, fixtures::HAND, 0, "Thousand-Tailed Watcher", 7);
        watcher.energy = Some(1);
        watcher.power = Some(0);
        fixture.table.cards.push(watcher);
        fixture.resolve();
        let action = fixtures::move_action(WATCHER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, WATCHER, Origin::Hand, Some(Location::Base(0))).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            1,
            "2 - 3 floors at 1"
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 1, "3 - 3 floors at 1");
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "friendly units are untouched"
        );
        assert!(ctx.card(WATCHER).unwrap().exhausted);
    }
}

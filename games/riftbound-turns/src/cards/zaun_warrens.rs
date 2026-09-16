use super::prelude::{battlefield, on_conquer};
use super::traveling_merchant::discard_then_draw;
use super::Card;

pub static CARD: Card = battlefield("Zaun Warrens", &[], &[on_conquer(&[], discard_then_draw)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::triggers;

    #[test]
    fn the_warrens_fire_for_the_seat_that_conquers_them() {
        assert_eq!(CARD.abilities[0].trigger, Trigger::Conquer(Who::You));
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Zaun Warrens".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        let found = triggers::find(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF1,
                seat: 1,
                units: vec![],
            },
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].controller, 1);
        assert_eq!(found[0].source, fixtures::GROUNDS);
        assert!(triggers::find(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF2,
                seat: 1,
                units: vec![],
            },
        )
        .is_empty());
    }
}

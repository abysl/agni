use super::prelude::{unit, with_statics};
use super::{Card, Static};

pub static CARD: Card = with_statics(
    unit("Ruin Runner", &[], &[]),
    &[Static::Untargetable(|_, _| true)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::targets;
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};

    const RUNNER: u32 = 90;

    #[test]
    fn the_runner_is_never_a_candidate_for_an_enemy_item_but_is_for_a_friendly_one() {
        assert!(CARD.has_static(Static::Untargetable(|_, _| true)));
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNNER, fixtures::BASE, 1, "Ruin Runner", 5));
        fixture.resolve();
        let ctx = fixture.ctx();
        let mine = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let spec = prelude::a_unit("a unit");
        assert!(!targets::candidates(&ctx, &mine, &spec).contains(&TargetRef::Card(RUNNER)));
        let theirs = ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        assert!(targets::candidates(&ctx, &theirs, &spec).contains(&TargetRef::Card(RUNNER)));
    }
}

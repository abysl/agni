use super::prelude::{battlefield, with_statics};
use super::{Card, Static};

pub static CARD: Card = with_statics(
    battlefield("Vilemaw's Lair", &[], &[]),
    &[Static::NoMoveToBase],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Location, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::march;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    fn lair() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Vilemaw's Lair".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_lair_is_a_battlefield_whose_only_text_is_the_static() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Vilemaw's Lair").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::NoMoveToBase));
    }

    #[test]
    fn a_unit_at_the_lair_cannot_march_home_but_can_gank_or_be_recalled_by_combat() {
        let mut fixture = lair();
        let ctx = fixture.ctx();
        assert!(!ctx.moves_to_base_from(fixtures::BF1));
        assert!(!ctx.movable_to_base(fixtures::VI));
        let home = EntryMove {
            card: fixtures::VI,
            from: Some(fixtures::BF1),
            from_seat: 0,
            to: Some(fixtures::BASE),
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &home),
            Err(Refusal::Illegal(Reason::NoMoveToBase))
        );
        assert_eq!(
            march::effect_destinations(&ctx, fixtures::VI),
            [Location::Battlefield(fixtures::BF2)],
            "an effect move offers the other battlefield only"
        );
        drop(ctx);
        let mut ctx = fixture.ctx();
        assert_eq!(
            march::effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Base(0)
            ),
            None,
            "an effect that names the base resolves as nothing (358.3.a)"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            march::effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Battlefield(fixtures::BF2)
            ),
            Some(Moved::Moved)
        );
        ctx.recall(fixtures::VI, true);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "the combat recall is not a move (456)"
        );
    }
}

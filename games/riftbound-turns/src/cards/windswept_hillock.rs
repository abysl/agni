use super::prelude::{battlefield, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

fn every_unit_here(_: &Ctx, _: u32, _: u32) -> bool {
    true
}

pub static CARD: Card = with_statics(
    battlefield("Windswept Hillock", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: every_unit_here,
        grants: &[Grant::Keyword(Keyword::Ganking)],
    }],
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

    fn hillock() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Windswept Hillock".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march_of(unit: u32, from: u16, to: u16) -> EntryMove {
        EntryMove {
            card: unit,
            from: Some(from),
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_hillock_is_a_battlefield_whose_only_text_is_a_ganking_aura_over_units_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Windswept Hillock").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Keyword(Keyword::Ganking)],
                ..
            }]
        ));
    }

    #[test]
    fn a_unit_here_has_ganking_and_may_march_to_the_other_battlefield() {
        let mut fixture = hillock();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(())
        );
        assert_eq!(
            legal::classify(
                &ctx,
                0,
                &march_of(fixtures::VI, fixtures::BF1, fixtures::BF2)
            ),
            Ok(legal::Intent::StandardMove {
                unit: fixtures::VI,
                from: Location::Battlefield(fixtures::BF1),
                to: Location::Battlefield(fixtures::BF2)
            })
        );
        assert_eq!(
            march::effect_destinations(&ctx, fixtures::VI),
            [Location::Base(0), Location::Battlefield(fixtures::BF2)],
            "an effect move offers the base and the other battlefield as always"
        );
        drop(ctx);
        let mut ctx = fixture.ctx();
        assert_eq!(
            march::effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Battlefield(fixtures::BF2)
            ),
            Some(Moved::Moved)
        );
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Ganking),
            "the keyword stays with the hillock, not the unit"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_at_another_battlefield_still_needs_its_own_ganking_to_march_across() {
        let mut fixture = hillock();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF2),
                Location::Battlefield(fixtures::BF1)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "marching onto the hillock is an ordinary battlefield-to-battlefield move"
        );
        assert_eq!(
            legal::classify(
                &ctx,
                0,
                &march_of(fixtures::VI, fixtures::BF2, fixtures::BF1)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        assert_eq!(
            march::effect_destinations(&ctx, fixtures::VI),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)],
            "427.1 · an effect move never needed Ganking"
        );
    }

    #[test]
    fn an_enemy_unit_here_ganks_as_well() {
        let mut fixture = hillock();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(())
        );
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub fn cannot_move_to_base(ctx: &Ctx, unit: u32) -> bool {
    !ctx.unit_moves_to_base(unit)
}

pub static CARD: Card = with_statics(unit("Determined Sentry", &[], &[]), &[Static::NoMoveToBase]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_destinations, Location};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::march;
    use crate::Refusal;

    const SENTRY: u32 = 90;

    fn posted(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(SENTRY, at, 0, "Determined Sentry", 1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SENTRY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn home(ctx: &Ctx, unit: u32) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            unit,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        )
    }

    #[test]
    fn the_script_is_a_keywordless_unit_carrying_the_no_move_to_base_static() {
        assert!(std::ptr::eq(script_of("Determined Sentry").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::NoMoveToBase]));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn the_veto_reads_the_sentry_in_play_only_and_never_another_unit() {
        let mut fixture = posted(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(cannot_move_to_base(&ctx, SENTRY));
        assert!(
            !cannot_move_to_base(&ctx, fixtures::VI),
            "I can't, the others can"
        );
        assert!(ctx.set_controller(SENTRY, 1, fixtures::SPRITE));
        assert!(cannot_move_to_base(&ctx, SENTRY), "under either controller");
        assert!(ctx.stun(SENTRY));
        assert!(cannot_move_to_base(&ctx, SENTRY), "stunned or not");
        drop(ctx);
        let mut fixture = posted(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!cannot_move_to_base(&ctx, SENTRY));
        drop(ctx);
        let mut fixture = posted(fixtures::TRASH);
        let ctx = fixture.ctx();
        assert!(!cannot_move_to_base(&ctx, SENTRY));
    }

    #[test]
    fn the_sentry_cannot_march_home_nor_be_moved_home_while_vi_beside_it_can() {
        let mut fixture = posted(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            home(&ctx, SENTRY),
            Err(Refusal::Illegal(Reason::NoMoveToBase))
        );
        assert!(!move_destinations(&ctx, SENTRY).contains(&Location::Base(0)));
        assert!(!ctx.movable_to_base(SENTRY));
        assert_eq!(
            march::effect_move(&mut ctx, &fixtures::effect_of(0), SENTRY, Location::Base(0)),
            None,
            "an effect cannot send it home either"
        );
        assert_eq!(
            ctx.location(SENTRY),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(home(&ctx, fixtures::VI), Ok(()), "Vi is bound by nothing");
        assert!(
            move_destinations(&ctx, SENTRY).contains(&Location::Battlefield(fixtures::BF2)),
            "an effect may still move it between battlefields"
        );
        assert!(ctx.fault.is_none());
    }
}

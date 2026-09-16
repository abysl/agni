use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub fn units_cannot_move_to_base(ctx: &Ctx) -> bool {
    !ctx.units_move_to_base()
}

pub static CARD: Card = with_statics(
    unit("Minotaur Reckoner", &[], &[]),
    &[Static::NoUnitsMoveToBase],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_destinations, Location};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::march;
    use crate::Refusal;

    const RECKONER: u32 = 90;

    fn arena(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(RECKONER, at, 1, "Minotaur Reckoner", 5));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RECKONER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn home(ctx: &Ctx, unit: u32, from: u16, seat: u8) -> Result<(), Refusal> {
        march::legal_destination(ctx, unit, Location::Battlefield(from), Location::Base(seat))
    }

    #[test]
    fn the_script_is_a_keywordless_unit_carrying_the_no_units_move_to_base_static() {
        assert!(std::ptr::eq(script_of("Minotaur Reckoner").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::NoUnitsMoveToBase]));
        assert!(
            !CARD.has_static(Static::NoMoveToBase),
            "the veto is the table's, not his own"
        );
    }

    #[test]
    fn the_veto_reads_any_reckoner_in_play_on_either_side_and_none_in_hand_or_the_trash() {
        let mut fixture = arena(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!units_cannot_move_to_base(&ctx));
        drop(ctx);
        let mut fixture = arena(fixtures::TRASH);
        let ctx = fixture.ctx();
        assert!(!units_cannot_move_to_base(&ctx));
        drop(ctx);
        let mut fixture = arena(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(units_cannot_move_to_base(&ctx), "his own base is in play");
        assert!(ctx.set_controller(RECKONER, 0, fixtures::VI));
        assert!(units_cannot_move_to_base(&ctx), "under either controller");
        ctx.recall(RECKONER, true);
        assert!(units_cannot_move_to_base(&ctx), "exhausted or not");
        assert!(ctx.stun(RECKONER));
        assert!(units_cannot_move_to_base(&ctx), "stunned or not");
    }

    #[test]
    fn a_reckoner_in_hand_binds_no_one_so_the_march_home_is_open() {
        let mut fixture = arena(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!units_cannot_move_to_base(&ctx));
        assert_eq!(home(&ctx, fixtures::VI, fixtures::BF1, 0), Ok(()));
        assert_eq!(home(&ctx, fixtures::SPRITE, fixtures::BF2, 1), Ok(()));
        assert!(move_destinations(&ctx, fixtures::VI).contains(&Location::Base(0)));
        assert!(ctx.movable_to_base(fixtures::VI));
    }

    #[test]
    fn with_a_reckoner_in_play_no_unit_of_either_side_can_move_to_base() {
        let mut fixture = arena(fixtures::BF2);
        let ctx = fixture.ctx();
        assert_eq!(
            home(&ctx, fixtures::VI, fixtures::BF1, 0),
            Err(Refusal::Illegal(Reason::NoMoveToBase))
        );
        assert_eq!(
            home(&ctx, fixtures::SPRITE, fixtures::BF2, 1),
            Err(Refusal::Illegal(Reason::NoMoveToBase)),
            "his own side is bound too"
        );
        assert_eq!(
            home(&ctx, RECKONER, fixtures::BF2, 1),
            Err(Refusal::Illegal(Reason::NoMoveToBase)),
            "and so is he"
        );
        assert!(!move_destinations(&ctx, fixtures::VI).contains(&Location::Base(0)));
        assert!(!ctx.movable_to_base(fixtures::VI));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "battlefield to battlefield is the ordinary rule, untouched"
        );
        drop(ctx);
        let mut fixture = arena(fixtures::BF2);
        let mut ctx = fixture.ctx();
        assert_eq!(
            march::effect_move(
                &mut ctx,
                &fixtures::effect_of(0),
                fixtures::VI,
                Location::Base(0)
            ),
            None,
            "an effect cannot send anyone home either"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.fault.is_none());
    }
}

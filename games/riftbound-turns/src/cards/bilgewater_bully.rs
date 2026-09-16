use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

fn buffed(ctx: &Ctx, card: u32) -> bool {
    ctx.is_buffed(card)
}

pub static CARD: Card = with_statics(
    unit("Bilgewater Bully", &[], &[]),
    &[Static::While(buffed, &[Grant::Keyword(Keyword::Ganking)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, statics};
    use crate::Refusal;

    const BULLY: u32 = 90;

    fn afield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            BULLY,
            fixtures::BF1,
            0,
            "Bilgewater Bully",
            6,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BULLY).unwrap(), &CARD));
        fixture
    }

    fn across(ctx: &Ctx) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            BULLY,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_is_a_unit_whose_while_grants_ganking_buffed() {
        assert!(std::ptr::eq(script_of("Bilgewater Bully").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.keywords.is_empty(),
            "Ganking is the grant, not printed"
        );
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Keyword(Keyword::Ganking)])]
        ));
    }

    #[test]
    fn an_unbuffed_bully_is_refused_battlefield_to_battlefield_and_a_buffed_one_goes() {
        let mut fixture = afield();
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(BULLY, Keyword::Ganking));
        assert_eq!(
            across(&ctx),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "736 · no Ganking, no battlefield-to-battlefield move"
        );
        assert!(ctx.buff(BULLY));
        assert!(matches!(
            statics::grants_on(&ctx, BULLY).as_slice(),
            [Grant::Keyword(Keyword::Ganking)]
        ));
        assert!(ctx.has_keyword(BULLY, Keyword::Ganking));
        assert_eq!(across(&ctx), Ok(()));
        assert_eq!(
            ctx.current_might(BULLY),
            7,
            "the buff's own +1 and nothing more"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_buffed_bully_still_walks_home_and_to_base_the_keyword_is_never_needed() {
        let mut fixture = afield();
        let mut ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                BULLY,
                Location::Battlefield(fixtures::BF1),
                Location::Base(0)
            ),
            Ok(())
        );
        assert!(ctx.buff(BULLY));
        assert_eq!(
            march::legal_destination(
                &ctx,
                BULLY,
                Location::Battlefield(fixtures::BF1),
                Location::Base(0)
            ),
            Ok(())
        );
    }
}

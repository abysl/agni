use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 1;

pub static WHEN_YOU_HAVE_DISCARDED: &[Grant] = &[
    Grant::Keyword(Keyword::Assault(ASSAULT)),
    Grant::Keyword(Keyword::Ganking),
];

pub fn you_discarded_this_turn_until_the_blob_counts_discards_per_seat(
    ctx: &Ctx,
    card: u32,
) -> bool {
    let _ = ctx.controller(card);
    false
}

pub static CARD: Card = with_statics(
    unit("Raging Soul", &[], &[]),
    &[Static::While(
        you_discarded_this_turn_until_the_blob_counts_discards_per_seat,
        WHEN_YOU_HAVE_DISCARDED,
    )],
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
    use agni_plugin_sdk::table::CardInfo;

    const SOUL: u32 = 90;

    fn soul(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(SOUL, zone, 0, "Raging Soul", 4);
        card.energy = Some(4);
        card
    }

    fn raging(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(soul(zone));
        if zone == fixtures::BF1 {
            fixture.blob.set_holder(fixtures::BF1, Some(0));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_keywordless_unit_whose_one_static_grants_assault_one_and_ganking() {
        assert!(std::ptr::eq(script_of("Raging Soul").unwrap(), &CARD));
        assert!(
            CARD.keywords.is_empty(),
            "the keywords are conditional, not printed"
        );
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        let Static::While(applies, grants) = CARD.statics[0] else {
            panic!("a While static");
        };
        assert!(std::ptr::eq(
            applies as *const (),
            you_discarded_this_turn_until_the_blob_counts_discards_per_seat as *const ()
        ));
        assert_eq!(grants.len(), 2);
        assert!(matches!(
            grants[0],
            Grant::Keyword(Keyword::Assault(ASSAULT))
        ));
        assert!(matches!(grants[1], Grant::Keyword(Keyword::Ganking)));
        assert_eq!(ASSAULT, 1);
        let fixture = raging(fixtures::BASE);
        assert!(std::ptr::eq(fixture.scripts.of_card(SOUL).unwrap(), &CARD));
    }

    #[test]
    fn without_a_discard_this_turn_it_has_neither_keyword_and_cannot_move_between_battlefields() {
        let mut fixture = raging(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(!you_discarded_this_turn_until_the_blob_counts_discards_per_seat(&ctx, SOUL));
        assert!(statics::grants_on(&ctx, SOUL).is_empty());
        assert!(!ctx.has_keyword(SOUL, Keyword::Assault(ASSAULT)));
        assert!(!ctx.has_keyword(SOUL, Keyword::Ganking));
        assert_eq!(ctx.current_might(SOUL), 4);
        assert_eq!(
            march::legal_destination(
                &ctx,
                SOUL,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "battlefield to battlefield needs Ganking"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · the blob has no per-seat discards-this-turn counter for the predicate to read; with it a discard grants [Assault 1] and [Ganking] until the turn ends"]
    fn after_a_discard_this_turn_it_has_assault_one_and_ganking() {
        let mut fixture = raging(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.trash(fixtures::HAND_UNIT);
        assert!(you_discarded_this_turn_until_the_blob_counts_discards_per_seat(&ctx, SOUL));
        assert!(ctx.has_keyword(SOUL, Keyword::Assault(ASSAULT)));
        assert!(ctx.has_keyword(SOUL, Keyword::Ganking));
        assert!(march::legal_destination(
            &ctx,
            SOUL,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3)
        )
        .is_ok());
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const SHIELD: u8 = 3;

pub fn other_friendly_units_here(ctx: &Ctx, me: u32) -> usize {
    let Some(here) = ctx.location(me) else {
        return 0;
    };
    let seat = ctx.controller(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| *unit != me && ctx.controller(*unit) == seat)
        .count()
}

pub fn at_a_battlefield_with_exactly_one_other_friendly_unit(ctx: &Ctx, me: u32) -> bool {
    ctx.at_battlefield(me) && other_friendly_units_here(ctx, me) == 1
}

pub static CARD: Card = with_statics(
    unit("Disciple of Shen", &[Keyword::Hidden], &[]),
    &[Static::While(
        at_a_battlefield_with_exactly_one_other_friendly_unit,
        &[Grant::Keyword(Keyword::Shield(SHIELD))],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::table::CardInfo;

    const DISCIPLE: u32 = 90;
    const FRIEND: u32 = 91;
    const SECOND: u32 = 92;
    const FOE: u32 = 93;
    const MIGHT: u8 = 1;

    fn disciple(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: None,
            domain: vec!["Order".into()],
            ..fixtures::unit(DISCIPLE, zone, seat, "Disciple of Shen", MIGHT)
        }
    }

    fn dojo(zone: u16, friends_here: &[u32], foe_here: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(disciple(zone, 0));
        for friend in friends_here {
            fixture
                .table
                .cards
                .push(fixtures::unit(*friend, zone, 0, "Friend", 2));
        }
        if foe_here {
            fixture
                .table
                .cards
                .push(fixtures::unit(FOE, zone, 1, "Foe", 2));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DISCIPLE).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_hidden_unit_whose_one_while_grants_shield_three() {
        assert!(std::ptr::eq(script_of("Disciple of Shen").unwrap(), &CARD));
        assert_eq!(CARD.name, "Disciple of Shen");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), 1);
        let Static::While(applies, grants) = CARD.statics[0] else {
            panic!("a While static");
        };
        assert!(std::ptr::eq(
            applies as *const (),
            at_a_battlefield_with_exactly_one_other_friendly_unit as *const ()
        ));
        assert!(matches!(grants, [Grant::Keyword(Keyword::Shield(SHIELD))]));
        assert_eq!(SHIELD, 3);
        assert!(
            !CARD.has_keyword(Keyword::Shield(1)),
            "the Shield is conditional, not printed"
        );
    }

    #[test]
    fn the_predicate_wants_a_battlefield_and_exactly_one_other_unit_of_yours_there() {
        let mut fixture = dojo(fixtures::BF1, &[FRIEND], true);
        let ctx = fixture.ctx();
        assert_eq!(other_friendly_units_here(&ctx, DISCIPLE), 1);
        assert!(at_a_battlefield_with_exactly_one_other_friendly_unit(
            &ctx, DISCIPLE
        ));
        assert!(
            !at_a_battlefield_with_exactly_one_other_friendly_unit(&ctx, FOE),
            "read from each unit's own controller: the foe stands alone"
        );
        drop(ctx);
        let mut fixture = dojo(fixtures::BF1, &[], true);
        let ctx = fixture.ctx();
        assert_eq!(other_friendly_units_here(&ctx, DISCIPLE), 0);
        assert!(
            !at_a_battlefield_with_exactly_one_other_friendly_unit(&ctx, DISCIPLE),
            "an enemy here is not a unit you control"
        );
        drop(ctx);
        let mut fixture = dojo(fixtures::BF1, &[FRIEND, SECOND], false);
        let ctx = fixture.ctx();
        assert_eq!(other_friendly_units_here(&ctx, DISCIPLE), 2);
        assert!(
            !at_a_battlefield_with_exactly_one_other_friendly_unit(&ctx, DISCIPLE),
            "two others is not exactly one"
        );
        drop(ctx);
        let mut fixture = dojo(fixtures::BASE, &[], false);
        let ctx = fixture.ctx();
        assert_eq!(
            other_friendly_units_here(&ctx, DISCIPLE),
            1,
            "Vi shares the base"
        );
        assert!(
            !at_a_battlefield_with_exactly_one_other_friendly_unit(&ctx, DISCIPLE),
            "a base is not a battlefield"
        );
        drop(ctx);
        let mut fixture = dojo(fixtures::HAND, &[], false);
        let ctx = fixture.ctx();
        assert_eq!(other_friendly_units_here(&ctx, DISCIPLE), 0);
        assert!(!at_a_battlefield_with_exactly_one_other_friendly_unit(
            &ctx, DISCIPLE
        ));
    }

    #[test]
    fn beside_one_friend_it_defends_with_shield_three_and_loses_it_when_the_friend_dies() {
        let mut fixture = dojo(fixtures::BF1, &[FRIEND], true);
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(DISCIPLE, Keyword::Shield(SHIELD)));
        assert_eq!(statics::grants_on(&ctx, DISCIPLE).len(), 1);
        assert_eq!(
            ctx.current_might(DISCIPLE),
            i32::from(MIGHT),
            "814.1.d · Shield is Might for a defender only"
        );
        assert!(ctx.mark_attacker(DISCIPLE));
        assert_eq!(ctx.current_might(DISCIPLE), i32::from(MIGHT));
        assert!(ctx.mark_defender(DISCIPLE));
        assert_eq!(
            ctx.current_might(DISCIPLE),
            i32::from(MIGHT) + i32::from(SHIELD)
        );
        assert_eq!(ctx.kill(FRIEND, Cause::Rule), Killed::Yes);
        assert!(!ctx.has_keyword(DISCIPLE, Keyword::Shield(SHIELD)));
        assert!(statics::grants_on(&ctx, DISCIPLE).is_empty());
        assert_eq!(
            ctx.current_might(DISCIPLE),
            i32::from(MIGHT),
            "alone there the Shield is gone mid-combat"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_friends_or_in_hand_it_has_no_shield_at_all() {
        let mut fixture = dojo(fixtures::BF1, &[FRIEND, SECOND], false);
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(DISCIPLE, Keyword::Shield(SHIELD)));
        assert!(ctx.mark_defender(DISCIPLE));
        assert_eq!(ctx.current_might(DISCIPLE), i32::from(MIGHT));
        assert_eq!(ctx.kill(SECOND, Cause::Rule), Killed::Yes);
        assert_eq!(
            ctx.current_might(DISCIPLE),
            i32::from(MIGHT) + i32::from(SHIELD),
            "down to exactly one friend the Shield switches on"
        );
        drop(ctx);
        let mut fixture = dojo(fixtures::HAND, &[], false);
        let ctx = fixture.ctx();
        assert!(
            statics::grants_on(&ctx, DISCIPLE).is_empty(),
            "365.1 · not on the board, so the passive is inactive"
        );
        assert!(!ctx.has_keyword(DISCIPLE, Keyword::Shield(SHIELD)));
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};

pub const HUNT: u8 = 2;
pub const LEVEL: u8 = 3;
pub const BONUS: i16 = 1;

pub static LEVELED: &[Grant] = &[Grant::Might(BONUS), Grant::Keyword(Keyword::Ganking)];

pub static CARD: Card = with_statics(
    unit("Gustwalker", &[Keyword::Hunt(HUNT)], &[]),
    &[Static::Level(LEVEL, LEVELED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, IMPLICIT_HUNT};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, priority, settle, statics, triggers};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WALKER: u32 = 90;

    fn walker(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(WALKER, zone, seat, "Gustwalker", 3)
        }
    }

    fn breeze(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(walker(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WALKER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn across(ctx: &crate::engine::ctx::Ctx) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            WALKER,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_prints_hunt_two_and_a_level_three_of_plus_one_and_ganking() {
        assert!(std::ptr::eq(script_of("Gustwalker").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(2)]);
        assert_eq!(CARD.hunt(), 2);
        assert!(!CARD.has_keyword(Keyword::Ganking), "Ganking is the grant");
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(matches!(
            CARD.statics,
            [Static::Level(
                3,
                [Grant::Might(1), Grant::Keyword(Keyword::Ganking)]
            )]
        ));
    }

    #[test]
    fn at_three_xp_it_is_a_four_that_marches_battlefield_to_battlefield_and_at_two_it_is_refused() {
        let mut fixture = breeze(2);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(WALKER), 3);
        assert!(!ctx.has_keyword(WALKER, Keyword::Ganking));
        assert!(statics::grants_on(&ctx, WALKER).is_empty());
        assert_eq!(
            across(&ctx),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "at 2 XP a battlefield-to-battlefield move is refused"
        );
        drop(ctx);

        let mut fixture = breeze(3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(WALKER), 4, "824.1.c · Level 3 at 3 XP");
        assert!(ctx.has_keyword(WALKER, Keyword::Ganking));
        assert_eq!(statics::grants_on(&ctx, WALKER).len(), 2);
        assert_eq!(
            across(&ctx),
            Ok(()),
            "736 · the granted Ganking lets it march battlefield to battlefield"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_xp_does_not_level_it() {
        let mut fixture = breeze(0);
        fixture.set_xp(1, 3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(WALKER), 3);
        assert!(!ctx.has_keyword(WALKER, Keyword::Ganking));
        assert_eq!(across(&ctx), Err(Refusal::Illegal(Reason::NeedsGanking)));
    }

    #[test]
    fn holding_hunts_two_xp_after_the_chain_and_levels_it() {
        let mut fixture = breeze(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(WALKER), 2);
        ctx.raise(Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![WALKER],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == WALKER && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.has_keyword(WALKER, Keyword::Ganking));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 3);
        assert_eq!(ctx.current_might(WALKER), 4);
        assert!(ctx.has_keyword(WALKER, Keyword::Ganking));
        assert_eq!(across(&ctx), Ok(()));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};

pub const HUNT: u8 = 1;
pub const LEVEL: u8 = 6;
pub const BONUS: i16 = 1;

pub static LEVELED: &[Grant] = &[Grant::Might(BONUS)];

pub static CARD: Card = with_statics(
    unit("Gemhand Hunter", &[Keyword::Hunt(HUNT)], &[]),
    &[Static::Level(LEVEL, LEVELED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, IMPLICIT_HUNT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, statics, triggers};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const HUNTER: u32 = 90;

    fn hunter(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(HUNTER, zone, seat, "Gemhand Hunter", 2)
        }
    }

    fn mine(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hunter(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HUNTER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_prints_hunt_one_and_a_level_six_of_plus_one_with_no_ambush() {
        assert!(std::ptr::eq(script_of("Gemhand Hunter").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(1)]);
        assert_eq!(CARD.hunt(), 1);
        assert!(
            !CARD.has_keyword(Keyword::Ambush),
            "the catalog's trailing 'ambush' is an artifact, not a keyword"
        );
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(matches!(
            CARD.statics,
            [Static::Level(6, [Grant::Might(1)])]
        ));
    }

    #[test]
    fn at_six_xp_it_is_a_three_and_at_five_the_printed_two() {
        let mut fixture = mine(5);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(HUNTER), 2);
        assert!(statics::grants_on(&ctx, HUNTER).is_empty());
        drop(ctx);

        let mut fixture = mine(6);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(HUNTER), 3, "824.1.c · Level 6 at 6 XP");
        assert!(matches!(
            statics::grants_on(&ctx, HUNTER).as_slice(),
            [Grant::Might(1)]
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_xp_does_not_level_it() {
        let mut fixture = mine(0);
        fixture.set_xp(1, 6);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.current_might(HUNTER),
            2,
            "Level reads its controller's XP"
        );
    }

    #[test]
    fn conquering_hunts_one_xp_after_the_chain_and_the_sixth_levels_it() {
        let mut fixture = mine(5);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(HUNTER), 1);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "a battlefield it did not conquer gains nothing"
        );
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![HUNTER],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == HUNTER && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.current_might(HUNTER), 2);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 6);
        assert_eq!(ctx.current_might(HUNTER), 3);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

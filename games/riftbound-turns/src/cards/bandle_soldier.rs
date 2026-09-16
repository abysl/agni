use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const LEVEL: u8 = 3;

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    statics::level_active(ctx, me, LEVEL)
}

pub static CARD: Card = with_statics(
    unit("Bandle Soldier", &[], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const SOLDIER: u32 = 90;

    fn soldier(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(SOLDIER, zone, seat, "Bandle Soldier", 5)
        }
    }

    fn barracks(zone: u16, xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(soldier(zone, 0));
        fixture.set_xp(0, xp);
        for rune in [100, 101] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SOLDIER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_level_three_entry_rule_is_the_enters_ready_static() {
        assert!(std::ptr::eq(script_of("Bandle Soldier").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert_eq!(LEVEL, 3);
    }

    #[test]
    fn the_seam_reads_its_controllers_xp_against_level_three() {
        let mut fixture = barracks(fixtures::HAND, 2);
        let ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, SOLDIER));
        drop(ctx);

        let mut fixture = barracks(fixtures::HAND, 3);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, SOLDIER), "824.1.c · Level 3 at 3 XP");
        drop(ctx);

        let mut fixture = barracks(fixtures::HAND, 0);
        fixture.set_xp(1, 3);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, SOLDIER),
            "the opponent's XP is not the controller's"
        );
        assert!(
            enters_ready(&ctx, fixtures::THEIR_UNIT),
            "read from the entering unit's controller"
        );
    }

    #[test]
    fn played_at_two_xp_it_enters_exhausted_as_any_unit_does() {
        let mut fixture = barracks(fixtures::HAND, 2);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SOLDIER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SOLDIER));
        assert!(ctx.card(SOLDIER).unwrap().exhausted);
        assert_eq!(ctx.current_might(SOLDIER), 5);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_at_three_xp_it_enters_ready() {
        let mut fixture = barracks(fixtures::HAND, 3);
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, SOLDIER));
        fixtures::play_from_hand(&mut ctx, 0, SOLDIER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SOLDIER));
        assert!(
            !ctx.card(SOLDIER).unwrap().exhausted,
            "I enter ready · the replacement on the way a played unit lands"
        );
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const HUNT: u8 = 2;
pub const LEVEL: u8 = 3;
pub const BONUS: i16 = 1;

pub static LEVELED: &[Grant] = &[Grant::Might(BONUS)];

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    statics::level_active(ctx, me, LEVEL)
}

pub static CARD: Card = with_statics(
    unit("Scorchclaw", &[Keyword::Hunt(HUNT)], &[]),
    &[
        Static::Level(LEVEL, LEVELED),
        Static::EntersReady(enters_ready),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, IMPLICIT_HUNT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, triggers};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const CLAW: u32 = 90;
    const MIGHT: u8 = 3;

    fn scorchclaw(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            ..fixtures::unit(CLAW, zone, seat, "Scorchclaw", MIGHT)
        }
    }

    fn kennel(zone: u16, xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(scorchclaw(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(CLAW).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_hunt_two_and_a_level_three_of_plus_one_and_names_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Scorchclaw").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(2)]);
        assert_eq!(CARD.hunt(), 2);
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(matches!(
            CARD.statics,
            [Static::Level(3, [Grant::Might(1)]), Static::EntersReady(_)]
        ));
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn at_three_xp_it_is_a_four_and_at_two_it_is_the_printed_three() {
        let mut fixture = kennel(fixtures::BF1, 2);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(CLAW), 3);
        assert!(!enters_ready(&ctx, CLAW));
        drop(ctx);

        let mut fixture = kennel(fixtures::BF1, 3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(CLAW), 4, "824.1.c · Level 3 at 3 XP");
        assert!(enters_ready(&ctx, CLAW));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_xp_does_not_level_it() {
        let mut fixture = kennel(fixtures::BF1, 0);
        fixture.set_xp(1, 3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.xp(1), 3);
        assert_eq!(
            ctx.current_might(CLAW),
            3,
            "Level reads its controller's XP"
        );
        assert!(!enters_ready(&ctx, CLAW));
    }

    #[test]
    fn conquering_hunts_two_xp_which_levels_it_and_a_conquer_by_another_unit_hunts_nothing() {
        let mut fixture = kennel(fixtures::BF1, 1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(CLAW), 2);
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
            units: vec![CLAW],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == CLAW && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the hunt waits on the chain");
        assert_eq!(ctx.current_might(CLAW), 3);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.xp(0), 3);
        assert_eq!(
            ctx.current_might(CLAW),
            4,
            "the third XP lands and the next check reads Level 3"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_at_three_xp_it_enters_ready() {
        let mut fixture = kennel(fixtures::HAND, 3);
        fixture
            .table
            .cards
            .push(fixtures::rune(100, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, CLAW));
        fixtures::play_from_hand(&mut ctx, 0, CLAW).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(CLAW));
        assert!(
            !ctx.card(CLAW).unwrap().exhausted,
            "I enter ready · the Level 3 half play::finalize does not read"
        );
    }

    #[test]
    fn played_at_two_xp_it_enters_exhausted_as_any_unit_does() {
        let mut fixture = kennel(fixtures::HAND, 2);
        fixture
            .table
            .cards
            .push(fixtures::rune(100, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, CLAW));
        fixtures::play_from_hand(&mut ctx, 0, CLAW).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(CLAW));
        assert!(ctx.card(CLAW).unwrap().exhausted);
        assert_eq!(ctx.current_might(CLAW), 3);
        assert!(ctx.fault.is_none());
    }
}

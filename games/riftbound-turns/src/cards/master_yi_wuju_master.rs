use super::prelude::{legend, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics::level_active;

pub const MIGHT_LEVEL: u8 = 6;
pub const READY_LEVEL: u8 = 11;
pub const BONUS: i16 = 1;

fn at_level_six(ctx: &Ctx, source: u32, _: u32) -> bool {
    level_active(ctx, source, MIGHT_LEVEL)
}

fn at_level_eleven(ctx: &Ctx, source: u32, _: u32) -> bool {
    level_active(ctx, source, READY_LEVEL)
}

pub fn master_of(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.table
        .cards
        .iter()
        .filter(|held| ctx.face_in_play(held) && !held.is_hidden())
        .filter(|held| ctx.is_legend(held.id) && ctx.controller(held.id) == seat)
        .find(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
}

pub fn enters_ready(ctx: &Ctx, unit: u32) -> bool {
    if !ctx.is_unit(unit) {
        return false;
    }
    let seat = ctx.controller(unit);
    master_of(ctx, seat).is_some_and(|yi| level_active(ctx, yi, READY_LEVEL))
}

pub static CARD: Card = with_statics(
    legend("Master Yi - Wuju Master", &[], &[]),
    &[
        Static::Aura {
            scope: Scope::FriendlyUnits,
            when: at_level_six,
            grants: &[Grant::Might(BONUS)],
        },
        Static::Aura {
            scope: Scope::FriendlyUnits,
            when: at_level_eleven,
            grants: &[Grant::Static(Static::EntersReady(enters_ready))],
        },
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const YI: u32 = fixtures::LEGEND_CARD;

    fn dojo(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(YI).unwrap().name = CARD.name.into();
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(YI).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_legend_is_a_level_six_aura_over_friendly_units_and_names_the_level_eleven_entry_seam() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(
            CARD.statics,
            [
                Static::Aura {
                    scope: Scope::FriendlyUnits,
                    grants: [Grant::Might(1)],
                    ..
                },
                Static::Aura {
                    scope: Scope::FriendlyUnits,
                    grants: [Grant::Static(Static::EntersReady(_))],
                    ..
                }
            ]
        ));
        assert!(CARD.has_aura());
        assert_eq!(MIGHT_LEVEL, 6);
        assert_eq!(READY_LEVEL, 11);
    }

    #[test]
    fn below_six_xp_the_aura_grants_nothing_and_from_six_every_friendly_unit_reads_one_more() {
        let mut low = dojo(5);
        let ctx = low.ctx();
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "824.1.c · five is not six"
        );
        drop(ctx);
        let mut fixture = dojo(6);
        let mut ctx = fixture.ctx();
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "the opponent's unit is not his"
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, YI));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "friendly follows the controller"
        );
        ctx.score_xp(0, 5);
        assert_eq!(ctx.xp(0), 11);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "Level 11 adds the entry rule, not more Might"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_reads_its_controllers_xp_so_the_opponents_master_helps_the_opponent_alone() {
        let mut theirs = Fixture::enforced();
        theirs.table.card_mut(YI).unwrap().name = CARD.name.into();
        theirs.table.card_mut(YI).unwrap().owner = 1;
        theirs.table.card_mut(YI).unwrap().seat = 1;
        theirs.set_xp(0, 6);
        theirs.set_xp(1, 6);
        theirs.resolve();
        let ctx = theirs.ctx();
        assert_eq!(ctx.controller(YI), 1);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "seat 0's XP is not his controller's"
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 3);
        assert_eq!(master_of(&ctx, 1), Some(YI));
        assert_eq!(master_of(&ctx, 0), None);
        drop(ctx);
        let mut exhausted = dojo(6);
        exhausted.table.card_mut(YI).unwrap().exhausted = true;
        let ctx = exhausted.ctx();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "a static reads nothing from his exhaustion"
        );
    }

    #[test]
    fn the_entry_seam_wants_eleven_xp_behind_a_wuju_master_of_the_units_controller() {
        let mut ten = dojo(10);
        let ctx = ten.ctx();
        assert!(
            !enters_ready(&ctx, fixtures::HAND_UNIT),
            "ten is not eleven"
        );
        assert!(!enters_ready(&ctx, fixtures::VI));
        drop(ctx);
        let mut eleven = dojo(11);
        let mut ctx = eleven.ctx();
        assert!(enters_ready(&ctx, fixtures::HAND_UNIT));
        assert!(enters_ready(&ctx, fixtures::VI));
        assert!(
            !enters_ready(&ctx, fixtures::THEIR_UNIT),
            "the opponent's units enter as the rules say"
        );
        assert!(!enters_ready(&ctx, fixtures::HAND_GEAR), "units only");
        ctx.score_xp(1, 11);
        assert!(
            !enters_ready(&ctx, fixtures::THEIR_UNIT),
            "eleven XP without a Wuju Master is nothing"
        );
        drop(ctx);
        let mut lillia = Fixture::enforced();
        lillia.set_xp(0, 11);
        lillia.resolve();
        let ctx = lillia.ctx();
        assert_eq!(master_of(&ctx, 0), None, "Lillia is no Wuju Master");
        assert!(!enters_ready(&ctx, fixtures::HAND_UNIT));
    }

    #[test]
    fn at_level_eleven_a_unit_played_from_hand_enters_ready() {
        let mut fixture = dojo(11);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert_eq!(ctx.location(fixtures::HAND_UNIT), Some(Location::Base(0)));
        assert!(!ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
        assert_eq!(
            ctx.current_might(fixtures::HAND_UNIT),
            3,
            "the Level 6 aura reads at once"
        );
        drop(ctx);
        let mut ten = dojo(10);
        let mut ctx = ten.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn enters_ready(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source
        && statics::in_play(ctx, source)
        && ctx.is_unit(unit)
        && ctx.controller(unit) == ctx.controller(source)
}

pub static CARD: Card = with_statics(
    unit("Magma Wurm", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: enters_ready,
        grants: &[Grant::Static(Static::EntersReady(|_, _| true))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};

    const WURM: u32 = 90;

    fn wurm(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(WURM, zone, 0, "Magma Wurm", 8));
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(WURM).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_an_aura_that_grants_enters_ready_to_other_friendly_units() {
        assert!(std::ptr::eq(script_of("Magma Wurm").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Static(Static::EntersReady(_))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
    }

    #[test]
    fn the_seam_names_other_friendly_units_while_the_wurm_is_on_the_board() {
        let mut fixture = wurm(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, WURM, fixtures::VI));
        assert!(enters_ready(&ctx, WURM, fixtures::HAND_UNIT));
        assert!(!enters_ready(&ctx, WURM, WURM), "other");
        assert!(!enters_ready(&ctx, WURM, fixtures::THEIR_UNIT), "friendly");
        assert!(!enters_ready(&ctx, WURM, fixtures::HAND_GEAR), "units");
        ctx.kill(WURM, crate::engine::ctx::Cause::Cleanup { last_item: None });
        assert!(
            !enters_ready(&ctx, WURM, fixtures::VI),
            "365.1 · a dead Wurm has no passive"
        );
        drop(ctx);
        let mut fixture = wurm(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, WURM, fixtures::VI), "nor one in hand");
    }

    #[test]
    fn a_friendly_unit_played_beside_the_wurm_enters_ready() {
        let mut fixture = wurm(fixtures::BASE);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(!ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
    }
}

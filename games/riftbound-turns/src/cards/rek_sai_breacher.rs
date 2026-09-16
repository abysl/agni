use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind, Origin};

pub const ASSAULT: u8 = 1;

pub fn grants_accelerate(ctx: &Ctx, item: &ChainItem, source: u32) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    statics::in_play(ctx, source)
        && ctx.controller(source) == item.controller
        && ctx.is_unit(card)
        && !matches!(item.origin, Origin::Hand)
}

pub static CARD: Card = with_statics(
    unit(
        "Rek'Sai - Breacher",
        &[Keyword::Accelerate, Keyword::Assault(ASSAULT)],
        &[],
    ),
    &[Static::GrantsAccelerate(grants_accelerate)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{Leave, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const REK_SAI: u32 = 90;
    const TUNNELER: u32 = 91;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn rek_sai(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Fury".into()],
            ..fixtures::unit(REK_SAI, zone, seat, "Rek'Sai - Breacher", MIGHT)
        }
    }

    fn burrow(zone: u16, seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rek_sai(zone, seat));
        let mut tunneler = fixtures::unit(TUNNELER, fixtures::CHAMPION, 0, "Tunneler", 2);
        tunneler.energy = Some(1);
        fixture.table.cards.push(tunneler);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REK_SAI).unwrap(),
            &CARD
        ));
        fixture
    }

    fn played(card: u32, seat: u8, origin: Origin) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card }, seat, origin)
    }

    #[test]
    fn the_script_prints_accelerate_and_assault_one_and_grants_accelerate_by_origin() {
        assert!(std::ptr::eq(
            script_of("Rek'Sai - Breacher").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Rek'Sai - Breacher");
        assert_eq!(CARD.keywords, &[Keyword::Accelerate, Keyword::Assault(1)]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::GrantsAccelerate(_)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = burrow(fixtures::HAND, 0);
        let ctx = fixture.ctx();
        assert!(cost::can_accelerate(&ctx, REK_SAI));
        assert!(ctx.has_keyword(REK_SAI, Keyword::Assault(1)));
    }

    #[test]
    fn the_seam_reads_a_friendly_unit_played_from_anywhere_but_a_hand_while_she_is_in_play() {
        let mut fixture = burrow(fixtures::BASE, 0);
        let ctx = fixture.ctx();
        assert!(grants_accelerate(
            &ctx,
            &played(TUNNELER, 0, Origin::Champion),
            REK_SAI
        ));
        assert!(grants_accelerate(
            &ctx,
            &played(
                fixtures::HAND_UNIT,
                0,
                Origin::Trash {
                    leave: Leave::Banish
                }
            ),
            REK_SAI
        ));
        assert!(grants_accelerate(
            &ctx,
            &played(
                fixtures::HAND_UNIT,
                0,
                Origin::Facedown {
                    zone: fixtures::BF1
                }
            ),
            REK_SAI
        ));
        assert!(
            !grants_accelerate(&ctx, &played(fixtures::HAND_UNIT, 0, Origin::Hand), REK_SAI),
            "a hand is the one origin she leaves alone"
        );
        assert!(
            !grants_accelerate(&ctx, &played(TUNNELER, 1, Origin::Champion), REK_SAI),
            "friendly units only"
        );
        assert!(
            !grants_accelerate(
                &ctx,
                &played(fixtures::HAND_GEAR, 0, Origin::Champion),
                REK_SAI
            ),
            "units, not gear"
        );
        drop(ctx);
        let mut fixture = burrow(fixtures::HAND, 0);
        let ctx = fixture.ctx();
        assert!(
            !grants_accelerate(&ctx, &played(TUNNELER, 0, Origin::Champion), REK_SAI),
            "365.1 · a Rek'Sai in hand has no passive"
        );
    }

    #[test]
    fn a_unit_played_from_hand_beside_her_is_not_offered_the_accelerate() {
        let mut fixture = burrow(fixtures::BASE, 0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(
            !matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "{:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.location(fixtures::HAND_UNIT), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::HAND_UNIT).unwrap().exhausted);
    }

    #[test]
    fn a_unit_played_from_the_champion_zone_beside_her_is_offered_the_accelerate() {
        let mut fixture = burrow(fixtures::BASE, 0);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            TUNNELER,
            Origin::Champion,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        let why = ctx.blob.why;
        assert!(
            matches!(why, Some(PromptWhy::OptionalCost { .. })),
            "{why:?}"
        );
        assert_eq!(
            prompts::status(&ctx, why.unwrap()),
            format!("accelerate {{card {TUNNELER}}} for 1 energy and 1 Fury power?")
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(TUNNELER), Some(Location::Base(0)));
        assert!(!ctx.card(TUNNELER).unwrap().exhausted);
    }
}

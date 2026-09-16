use super::herald_of_scales::is_dragon;
use super::prelude::{friendly_units, unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    friendly_units(ctx, ctx.controller(me))
        .into_iter()
        .any(|other| other != me && is_dragon(ctx, other))
}

pub static CARD: Card = with_statics(
    unit("Direwing", &[], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::herald_of_scales::DRAGONS;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const DIREWING: u32 = 90;
    const DRAKEHOUND: u32 = 91;
    const THEIR_DRAKE: u32 = 92;
    const ENERGY: u8 = 7;
    const MIGHT: u8 = 7;

    fn direwing(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Body".into()],
            ..fixtures::unit(DIREWING, zone, 0, "Direwing", MIGHT)
        }
    }

    fn roost(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(direwing(zone));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DIREWING).unwrap(),
            &CARD
        ));
        fixture
    }

    fn with_drakehound(fixture: &mut Fixture, zone: u16, seat: u8) {
        let id = if seat == 0 { DRAKEHOUND } else { THEIR_DRAKE };
        fixture
            .table
            .cards
            .push(fixtures::unit(id, zone, seat, "Eager Drakehound", 3));
        fixture.resolve();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_entry_rule_is_the_enters_ready_static() {
        assert!(std::ptr::eq(script_of("Direwing").unwrap(), &CARD));
        assert_eq!(CARD.name, "Direwing");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn dragons_are_read_by_base_name_through_the_heralds_seam_since_the_table_carries_no_tags() {
        for name in [
            "Direwing",
            "Eager Drakehound",
            "Fae Dragon",
            "Perched Grimwyrm",
        ] {
            assert!(DRAGONS.contains(&name), "{name} is on the Herald's list");
        }
        let mut fixture = roost(fixtures::BASE);
        with_drakehound(&mut fixture, fixtures::BASE, 0);
        fixture
            .table
            .cards
            .push(fixtures::unit(93, fixtures::BASE, 0, "Mountain Drake", 4));
        fixture.table.cards.push(fixtures::unit(
            94,
            fixtures::BASE,
            0,
            "Eager Drakehound (Alternate Art)",
            3,
        ));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, DIREWING));
        assert!(is_dragon(&ctx, DRAKEHOUND));
        assert!(is_dragon(&ctx, 93), "the list spans both sets");
        assert!(
            is_dragon(&ctx, 94),
            "a print suffix is not part of the name"
        );
        assert!(!is_dragon(&ctx, fixtures::VI));
        assert!(!is_dragon(&ctx, 999));
    }

    #[test]
    fn the_seam_reads_another_friendly_dragon_on_the_board() {
        let mut fixture = roost(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, DIREWING), "itself is not another");
        drop(ctx);
        let mut fixture = roost(fixtures::BASE);
        with_drakehound(&mut fixture, fixtures::BF1, 0);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DIREWING));
        assert_eq!(
            ctx.location(DRAKEHOUND),
            Some(Location::Battlefield(fixtures::BF1))
        );
        drop(ctx);
        let mut fixture = roost(fixtures::BASE);
        with_drakehound(&mut fixture, fixtures::BASE, 1);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, DIREWING),
            "an enemy Dragon is not yours"
        );
        assert!(
            enters_ready(&ctx, fixtures::THEIR_UNIT),
            "read from the entering unit's controller"
        );
        drop(ctx);
        let mut fixture = roost(fixtures::BASE);
        with_drakehound(&mut fixture, fixtures::HAND, 0);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, DIREWING),
            "a Dragon in hand is not controlled"
        );
    }

    #[test]
    fn played_beside_a_friendly_dragon_it_enters_ready() {
        let mut fixture = roost(fixtures::HAND);
        with_drakehound(&mut fixture, fixtures::BASE, 0);
        for rune in [100, 101, 102, 103] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DIREWING));
        fixtures::play_from_hand(&mut ctx, 0, DIREWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(DIREWING));
        assert!(!ctx.card(DIREWING).unwrap().exhausted);
    }
}

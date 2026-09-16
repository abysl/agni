use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::{Ctx, Location};

pub const OTHERS_IN_BASE: usize = 2;

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    let seat = ctx.controller(me);
    ctx.units_at(Location::Base(seat))
        .into_iter()
        .filter(|other| *other != me && ctx.controller(*other) == seat)
        .count()
        >= OTHERS_IN_BASE
}

pub static CARD: Card = with_statics(
    unit("Xin Zhao - Vigilant", &[Keyword::Tank], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::combat;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown};
    use crate::state::{GameBlob, Mode, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const XIN_ZHAO: u32 = 90;
    const SQUIRE: u32 = 91;
    const PAGE: u32 = 92;
    const BRUTE: u32 = 93;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 4;

    fn xin_zhao(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(XIN_ZHAO, zone, seat, "Xin Zhao - Vigilant", MIGHT)
        }
    }

    fn garrison(zone: u16, others_in_base: &[(u32, u8)]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(xin_zhao(zone, 0));
        for (id, seat) in others_in_base {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BASE, *seat, "Guard", 2));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(XIN_ZHAO).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_tank_unit_whose_entry_rule_is_the_enters_ready_seam() {
        assert!(std::ptr::eq(
            script_of("Xin Zhao - Vigilant").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Xin Zhao - Vigilant");
        assert_eq!(CARD.keywords, &[Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = garrison(fixtures::BASE, &[]);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(XIN_ZHAO, Keyword::Tank));
    }

    #[test]
    fn the_seam_counts_two_other_friendly_units_in_the_base_and_never_himself() {
        let mut fixture = garrison(fixtures::HAND, &[(SQUIRE, 0)]);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.units_at(Location::Base(0)).len(),
            2,
            "Vi and the Squire"
        );
        assert!(enters_ready(&ctx, XIN_ZHAO));
        drop(ctx);
        let mut fixture = garrison(fixtures::HAND, &[]);
        let ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, XIN_ZHAO), "Vi alone is one");
        drop(ctx);
        let mut fixture = garrison(fixtures::BASE, &[]);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, XIN_ZHAO),
            "he does not count himself once in the base"
        );
        drop(ctx);
        let mut fixture = garrison(fixtures::HAND, &[(SQUIRE, 1), (PAGE, 1)]);
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, XIN_ZHAO),
            "the opponent's base is not yours"
        );
        assert!(
            enters_ready(&ctx, fixtures::THEIR_UNIT),
            "read from the entering unit's controller"
        );
        drop(ctx);
        let mut fixture = garrison(fixtures::HAND, &[]);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(SQUIRE, fixtures::BF1, 0, "Guard", 2));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, XIN_ZHAO),
            "units at a battlefield are not in your base"
        );
    }

    #[test]
    fn as_a_tank_he_must_be_assigned_combat_damage_first() {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(xin_zhao(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(PAGE, fixtures::BF1, 0, "Page", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SQUIRE, fixtures::BF1, 0, "Squire", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.is_some());
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(
            combat::assigner(&ctx),
            Some((1, 1)),
            "741.1.b · his lethal was assigned first without a question"
        );
        assert_eq!(
            combat::candidates(&ctx),
            [PAGE, SQUIRE],
            "the last point is chosen among the rest"
        );
        assert_eq!(
            combat::ordered(&ctx, &[PAGE, XIN_ZHAO, SQUIRE], false),
            [XIN_ZHAO]
        );
    }

    #[test]
    fn played_with_two_other_units_in_his_base_he_enters_ready() {
        let mut fixture = garrison(fixtures::HAND, &[(SQUIRE, 0)]);
        fixture
            .table
            .cards
            .push(fixtures::rune(100, 0, "Order", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, XIN_ZHAO));
        fixtures::play_from_hand(&mut ctx, 0, XIN_ZHAO).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(XIN_ZHAO));
        assert!(!ctx.card(XIN_ZHAO).unwrap().exhausted);
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub fn a_unit_died_this_turn(ctx: &Ctx) -> bool {
    ctx.deaths_this_turn().iter().any(|death| death.unit)
}

pub fn enters_ready(ctx: &Ctx, _: u32) -> bool {
    a_unit_died_this_turn(ctx)
}

pub static CARD: Card = with_statics(
    unit("Towering Pairofant", &[Keyword::Assault(1)], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PAIROFANT: u32 = 90;
    const ENERGY: u8 = 6;
    const MIGHT: u8 = 6;
    const FURY_RUNES: [u32; 3] = [46, 47, 48];

    fn pairofant(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(PAIROFANT, zone, seat, "Towering Pairofant", MIGHT)
        }
    }

    fn savanna(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pairofant(zone, 0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in FURY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PAIROFANT).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_prints_assault_one_and_carries_the_enters_ready_static() {
        assert!(std::ptr::eq(
            script_of("Towering Pairofant").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Towering Pairofant");
        assert_eq!(CARD.keywords, [Keyword::Assault(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = savanna(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(PAIROFANT, Keyword::Assault(1)));
    }

    #[test]
    fn the_seam_reads_any_units_death_raised_this_request_and_not_a_gears() {
        let mut fixture = savanna(fixtures::HAND);
        fixture
            .table
            .cards
            .push(fixtures::gear(91, fixtures::BASE, 1, "Trinket", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!a_unit_died_this_turn(&ctx));
        assert!(!enters_ready(&ctx, PAIROFANT));
        ctx.kill(91, Cause::Rule);
        assert!(!enters_ready(&ctx, PAIROFANT), "a gear is not a unit");
        ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
        assert!(enters_ready(&ctx, PAIROFANT), "an enemy unit counts");
        drop(ctx);
        let mut own = savanna(fixtures::HAND);
        let mut ctx = own.ctx();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(enters_ready(&ctx, PAIROFANT), "a friendly unit counts");
        assert!(
            enters_ready(&ctx, fixtures::THEIR_UNIT),
            "the seam reads the turn, not the entering unit"
        );
    }

    #[test]
    fn played_with_no_death_this_turn_it_lands_in_the_base_exhausted_and_is_refused_when_short() {
        let mut fixture = savanna(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 7);
        fixtures::play_from_hand(&mut ctx, 0, PAIROFANT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(PAIROFANT), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.card(PAIROFANT).unwrap().exhausted);
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut short = savanna(fixtures::HAND);
        for rune in [41, 42] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, PAIROFANT),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }

    #[test]
    fn played_after_a_unit_died_this_request_it_enters_ready() {
        let mut fixture = savanna(fixtures::HAND);
        let mut ctx = fixture.ctx();
        ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
        fixtures::play_from_hand(&mut ctx, 0, PAIROFANT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(PAIROFANT), Some(Location::Base(0)));
        assert!(
            !ctx.card(PAIROFANT).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_after_a_unit_died_in_an_earlier_request_this_turn_it_enters_ready() {
        let mut fixture = savanna(fixtures::HAND);
        let table = {
            let mut ctx = fixture.ctx();
            ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
            ctx.table.clone()
        };
        fixture.commit(table);
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request carries no events");
        assert!(
            enters_ready(&ctx, PAIROFANT),
            "the death this turn is remembered"
        );
        fixtures::play_from_hand(&mut ctx, 0, PAIROFANT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(PAIROFANT));
        assert!(
            !ctx.card(PAIROFANT).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
    }
}

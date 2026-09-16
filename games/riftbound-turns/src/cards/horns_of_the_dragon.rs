use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Horns of the Dragon", &[Keyword::Tank], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::combat;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown};
    use crate::state::{GameBlob, Mode, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HORNS: u32 = 90;
    const PAGE: u32 = 91;
    const SQUIRE: u32 = 92;
    const BRUTE: u32 = 93;
    const ENERGY: u8 = 6;
    const MIGHT: u8 = 6;

    fn horns(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Order".into()],
            ..fixtures::unit(HORNS, zone, seat, "Horns of the Dragon", MIGHT)
        }
    }

    fn temple(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(horns(zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(HORNS).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_vanilla_tank_and_nothing_else() {
        assert!(std::ptr::eq(
            script_of("Horns of the Dragon").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Horns of the Dragon");
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = temple(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(HORNS, Keyword::Tank));
        assert!(!ctx.has_keyword(HORNS, Keyword::Backline));
        assert_eq!(ctx.deflect_of(HORNS), 0);
        assert_eq!(ctx.current_might(HORNS), i32::from(MIGHT));
    }

    #[test]
    fn as_a_tank_it_is_assigned_combat_damage_before_the_rest_of_its_side() {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.cards.push(horns(fixtures::BF1, 0));
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
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 7));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.is_some());
        assert_eq!(
            combat::ordered(&ctx, &[PAGE, HORNS, SQUIRE], false),
            [HORNS],
            "815.1.c.2 · the tank is the only first choice"
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(
            combat::assigner(&ctx),
            Some((1, 1)),
            "six of the brute's seven went to the horns without a question"
        );
        assert_eq!(
            combat::candidates(&ctx),
            [PAGE, SQUIRE],
            "the last point is chosen among the rest"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_plays_for_six_off_six_ready_runes_and_is_refused_off_three() {
        let mut fixture = temple(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, HORNS),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 3
            })
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut fixture = temple(fixtures::HAND);
        for rune in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 6);
        fixtures::play_from_hand(&mut ctx, 0, HORNS).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(HORNS), Some(Location::Base(0)));
        assert!(ctx.ready_runes_of(0).is_empty(), "six energy off six runes");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

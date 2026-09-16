use super::prelude::{unit, Location};
use super::Card;
use crate::engine::cost::{Cost, Need};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn taxes_moves_to(ctx: &Ctx, seat: u8, to: Location) -> bool {
    let Location::Battlefield(_) = to else {
        return false;
    };
    let mut investigators: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
        .collect();
    investigators.sort_unstable();
    investigators.into_iter().any(|investigator| {
        ctx.controller(investigator) != seat
            && statics::in_play(ctx, investigator)
            && ctx.location(investigator) == Some(to)
    })
}

pub fn group_move_surcharge(ctx: &Ctx, seat: u8, to: Location, moving: usize) -> Cost {
    if moving < 2 || !taxes_moves_to(ctx, seat, to) {
        return Cost::free();
    }
    Cost {
        power: vec![Need::Rainbow; moving - 1],
        ..Cost::free()
    }
}

pub static CARD: Card = unit("Mageseeker Investigator", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, pay};
    use crate::state::PromptWhy;

    const INVESTIGATOR: u32 = 90;
    const SECOND: u32 = 91;
    const THIRD: u32 = 92;

    fn watched(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(fixtures::unit(
            INVESTIGATOR,
            at,
            1,
            "Mageseeker Investigator",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BASE, 0, "Second", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THIRD, fixtures::BASE, 0, "Third", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(INVESTIGATOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn here() -> Location {
        Location::Battlefield(fixtures::BF1)
    }

    #[test]
    fn the_script_is_a_keywordless_unit_whose_surcharge_is_a_named_seam() {
        assert!(std::ptr::eq(
            script_of("Mageseeker Investigator").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn opponents_pay_a_rainbow_per_unit_beyond_the_first_moving_to_his_battlefield_together() {
        let mut fixture = watched(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(taxes_moves_to(&ctx, 0, here()));
        assert!(
            !taxes_moves_to(&ctx, 1, here()),
            "his own side moves freely"
        );
        assert!(
            !taxes_moves_to(&ctx, 0, Location::Battlefield(fixtures::BF2)),
            "my battlefield only"
        );
        assert!(!taxes_moves_to(&ctx, 0, Location::Base(0)));
        assert!(group_move_surcharge(&ctx, 0, here(), 1).is_free());
        assert_eq!(
            group_move_surcharge(&ctx, 0, here(), 2).power,
            [Need::Rainbow]
        );
        assert_eq!(
            group_move_surcharge(&ctx, 0, here(), 3).power,
            [Need::Rainbow, Need::Rainbow]
        );
        assert_eq!(group_move_surcharge(&ctx, 0, here(), 3).energy, 0);
        assert!(group_move_surcharge(&ctx, 1, here(), 3).is_free());
        assert!(group_move_surcharge(&ctx, 0, Location::Battlefield(fixtures::BF2), 3).is_free());
        assert!(pay::affordable(
            &ctx,
            0,
            &group_move_surcharge(&ctx, 0, here(), 3)
        ));
        assert!(
            !pay::affordable(&ctx, 0, &group_move_surcharge(&ctx, 0, here(), 6)),
            "four runes recycle for four rainbows, not five"
        );
    }

    #[test]
    fn an_investigator_in_base_in_hand_stunned_or_stolen_reads_by_where_and_whose_he_is() {
        let mut fixture = watched(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            !taxes_moves_to(&ctx, 0, Location::Base(1)),
            "a base is nobody's battlefield"
        );
        drop(ctx);
        let mut fixture = watched(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!taxes_moves_to(&ctx, 0, here()));
        drop(ctx);
        let mut fixture = watched(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(INVESTIGATOR));
        assert!(taxes_moves_to(&ctx, 0, here()), "stunned, he still watches");
        assert!(ctx.set_controller(INVESTIGATOR, 0, fixtures::VI));
        assert!(
            !taxes_moves_to(&ctx, 1, here()),
            "stolen, he went to my base and watches nothing"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_the_group_move_prompt_offers_every_companion_and_pays_nothing() {
        let mut fixture = watched(fixtures::BF1);
        for rune in &mut fixture.table.cards {
            if rune.is_kind("Rune") && rune.owner == 0 {
                rune.exhausted = true;
            }
        }
        fixture.resolve();
        let action = fixtures::move_action(fixtures::VI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, fixtures::VI, Location::Base(0), here());
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove {
                unit: fixtures::VI,
                to: fixtures::BF1
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SECOND}}}"),
                format!("{{card {THIRD}}}"),
                "done".to_string()
            ]
        );
        ctx.blob.close_prompt();
        march::group_move(&mut ctx, 0, fixtures::BF1, &[SECOND, THIRD]);
        assert_eq!(ctx.location(SECOND), Some(here()));
        assert_eq!(ctx.location(THIRD), Some(here()));
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · a move surcharge: PromptWhy::GroupMove carries no cost and march::group_move pays nothing, the engine owes group_move_surcharge(ctx, seat, to, picked + 1) priced into the companion offers (prompts::options greys a companion the seat cannot afford beside the picks so far) and paid by march::group_move before the units move"]
    fn without_a_spare_rune_the_opponent_moves_one_unit_to_him_and_with_one_pays_it_for_a_second() {
        let mut fixture = watched(fixtures::BF1);
        for rune in &mut fixture.table.cards {
            if rune.is_kind("Rune") && rune.owner == 0 {
                rune.exhausted = true;
            }
        }
        fixture.resolve();
        let action = fixtures::move_action(fixtures::VI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, fixtures::VI, Location::Base(0), here());
        assert_eq!(
            fixtures::labels(&ctx),
            ["done".to_string()],
            "no rune, no company"
        );
        drop(ctx);
        let mut fixture = watched(fixtures::BF1);
        for rune in &mut fixture.table.cards {
            if rune.is_kind("Rune") && rune.owner == 0 {
                rune.exhausted = rune.id != 41;
            }
        }
        fixture.resolve();
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, fixtures::VI, Location::Base(0), here());
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SECOND}}}"),
                format!("{{card {THIRD}}}"),
                "done".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done".to_string()],
            "the one rune bought one companion"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.location(SECOND), Some(here()));
        assert_eq!(ctx.location(THIRD), Some(Location::Base(0)));
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "the rainbow was paid with the rune"
        );
        assert!(ctx.fault.is_none());
    }
}

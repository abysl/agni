use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::Effect;

pub const POWER_SPENT: usize = 2;
pub const BONUS: i16 = 2;

pub fn power_spent_this_turn(ctx: &Ctx, seat: u8) -> usize {
    let Some(deck) = ctx.zones.rune_deck else {
        return 0;
    };
    ctx.effects
        .iter()
        .filter(|effect| {
            matches!(
                effect,
                Effect::Move { zone, seat: paid_by, .. } if *zone == deck && *paid_by == seat
            )
        })
        .count()
}

pub fn spent_two_power_this_turn(ctx: &Ctx, card: u32) -> bool {
    power_spent_this_turn(ctx, ctx.controller(card)) >= POWER_SPENT
}

pub static MERCENARY: &[Grant] = &[Grant::Might(BONUS), Grant::Keyword(Keyword::Ganking)];

pub static CARD: Card = with_statics(
    unit("Sivir - Mercenary", &[Keyword::Accelerate], &[]),
    &[Static::While(spent_two_power_this_turn, MERCENARY)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, priority, statics};
    use crate::Refusal;

    const SIVIR: u32 = 90;
    const THEIR_SIVIR: u32 = 91;
    const TWO_POWER: u32 = 92;

    fn contract() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            SIVIR,
            fixtures::BF1,
            0,
            "Sivir - Mercenary",
            4,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_SIVIR,
            fixtures::BASE,
            1,
            "Sivir - Mercenary",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::spell(TWO_POWER, fixtures::HAND, 0, "Tithe", 1, 2));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SIVIR).unwrap(), &CARD));
        fixture
    }

    fn across(ctx: &Ctx) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            SIVIR,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3),
        )
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_accelerate_unit_whose_while_is_plus_two_and_ganking_after_two_power() {
        assert!(std::ptr::eq(script_of("Sivir - Mercenary").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate], "Ganking is the grant");
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [Grant::Might(2), Grant::Keyword(Keyword::Ganking)]
            )]
        ));
    }

    #[test]
    fn before_the_second_power_she_is_a_plain_four_refused_battlefield_to_battlefield() {
        let mut fixture = contract();
        let mut ctx = fixture.ctx();
        assert_eq!(power_spent_this_turn(&ctx, 0), 0);
        assert!(!spent_two_power_this_turn(&ctx, SIVIR));
        assert!(statics::grants_on(&ctx, SIVIR).is_empty());
        assert_eq!(ctx.current_might(SIVIR), 4);
        assert_eq!(across(&ctx), Err(Refusal::Illegal(Reason::NeedsGanking)));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            power_spent_this_turn(&ctx, 0),
            1,
            "Spark recycled one rune for its one power"
        );
        assert!(!spent_two_power_this_turn(&ctx, SIVIR));
        assert_eq!(ctx.current_might(SIVIR), 4);
        assert_eq!(across(&ctx), Err(Refusal::Illegal(Reason::NeedsGanking)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_power_spent_on_one_spell_gives_her_plus_two_and_the_battlefield_to_battlefield_move() {
        let mut fixture = contract();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        fixtures::play_from_hand(&mut ctx, 0, TWO_POWER).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.runes_of(0).len(), 2, "two Fury runes recycled");
        assert_eq!(power_spent_this_turn(&ctx, 0), 2);
        assert!(spent_two_power_this_turn(&ctx, SIVIR));
        assert!(matches!(
            statics::grants_on(&ctx, SIVIR).as_slice(),
            [Grant::Might(2), Grant::Keyword(Keyword::Ganking)]
        ));
        assert_eq!(ctx.current_might(SIVIR), 6);
        assert!(ctx.has_keyword(SIVIR, Keyword::Ganking));
        assert_eq!(across(&ctx), Ok(()));
        assert_eq!(
            ctx.current_might(THEIR_SIVIR),
            4,
            "your spending is not the opponent's"
        );
        assert!(!ctx.has_keyword(THEIR_SIVIR, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: power_spent_this_turn reads this request's rune recycles, the engine owes a per-seat power_spent counter on SeatState that pay::pay raises for runes and Golds alike and expiration resets"]
    fn power_spent_in_an_earlier_request_still_counts_for_the_rest_of_the_turn() {
        let mut fixture = contract();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TWO_POWER).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(SIVIR), 6);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx();
        assert_eq!(power_spent_this_turn(&ctx, 0), 2);
        assert_eq!(ctx.current_might(SIVIR), 6);
        assert!(ctx.has_keyword(SIVIR, Keyword::Ganking));
    }
}

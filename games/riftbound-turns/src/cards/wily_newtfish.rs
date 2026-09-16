use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::rules::COUNTER_XP;
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Target;

pub const BONUS: i16 = 1;

pub fn xp_gained_this_turn(ctx: &Ctx, seat: u8) -> i32 {
    ctx.effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Counter {
                target: Target::Seat(scored),
                counter: COUNTER_XP,
                delta,
            } if *scored == seat && *delta > 0 => Some(*delta),
            _ => None,
        })
        .sum()
}

pub fn gained_xp_this_turn(ctx: &Ctx, me: u32) -> bool {
    xp_gained_this_turn(ctx, ctx.controller(me)) > 0
}

pub static SLIPPERY: &[Grant] = &[Grant::Might(BONUS), Grant::Keyword(Keyword::Ganking)];

pub static CARD: Card = with_statics(
    unit("Wily Newtfish", &[], &[]),
    &[Static::While(gained_xp_this_turn, SLIPPERY)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, priority, statics};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const NEWTFISH: u32 = 90;
    const THEIR_NEWTFISH: u32 = 91;
    const HERALD: u32 = 92;

    fn newtfish(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(id, zone, seat, "Wily Newtfish", 4)
        }
    }

    fn pond() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(newtfish(NEWTFISH, fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(newtfish(THEIR_NEWTFISH, fixtures::BASE, 1));
        fixture.table.cards.push(CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(HERALD, fixtures::HAND, 0, "Herald of Spring", 4)
        });
        fixture
            .table
            .cards
            .push(fixtures::rune(100, 0, "Calm", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, 3);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(NEWTFISH).unwrap(),
            &CARD
        ));
        fixture
    }

    fn across(ctx: &Ctx) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            NEWTFISH,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_is_a_keywordless_unit_whose_while_is_plus_one_and_ganking_after_an_xp_gain() {
        assert!(std::ptr::eq(script_of("Wily Newtfish").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty(), "Ganking is the grant");
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [Grant::Might(1), Grant::Keyword(Keyword::Ganking)]
            )]
        ));
    }

    #[test]
    fn before_any_gain_it_is_a_plain_four_refused_battlefield_to_battlefield() {
        let mut fixture = pond();
        let ctx = fixture.ctx();
        assert_eq!(ctx.xp(0), 3, "XP held from earlier turns is not a gain");
        assert_eq!(xp_gained_this_turn(&ctx, 0), 0);
        assert!(!gained_xp_this_turn(&ctx, NEWTFISH));
        assert!(statics::grants_on(&ctx, NEWTFISH).is_empty());
        assert_eq!(ctx.current_might(NEWTFISH), 4);
        assert!(!ctx.has_keyword(NEWTFISH, Keyword::Ganking));
        assert_eq!(across(&ctx), Err(Refusal::Illegal(Reason::NeedsGanking)));
    }

    #[test]
    fn spending_xp_is_not_gaining_it_and_the_opponents_gain_is_not_yours() {
        let mut fixture = pond();
        let mut ctx = fixture.ctx();
        assert!(ctx.spend_xp(0, 2));
        assert_eq!(xp_gained_this_turn(&ctx, 0), 0);
        assert!(!gained_xp_this_turn(&ctx, NEWTFISH));
        assert_eq!(ctx.current_might(NEWTFISH), 4);
        ctx.score_xp(1, 2);
        assert_eq!(xp_gained_this_turn(&ctx, 1), 2);
        assert!(gained_xp_this_turn(&ctx, THEIR_NEWTFISH));
        assert_eq!(ctx.current_might(THEIR_NEWTFISH), 5);
        assert!(
            !gained_xp_this_turn(&ctx, NEWTFISH),
            "your fish reads your seat's gains only"
        );
        assert_eq!(ctx.current_might(NEWTFISH), 4);
        assert_eq!(across(&ctx), Err(Refusal::Illegal(Reason::NeedsGanking)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_heralds_two_xp_give_it_plus_one_and_the_battlefield_to_battlefield_move() {
        let mut fixture = pond();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HERALD).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.current_might(NEWTFISH),
            4,
            "the gain waits for the herald's trigger to resolve"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 5);
        assert_eq!(xp_gained_this_turn(&ctx, 0), 2);
        assert!(gained_xp_this_turn(&ctx, NEWTFISH));
        assert!(matches!(
            statics::grants_on(&ctx, NEWTFISH).as_slice(),
            [Grant::Might(1), Grant::Keyword(Keyword::Ganking)]
        ));
        assert_eq!(ctx.current_might(NEWTFISH), 5);
        assert!(ctx.has_keyword(NEWTFISH, Keyword::Ganking));
        assert_eq!(across(&ctx), Ok(()));
        assert_eq!(ctx.current_might(THEIR_NEWTFISH), 4);
        assert!(!ctx.has_keyword(THEIR_NEWTFISH, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: xp_gained_this_turn reads this request's XP scores, the engine owes a per-seat xp_gained counter on SeatState that Ctx::score_xp raises and expiration resets"]
    fn xp_gained_in_an_earlier_request_still_counts_for_the_rest_of_the_turn() {
        let mut fixture = pond();
        let mut ctx = fixture.ctx();
        ctx.score_xp(0, 1);
        assert_eq!(ctx.current_might(NEWTFISH), 5);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx();
        assert_eq!(xp_gained_this_turn(&ctx, 0), 1);
        assert_eq!(ctx.current_might(NEWTFISH), 5);
        assert!(ctx.has_keyword(NEWTFISH, Keyword::Ganking));
    }
}

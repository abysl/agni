use super::prelude::{unit, with_statics};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 1;

fn buffed(ctx: &Ctx, card: u32) -> bool {
    ctx.is_buffed(card)
}

pub static CARD: Card = with_statics(
    unit("Wizened Elder", &[], &[]),
    &[Static::While(buffed, &[Grant::Might(BONUS)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::Target;

    const ELDER: u32 = 90;
    const IN_HAND: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(ELDER, fixtures::BASE, 0, "Wizened Elder", 4));
        fixture.table.cards.push(fixtures::unit(
            IN_HAND,
            fixtures::HAND,
            0,
            "Wizened Elder",
            4,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ELDER).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_whose_while_reads_plus_one_buffed() {
        assert!(std::ptr::eq(script_of("Wizened Elder").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(1)])]
        ));
    }

    #[test]
    fn a_buff_is_worth_two_on_the_elder_and_one_again_once_it_is_gone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(ELDER), 4);
        assert!(statics::grants_on(&ctx, ELDER).is_empty());
        assert!(ctx.buff(ELDER));
        assert!(matches!(
            statics::grants_on(&ctx, ELDER).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(
            ctx.current_might(ELDER),
            6,
            "the buff's +1 and the additional +1"
        );
        assert!(!ctx.buff(ELDER), "a second buff is idempotent");
        assert_eq!(ctx.current_might(ELDER), 6);
        ctx.emit(Effect::Counter {
            target: Target::Card(ELDER),
            counter: COUNTER_BUFFED,
            delta: -1,
        });
        assert!(!ctx.is_buffed(ELDER));
        assert_eq!(ctx.current_might(ELDER), 4, "both go with the buff");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_buffed_elder_in_hand_or_stunned_still_projects_by_the_same_rule() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(
            !ctx.buff(IN_HAND),
            "a card in hand is not on the board, so it cannot be buffed"
        );
        assert!(statics::grants_on(&ctx, IN_HAND).is_empty());
        assert!(ctx.buff(ELDER));
        assert!(ctx.stun(ELDER));
        assert_eq!(
            ctx.current_might(ELDER),
            6,
            "a stun zeroes its combat contribution, not its Might"
        );
    }
}

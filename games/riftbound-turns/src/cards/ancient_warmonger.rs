use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn enemy_units_here(ctx: &Ctx, card: u32) -> usize {
    let Some(here) = ctx.location(card) else {
        return 0;
    };
    let seat = ctx.controller(card);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .count()
}

pub fn assault_value(ctx: &Ctx, card: u32) -> u8 {
    u8::try_from(enemy_units_here(ctx, card).min(LADDER)).unwrap_or(u8::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32) -> bool {
    enemy_units_here(ctx, card) >= N
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Static::While(at_least::<$step>, &[Grant::Keyword(Keyword::Assault(1))])),+]
    };
}

pub static ENEMIES_HERE: &[Static] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Ancient Warmonger", &[Keyword::Accelerate], &[]),
    ENEMIES_HERE,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const WARMONGER: u32 = 90;
    const FIRST_ENEMY: u32 = 100;

    fn warpath(enemies_here: u32, friends_here: u32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            WARMONGER,
            fixtures::BF1,
            0,
            "Ancient Warmonger",
            4,
        ));
        for offset in 0..enemies_here {
            fixture.table.cards.push(fixtures::unit(
                FIRST_ENEMY + offset,
                fixtures::BF1,
                1,
                "Enemy",
                2,
            ));
        }
        for offset in 0..friends_here {
            fixture.table.cards.push(fixtures::unit(
                FIRST_ENEMY + 50 + offset,
                fixtures::BF1,
                0,
                "Friend",
                2,
            ));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WARMONGER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_an_accelerate_unit_whose_whiles_are_one_assault_per_enemy_here() {
        assert!(std::ptr::eq(script_of("Ancient Warmonger").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert_eq!(CARD.statics.len(), LADDER);
        assert!(CARD.statics.iter().all(|held| matches!(
            held,
            Static::While(_, [Grant::Keyword(Keyword::Assault(1))])
        )));
    }

    #[test]
    fn his_assault_is_the_enemy_count_here_and_reads_as_might_only_while_attacking() {
        let mut fixture = warpath(3, 2);
        let mut ctx = fixture.ctx();
        assert_eq!(enemy_units_here(&ctx, WARMONGER), 3);
        assert_eq!(assault_value(&ctx, WARMONGER), 3);
        assert_eq!(
            statics::grants_on(&ctx, WARMONGER).len(),
            3,
            "two friends here do not count"
        );
        assert!(ctx.has_keyword(WARMONGER, Keyword::Assault(1)));
        assert_eq!(
            ctx.current_might(WARMONGER),
            4,
            "Assault is Might only for an attacker"
        );
        assert!(ctx.mark_attacker(WARMONGER));
        assert_eq!(ctx.current_might(WARMONGER), 7);
        assert_eq!(ctx.kill(FIRST_ENEMY, Cause::Rule), Killed::Yes);
        assert_eq!(assault_value(&ctx, WARMONGER), 2);
        assert_eq!(ctx.current_might(WARMONGER), 6, "one enemy fewer");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn alone_with_friends_or_in_base_he_has_no_assault_at_all() {
        let mut fixture = warpath(0, 2);
        let mut ctx = fixture.ctx();
        assert_eq!(assault_value(&ctx, WARMONGER), 0);
        assert!(!ctx.has_keyword(WARMONGER, Keyword::Assault(1)));
        assert!(ctx.mark_attacker(WARMONGER));
        assert_eq!(ctx.current_might(WARMONGER), 4);
        ctx.recall(WARMONGER, false);
        assert_eq!(
            enemy_units_here(&ctx, WARMONGER),
            0,
            "an enemy in their own base is not here"
        );
        assert_eq!(ctx.current_might(WARMONGER), 4);
    }
}

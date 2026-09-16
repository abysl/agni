use super::prelude::{unit, with_statics};
use super::{base_name, Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn namesakes_here(ctx: &Ctx, me: u32) -> usize {
    let (Some(here), Some(mine)) = (ctx.location(me), ctx.card(me)) else {
        return 0;
    };
    let name = base_name(&mine.name);
    let seat = ctx.controller(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| *unit != me && ctx.controller(*unit) == seat)
        .filter(|unit| {
            ctx.card(*unit)
                .is_some_and(|held| base_name(&held.name) == name)
        })
        .count()
}

pub fn might_bonus(ctx: &Ctx, me: u32) -> i16 {
    i16::try_from(namesakes_here(ctx, me).min(LADDER)).unwrap_or(i16::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32, _: u32) -> bool {
    namesakes_here(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static NAMESAKES: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Spiderling", &[Keyword::Hidden], &[]),
    &[Static::While(always, NAMESAKES)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Killed, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::table::CardInfo;

    const FIRST: u32 = 90;
    const SECOND: u32 = 91;
    const THIRD: u32 = 92;
    const THEIRS: u32 = 93;
    const HOMEBODY: u32 = 94;
    const MIGHT: u8 = 1;

    fn spiderling(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: None,
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Spiderling", MIGHT)
        }
    }

    fn nest(here: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        for id in here {
            fixture.table.cards.push(spiderling(*id, fixtures::BF1, 0));
        }
        fixture
            .table
            .cards
            .push(spiderling(THEIRS, fixtures::BF1, 1));
        fixture
            .table
            .cards
            .push(spiderling(HOMEBODY, fixtures::BASE, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FIRST).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_hidden_unit_whose_while_is_one_might_per_namesake_up_to_the_ladder() {
        assert!(std::ptr::eq(script_of("Spiderling").unwrap(), &CARD));
        assert_eq!(CARD.name, "Spiderling");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(NAMESAKES.len(), LADDER);
        assert!(NAMESAKES
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn each_spiderling_here_counts_the_others_of_yours_and_never_itself_the_enemys_or_one_at_home()
    {
        let mut fixture = nest(&[FIRST, SECOND, THIRD]);
        let ctx = fixture.ctx();
        assert_eq!(namesakes_here(&ctx, FIRST), 2);
        assert_eq!(namesakes_here(&ctx, SECOND), 2);
        assert_eq!(might_bonus(&ctx, THIRD), 2);
        assert_eq!(
            namesakes_here(&ctx, THEIRS),
            0,
            "the enemy Spiderling sees only units its controller controls"
        );
        assert_eq!(namesakes_here(&ctx, HOMEBODY), 0, "the base is not here");
        assert_eq!(
            namesakes_here(&ctx, fixtures::VI),
            0,
            "my name is the reading card's own: the Homebody is no Vi"
        );
        assert_eq!(statics::grants_on(&ctx, FIRST).len(), 2);
        assert_eq!(ctx.current_might(FIRST), i32::from(MIGHT) + 2);
        assert_eq!(ctx.current_might(SECOND), i32::from(MIGHT) + 2);
        assert_eq!(ctx.current_might(THIRD), i32::from(MIGHT) + 2);
        assert_eq!(ctx.current_might(THEIRS), i32::from(MIGHT));
        assert_eq!(ctx.current_might(HOMEBODY), i32::from(MIGHT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_follows_the_swarm_as_it_dies_and_arrives() {
        let mut fixture = nest(&[FIRST, SECOND]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(FIRST), i32::from(MIGHT) + 1);
        assert_eq!(ctx.kill(SECOND, Cause::Rule), Killed::Yes);
        assert_eq!(namesakes_here(&ctx, FIRST), 0);
        assert_eq!(ctx.current_might(FIRST), i32::from(MIGHT), "alone again");
        assert_eq!(
            ctx.move_unit(
                HOMEBODY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(namesakes_here(&ctx, FIRST), 1);
        assert_eq!(ctx.current_might(FIRST), i32::from(MIGHT) + 1);
        assert_eq!(ctx.current_might(HOMEBODY), i32::from(MIGHT) + 1);
        assert!(ctx.set_controller(THEIRS, 0, FIRST));
        assert_eq!(ctx.location(THEIRS), Some(Location::Base(0)));
        assert_eq!(
            namesakes_here(&ctx, FIRST),
            1,
            "a stolen Spiderling lands in your base first"
        );
        assert_eq!(
            ctx.move_unit(
                THEIRS,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            namesakes_here(&ctx, FIRST),
            2,
            "marched back in under your control it joins the count"
        );
        assert_eq!(ctx.current_might(THEIRS), i32::from(MIGHT) + 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_spiderling_or_one_in_hand_is_a_plain_one() {
        let mut fixture = nest(&[FIRST]);
        fixture
            .table
            .cards
            .push(spiderling(SECOND, fixtures::HAND, 0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(statics::grants_on(&ctx, FIRST).is_empty());
        assert_eq!(ctx.current_might(FIRST), i32::from(MIGHT));
        assert!(
            statics::grants_on(&ctx, SECOND).is_empty(),
            "365.1 · not on the board, so the passive is inactive; the any-number deck line is agni_riftbound::legality's copy limit, not the engine's"
        );
        assert_eq!(namesakes_here(&ctx, SECOND), 0);
    }
}

use super::prelude::{attachments_of, unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::attach;
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn equipment_base_bonus(ctx: &Ctx, card: u32) -> i16 {
    attachments_of(ctx, card)
        .into_iter()
        .filter(|gear| {
            ctx.script(*gear)
                .is_some_and(|script| script.is_equipment())
        })
        .filter_map(|gear| attach::might_bonus_of(ctx, gear))
        .fold(0i16, i16::saturating_add)
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    let ladder = i16::try_from(LADDER).unwrap_or(i16::MAX);
    equipment_base_bonus(ctx, card).clamp(0, ladder)
}

fn at_least<const N: i16>(ctx: &Ctx, card: u32, _: u32) -> bool {
    equipment_base_bonus(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static DOUBLED: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit("Gearhead", &[Keyword::Accelerate], &[]),
    &[Static::While(always, DOUBLED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        attach_gear, detach_gear, equip, gear, while_attached, with_statics as statics_of,
        Attached, CHAOS,
    };
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;

    const GEARHEAD: u32 = 90;
    const BOOTS: u32 = 91;
    const SWORD: u32 = 92;
    const TRINKET: u32 = 93;
    const OTHER: u32 = 94;

    static SWORD_CARD: Card = statics_of(
        gear("Sword", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[Grant::Might(3)])],
    );

    fn always_fresh(_: &Ctx, _: u32, _: u32) -> bool {
        true
    }

    static TRINKET_CARD: Card = statics_of(
        gear("Trinket", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[
            Grant::Might(0),
            Grant::MightIf(always_fresh, 2),
            Grant::Keyword(Keyword::Ganking),
        ])],
    );

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(GEARHEAD, fixtures::BASE, 0, "Gearhead", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(OTHER, fixtures::BASE, 0, "Other", 3));
        fixture.table.cards.push(fixtures::gear(
            BOOTS,
            fixtures::BASE,
            0,
            "Boots of Swiftness",
            3,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(SWORD, fixtures::BASE, 0, "Sword", 4));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(SWORD, &SWORD_CARD)
            .with_script(TRINKET, &TRINKET_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GEARHEAD).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_an_accelerate_unit_whose_while_repeats_each_attached_equipments_base_bonus() {
        assert!(std::ptr::eq(script_of("Gearhead").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(DOUBLED.len(), LADDER);
        assert!(DOUBLED
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn each_equipment_attached_gives_its_badge_twice_and_the_second_helping_leaves_with_it() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        assert_eq!(equipment_base_bonus(&ctx, GEARHEAD), 0);
        assert!(statics::grants_on(&ctx, GEARHEAD).is_empty());
        assert_eq!(ctx.current_might(GEARHEAD), 3);
        assert_eq!(attach_gear(&mut ctx, BOOTS, GEARHEAD), Attached::Yes);
        assert_eq!(equipment_base_bonus(&ctx, GEARHEAD), 2);
        assert_eq!(might_bonus(&ctx, GEARHEAD), 2);
        assert_eq!(statics::grants_on(&ctx, GEARHEAD).len(), 2);
        assert_eq!(
            ctx.current_might(GEARHEAD),
            7,
            "the boots' +2 as the attach layer and +2 again as his own"
        );
        assert!(ctx.has_keyword(GEARHEAD, Keyword::Ganking));
        assert_eq!(attach_gear(&mut ctx, SWORD, GEARHEAD), Attached::Yes);
        assert_eq!(equipment_base_bonus(&ctx, GEARHEAD), 5);
        assert_eq!(ctx.current_might(GEARHEAD), 13, "3 + (2 + 3) twice");
        assert!(detach_gear(&mut ctx, BOOTS));
        assert_eq!(ctx.current_might(GEARHEAD), 9, "3 + 3 twice");
        assert_eq!(attach_gear(&mut ctx, SWORD, OTHER), Attached::Yes);
        assert_eq!(
            ctx.current_might(GEARHEAD),
            3,
            "the sword moved to another unit"
        );
        assert_eq!(ctx.current_might(OTHER), 6, "who gets the badge once");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn only_the_base_bonus_is_doubled_a_conditional_bonus_and_a_zero_badge_add_nothing_more() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        assert_eq!(attach_gear(&mut ctx, TRINKET, GEARHEAD), Attached::Yes);
        assert_eq!(
            equipment_base_bonus(&ctx, GEARHEAD),
            0,
            "a +0 badge is the base bonus, the +2 is conditional"
        );
        assert_eq!(
            ctx.current_might(GEARHEAD),
            5,
            "the trinket's conditional +2 once, nothing doubled"
        );
        assert!(ctx.has_keyword(GEARHEAD, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gearhead_in_hand_projects_nothing_and_the_ladder_holds_four_swords() {
        let mut fixture = workshop();
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::HAND, 0, "Gearhead", 3));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(statics::grants_on(&ctx, 95).is_empty());
        assert_eq!(might_bonus(&ctx, 95), 0);
        assert!(
            i16::try_from(LADDER).unwrap() >= 3 * 4,
            "four swords fit on the ladder"
        );
    }
}

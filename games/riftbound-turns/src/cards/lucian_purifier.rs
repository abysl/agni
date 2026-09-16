use super::prelude::{attachments_of, legend, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const ASSAULT: Keyword = Keyword::Assault(1);
pub const LADDER: usize = 8;

pub fn equipment_of_yours_on(ctx: &Ctx, source: u32, unit: u32) -> usize {
    let seat = ctx.controller(source);
    attachments_of(ctx, unit)
        .into_iter()
        .filter(|gear| ctx.controller(*gear) == seat)
        .filter(|gear| ctx.script(*gear).is_some_and(Card::is_equipment))
        .count()
}

fn at_least<const N: usize>(ctx: &Ctx, source: u32, unit: u32) -> bool {
    equipment_of_yours_on(ctx, source, unit) >= N
}

macro_rules! rungs {
    ($($step:literal)+) => {
        &[$(Static::Aura {
            scope: Scope::FriendlyUnits,
            when: at_least::<$step>,
            grants: &[Grant::Keyword(ASSAULT)],
        }),+]
    };
}

pub static PER_EQUIPMENT: &[Static] = rungs!(1 2 3 4 5 6 7 8);

pub static CARD: Card = with_statics(legend("Lucian - Purifier", &[], &[]), PER_EQUIPMENT);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{equip, gear, unit, while_attached};
    use crate::cards::{script_of, Cost};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{attach, statics};
    use crate::state::{FLAG_ATTACKER, FLAG_DEFENDER};

    const LUCIAN: u32 = fixtures::LEGEND_CARD;
    const PISTOL: u32 = 90;
    const SECOND_PISTOL: u32 = 91;
    const TRINKET: u32 = 92;
    const THEIR_PISTOL: u32 = 93;

    static PISTOL_CARD: Card = with_statics(
        gear(
            "Pistol",
            &[Keyword::Equip(Cost::FREE)],
            &[equip(Cost::FREE)],
        ),
        &[while_attached(&[])],
    );

    static TRINKET_CARD: Card = gear("Trinket", &[], &[]);

    static GUNSLINGER: Card = unit("Gunslinger", &[Keyword::Assault(2)], &[]);

    fn armory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LUCIAN).unwrap().name = CARD.name.into();
        for (id, seat) in [(PISTOL, 0), (SECOND_PISTOL, 0), (THEIR_PISTOL, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::gear(id, fixtures::BASE, seat, "Pistol", 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(PISTOL, &PISTOL_CARD)
            .with_script(SECOND_PISTOL, &PISTOL_CARD)
            .with_script(THEIR_PISTOL, &PISTOL_CARD)
            .with_script(TRINKET, &TRINKET_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LUCIAN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn assaults(ctx: &Ctx, unit: u32) -> usize {
        statics::grants_on(ctx, unit)
            .into_iter()
            .filter(|grant| matches!(grant, Grant::Keyword(Keyword::Assault(1))))
            .count()
    }

    #[test]
    fn the_legend_is_a_ladder_of_auras_one_assault_per_equipment_on_a_friendly_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.statics.len(), LADDER);
        assert!(CARD.statics.iter().all(|held| matches!(
            held,
            Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Assault(1))],
                ..
            }
        )));
        assert!(CARD.has_aura());
    }

    #[test]
    fn each_of_your_equipment_gives_its_wearer_assault_and_a_plain_gear_gives_nothing() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        assert_eq!(equipment_of_yours_on(&ctx, LUCIAN, fixtures::VI), 0);
        assert_eq!(assaults(&ctx, fixtures::VI), 0);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(1)));
        assert_eq!(
            attach::attach(&mut ctx, PISTOL, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(equipment_of_yours_on(&ctx, LUCIAN, fixtures::VI), 1);
        assert_eq!(assaults(&ctx, fixtures::VI), 1);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Assault(1)));
        assert_eq!(
            attach::attach(&mut ctx, SECOND_PISTOL, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(equipment_of_yours_on(&ctx, LUCIAN, fixtures::VI), 2);
        assert_eq!(
            assaults(&ctx, fixtures::VI),
            2,
            "each Equipment gives its own"
        );
        assert_eq!(
            attach::attach(&mut ctx, TRINKET, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(
            equipment_of_yours_on(&ctx, LUCIAN, fixtures::VI),
            2,
            "a gear without Equip is not Equipment"
        );
        assert_eq!(assaults(&ctx, fixtures::VI), 2);
        assert!(attach::detach(&mut ctx, PISTOL));
        assert_eq!(assaults(&ctx, fixtures::VI), 1);
        assert!(
            assaults(&ctx, PISTOL) == 0 && assaults(&ctx, SECOND_PISTOL) == 0,
            "the gear itself is no unit"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_granted_assault_reads_as_might_for_an_attacker_alone_and_stacks_with_printed_assault() {
        let mut fixture = armory();
        fixture
            .table
            .cards
            .push(fixtures::unit(94, fixtures::BF1, 0, "Gunslinger", 2));
        fixture.scripts = fixture.scripts.clone().with_script(94, &GUNSLINGER);
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, PISTOL, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(
            attach::attach(&mut ctx, SECOND_PISTOL, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "Assault waits for an attack"
        );
        ctx.set_flag(fixtures::VI, FLAG_DEFENDER, true);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "a defender gets nothing"
        );
        ctx.set_flag(fixtures::VI, FLAG_DEFENDER, false);
        ctx.set_flag(fixtures::VI, FLAG_ATTACKER, true);
        assert_eq!(ctx.current_might(fixtures::VI), 5, "two Equipment, +2");
        assert!(attach::detach(&mut ctx, SECOND_PISTOL));
        assert_eq!(
            attach::attach(&mut ctx, SECOND_PISTOL, 94),
            attach::Attached::Yes
        );
        ctx.set_flag(94, FLAG_ATTACKER, true);
        assert_eq!(
            ctx.current_might(94),
            5,
            "printed Assault 2 and the granted Assault 1 add up"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4);
    }

    #[test]
    fn an_opponents_equipment_and_an_enemy_wearer_get_nothing_from_him() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, THEIR_PISTOL, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(
            equipment_of_yours_on(&ctx, LUCIAN, fixtures::VI),
            0,
            "the Pistol is seat 1's, not Lucian's controller's"
        );
        assert_eq!(assaults(&ctx, fixtures::VI), 0);
        assert!(attach::detach(&mut ctx, THEIR_PISTOL));
        assert_eq!(
            attach::attach(&mut ctx, PISTOL, fixtures::THEIR_UNIT),
            attach::Attached::Yes
        );
        assert_eq!(
            equipment_of_yours_on(&ctx, LUCIAN, fixtures::THEIR_UNIT),
            1,
            "the count is his, the aura's scope is not"
        );
        assert_eq!(
            assaults(&ctx, fixtures::THEIR_UNIT),
            0,
            "an enemy unit is outside the friendly aura"
        );
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(
            assaults(&ctx, fixtures::THEIR_UNIT),
            1,
            "friendly follows the controller"
        );
        assert!(ctx.fault.is_none());
    }
}

use super::prelude::{equip, gear, location_of, while_attached, with_statics, Location};
use super::{Card, Cost, Domain, Grant, Keyword, Power};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const PAIRED_BONUS: i16 = 2;
pub const COMPANIONS: usize = 1;

pub fn other_friendly_units_here(ctx: &Ctx, unit: u32) -> Vec<u32> {
    let Some(here @ Location::Battlefield(_)) = location_of(ctx, unit) else {
        return Vec::new();
    };
    let seat = ctx.controller(unit);
    ctx.units_at(here)
        .into_iter()
        .filter(|other| *other != unit && ctx.controller(*other) == seat)
        .collect()
}

pub fn with_exactly_one_other_friendly_unit(ctx: &Ctx, unit: u32, _gear: u32) -> bool {
    other_friendly_units_here(ctx, unit).len() == COMPANIONS
}

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Might(MIGHT_BONUS),
    Grant::MightIf(with_exactly_one_other_friendly_unit, PAIRED_BONUS),
];

pub static CARD: Card = with_statics(
    gear("Hand Hammer", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::{attach_gear, attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, statics};
    use crate::state::PromptWhy;
    use crate::Refusal;

    const EQUIP_INDEX: u8 = 0;
    const ALLY: u32 = 95;
    const SECOND_ALLY: u32 = 96;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Hand Hammer", 2, "Calm"));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Wailer", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND_ALLY, fixtures::BASE, 0, "Scout", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GEAR).unwrap(), &CARD));
        fixture
    }

    fn send(ctx: &mut Ctx, unit: u32, to: Location) {
        assert_eq!(ctx.move_unit(unit, to, MoveCause::Effect), Moved::Moved);
    }

    #[test]
    fn the_script_is_a_calm_equipment_with_a_stored_plus_one_and_a_projected_plus_two() {
        assert!(std::ptr::eq(script_of("Hand Hammer").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::MightIf(_, 2)]
        ));
        assert_eq!(COMPANIONS, 1);
    }

    #[test]
    fn the_wearer_reads_plus_one_and_plus_three_with_exactly_one_other_friendly_unit_beside_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one Calm rune recycled for the power"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the base is not a battlefield: +1 alone"
        );
        assert!(other_friendly_units_here(&ctx, fixtures::VI).is_empty());
        let here = Location::Battlefield(fixtures::BF1);
        send(&mut ctx, fixtures::VI, here);
        assert_eq!(ctx.current_might(fixtures::VI), 4, "alone there: +1");
        send(&mut ctx, ALLY, here);
        assert_eq!(other_friendly_units_here(&ctx, fixtures::VI), [ALLY]);
        assert!(with_exactly_one_other_friendly_unit(
            &ctx,
            fixtures::VI,
            GEAR
        ));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            6,
            "+1 stored, +2 projected"
        );
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::MightIf(_, 2)]
        ));
        assert_eq!(ctx.current_might(ALLY), 2, "the companion gets nothing");
        send(&mut ctx, SECOND_ALLY, here);
        assert_eq!(
            other_friendly_units_here(&ctx, fixtures::VI),
            [ALLY, SECOND_ALLY]
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "two companions are not exactly one"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_beside_the_wearer_is_not_a_companion_and_a_companions_death_drops_the_bonus() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let there = Location::Battlefield(fixtures::BF2);
        send(&mut ctx, fixtures::VI, there);
        assert_eq!(
            ctx.units_at(there).len(),
            2,
            "the Sprite already stands at their battlefield"
        );
        assert!(other_friendly_units_here(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        send(&mut ctx, ALLY, there);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(
            !with_exactly_one_other_friendly_unit(&ctx, fixtures::VI, GEAR),
            "the companion is in the trash"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn both_parts_leave_with_the_hammer_and_follow_it_to_a_new_wearer() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let here = Location::Battlefield(fixtures::BF1);
        send(&mut ctx, fixtures::VI, here);
        send(&mut ctx, ALLY, here);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        ctx.detach(GEAR);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        attach_gear(&mut ctx, GEAR, ALLY);
        assert_eq!(
            ctx.current_might(ALLY),
            2 + 1 + 2,
            "the new wearer stands with exactly one other friendly unit: Vi"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.kill(GEAR, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert!(!is_attached(&ctx, GEAR));
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_a_missing_calm_rune_and_an_enemy_wearer_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, GEAR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, GEAR));
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        {
            let held = broke.table.card_mut(42).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }
}

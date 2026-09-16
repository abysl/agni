use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Trigger};

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 2;

pub fn mirrored(trigger: Trigger) -> Option<Trigger> {
    match trigger {
        Trigger::Hold(who) => Some(Trigger::Conquer(who)),
        Trigger::Conquer(who) => Some(Trigger::Hold(who)),
        _ => None,
    }
}

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Mirror(mirrored)];

pub static CARD: Card = with_statics(
    gear(
        "Skyfall of Areion",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, held, GEAR};
    use crate::cards::prelude::{attach_gear, done, on_conquer_me, on_hold_me, spawn_gold, unit};
    use crate::cards::{script_of, Flow, Item, Stage, Static, Who};
    use crate::engine::ctx::Ctx;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::triggers;

    const RAIDER: u32 = 91;
    const ON_CONQUER: u8 = 0;
    const ON_HOLD: u8 = 1;

    fn gold(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        spawn_gold(ctx, item.controller, false);
        done()
    }

    static RAIDER_CARD: Card = unit(
        "Raider",
        &[],
        &[on_conquer_me(&[], gold), on_hold_me(&[], gold)],
    );

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Skyfall of Areion", 3, "Fury"));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 0, "Raider", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(RAIDER, &RAIDER_CARD);
        fixture
    }

    #[test]
    fn the_script_is_a_one_energy_fury_equipment_with_two_might_and_no_listener_of_its_own() {
        assert!(std::ptr::eq(script_of("Skyfall of Areion").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(EQUIP.energy, 1);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].label, Some("equip"));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(2), Grant::Mirror(_)]
        ));
        assert_eq!(
            mirrored(Trigger::Hold(Who::Me)),
            Some(Trigger::Conquer(Who::Me))
        );
        assert_eq!(
            mirrored(Trigger::Conquer(Who::You)),
            Some(Trigger::Hold(Who::You))
        );
        assert_eq!(mirrored(Trigger::Play), None);
        assert_eq!(mirrored(Trigger::Attacks(Who::Me)), None);
    }

    #[test]
    fn a_wearers_conquer_trigger_matches_a_hold_and_its_hold_trigger_a_conquer() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let conquer = RAIDER_CARD.abilities[usize::from(ON_CONQUER)].trigger;
        let hold = RAIDER_CARD.abilities[usize::from(ON_HOLD)].trigger;
        assert_eq!(
            triggers::matches(&ctx, conquer, &held(&[RAIDER]), RAIDER),
            None,
            "without the skyfall a hold is a hold"
        );
        assert_eq!(
            triggers::matches(&ctx, hold, &held(&[RAIDER]), RAIDER),
            Some(0)
        );
        attach_gear(&mut ctx, GEAR, RAIDER);
        assert_eq!(ctx.current_might(RAIDER), 5);
        assert_eq!(
            triggers::matches(&ctx, conquer, &held(&[RAIDER]), RAIDER),
            Some(0),
            "my hold effects are also conquer effects"
        );
        assert_eq!(
            triggers::matches(&ctx, hold, &conquered(&[RAIDER]), RAIDER),
            Some(0),
            "and vice versa"
        );
        assert_eq!(
            triggers::matches(&ctx, conquer, &held(&[fixtures::VI]), RAIDER),
            None,
            "another unit's hold is nothing of mine"
        );
        assert_eq!(
            triggers::matches(&ctx, Trigger::Attacks(Who::Me), &held(&[RAIDER]), RAIDER),
            None,
            "only hold and conquer mirror"
        );
        assert_eq!(
            triggers::matches(&ctx, conquer, &held(&[RAIDER]), fixtures::VI),
            None,
            "a unit without the skyfall reads no mirror"
        );
    }

    #[test]
    fn a_hold_by_the_wearer_queues_both_of_its_triggers_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, RAIDER);
        let found = triggers::find(&ctx, &held(&[RAIDER]));
        let mut indexes: Vec<u8> = found
            .iter()
            .filter(|found| found.source == RAIDER)
            .map(|found| found.index)
            .collect();
        indexes.sort_unstable();
        assert_eq!(indexes, [ON_CONQUER, ON_HOLD]);
        let found = triggers::find(&ctx, &conquered(&[RAIDER]));
        let mut indexes: Vec<u8> = found
            .iter()
            .filter(|found| found.source == RAIDER)
            .map(|found| found.index)
            .collect();
        indexes.sort_unstable();
        assert_eq!(indexes, [ON_CONQUER, ON_HOLD]);
    }
}

use super::prelude::{battlefield, location_of, with_statics};
use super::{Card, Static};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::{prevent, statics};
use crate::state::DamageSource;

pub const BONUS: u8 = 1;

pub static CARD: Card = with_statics(
    battlefield("Void Gate", &[], &[]),
    &[Static::BonusDamage(|ctx, gate, unit, cause| {
        bonus_damage_from(ctx, gate, unit, *cause)
    })],
);

pub fn is_gate(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn from_a_spell_or_ability(cause: Cause) -> bool {
    prevent::covers(DamageSource::SpellOrAbility, cause)
}

pub fn bonus_damage_from(ctx: &Ctx, gate: u32, unit: u32, cause: Cause) -> u8 {
    if !is_gate(ctx, gate) || !statics::in_play(ctx, gate) || !from_a_spell_or_ability(cause) {
        return 0;
    }
    let here = location_of(ctx, gate);
    if here.is_none() || location_of(ctx, unit) != here || !ctx.is_unit(unit) {
        return 0;
    }
    BONUS
}

pub fn bonus_damage(ctx: &Ctx, unit: u32, cause: Cause) -> u8 {
    ctx.table
        .cards
        .iter()
        .map(|card| bonus_damage_from(ctx, card.id, unit, cause))
        .fold(0u8, u8::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::deal;
    use crate::cards::Source;
    use crate::engine::ctx::{Location, MoveCause, Moved, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::Target;

    const GATE: u32 = fixtures::GROUNDS;

    fn gate() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(GATE).unwrap().name = "Void Gate".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GATE).unwrap(), &CARD));
        fixture
    }

    fn spark() -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        )
    }

    fn damage_on(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    #[test]
    fn the_gate_is_a_battlefield_with_no_abilities_whose_text_is_a_bonus_damage_static() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Void Gate").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::BonusDamage(|_, _, _, _| 0)));
    }

    #[test]
    fn spells_and_abilities_carry_one_bonus_damage_against_units_here_only() {
        let mut fixture = gate();
        let ctx = fixture.ctx();
        let ability = Cause::Ability(Source {
            card: fixtures::LEGEND_CARD,
            ability: 0,
        });
        assert!(is_gate(&ctx, GATE));
        assert!(!is_gate(&ctx, fixtures::ROCKFALL));
        assert_eq!(
            bonus_damage_from(&ctx, GATE, fixtures::VI, Cause::Item(7)),
            BONUS
        );
        assert_eq!(bonus_damage_from(&ctx, GATE, fixtures::VI, ability), BONUS);
        assert_eq!(bonus_damage(&ctx, fixtures::VI, Cause::Item(7)), BONUS);
        assert_eq!(
            bonus_damage(&ctx, fixtures::SPRITE, Cause::Item(7)),
            0,
            "a unit at the other battlefield"
        );
        assert_eq!(
            bonus_damage(&ctx, fixtures::THEIR_UNIT, Cause::Item(7)),
            0,
            "a unit in a base"
        );
        assert_eq!(
            bonus_damage_from(&ctx, fixtures::ROCKFALL, fixtures::SPRITE, Cause::Item(7)),
            0,
            "Rockfall Path grants nothing"
        );
        assert_eq!(
            bonus_damage_from(&ctx, GATE, GATE, Cause::Item(7)),
            0,
            "the battlefield card is not a unit here"
        );
    }

    #[test]
    fn combat_damage_and_rule_damage_carry_no_bonus() {
        let mut fixture = gate();
        let ctx = fixture.ctx();
        for cause in [
            Cause::Combat,
            Cause::Rule,
            Cause::Cost,
            Cause::Replacement,
            Cause::Cleanup { last_item: None },
        ] {
            assert!(!from_a_spell_or_ability(cause), "{cause:?}");
            assert_eq!(bonus_damage(&ctx, fixtures::VI, cause), 0, "{cause:?}");
        }
        assert!(from_a_spell_or_ability(Cause::Cleanup {
            last_item: Some(3)
        }));
    }

    #[test]
    fn a_unit_arriving_here_picks_the_bonus_up_and_one_leaving_sheds_it() {
        let mut fixture = gate();
        let mut ctx = fixture.ctx();
        assert_eq!(bonus_damage(&ctx, fixtures::THEIR_UNIT, Cause::Item(7)), 0);
        assert_eq!(
            ctx.move_unit(
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            bonus_damage(&ctx, fixtures::THEIR_UNIT, Cause::Item(7)),
            BONUS,
            "either seat's unit here"
        );
        ctx.recall(fixtures::VI, false);
        assert_eq!(bonus_damage(&ctx, fixtures::VI, Cause::Item(7)), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_dealing_one_to_a_unit_elsewhere_deals_exactly_one() {
        let mut fixture = gate();
        let mut ctx = fixture.ctx();
        let item = spark();
        assert!(deal(&mut ctx, &item, fixtures::SPRITE, 1));
        assert_eq!(damage_on(&ctx, fixtures::SPRITE), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_dealing_one_to_a_unit_here_deals_two() {
        let mut fixture = gate();
        let mut ctx = fixture.ctx();
        let item = spark();
        assert!(deal(&mut ctx, &item, fixtures::VI, 1));
        assert_eq!(damage_on(&ctx, fixtures::VI), 2);
        assert!(deal(&mut ctx, &item, fixtures::SPRITE, 1));
        assert_eq!(damage_on(&ctx, fixtures::SPRITE), 1, "not here");
        assert!(ctx.fault.is_none());
    }
}

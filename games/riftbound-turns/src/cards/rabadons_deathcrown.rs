use super::prelude::{equip, gear, is_attached, while_attached, with_statics, RAINBOW};
use super::{Card, Cost, Grant, Keyword, Static};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::prevent;
use crate::state::DamageSource;

pub const EQUIP: Cost = RAINBOW;

pub const MIGHT_BONUS: i16 = 3;
pub const BONUS: u8 = 3;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub fn is_crown(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn controller_of_cause(ctx: &Ctx, cause: Cause) -> Option<u8> {
    if !prevent::covers(DamageSource::SpellOrAbility, cause) {
        return None;
    }
    match cause {
        Cause::Item(item)
        | Cause::Cleanup {
            last_item: Some(item),
        } => ctx.live_item(item).map(|held| held.controller),
        Cause::Ability(source) => Some(ctx.controller(source.card)),
        _ => None,
    }
}

pub fn bonus_damage_from(ctx: &Ctx, crown: u32, unit: u32, cause: Cause) -> u8 {
    if !is_crown(ctx, crown) || !is_attached(ctx, crown) || !ctx.is_unit(unit) {
        return 0;
    }
    if controller_of_cause(ctx, cause) != Some(ctx.controller(crown)) {
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

pub static CARD: Card = with_statics(
    gear(
        "Rabadon's Deathcrown",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[
        while_attached(EFFECT_TEXT),
        Static::BonusDamage(|ctx, crown, unit, cause| bonus_damage_from(ctx, crown, unit, *cause)),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::{a_unit, attach_gear, card_target, deal, done, play, spell};
    use crate::cards::{script_of, Flow, Item, Source, Stage, Static};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};

    const ZAP: u32 = 91;
    const DAMAGE: u8 = 1;

    fn zap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        if let Some(unit) = card_target(ctx, item, 0) {
            deal(ctx, item, unit, DAMAGE);
        }
        done()
    }

    static ZAP_CARD: Card = spell("Zap", &[], &[play(&[a_unit("a unit")], zap)]);

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut crown = equipment(GEAR, 0, "Rabadon's Deathcrown", 4, "Calm");
        crown.domain = vec!["Calm".into(), "Mind".into()];
        crown.power = Some(2);
        fixture.table.cards.push(crown);
        fixture
            .table
            .cards
            .push(fixtures::spell(ZAP, fixtures::HAND, 0, "Zap", 1, 0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &ZAP_CARD);
        fixture
    }

    fn on_chain(ctx: &mut Ctx, id: u16, seat: u8) -> Cause {
        ctx.blob.chain.push(ChainItem::new(
            id,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            seat,
            Origin::Hand,
        ));
        Cause::Item(id)
    }

    #[test]
    fn the_script_is_a_rainbow_equipment_with_three_might_and_a_bonus_damage_static() {
        assert!(std::ptr::eq(
            script_of("Rabadon's Deathcrown").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Equip(RAINBOW)]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].label, Some("equip"));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(CARD.has_static(Static::BonusDamage(|_, _, _, _| 0)));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(3)]));
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn your_spells_and_abilities_carry_three_bonus_damage_while_the_crown_is_attached() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let mine = on_chain(&mut ctx, 7, 0);
        let theirs = on_chain(&mut ctx, 8, 1);
        let my_ability = Cause::Ability(Source {
            card: fixtures::LEGEND_CARD,
            ability: 0,
        });
        let their_ability = Cause::Ability(Source {
            card: fixtures::THEIR_UNIT,
            ability: 0,
        });
        assert!(is_crown(&ctx, GEAR));
        assert!(!is_crown(&ctx, fixtures::HAND_GEAR));
        assert_eq!(controller_of_cause(&ctx, mine), Some(0));
        assert_eq!(controller_of_cause(&ctx, theirs), Some(1));
        assert_eq!(controller_of_cause(&ctx, my_ability), Some(0));
        assert_eq!(controller_of_cause(&ctx, Cause::Combat), None);
        assert_eq!(
            controller_of_cause(&ctx, Cause::Item(99)),
            None,
            "gone from the chain"
        );
        assert_eq!(
            bonus_damage(&ctx, fixtures::THEIR_UNIT, mine),
            0,
            "a loose crown grants nothing"
        );
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(
            bonus_damage_from(&ctx, GEAR, fixtures::THEIR_UNIT, mine),
            BONUS
        );
        assert_eq!(bonus_damage(&ctx, fixtures::THEIR_UNIT, mine), BONUS);
        assert_eq!(bonus_damage(&ctx, fixtures::SPRITE, my_ability), BONUS);
        assert_eq!(
            bonus_damage(&ctx, fixtures::VI, mine),
            BONUS,
            "my own units are not spared"
        );
        assert_eq!(
            bonus_damage(&ctx, fixtures::THEIR_UNIT, theirs),
            0,
            "their spell"
        );
        assert_eq!(
            bonus_damage(&ctx, fixtures::VI, their_ability),
            0,
            "their ability"
        );
        assert_eq!(bonus_damage(&ctx, GEAR, mine), 0, "a gear takes no damage");
        assert_eq!(
            bonus_damage_from(&ctx, fixtures::HAND_GEAR, fixtures::THEIR_UNIT, mine),
            0,
            "another gear grants nothing"
        );
        for cause in [
            Cause::Combat,
            Cause::Rule,
            Cause::Cost,
            Cause::Replacement,
            Cause::Cleanup { last_item: None },
        ] {
            assert_eq!(
                bonus_damage(&ctx, fixtures::THEIR_UNIT, cause),
                0,
                "{cause:?}"
            );
        }
        ctx.detach(GEAR);
        assert_eq!(bonus_damage(&ctx, fixtures::THEIR_UNIT, mine), 0);
    }

    #[test]
    fn a_spell_of_the_wearers_controller_deals_three_more_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, ZAP).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(fixtures::SPRITE));
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Static};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::statics;

pub const RUNES: usize = 7;

pub fn you_control_seven_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() >= RUNES
}

pub fn from_an_enemy_spell_or_ability(ctx: &Ctx, me: u32, cause: Cause) -> bool {
    let seat = ctx.controller(me);
    match cause {
        Cause::Item(item)
        | Cause::Cleanup {
            last_item: Some(item),
        } => ctx
            .live_item(item)
            .is_some_and(|held| held.controller != seat),
        Cause::Ability(source) => ctx.controller(source.card) != seat,
        Cause::Combat
        | Cause::Replacement
        | Cause::Cost
        | Cause::Rule
        | Cause::Cleanup { last_item: None } => false,
    }
}

pub fn prevents_all_damage_from(ctx: &Ctx, me: u32, cause: Cause) -> bool {
    statics::in_play(ctx, me)
        && you_control_seven_runes(ctx, ctx.controller(me))
        && from_an_enemy_spell_or_ability(ctx, me, cause)
}

pub static CARD: Card = with_statics(
    unit("Esteemed Hierophant", &[], &[]),
    &[Static::NoDamage(|ctx, me, cause| {
        prevents_all_damage_from(ctx, me, *cause)
    })],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, card_target, deal, play, spell};
    use crate::cards::script_of;
    use crate::cards::{Flow, Source};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const HIEROPHANT: u32 = 90;
    const ENEMY_SPELL: u16 = 7;
    const OWN_SPELL: u16 = 8;
    const MIGHT: u8 = 5;

    static ENEMY_BLAST: Card = spell(
        "Enemy Blast",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                deal(ctx, item, unit, 6);
            }
            Flow::Done
        })],
    );

    fn hierophant(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: None,
            domain: vec!["Calm".into()],
            ..fixtures::unit(HIEROPHANT, zone, seat, "Esteemed Hierophant", MIGHT)
        }
    }

    fn sanctum(zone: u16, runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hierophant(zone, 0));
        for offset in 0..runes.saturating_sub(4) {
            let id = 46 + u32::try_from(offset).unwrap();
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Calm", offset % 2 == 0));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HIEROPHANT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn with_spells_on_the_chain(ctx: &mut Ctx) {
        ctx.blob.chain.push(ChainItem::new(
            ENEMY_SPELL,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        ));
        ctx.blob.chain.push(ChainItem::new(
            OWN_SPELL,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_whole_text_is_a_no_damage_static() {
        assert!(std::ptr::eq(
            script_of("Esteemed Hierophant").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Esteemed Hierophant");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::NoDamage(_)]));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(RUNES, 7);
    }

    #[test]
    fn the_rune_count_is_your_whole_pool_ready_or_not_and_seven_is_the_line() {
        let mut fixture = sanctum(fixtures::BASE, 6);
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 6);
        assert!(!you_control_seven_runes(&ctx, 0));
        assert!(!you_control_seven_runes(&ctx, 1), "the opponent has two");
        drop(ctx);
        let mut fixture = sanctum(fixtures::BASE, 7);
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 7);
        assert!(
            ctx.runes_of(0).iter().any(|rune| rune.exhausted),
            "exhausted runes are still controlled"
        );
        assert!(you_control_seven_runes(&ctx, 0));
        drop(ctx);
        let mut fixture = sanctum(fixtures::BASE, 9);
        let ctx = fixture.ctx();
        assert!(you_control_seven_runes(&ctx, 0));
    }

    #[test]
    fn the_seam_reads_an_enemy_spell_or_ability_and_never_combat_or_your_own() {
        let mut fixture = sanctum(fixtures::BASE, 7);
        let mut ctx = fixture.ctx();
        with_spells_on_the_chain(&mut ctx);
        let enemy = Cause::Item(ENEMY_SPELL);
        let own = Cause::Item(OWN_SPELL);
        let their_ability = Cause::Ability(Source {
            card: fixtures::THEIR_UNIT,
            ability: 0,
        });
        let my_ability = Cause::Ability(Source {
            card: fixtures::VI,
            ability: 0,
        });
        assert!(from_an_enemy_spell_or_ability(&ctx, HIEROPHANT, enemy));
        assert!(from_an_enemy_spell_or_ability(
            &ctx,
            HIEROPHANT,
            their_ability
        ));
        assert!(from_an_enemy_spell_or_ability(
            &ctx,
            HIEROPHANT,
            Cause::Cleanup {
                last_item: Some(ENEMY_SPELL)
            }
        ));
        assert!(!from_an_enemy_spell_or_ability(&ctx, HIEROPHANT, own));
        assert!(!from_an_enemy_spell_or_ability(
            &ctx, HIEROPHANT, my_ability
        ));
        for cause in [
            Cause::Combat,
            Cause::Rule,
            Cause::Cost,
            Cause::Replacement,
            Cause::Cleanup { last_item: None },
            Cause::Item(99),
        ] {
            assert!(
                !from_an_enemy_spell_or_ability(&ctx, HIEROPHANT, cause),
                "{cause:?}"
            );
        }
        assert!(
            from_an_enemy_spell_or_ability(&ctx, fixtures::THEIR_UNIT, own),
            "read from the damaged unit's controller"
        );
        assert!(prevents_all_damage_from(&ctx, HIEROPHANT, enemy));
        assert!(prevents_all_damage_from(&ctx, HIEROPHANT, their_ability));
        assert!(!prevents_all_damage_from(&ctx, HIEROPHANT, own));
        assert!(!prevents_all_damage_from(&ctx, HIEROPHANT, Cause::Combat));
        drop(ctx);
        let mut fixture = sanctum(fixtures::BASE, 6);
        let mut ctx = fixture.ctx();
        with_spells_on_the_chain(&mut ctx);
        assert!(
            !prevents_all_damage_from(&ctx, HIEROPHANT, Cause::Item(ENEMY_SPELL)),
            "six runes is short of the line"
        );
        drop(ctx);
        let mut fixture = sanctum(fixtures::HAND, 7);
        let mut ctx = fixture.ctx();
        with_spells_on_the_chain(&mut ctx);
        assert!(
            !prevents_all_damage_from(&ctx, HIEROPHANT, Cause::Item(ENEMY_SPELL)),
            "365.1 · not on the board, so the passive is inactive"
        );
    }

    #[test]
    fn with_seven_runes_an_enemy_spell_deals_it_nothing_while_your_own_and_combat_still_land() {
        let mut fixture = sanctum(fixtures::BASE, 7);
        let mut ctx = fixture.ctx();
        with_spells_on_the_chain(&mut ctx);
        assert!(
            !ctx.damage(HIEROPHANT, 6, Cause::Item(ENEMY_SPELL)),
            "prevented in full"
        );
        assert_eq!(ctx.damage_on(HIEROPHANT), 0);
        assert!(ctx.on_board(HIEROPHANT));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {HIEROPHANT}}}: the damage is prevented")));
        assert!(!ctx.damage(
            HIEROPHANT,
            1,
            Cause::Ability(Source {
                card: fixtures::THEIR_UNIT,
                ability: 0
            })
        ));
        assert!(ctx.damage(HIEROPHANT, 1, Cause::Item(OWN_SPELL)));
        assert_eq!(
            ctx.damage_on(HIEROPHANT),
            1,
            "your own spell is not an enemy's"
        );
        assert!(ctx.damage(HIEROPHANT, 1, Cause::Combat));
        assert_eq!(
            ctx.damage_on(HIEROPHANT),
            2,
            "combat damage is not prevented"
        );
        assert!(
            ctx.damage(fixtures::VI, 1, Cause::Item(ENEMY_SPELL)),
            "only the Hierophant is shielded"
        );
    }

    #[test]
    fn a_resolving_enemy_spell_uses_its_live_item_for_the_immunity() {
        let mut fixture = sanctum(fixtures::BASE, 7);
        let card = fixture.table.card_mut(fixtures::THEIR_HAND_CARD).unwrap();
        card.name = "Enemy Blast".into();
        card.kind = Some("Spell".into());
        card.energy = Some(2);
        card.domain = vec!["Calm".into()];
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::THEIR_HAND_CARD, &ENEMY_BLAST);
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 1, fixtures::THEIR_HAND_CARD).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {HIEROPHANT}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(HIEROPHANT), 0);
        assert!(ctx.on_board(HIEROPHANT));
        assert!(ctx.fault.is_none());
    }
}

use super::diana_lunari::a_showdown_is_open_here;
use super::prelude::{battlefield, granted_reaction, with_statics, RAINBOW};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind, Origin};

pub const SURCHARGE: Cost = RAINBOW;

pub static CARD: Card = with_statics(
    battlefield("Mystic Vortex", &[], &[]),
    &[Static::Surcharge(reaction_surcharge)],
);

pub fn has_reaction(ctx: &Ctx, item: &ChainItem) -> bool {
    let (ItemKind::Spell { card } | ItemKind::Permanent { card }) = item.kind else {
        return false;
    };
    matches!(item.origin, Origin::Facedown { .. })
        || ctx.has_keyword(card, Keyword::Reaction)
        || granted_reaction(ctx, card)
}

pub fn a_reaction_card_during_a_showdown_here(ctx: &Ctx, item: &ChainItem, vortex: u32) -> bool {
    a_showdown_is_open_here(ctx, vortex) && has_reaction(ctx, item)
}

pub fn reaction_surcharge(ctx: &Ctx, item: &ChainItem, vortex: u32) -> Cost {
    if a_reaction_card_during_a_showdown_here(ctx, item, vortex) {
        SURCHARGE
    } else {
        Cost::FREE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Power};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};
    use agni_plugin_sdk::table::CardInfo;

    const VORTEX: u32 = fixtures::GROUNDS;
    const REACTION: u32 = 90;
    const HIDDEN_UNIT: u32 = 91;
    const THEIR_REACTION: u32 = 92;

    fn reaction_spell(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(id, fixtures::HAND, seat, "Decree of Insight", 2, 1)
        }
    }

    fn vortex() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(VORTEX).unwrap().name = "Mystic Vortex".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(reaction_spell(REACTION, 0));
        fixture.table.cards.push(reaction_spell(THEIR_REACTION, 1));
        fixture.table.cards.push(CardInfo {
            energy: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(HIDDEN_UNIT, fixtures::HAND, 0, "Blastcone Fae", 2)
        });
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VORTEX).unwrap(),
            &CARD
        ));
        fixture
    }

    fn open_combat_here(fixture: &mut Fixture) -> Ctx<'_> {
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(a_showdown_is_open_here(&ctx, VORTEX));
        ctx
    }

    fn spell(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card }, seat, Origin::Hand)
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_surcharge_is_one_rainbow() {
        assert!(std::ptr::eq(script_of("Mystic Vortex").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::Surcharge(reaction_surcharge)));
        assert_eq!(SURCHARGE.energy, 0);
        assert_eq!(SURCHARGE.power, [Power::Rainbow]);
    }

    #[test]
    fn out_of_a_showdown_no_card_is_surcharged_here() {
        let mut fixture = vortex();
        let ctx = fixture.ctx();
        assert!(!a_showdown_is_open_here(&ctx, VORTEX));
        let reaction = spell(REACTION, 0);
        assert!(has_reaction(&ctx, &reaction));
        assert!(!a_reaction_card_during_a_showdown_here(
            &ctx, &reaction, VORTEX
        ));
        assert_eq!(reaction_surcharge(&ctx, &reaction, VORTEX), Cost::FREE);
        assert_eq!(
            reaction_surcharge(&ctx, &spell(THEIR_REACTION, 1), VORTEX),
            Cost::FREE
        );
    }

    #[test]
    fn during_a_showdown_here_every_players_reaction_cards_cost_a_rainbow_more() {
        let mut fixture = vortex();
        let ctx = open_combat_here(&mut fixture);
        let mine = spell(REACTION, 0);
        assert!(a_reaction_card_during_a_showdown_here(&ctx, &mine, VORTEX));
        assert_eq!(reaction_surcharge(&ctx, &mine, VORTEX), SURCHARGE);
        let theirs = spell(THEIR_REACTION, 1);
        assert_eq!(
            reaction_surcharge(&ctx, &theirs, VORTEX),
            SURCHARGE,
            "the vortex taxes both sides"
        );
        let action = spell(fixtures::HAND_SPELL, 0);
        assert!(!has_reaction(&ctx, &action));
        assert_eq!(
            reaction_surcharge(&ctx, &action, VORTEX),
            Cost::FREE,
            "Spark is an Action"
        );
        let hidden = ChainItem::new(
            2,
            ItemKind::Permanent { card: HIDDEN_UNIT },
            0,
            Origin::Hand,
        );
        assert!(
            !has_reaction(&ctx, &hidden),
            "811.3 and 811.6 · a Hidden card played from hand is played as normal; it gains Reaction only facedown or played from facedown"
        );
        assert_eq!(reaction_surcharge(&ctx, &hidden, VORTEX), Cost::FREE);
        let facedown = ChainItem::new(
            3,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
        );
        assert!(has_reaction(&ctx, &facedown), "played from face down");
        assert_eq!(reaction_surcharge(&ctx, &facedown, VORTEX), SURCHARGE);
        let ability = ChainItem::new(
            4,
            ItemKind::Ability {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert!(!has_reaction(&ctx, &ability), "an ability is not a card");
        assert_eq!(reaction_surcharge(&ctx, &ability, VORTEX), Cost::FREE);
    }

    #[test]
    fn a_showdown_at_another_battlefield_taxes_nothing_here() {
        let mut fixture = vortex();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx
            .blob
            .showdown
            .as_ref()
            .is_some_and(|held| held.zone == fixtures::BF2));
        assert!(!a_showdown_is_open_here(&ctx, VORTEX));
        assert_eq!(
            reaction_surcharge(&ctx, &spell(REACTION, 0), VORTEX),
            Cost::FREE
        );
    }

    #[test]
    fn during_a_showdown_here_a_reaction_spell_is_priced_a_rainbow_higher() {
        let mut fixture = vortex();
        let ctx = open_combat_here(&mut fixture);
        let mine = spell(REACTION, 0);
        assert_eq!(reaction_surcharge(&ctx, &mine, VORTEX), SURCHARGE);
        let priced = cost::of_item(&ctx, &mine, None);
        assert_eq!(priced.energy, 2);
        assert_eq!(
            priced.power,
            [Need::Domain(crate::cards::Domain::Mind), Need::Rainbow]
        );
        let action = cost::of_item(&ctx, &spell(fixtures::HAND_SPELL, 0), None);
        assert_eq!(action.power, [Need::Domain(crate::cards::Domain::Fury)]);
    }
}

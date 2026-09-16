use super::prelude::{empower, gear, with_statics, RAINBOW};
use super::{Card, Cost, Domain, Keyword, Power, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind};

pub const EMPOWER: Cost = Cost {
    energy: 4,
    power: &[Power::Domain(Domain::Calm)],
};

pub const SURCHARGE: Cost = Cost {
    energy: 1,
    power: &[],
};

pub const EMPOWERED_SURCHARGE: Cost = Cost {
    energy: 1,
    power: RAINBOW.power,
};

pub fn suppresses(ctx: &Ctx, item: &ChainItem, helm: u32) -> bool {
    matches!(item.kind, ItemKind::Spell { .. })
        && statics::in_play(ctx, helm)
        && item.controller != ctx.controller(helm)
}

pub fn spell_surcharge(ctx: &Ctx, item: &ChainItem, helm: u32) -> Cost {
    if !suppresses(ctx, item, helm) {
        Cost::FREE
    } else if ctx.is_empowered(helm) {
        EMPOWERED_SURCHARGE
    } else {
        SURCHARGE
    }
}

pub static CARD: Card = with_statics(
    gear(
        "Helm of Suppression",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::Surcharge(spell_surcharge)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HELM: u32 = 90;
    const THEIR_SPELL: u32 = 92;
    const MIND_RUNE: u32 = 46;
    const EMPOWER_INDEX: u8 = 0;

    fn helm(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::gear(HELM, zone, seat, "Helm of Suppression", 4)
        }
    }

    fn vault(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(helm(zone, 0));
        let mut theirs = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Their Spark", 2, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 1, "Mind", false));
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(HELM).unwrap(), &CARD));
        fixture
    }

    fn theirs() -> ChainItem {
        ChainItem::new(2, ItemKind::Spell { card: THEIR_SPELL }, 1, Origin::Hand)
    }

    fn mine() -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        )
    }

    fn their_unit() -> ChainItem {
        ChainItem::new(
            3,
            ItemKind::Permanent {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        )
    }

    fn empower_it(ctx: &mut Ctx) {
        activate::activate(ctx, 0, HELM, EMPOWER_INDEX).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(HELM));
    }

    #[test]
    fn the_script_prints_empower_for_four_and_a_calm_and_carries_the_surcharge() {
        assert!(std::ptr::eq(
            script_of("Helm of Suppression").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 4);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_INDEX)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::Surcharge(spell_surcharge)));
        assert_eq!(SURCHARGE.energy, 1);
        assert!(SURCHARGE.power.is_empty());
        assert_eq!(EMPOWERED_SURCHARGE.energy, 1);
        assert_eq!(EMPOWERED_SURCHARGE.power, [Power::Rainbow]);
    }

    #[test]
    fn the_surcharge_prices_opponents_spells_one_energy_more_and_a_rainbow_on_top_while_empowered()
    {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(suppresses(&ctx, &theirs(), HELM));
        assert_eq!(spell_surcharge(&ctx, &theirs(), HELM), SURCHARGE);
        assert!(
            !suppresses(&ctx, &mine(), HELM),
            "your own spells are free of it"
        );
        assert_eq!(spell_surcharge(&ctx, &mine(), HELM), Cost::FREE);
        assert!(
            !suppresses(&ctx, &their_unit(), HELM),
            "a unit is not a spell"
        );
        assert_eq!(spell_surcharge(&ctx, &their_unit(), HELM), Cost::FREE);
        empower_it(&mut ctx);
        assert_eq!(spell_surcharge(&ctx, &theirs(), HELM), EMPOWERED_SURCHARGE);
        assert_eq!(spell_surcharge(&ctx, &mine(), HELM), Cost::FREE);
        assert!(ctx.disempower(HELM));
        assert_eq!(spell_surcharge(&ctx, &theirs(), HELM), SURCHARGE);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_helm_in_hand_or_in_the_trash_suppresses_nothing_and_a_stolen_helm_turns_on_its_owner() {
        let mut fixture = vault(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!suppresses(&ctx, &theirs(), HELM));
        assert_eq!(spell_surcharge(&ctx, &theirs(), HELM), Cost::FREE);
        drop(ctx);
        let mut fixture = vault(fixtures::TRASH);
        let ctx = fixture.ctx();
        assert!(!suppresses(&ctx, &theirs(), HELM));
        drop(ctx);
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller(HELM, 1, fixtures::SPRITE));
        assert!(!suppresses(&ctx, &theirs(), HELM));
        assert!(suppresses(&ctx, &mine(), HELM));
        assert_eq!(spell_surcharge(&ctx, &mine(), HELM), SURCHARGE);
    }

    #[test]
    fn the_empower_pays_four_and_a_calm_and_their_spark_is_priced_a_rainbow_higher_after() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let priced = cost::of_item(&ctx, &theirs(), None);
        assert_eq!(priced.energy, 3);
        assert_eq!(priced.power, [Need::Domain(Domain::Mind)]);
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == HELM)
            .expect("the helm offers its Empower");
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {HELM}}}: empower (4 energy and 1 Calm power)")
        );
        let ready = ctx.ready_runes_of(0).len();
        empower_it(&mut ctx);
        assert!(ctx.ready_runes_of(0).len() <= ready - 4);
        assert!(
            ctx.card(42).unwrap().zone != Some(fixtures::RUNE_POOL),
            "the Calm rune is recycled for the power"
        );
        let priced = cost::of_item(&ctx, &theirs(), None);
        assert_eq!(priced.energy, 3);
        assert_eq!(priced.power, [Need::Domain(Domain::Mind), Need::Rainbow]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_a_second_empower_and_a_missing_calm_rune_are_refused() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, HELM, EMPOWER_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        empower_it(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, HELM, EMPOWER_INDEX),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        drop(ctx);

        let mut broke = vault(fixtures::BASE);
        {
            let held = broke.table.card_mut(42).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HELM, EMPOWER_INDEX),
            Err(Refusal::NoPowerOf)
        );
        assert!(!ctx.is_empowered(HELM));
        assert_eq!(spell_surcharge(&ctx, &theirs(), HELM), SURCHARGE);
    }

    #[test]
    fn their_spark_costs_three_and_four_with_a_rainbow_once_the_helm_is_empowered() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let priced = cost::of_item(&ctx, &theirs(), None);
        assert_eq!(priced.energy, 3);
        assert_eq!(priced.power, [Need::Domain(Domain::Mind)]);
        empower_it(&mut ctx);
        let priced = cost::of_item(&ctx, &theirs(), None);
        assert_eq!(priced.energy, 3);
        assert_eq!(priced.power, [Need::Domain(Domain::Mind), Need::Rainbow]);
        assert_eq!(cost::of_item(&ctx, &mine(), None).energy, 2);
    }
}

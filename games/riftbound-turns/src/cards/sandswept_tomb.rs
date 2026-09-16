use super::prelude::{battlefield, with_statics, Location, RAINBOW};
use super::{Card, Cost, Item, Static};
use crate::engine::ctx::Ctx;
use crate::state::{ItemKind, TargetRef};

pub fn chooses_a_friendly_unit_here(ctx: &Ctx, item: &Item, tomb: u32) -> bool {
    if !matches!(item.kind, ItemKind::Spell { .. }) {
        return false;
    }
    let Some(zone) = ctx.card(tomb).and_then(|held| held.zone) else {
        return false;
    };
    if !ctx.zones.is_battlefield(zone) {
        return false;
    }
    item.targets.iter().any(|target| match target {
        TargetRef::Card(card) => {
            ctx.is_unit(*card)
                && ctx.controller(*card) == item.controller
                && ctx.location(*card) == Some(Location::Battlefield(zone))
        }
        _ => false,
    })
}

fn discount(ctx: &Ctx, item: &Item, tomb: u32) -> Cost {
    if chooses_a_friendly_unit_here(ctx, item, tomb) {
        RAINBOW
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    battlefield("Sandswept Tomb", &[], &[]),
    &[Static::PlayDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Domain;
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const TOMB: u32 = 90;
    const PUNCH: u32 = 91;
    const MY_UNIT_HERE: u32 = 92;
    const THEIR_UNIT_HERE: u32 = 93;
    const BODY_RUNE: u32 = 46;
    const SPARE_BODY_RUNE: u32 = 47;

    fn punch() -> CardInfo {
        CardInfo {
            energy: Some(1),
            power: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::spell(PUNCH, fixtures::HAND, 0, "Punch First", 1, 2)
        }
    }

    fn dig() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::GROUNDS);
        fixture.table.cards.push(fixtures::card(
            TOMB,
            fixtures::BF1,
            0,
            "Sandswept Tomb",
            "Battlefield",
        ));
        fixture.table.cards.push(punch());
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_UNIT_HERE, fixtures::BF1, 0, "Digger", 2));
        fixture.table.cards.push(fixtures::unit(
            THEIR_UNIT_HERE,
            fixtures::BF1,
            1,
            "Raider",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_BODY_RUNE, 0, "Body", false));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    fn punch_at(target: u32) -> ChainItem {
        let mut item = ChainItem::new(1, ItemKind::Spell { card: PUNCH }, 0, Origin::Hand);
        item.targets.push(TargetRef::Card(target));
        item
    }

    #[test]
    fn the_tomb_is_a_bare_battlefield_with_a_spell_discount() {
        let fixture = dig();
        assert!(std::ptr::eq(fixture.scripts.of_card(TOMB).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::PlayDiscount(discount)));
    }

    #[test]
    fn a_spell_aimed_at_a_friendly_unit_here_loses_one_body_need_and_one_aimed_at_an_enemy_here_pays_both(
    ) {
        let mut fixture = dig();
        let ctx = fixture.ctx();
        let friendly = cost::of_item(&ctx, &punch_at(MY_UNIT_HERE), None);
        assert_eq!(friendly.energy, 1);
        assert_eq!(
            friendly.power,
            [Need::Domain(Domain::Body)],
            "821.1.c · [A] less takes a printed domain need when no rainbow is left"
        );
        let enemy = cost::of_item(&ctx, &punch_at(THEIR_UNIT_HERE), None);
        assert_eq!(
            enemy.power,
            [Need::Domain(Domain::Body), Need::Domain(Domain::Body)]
        );
        let elsewhere = cost::of_item(&ctx, &punch_at(fixtures::VI), None);
        assert_eq!(elsewhere.power.len(), 2, "Vi is at the base, not here");
        let mut theirs = punch_at(THEIR_UNIT_HERE);
        theirs.controller = 1;
        assert_eq!(
            cost::of_item(&ctx, &theirs, None).power.len(),
            1,
            "friendly to the spell, not to the Tomb's holder"
        );
        assert_eq!(
            cost::total(&ctx, PUNCH, false).power.len(),
            2,
            "before a target is chosen the discount is not known"
        );
    }

    #[test]
    fn the_discount_is_read_at_payment_after_the_target_is_chosen() {
        let mut fixture = dig();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PUNCH).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_UNIT_HERE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", prompts::offered(&ctx));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "one Body rune is recycled for the one need left"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "one Body rune is exhausted for the energy and recycled for the power; the other stays"
        );
        assert_eq!(
            ctx.card(SPARE_BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL)
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(MY_UNIT_HERE), 7);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}

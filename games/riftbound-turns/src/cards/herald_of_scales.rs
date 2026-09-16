use super::prelude::{unit, with_statics};
use super::{base_name, Card, Cost, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind};

pub const REDUCTION: u8 = 2;
pub const MINIMUM: u8 = 1;

pub const DRAGONS: [&str; 19] = [
    "Blazing Scorcher",
    "Cloud Drake",
    "Corrupted Dragon",
    "Direwing",
    "Dune Drake",
    "Eager Drakehound",
    "Eclipse Dragon",
    "Elder Dragon",
    "Fae Dragon",
    "Gentle Gemdragon",
    "Harnessed Dragon",
    "Inviolus Vox",
    "Kadregrin the Infernal",
    "Mindsplitter",
    "Mountain Drake",
    "Ocean Drake",
    "Perched Grimwyrm",
    "Raging Firebrand",
    "Whiteflame Protector",
];

pub fn is_dragon(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| DRAGONS.contains(&base_name(&held.name)))
}

pub fn heralding(ctx: &Ctx, item: &ChainItem, herald: u32) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    ctx.card(herald)
        .is_some_and(|held| ctx.face_in_play(held) && !held.is_hidden())
        && ctx.controller(herald) == item.controller
        && ctx.is_unit(card)
        && is_dragon(ctx, card)
}

fn heralds_before(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| heralding(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

pub fn dragon_discount(ctx: &Ctx, item: &ChainItem, herald: u32) -> Cost {
    if !heralding(ctx, item, herald) {
        return Cost::FREE;
    }
    let base = cost::base_of_item(ctx, item).energy;
    let room = base
        .saturating_sub(MINIMUM)
        .saturating_sub(heralds_before(ctx, item, herald).saturating_mul(REDUCTION));
    Cost {
        energy: room.min(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Herald of Scales", &[], &[]),
    &[Static::PlayDiscount(dragon_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::Origin;
    use agni_plugin_sdk::table::CardInfo;

    const HERALD: u32 = 90;
    const SECOND: u32 = 91;
    const DRAKE: u32 = 92;
    const HATCHLING: u32 = 93;
    const THEIRS: u32 = 94;

    fn herald(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(id, zone, seat, "Herald of Scales", 3)
        }
    }

    fn roost() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herald(HERALD, fixtures::BASE, 0));
        fixture.table.cards.push(herald(THEIRS, fixtures::BASE, 1));
        fixture.table.cards.push(CardInfo {
            energy: Some(5),
            domain: vec!["Body".into()],
            ..fixtures::unit(DRAKE, fixtures::HAND, 0, "Dune Drake", 5)
        });
        fixture.table.cards.push(CardInfo {
            energy: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(
                HATCHLING,
                fixtures::HAND,
                0,
                "Blazing Scorcher (Starter)",
                5,
            )
        });
        fixture.resolve();
        fixture
    }

    fn play_of(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card }, seat, Origin::Hand)
    }

    #[test]
    fn the_herald_is_a_unit_whose_whole_text_is_a_discount_for_other_cards() {
        assert!(std::ptr::eq(script_of("Herald of Scales").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(dragon_discount)));
        assert_eq!((REDUCTION, MINIMUM), (2, 1));
        let mut sorted = DRAGONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(DRAGONS.to_vec(), sorted);
        for name in DRAGONS {
            assert!(
                script_of(name).is_some(),
                "{name} is a pool card with a script of its own"
            );
        }
    }

    #[test]
    fn dragons_are_read_by_base_name_since_the_table_carries_no_tags() {
        let mut fixture = roost();
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, DRAKE));
        assert!(
            is_dragon(&ctx, HATCHLING),
            "a print name resolves to its base"
        );
        assert!(!is_dragon(&ctx, HERALD));
        assert!(!is_dragon(&ctx, fixtures::VI));
        assert!(!is_dragon(&ctx, 999));
    }

    #[test]
    fn a_friendly_dragon_costs_two_less_down_to_one_and_a_second_herald_takes_what_room_is_left() {
        let mut fixture = roost();
        let ctx = fixture.ctx();
        assert!(heralding(&ctx, &play_of(DRAKE, 0), HERALD));
        assert_eq!(dragon_discount(&ctx, &play_of(DRAKE, 0), HERALD).energy, 2);
        assert_eq!(
            dragon_discount(&ctx, &play_of(HATCHLING, 0), HERALD).energy,
            1,
            "a two-cost Dragon stops at one"
        );
        assert_eq!(
            dragon_discount(&ctx, &play_of(HERALD, 0), HERALD).energy,
            0,
            "the Herald is no Dragon"
        );
        assert_eq!(
            dragon_discount(&ctx, &play_of(DRAKE, 0), THEIRS).energy,
            0,
            "your Dragons, not the opponent's Herald's"
        );
        assert!(!heralding(&ctx, &play_of(DRAKE, 1), HERALD));
        let spell = ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(dragon_discount(&ctx, &spell, HERALD), Cost::FREE);
        let mut two = roost();
        two.table.cards.push(herald(SECOND, fixtures::BF1, 0));
        two.resolve();
        let ctx = two.ctx();
        assert_eq!(dragon_discount(&ctx, &play_of(DRAKE, 0), HERALD).energy, 2);
        assert_eq!(
            dragon_discount(&ctx, &play_of(DRAKE, 0), SECOND).energy,
            2,
            "five less two less two is still above one"
        );
        assert_eq!(
            dragon_discount(&ctx, &play_of(HATCHLING, 0), SECOND).energy,
            0
        );
        let mut away = roost();
        away.table.card_mut(HERALD).unwrap().zone = Some(fixtures::HAND);
        away.resolve();
        let ctx = away.ctx();
        assert_eq!(
            dragon_discount(&ctx, &play_of(DRAKE, 0), HERALD),
            Cost::FREE,
            "a Herald in the hand discounts nothing"
        );
    }

    #[test]
    #[ignore = "rules question · the roost seats a live Herald for seat 1 too, so the Dune Drake seat 1 plays costs three under the Herald's own text and 356.4.a, not the printed five this test's third line expects; the test beside it prices the same fixture and pins five only once seat 1's Herald is in hand"]
    fn playing_a_dragon_with_the_herald_in_play_costs_two_energy_less() {
        let mut fixture = roost();
        let ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &play_of(DRAKE, 0), None).energy, 3);
        assert_eq!(cost::of_item(&ctx, &play_of(HATCHLING, 0), None).energy, 1);
        assert_eq!(cost::of_item(&ctx, &play_of(DRAKE, 1), None).energy, 5);
    }

    #[test]
    fn the_engine_prices_each_seats_dragons_by_its_own_herald_and_pays_the_discount() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &play_of(DRAKE, 0), None).energy, 3);
        assert_eq!(cost::of_item(&ctx, &play_of(HATCHLING, 0), None).energy, 1);
        assert_eq!(
            cost::of_item(&ctx, &play_of(DRAKE, 1), None).energy,
            3,
            "seat 1 has a Herald of its own"
        );
        assert_eq!(
            cost::total(&ctx, DRAKE, false).energy,
            3,
            "the pre-play gate agrees"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::play_from_hand(&mut ctx, 0, DRAKE).unwrap();
        if ctx.blob.prompt.is_some() {
            fixtures::choose(&mut ctx, 0, "your base").unwrap();
        }
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "a five-energy Drake plays on three ready runes"
        );
        assert_eq!(ctx.card(DRAKE).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut alone = roost();
        alone.table.card_mut(THEIRS).unwrap().zone = Some(fixtures::HAND);
        alone.resolve();
        let ctx = alone.ctx();
        assert_eq!(
            cost::of_item(&ctx, &play_of(DRAKE, 1), None).energy,
            5,
            "your Herald prices your Dragons, not the opponent's"
        );
    }
}

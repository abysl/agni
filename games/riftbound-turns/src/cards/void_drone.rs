use super::prelude::{unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;
use crate::state::Origin;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn play_origin(ctx: &Ctx, card: u32) -> Origin {
    if let Some(pending) = ctx
        .blob
        .queue
        .iter()
        .find(|pending| pending.item.kind.card() == Some(card))
    {
        return pending.item.origin;
    }
    let zone = ctx.card(card).and_then(|held| held.zone);
    if zone.is_none() || zone == ctx.zones.hand {
        Origin::Hand
    } else if zone == ctx.zones.champion {
        Origin::Champion
    } else if zone == ctx.zones.banishment {
        Origin::Banishment
    } else if zone == ctx.zones.trash {
        Origin::Trash {
            leave: crate::state::Leave::Banish,
        }
    } else {
        Origin::Board
    }
}

pub fn played_from_elsewhere_than_hand(ctx: &Ctx, card: u32) -> bool {
    play_origin(ctx, card) != Origin::Hand
}

fn discount(ctx: &Ctx, card: u32, _: u8) -> Cost {
    if played_from_elsewhere_than_hand(ctx, card) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit("Void Drone", &[], &[]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, play, settle};
    use crate::state::{ChainItem, ItemKind, Needs, Pending};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};

    const DRONE: u32 = 90;
    const PRINTED: u8 = 3;

    fn hive(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut drone = fixtures::unit(DRONE, zone, 0, "Void Drone", 3);
        drone.energy = Some(PRINTED);
        fixture.table.cards.push(drone);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DRONE).unwrap(), &CARD));
        fixture
    }

    fn item(origin: Origin) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: DRONE }, 0, origin)
    }

    fn entry(ctx: &Ctx, from: Option<u16>) -> EntryMove {
        EntryMove {
            card: DRONE,
            from,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_self_discount_of_two_energy() {
        assert!(std::ptr::eq(script_of("Void Drone").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!(DISCOUNT.energy, 2);
        assert!(DISCOUNT.power.is_empty());
    }

    #[test]
    fn from_hand_it_is_the_printed_three_and_from_the_champion_zone_it_is_one() {
        let mut fixture = hive(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(play_origin(&ctx, DRONE), Origin::Hand);
        assert!(!played_from_elsewhere_than_hand(&ctx, DRONE));
        assert_eq!(cost::total(&ctx, DRONE, false).energy, 3);
        assert_eq!(cost::of_item(&ctx, &item(Origin::Hand), None).energy, 3);
        drop(ctx);

        let mut fixture = hive(fixtures::CHAMPION);
        let ctx = fixture.ctx();
        assert_eq!(play_origin(&ctx, DRONE), Origin::Champion);
        assert!(played_from_elsewhere_than_hand(&ctx, DRONE));
        assert_eq!(
            cost::total(&ctx, DRONE, false).energy,
            1,
            "legal::classify prices the champion-zone play with the discount"
        );
        assert_eq!(cost::of_item(&ctx, &item(Origin::Champion), None).energy, 1);
        drop(ctx);

        let mut fixture = hive(fixtures::BANISHMENT);
        let ctx = fixture.ctx();
        assert_eq!(play_origin(&ctx, DRONE), Origin::Banishment);
        assert!(played_from_elsewhere_than_hand(&ctx, DRONE));
    }

    #[test]
    fn once_the_play_is_pending_the_items_origin_is_what_counts_not_the_chain_zone() {
        let mut fixture = hive(fixtures::CHAMPION);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.emit(Effect::Move {
            card: DRONE,
            zone: chain,
            seat: 0,
            index: TOP,
        });
        assert_eq!(
            play_origin(&ctx, DRONE),
            Origin::Board,
            "on the chain with no pending play the zone says nothing useful"
        );
        ctx.blob.queue.push(Pending {
            item: item(Origin::Champion),
            needs: Needs::Choices,
        });
        assert!(ctx.is_pending_play(DRONE));
        assert_eq!(play_origin(&ctx, DRONE), Origin::Champion);
        assert_eq!(cost::of_item(&ctx, &item(Origin::Champion), None).energy, 1);
        ctx.blob.queue.clear();
        ctx.blob.queue.push(Pending {
            item: item(Origin::Hand),
            needs: Needs::Choices,
        });
        assert_eq!(play_origin(&ctx, DRONE), Origin::Hand);
        assert_eq!(
            cost::of_item(&ctx, &item(Origin::Hand), None).energy,
            3,
            "a hand play on the chain pays the printed three"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_the_champion_zone_through_the_engine_it_takes_one_rune() {
        let mut fixture = hive(fixtures::CHAMPION);
        let mut ctx = fixture.ctx();
        let champion = ctx.zones.champion;
        assert!(legal::classify(&ctx, 0, &entry(&ctx, champion)).is_ok());
        let chain = ctx.zones.chain.unwrap();
        ctx.enter(&fixtures::move_action(DRONE, chain, 0), 0)
            .unwrap();
        play::begin(&mut ctx, 0, DRONE, Origin::Champion, None).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(DRONE));
        assert_eq!(ctx.location(DRONE), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one energy from the three ready runes"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_hand_on_two_ready_runes_it_is_refused_and_from_the_champion_zone_it_plays() {
        let mut fixture = hive(fixtures::HAND);
        fixture.table.card_mut(41).unwrap().exhausted = true;
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        let hand = ctx.zones.hand;
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, hand)),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        drop(ctx);

        let mut fixture = hive(fixtures::CHAMPION);
        fixture.table.card_mut(41).unwrap().exhausted = true;
        fixture.resolve();
        let ctx = fixture.ctx();
        let champion = ctx.zones.champion;
        assert!(legal::classify(&ctx, 0, &entry(&ctx, champion)).is_ok());
    }
}

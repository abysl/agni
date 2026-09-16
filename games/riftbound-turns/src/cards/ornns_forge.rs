use super::prelude::{battlefield, with_statics, Location};
use super::{Card, Cost, Static};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, statics};
use crate::state::{ChainItem, ItemKind};

pub const REDUCTION: u8 = 1;

pub fn controls_it(ctx: &Ctx, forge: u32, seat: u8) -> bool {
    statics::in_play(ctx, forge)
        && matches!(ctx.location(forge), Some(Location::Battlefield(zone)) if ctx.holds(seat, zone))
}

pub fn non_token_gear_played_this_turn(ctx: &Ctx, seat: u8) -> usize {
    usize::from(ctx.blob.seat(seat).gear_played)
}

pub fn forging(ctx: &Ctx, item: &ChainItem, forge: u32) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    ctx.is_gear(card)
        && !ctx.is_token(card)
        && controls_it(ctx, forge, item.controller)
        && non_token_gear_played_this_turn(ctx, item.controller) == 0
}

pub fn first_gear_discount(ctx: &Ctx, item: &ChainItem, forge: u32) -> Cost {
    if !forging(ctx, item, forge) {
        return Cost::FREE;
    }
    Cost {
        energy: cost::base_of_item(ctx, item).energy.min(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    battlefield("Ornn's Forge", &[], &[]),
    &[Static::PlayDiscount(first_gear_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FORGE: u32 = fixtures::GROUNDS;
    const BOOTS: u32 = fixtures::HAND_GEAR;
    const SWORD: u32 = 90;
    const FREEBIE: u32 = 91;
    const THEIR_SWORD: u32 = 92;

    fn forge_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(FORGE).unwrap().name = "Ornn's Forge".into();
        fixture
            .table
            .cards
            .push(fixtures::gear(SWORD, fixtures::HAND, 0, "Turret", 1));
        fixture.table.cards.push(CardInfo {
            energy: Some(0),
            ..fixtures::gear(FREEBIE, fixtures::HAND, 0, "Trinket", 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_SWORD, fixtures::HAND, 1, "Turret", 1));
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FORGE).unwrap(), &CARD));
        fixture
    }

    fn play_of(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card }, seat, Origin::Hand)
    }

    fn entry(ctx: &Ctx, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    fn play_gear(ctx: &mut Ctx, card: u32) {
        fixtures::play_from_hand(ctx, 0, card).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "a permanent's finalization is its resolution"
        );
        assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::BASE));
    }

    #[test]
    fn the_forge_is_a_battlefield_whose_whole_text_is_a_discount_for_another_card() {
        assert!(std::ptr::eq(script_of("Ornn's Forge").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(first_gear_discount)));
        assert!(CARD.replacement.is_none());
        assert_eq!(REDUCTION, 1);
    }

    #[test]
    fn control_is_read_off_the_holder_of_the_forges_zone() {
        let mut fixture = forge_held_by(Some(0));
        let ctx = fixture.ctx();
        assert!(controls_it(&ctx, FORGE, 0));
        assert!(!controls_it(&ctx, FORGE, 1));
        assert!(!controls_it(&ctx, fixtures::VI, 0), "not a battlefield");
        drop(ctx);
        let mut theirs = forge_held_by(Some(1));
        let ctx = theirs.ctx();
        assert!(!controls_it(&ctx, FORGE, 0));
        assert!(controls_it(&ctx, FORGE, 1));
        drop(ctx);
        let mut nobody = forge_held_by(None);
        let ctx = nobody.ctx();
        assert!(!controls_it(&ctx, FORGE, 0));
    }

    #[test]
    fn the_first_non_token_gear_of_the_holders_turn_is_one_energy_cheaper_and_the_second_is_not() {
        let mut fixture = forge_held_by(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(non_token_gear_played_this_turn(&ctx, 0), 0);
        assert!(forging(&ctx, &play_of(BOOTS, 0), FORGE));
        assert_eq!(
            first_gear_discount(&ctx, &play_of(BOOTS, 0), FORGE).energy,
            1
        );
        assert_eq!(
            first_gear_discount(&ctx, &play_of(SWORD, 0), FORGE).energy,
            1
        );
        assert_eq!(
            first_gear_discount(&ctx, &play_of(FREEBIE, 0), FORGE).energy,
            0,
            "a free gear has no energy to take"
        );
        assert!(
            !forging(&ctx, &play_of(fixtures::HAND_UNIT, 0), FORGE),
            "units are not gear"
        );
        assert!(
            !forging(&ctx, &play_of(THEIR_SWORD, 1), FORGE),
            "the opponent does not hold the Forge"
        );
        play_gear(&mut ctx, SWORD);
        assert_eq!(non_token_gear_played_this_turn(&ctx, 0), 1);
        assert!(!forging(&ctx, &play_of(BOOTS, 0), FORGE));
        assert_eq!(
            first_gear_discount(&ctx, &play_of(BOOTS, 0), FORGE).energy,
            0,
            "the second gear this turn pays in full"
        );
        assert_eq!(
            non_token_gear_played_this_turn(&ctx, 1),
            0,
            "the count is per seat"
        );
    }

    #[test]
    fn a_gold_token_is_neither_counted_nor_discounted_and_an_unheld_forge_discounts_nothing() {
        let mut fixture = forge_held_by(Some(0));
        let mut ctx = fixture.ctx();
        let gold = crate::cards::prelude::spawn_gold(&mut ctx, 0, true).unwrap();
        assert!(ctx.is_token(gold));
        assert_eq!(
            non_token_gear_played_this_turn(&ctx, 0),
            0,
            "a Gold played this turn is a token"
        );
        assert!(forging(&ctx, &play_of(BOOTS, 0), FORGE));
        assert!(!forging(&ctx, &play_of(gold, 0), FORGE));
        drop(ctx);
        let mut theirs = forge_held_by(Some(1));
        let ctx = theirs.ctx();
        assert!(!forging(&ctx, &play_of(BOOTS, 0), FORGE));
        assert_eq!(
            first_gear_discount(&ctx, &play_of(BOOTS, 0), FORGE).energy,
            0
        );
        assert!(forging(&ctx, &play_of(THEIR_SWORD, 1), FORGE));
    }

    #[test]
    fn unheld_the_boots_pay_their_full_two_energy_and_the_count_resets_with_the_turn() {
        let mut fixture = forge_held_by(Some(1));
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        play_gear(&mut ctx, BOOTS);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy, unheld");
        assert_eq!(non_token_gear_played_this_turn(&ctx, 0), 1);
        crate::engine::expiry::at_expiration(&mut ctx);
        assert_eq!(
            non_token_gear_played_this_turn(&ctx, 0),
            0,
            "the counter is per turn"
        );
        drop(ctx);
        let mut short = forge_held_by(Some(1));
        for rune in [42, 43] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = short.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, BOOTS)),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
    }

    #[test]
    fn one_ready_rune_plays_the_two_energy_boots_while_the_forge_is_held() {
        let mut fixture = forge_held_by(Some(0));
        for rune in [42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(cost::of_item(&ctx, &play_of(BOOTS, 0), None).energy, 1);
        legal::classify(&ctx, 0, &entry(&ctx, BOOTS)).unwrap();
        play_gear(&mut ctx, BOOTS);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert_eq!(
            cost::of_item(&ctx, &play_of(SWORD, 0), None).energy,
            1,
            "the second gear pays in full"
        );
    }
}

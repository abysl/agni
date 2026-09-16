use super::prelude::{unit, with_statics};
use super::{Card, Cost, Domain, Keyword, Power, Static};
use crate::engine::ctx::{Ctx, Event};

pub const ENERGY_PER_HOLD: u8 = 2;
pub const LADDER: usize = 6;

const CALM: Power = Power::Domain(Domain::Calm);

pub static CALM_PER_HOLD: [&[Power]; LADDER + 1] = [
    &[],
    &[CALM],
    &[CALM, CALM],
    &[CALM, CALM, CALM],
    &[CALM, CALM, CALM, CALM],
    &[CALM, CALM, CALM, CALM, CALM],
    &[CALM, CALM, CALM, CALM, CALM, CALM],
];

pub fn points_scored_from_holding_this_turn(ctx: &Ctx, seat: u8) -> usize {
    ctx.events
        .iter()
        .filter(|event| matches!(event, Event::Held { seat: holder, .. } if *holder == seat))
        .count()
}

pub fn hold_discount(ctx: &Ctx, seat: u8) -> Cost {
    let holds = points_scored_from_holding_this_turn(ctx, seat).min(LADDER);
    Cost {
        energy: u8::try_from(holds)
            .unwrap_or(u8::MAX)
            .saturating_mul(ENERGY_PER_HOLD),
        power: CALM_PER_HOLD[holds],
    }
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    hold_discount(ctx, seat)
}

pub static CARD: Card = with_statics(
    unit(
        "Needlessly Large Yordle",
        &[Keyword::Shield(5), Keyword::Tank],
        &[],
    ),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, legal};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const YORDLE: u32 = 90;
    const HOLDER: u32 = 91;
    const CALM_RUNE: u32 = 46;

    fn yordle() -> CardInfo {
        CardInfo {
            energy: Some(10),
            power: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(YORDLE, fixtures::HAND, 0, "Needlessly Large Yordle", 5)
        }
    }

    fn bandle(held: &[u16]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(yordle());
        for (offset, zone) in held.iter().enumerate() {
            fixture.table.cards.push(fixtures::unit(
                HOLDER + offset as u32,
                *zone,
                0,
                "Holder",
                2,
            ));
            fixture.blob.set_holder(*zone, Some(0));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(YORDLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn item() -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: YORDLE }, 0, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: YORDLE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_shield_five_tank_with_one_self_discount() {
        assert!(std::ptr::eq(
            script_of("Needlessly Large Yordle").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Shield(5), Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CALM_PER_HOLD
            .iter()
            .enumerate()
            .all(|(holds, power)| power.len() == holds && power.iter().all(|p| *p == CALM)));
    }

    #[test]
    fn each_hold_scored_this_turn_takes_two_energy_and_one_calm_off_the_printed_cost() {
        let mut fixture = bandle(&[fixtures::BF1, fixtures::BF2]);
        let mut ctx = fixture.ctx();
        assert_eq!(points_scored_from_holding_this_turn(&ctx, 0), 0);
        let printed = cost::of_item(&ctx, &item(), None);
        assert_eq!(printed.energy, 10);
        assert_eq!(printed.power, vec![Need::Domain(Domain::Calm); 3]);
        assert_eq!(
            cleanup::score_holds(&mut ctx, 0),
            [fixtures::BF1, fixtures::BF2]
        );
        assert_eq!(ctx.points(0), 2);
        assert_eq!(points_scored_from_holding_this_turn(&ctx, 0), 2);
        let discount = hold_discount(&ctx, 0);
        assert_eq!(discount.energy, 4);
        assert_eq!(discount.power, [CALM, CALM]);
        let priced = cost::of_item(&ctx, &item(), None);
        assert_eq!(priced.energy, 6);
        assert_eq!(
            priced.power,
            vec![Need::Domain(Domain::Calm); 1],
            "two of the three Calm are struck"
        );
        assert_eq!(cost::total(&ctx, YORDLE, false).energy, 6);
        assert_eq!(
            points_scored_from_holding_this_turn(&ctx, 1),
            0,
            "the opponent held nothing"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_this_turn_is_a_point_but_not_a_hold_and_the_ladder_caps_at_six() {
        let mut fixture = bandle(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        assert!(cleanup::conquer(&mut ctx, fixtures::BF2, 0));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(points_scored_from_holding_this_turn(&ctx, 0), 0);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 10);
        for _ in 0..8 {
            cleanup::hold(&mut ctx, fixtures::BF1, 0);
        }
        assert_eq!(points_scored_from_holding_this_turn(&ctx, 0), 8);
        assert_eq!(hold_discount(&ctx, 0).power.len(), LADDER);
        assert_eq!(hold_discount(&ctx, 0).energy, 12);
    }

    #[test]
    fn after_two_holds_the_yordle_is_six_energy_and_one_calm_and_plays_on_six_runes() {
        let mut fixture = bandle(&[fixtures::BF1, fixtures::BF2]);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE + 1, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 10,
                ready: 6
            }),
            "ten energy before the Beginning Phase scores"
        );
        assert_eq!(cleanup::score_holds(&mut ctx, 0).len(), 2);
        assert_eq!(cost::total(&ctx, YORDLE, false).energy, 6);
        assert!(legal::classify(&ctx, 0, &entry(&ctx)).is_ok());
        fixtures::play_from_hand(&mut ctx, 0, YORDLE).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })),
            "two held battlefields and the base to choose from"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.on_board(YORDLE));
        assert_eq!(
            ctx.location(YORDLE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "six energy from six runes");
        assert_eq!(ctx.runes_of(0).len(), 5, "one Calm rune recycled");
        assert_eq!(ctx.current_might(YORDLE), 5);
        assert!(ctx.mark_defender(YORDLE));
        assert_eq!(ctx.current_might(YORDLE), 10, "Shield 5 as a defender");
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: holds are scored in the Beginning Phase request and ctx.events is per request, the engine owes a per-seat holds_this_turn counter on SeatState reset at expiration"]
    fn a_hold_scored_in_an_earlier_request_still_discounts_the_yordle() {
        let mut fixture = bandle(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx();
        assert_eq!(ctx.points(0), 1);
        assert_eq!(points_scored_from_holding_this_turn(&ctx, 0), 1);
        assert_eq!(cost::of_item(&ctx, &item(), None).energy, 8);
    }
}

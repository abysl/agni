use super::prelude::{costing, empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Source, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 5,
    power: &[],
};
pub const DISCOUNT: u8 = 3;
pub const FEW_RUNES: usize = 4;
pub const DEFLECT: u8 = 1;
pub const ASSAULT: u8 = 2;

pub fn few_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() <= FEW_RUNES
}

fn price(ctx: &Ctx, source: Source) -> Cost {
    let seat = ctx.controller(source.card);
    if few_runes(ctx, seat) {
        Cost {
            energy: EMPOWER.energy.saturating_sub(DISCOUNT),
            power: EMPOWER.power,
        }
    } else {
        EMPOWER
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Baccai Sandspinner",
        &[Keyword::Empower(EMPOWER)],
        &[costing(empower(EMPOWER), price)],
    ),
    &[Static::While(
        is_empowered,
        &[
            Grant::Keyword(Keyword::Deflect(DEFLECT)),
            Grant::Keyword(Keyword::Assault(ASSAULT)),
        ],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::FLAG_ATTACKER;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SPINNER: u32 = 90;
    const FIFTH_RUNE: u32 = 46;

    fn spinner() -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Fury".into()],
            ..fixtures::unit(SPINNER, fixtures::BASE, 0, "Baccai Sandspinner", 6)
        }
    }

    fn dunes(fifth_rune: bool, ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spinner());
        if fifth_rune {
            fixture
                .table
                .cards
                .push(fixtures::rune(FIFTH_RUNE, 0, "Fury", false));
        }
        let mut count = 0;
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = count >= ready;
                count += 1;
            }
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_empower_five_priced_by_the_rune_count_and_grants_deflect_and_assault_two()
    {
        assert!(std::ptr::eq(
            script_of("Baccai Sandspinner").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 5);
        assert_eq!(DISCOUNT, 3);
        assert_eq!(FEW_RUNES, 4);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert!(
            empower.extra.is_some(),
            "the rider replaces the printed cost"
        );
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(!CARD.has_keyword(Keyword::Deflect(0)));
        assert!(!CARD.has_keyword(Keyword::Assault(0)));
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [
                    Grant::Keyword(Keyword::Deflect(1)),
                    Grant::Keyword(Keyword::Assault(2))
                ]
            )]
        ));
    }

    #[test]
    fn four_runes_price_the_empower_at_two_and_a_fifth_rune_restores_the_five() {
        let mut four = dunes(false, 4);
        let ctx = four.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(few_runes(&ctx, 0));
        assert_eq!(cost::of_activation(&ctx, SPINNER, 0).energy, 2);
        assert!(cost::of_activation(&ctx, SPINNER, 0).power.is_empty());
        drop(ctx);
        let mut five = dunes(true, 5);
        let ctx = five.ctx();
        assert_eq!(ctx.runes_of(0).len(), 5);
        assert!(!few_runes(&ctx, 0));
        assert_eq!(cost::of_activation(&ctx, SPINNER, 0).energy, 5);
        assert_eq!(ctx.runes_of(1).len(), 2);
        assert!(
            few_runes(&ctx, 1),
            "the count is each controller's own, not the table's"
        );
    }

    #[test]
    fn under_four_runes_two_energy_empowers_him_and_he_deflects_and_attacks_two_harder() {
        let mut fixture = dunes(false, 2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(SPINNER), 0);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == SPINNER)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {SPINNER}}}: empower (2 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, SPINNER, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "two energy from two runes");
        assert!(!ctx.card(SPINNER).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(SPINNER));
        assert!(matches!(
            statics::grants_on(&ctx, SPINNER).as_slice(),
            [
                Grant::Keyword(Keyword::Deflect(1)),
                Grant::Keyword(Keyword::Assault(2))
            ]
        ));
        assert_eq!(ctx.deflect_of(SPINNER), 1);
        assert_eq!(ctx.current_might(SPINNER), 6);
        ctx.set_flag(SPINNER, FLAG_ATTACKER, true);
        assert_eq!(ctx.current_might(SPINNER), 8, "Assault 2 as an attacker");
        assert_eq!(
            activate::activate(&mut ctx, 0, SPINNER, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_five_runes_four_ready_are_refused_and_five_ready_pay_the_full_price() {
        let mut fixture = dunes(true, 4);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SPINNER && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, SPINNER, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            })
        );
        assert!(!ctx.is_empowered(SPINNER));
        drop(ctx);
        let mut full = dunes(true, 5);
        let mut ctx = full.ctx();
        activate::activate(&mut ctx, 0, SPINNER, 0).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "five energy from five runes"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(SPINNER));
        assert_eq!(ctx.deflect_of(SPINNER), 1);
    }
}

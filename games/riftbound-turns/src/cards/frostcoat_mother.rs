use super::prelude::{costing, empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Source, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 12,
    power: &[],
};
pub const MIGHT: i16 = 3;

pub fn price_for(ctx: &Ctx, seat: u8) -> Cost {
    let runes = u8::try_from(ctx.runes_of(seat).len()).unwrap_or(u8::MAX);
    Cost {
        energy: EMPOWER.energy.saturating_sub(runes),
        power: EMPOWER.power,
    }
}

fn price(ctx: &Ctx, source: Source) -> Cost {
    price_for(ctx, ctx.controller(source.card))
}

pub static CARD: Card = with_statics(
    unit(
        "Frostcoat Mother",
        &[Keyword::Empower(EMPOWER)],
        &[costing(empower(EMPOWER), price)],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MOTHER: u32 = 90;
    const FIRST_EXTRA_RUNE: u32 = 100;

    fn mother() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(MOTHER, fixtures::BASE, 0, "Frostcoat Mother", 3)
        }
    }

    fn den(runes: usize, ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mother());
        for extra in 4..runes {
            let id = FIRST_EXTRA_RUNE + u32::try_from(extra).unwrap();
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Calm", false));
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
    fn the_script_prints_empower_twelve_priced_per_rune_and_grants_three_might_while_empowered() {
        assert!(std::ptr::eq(script_of("Frostcoat Mother").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 12);
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
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(3)])]
        ));
    }

    #[test]
    fn each_rune_you_control_takes_one_off_the_twelve_down_to_nothing() {
        let mut four = den(4, 4);
        let ctx = four.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(price_for(&ctx, 0).energy, 8);
        assert_eq!(cost::of_activation(&ctx, MOTHER, 0).energy, 8);
        assert_eq!(
            price_for(&ctx, 1).energy,
            10,
            "the count is the controller's own runes"
        );
        drop(ctx);
        let mut eight = den(8, 8);
        assert_eq!(cost::of_activation(&eight.ctx(), MOTHER, 0).energy, 4);
        let mut twelve = den(12, 0);
        let ctx = twelve.ctx();
        assert!(cost::of_activation(&ctx, MOTHER, 0).is_free());
        drop(ctx);
        let mut thirteen = den(13, 0);
        let ctx = thirteen.ctx();
        assert!(
            cost::of_activation(&ctx, MOTHER, 0).is_free(),
            "never below nothing"
        );
        assert!(
            cost::of_activation(&ctx, MOTHER, 0).power.is_empty(),
            "exhausted runes still count as controlled"
        );
    }

    #[test]
    fn with_eight_runes_four_energy_empowers_her_and_she_stands_at_six_might() {
        let mut fixture = den(8, 4);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(MOTHER), 3);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MOTHER)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MOTHER}}}: empower (4 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, MOTHER, 0).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy from four runes"
        );
        assert!(!ctx.card(MOTHER).unwrap().exhausted);
        assert_eq!(ctx.current_might(MOTHER), 3, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MOTHER));
        assert!(matches!(
            statics::grants_on(&ctx, MOTHER).as_slice(),
            [Grant::Might(3)]
        ));
        assert_eq!(ctx.current_might(MOTHER), 6);
        assert_eq!(
            activate::activate(&mut ctx, 0, MOTHER, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_eight_runes_three_ready_are_refused_and_twelve_runes_empower_her_for_nothing() {
        let mut fixture = den(8, 3);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == MOTHER && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, MOTHER, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
        assert!(!ctx.is_empowered(MOTHER));
        assert_eq!(ctx.current_might(MOTHER), 3);
        drop(ctx);
        let mut free = den(12, 0);
        let mut ctx = free.ctx();
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MOTHER)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].label, format!("{{card {MOTHER}}}: empower"));
        activate::activate(&mut ctx, 0, MOTHER, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MOTHER));
        assert_eq!(ctx.current_might(MOTHER), 6);
    }
}

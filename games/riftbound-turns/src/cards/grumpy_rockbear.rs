use super::prelude::{costing, empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Source, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 12,
    power: &[],
};
pub const DEFLECT: u8 = 1;
pub const SHIELD: u8 = 3;

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
        "Grumpy Rockbear",
        &[Keyword::Empower(EMPOWER)],
        &[costing(empower(EMPOWER), price)],
    ),
    &[Static::While(
        is_empowered,
        &[
            Grant::Keyword(Keyword::Deflect(DEFLECT)),
            Grant::Keyword(Keyword::Shield(SHIELD)),
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
    use crate::state::FLAG_DEFENDER;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ROCKBEAR: u32 = 90;
    const FIRST_EXTRA_RUNE: u32 = 100;

    fn rockbear() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Mind".into()],
            ..fixtures::unit(ROCKBEAR, fixtures::BASE, 0, "Grumpy Rockbear", 4)
        }
    }

    fn cave(runes: usize, ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rockbear());
        for extra in 4..runes {
            let id = FIRST_EXTRA_RUNE + u32::try_from(extra).unwrap();
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Mind", false));
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
    fn the_script_prints_empower_twelve_priced_per_rune_and_grants_deflect_and_shield_three() {
        assert!(std::ptr::eq(script_of("Grumpy Rockbear").unwrap(), &CARD));
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
        assert!(!CARD.has_keyword(Keyword::Deflect(0)));
        assert!(!CARD.has_keyword(Keyword::Shield(0)));
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [
                    Grant::Keyword(Keyword::Deflect(1)),
                    Grant::Keyword(Keyword::Shield(3))
                ]
            )]
        ));
    }

    #[test]
    fn each_rune_you_control_takes_one_off_the_twelve_down_to_nothing() {
        let mut four = cave(4, 4);
        let ctx = four.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(price_for(&ctx, 0).energy, 8);
        assert_eq!(cost::of_activation(&ctx, ROCKBEAR, 0).energy, 8);
        assert_eq!(
            price_for(&ctx, 1).energy,
            10,
            "the count is the controller's own runes"
        );
        drop(ctx);
        let mut six = cave(6, 6);
        assert_eq!(cost::of_activation(&six.ctx(), ROCKBEAR, 0).energy, 6);
        let mut twelve = cave(12, 0);
        assert!(cost::of_activation(&twelve.ctx(), ROCKBEAR, 0).is_free());
        let mut fourteen = cave(14, 0);
        assert!(
            cost::of_activation(&fourteen.ctx(), ROCKBEAR, 0).is_free(),
            "never below nothing"
        );
    }

    #[test]
    fn with_six_runes_six_energy_empowers_him_and_he_deflects_and_defends_three_harder() {
        let mut fixture = cave(6, 6);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(ROCKBEAR), 0);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == ROCKBEAR)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {ROCKBEAR}}}: empower (6 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, ROCKBEAR, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "six energy from six runes");
        assert!(!ctx.card(ROCKBEAR).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(ROCKBEAR));
        assert!(matches!(
            statics::grants_on(&ctx, ROCKBEAR).as_slice(),
            [
                Grant::Keyword(Keyword::Deflect(1)),
                Grant::Keyword(Keyword::Shield(3))
            ]
        ));
        assert_eq!(ctx.deflect_of(ROCKBEAR), 1);
        assert_eq!(ctx.current_might(ROCKBEAR), 4);
        ctx.set_flag(ROCKBEAR, FLAG_DEFENDER, true);
        assert_eq!(ctx.current_might(ROCKBEAR), 7, "Shield 3 as a defender");
        assert_eq!(
            activate::activate(&mut ctx, 0, ROCKBEAR, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_six_runes_five_ready_are_refused() {
        let mut fixture = cave(6, 5);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ROCKBEAR && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, ROCKBEAR, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 5
            })
        );
        assert!(!ctx.is_empowered(ROCKBEAR));
        assert_eq!(ctx.deflect_of(ROCKBEAR), 0);
    }
}

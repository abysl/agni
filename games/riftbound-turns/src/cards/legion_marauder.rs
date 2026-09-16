use super::prelude::{costing, empower, is_empowered, unit, with_statics, ONE_ENERGY};
use super::{Card, Cost, Domain, Grant, Keyword, Paying, Power, Source, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::pay;

pub const BODY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};
pub const ALTERNATIVES: [Cost; 2] = [ONE_ENERGY, BODY];
pub const EMPOWER: Cost = ALTERNATIVES[0];
pub const MIGHT: i16 = 1;

pub fn chosen_alternative_until_the_pay_stage_asks_energy_or_body(
    ctx: &Ctx,
    seat: u8,
    source: Source,
) -> Cost {
    let item = cost::activation_item(ctx, source.card, source.ability);
    let paying = Paying::Item(&item);
    let energy = pay::affordable_for(ctx, seat, &cost::of_script(&ONE_ENERGY, &[]), paying);
    let body = pay::affordable_for(ctx, seat, &cost::of_script(&BODY, &[]), paying);
    if body && !energy {
        BODY
    } else {
        ONE_ENERGY
    }
}

fn price(ctx: &Ctx, source: Source) -> Cost {
    chosen_alternative_until_the_pay_stage_asks_energy_or_body(
        ctx,
        ctx.controller(source.card),
        source,
    )
}

pub static CARD: Card = with_statics(
    unit(
        "Legion Marauder",
        &[Keyword::Empower(EMPOWER)],
        &[costing(empower(EMPOWER), price)],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::cost::Need;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MARAUDER: u32 = 90;
    const BODY_RUNE: u32 = 46;

    fn marauder() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(MARAUDER, fixtures::BASE, 0, "Legion Marauder", 2)
        }
    }

    fn camp(ready: usize, body_rune: Option<bool>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(marauder());
        for (index, id) in [41, 42, 43].into_iter().enumerate() {
            fixture.table.card_mut(id).unwrap().exhausted = index >= ready;
        }
        if let Some(exhausted) = body_rune {
            fixture
                .table
                .cards
                .push(fixtures::rune(BODY_RUNE, 0, "Body", exhausted));
        }
        fixture.resolve();
        fixture
    }

    fn empower_of(card: u32) -> Source {
        Source { card, ability: 0 }
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MARAUDER)
            .collect()
    }

    #[test]
    fn the_script_prints_the_energy_half_of_the_either_cost_and_grants_one_might_while_empowered() {
        assert!(std::ptr::eq(script_of("Legion Marauder").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(ONE_ENERGY));
        assert_eq!(ALTERNATIVES[0].energy, 1);
        assert!(ALTERNATIVES[0].power.is_empty());
        assert_eq!(ALTERNATIVES[1].energy, 0);
        assert_eq!(ALTERNATIVES[1].power, [Power::Domain(Domain::Body)]);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(ONE_ENERGY));
        assert!(
            empower.extra.is_some(),
            "the pick replaces the printed half"
        );
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(1)])]
        ));
    }

    #[test]
    fn with_a_ready_rune_the_energy_is_taken_and_he_stands_at_three_might() {
        let mut fixture = camp(1, Some(false));
        let mut ctx = fixture.ctx();
        assert_eq!(
            chosen_alternative_until_the_pay_stage_asks_energy_or_body(
                &ctx,
                0,
                empower_of(MARAUDER)
            ),
            ONE_ENERGY
        );
        assert_eq!(cost::of_activation(&ctx, MARAUDER, 0).energy, 1);
        assert!(cost::of_activation(&ctx, MARAUDER, 0).power.is_empty());
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MARAUDER}}}: empower (1 energy)")
        );
        assert!(offers[0].enabled);
        let runes = ctx.runes_of(0).len();
        activate::activate(&mut ctx, 0, MARAUDER, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one rune is exhausted");
        assert_eq!(ctx.runes_of(0).len(), runes, "none is recycled");
        assert!(!ctx.card(MARAUDER).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MARAUDER));
        assert!(matches!(
            statics::grants_on(&ctx, MARAUDER).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(MARAUDER), 3);
        assert_eq!(
            activate::activate(&mut ctx, 0, MARAUDER, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_no_ready_rune_a_spent_body_rune_is_recycled_instead() {
        let mut fixture = camp(0, Some(true));
        let mut ctx = fixture.ctx();
        assert!(ctx.ready_runes_of(0).is_empty());
        assert_eq!(
            chosen_alternative_until_the_pay_stage_asks_energy_or_body(
                &ctx,
                0,
                empower_of(MARAUDER)
            ),
            BODY
        );
        let priced = cost::of_activation(&ctx, MARAUDER, 0);
        assert_eq!(priced.energy, 0);
        assert_eq!(priced.power, [Need::Domain(Domain::Body)]);
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MARAUDER}}}: empower (1 Body power)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, MARAUDER, 0).unwrap();
        assert_ne!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "the Body rune is recycled for its power"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MARAUDER));
        assert_eq!(ctx.current_might(MARAUDER), 3);
    }

    #[test]
    fn with_neither_a_ready_rune_nor_a_body_rune_the_empower_is_refused() {
        let mut fixture = camp(0, None);
        let mut ctx = fixture.ctx();
        assert_eq!(
            chosen_alternative_until_the_pay_stage_asks_energy_or_body(
                &ctx,
                0,
                empower_of(MARAUDER)
            ),
            ONE_ENERGY,
            "the refusal names the energy half"
        );
        assert!(his_offers(&ctx).iter().all(|offer| !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, MARAUDER, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert!(!ctx.is_empowered(MARAUDER));
        assert_eq!(ctx.current_might(MARAUDER), 2);
    }

    #[test]
    #[ignore = "engine gap · an either-or Empower cost is the player's pick at the pay stage (the Irelia - Graceful REDUCTIONS row): ExtraCost returns one Cost, so chosen_alternative_until_the_pay_stage_asks_energy_or_body decides for them and a ready Body rune can never be recycled while another rune could be exhausted"]
    fn the_player_may_recycle_a_ready_body_rune_instead_of_exhausting_one() {
        let mut fixture = camp(1, Some(false));
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MARAUDER, 0).unwrap();
        assert!(
            ctx.blob.prompt.is_some(),
            "which half is paid: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "1 Body power").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "no rune is exhausted");
        assert_ne!(ctx.card(BODY_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MARAUDER));
    }
}

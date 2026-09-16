use super::prelude::{empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Static};

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Fury)],
};
pub const ASSAULT: u8 = 3;

pub static CARD: Card = with_statics(
    unit(
        "Shadow Fiend",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::While(
        is_empowered,
        &[Grant::Keyword(Keyword::Assault(ASSAULT))],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::Ctx;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::FLAG_ATTACKER;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FIEND: u32 = 90;

    fn fiend() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(FIEND, fixtures::BASE, 0, "Shadow Fiend", 2)
        }
    }

    fn shrine(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fiend());
        for (index, id) in [41, 42, 43].into_iter().enumerate() {
            fixture.table.card_mut(id).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_empower_and_grants_assault_three_only_while_empowered() {
        assert!(std::ptr::eq(script_of("Shadow Fiend").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 2);
        assert_eq!(EMPOWER.power, [Power::Domain(Domain::Fury)]);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        assert!(
            !CARD.has_keyword(Keyword::Assault(0)),
            "Assault is the grant, not printed"
        );
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Keyword(Keyword::Assault(3))])]
        ));
    }

    #[test]
    fn empowering_him_pays_two_and_a_fury_and_he_hits_three_harder_as_an_attacker() {
        let mut fixture = shrine(3);
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(FIEND, Keyword::Assault(0)));
        ctx.set_flag(FIEND, FLAG_ATTACKER, true);
        assert_eq!(ctx.current_might(FIEND), 2, "no Assault before Empower");
        ctx.set_flag(FIEND, FLAG_ATTACKER, false);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == FIEND)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {FIEND}}}: empower (2 energy and 1 Fury power)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, FIEND, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.runes_of(0).len() < 4,
            "a Fury rune is recycled for the power"
        );
        assert!(
            !ctx.card(FIEND).unwrap().exhausted,
            "827.1 · Empower never exhausts him"
        );
        assert!(!ctx.is_empowered(FIEND), "nothing until it resolves");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(FIEND));
        assert!(matches!(
            statics::grants_on(&ctx, FIEND).as_slice(),
            [Grant::Keyword(Keyword::Assault(3))]
        ));
        assert!(ctx.has_keyword(FIEND, Keyword::Assault(0)));
        assert_eq!(ctx.current_might(FIEND), 2, "Assault waits for an attack");
        ctx.set_flag(FIEND, FLAG_ATTACKER, true);
        assert_eq!(ctx.current_might(FIEND), 5);
        assert_eq!(
            activate::activate(&mut ctx, 0, FIEND, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn one_ready_rune_greys_the_offer_and_refuses_the_activation() {
        let mut fixture = shrine(1);
        let mut ctx = fixture.ctx();
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == FIEND)
            .collect();
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, FIEND, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        assert!(!ctx.is_empowered(FIEND));
        assert!(!ctx.has_keyword(FIEND, Keyword::Assault(0)));
    }
}

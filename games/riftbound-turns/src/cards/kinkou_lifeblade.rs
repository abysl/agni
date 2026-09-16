use super::prelude::{empower, is_empowered, unit, with_statics, Location};
use super::{Card, Cost, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const MIGHT: i16 = 1;

pub fn may_gank(ctx: &Ctx, unit: u32, from: Location, to: Location) -> bool {
    crate::engine::march::legal_destination(ctx, unit, from, to).is_ok()
}

pub static CARD: Card = with_statics(
    unit(
        "Kinkou Lifeblade",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::While(
        is_empowered,
        &[Grant::Might(MIGHT), Grant::Keyword(Keyword::Ganking)],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const LIFEBLADE: u32 = 90;

    fn lifeblade() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(LIFEBLADE, fixtures::BF1, 0, "Kinkou Lifeblade", 4)
        }
    }

    fn temple(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lifeblade());
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        for (index, id) in [41, 42, 43].into_iter().enumerate() {
            fixture.table.card_mut(id).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    fn across(ctx: &Ctx) -> bool {
        may_gank(
            ctx,
            LIFEBLADE,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_prints_empower_and_grants_one_might_and_ganking_while_empowered() {
        assert!(std::ptr::eq(script_of("Kinkou Lifeblade").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 2);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [Grant::Might(1), Grant::Keyword(Keyword::Ganking)]
            )]
        ));
    }

    #[test]
    fn empowering_her_pays_two_and_she_is_five_might_and_ganks() {
        let mut fixture = temple(2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(LIFEBLADE), 4);
        assert!(!across(&ctx));
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == LIFEBLADE)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {LIFEBLADE}}}: empower (2 energy)")
        );
        activate::activate(&mut ctx, 0, LIFEBLADE, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(!ctx.card(LIFEBLADE).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(LIFEBLADE));
        assert!(matches!(
            statics::grants_on(&ctx, LIFEBLADE).as_slice(),
            [Grant::Might(1), Grant::Keyword(Keyword::Ganking)]
        ));
        assert_eq!(ctx.current_might(LIFEBLADE), 5);
        assert!(ctx.has_keyword(LIFEBLADE, Keyword::Ganking));
        assert!(across(&ctx));
        assert_eq!(
            activate::activate(&mut ctx, 0, LIFEBLADE, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn one_ready_rune_refuses_the_empower() {
        let mut fixture = temple(1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, LIFEBLADE, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        assert!(!ctx.is_empowered(LIFEBLADE));
        assert_eq!(ctx.current_might(LIFEBLADE), 4);
        assert!(!across(&ctx));
    }
}

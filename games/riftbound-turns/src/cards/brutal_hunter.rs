use super::prelude::{empower, is_empowered, unit, with_statics, Location};
use super::{Card, Cost, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const MIGHT: i16 = 2;

pub fn may_gank(ctx: &Ctx, unit: u32, from: Location, to: Location) -> bool {
    crate::engine::march::legal_destination(ctx, unit, from, to).is_ok()
}

pub static CARD: Card = with_statics(
    unit(
        "Brutal Hunter",
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

    const HUNTER: u32 = 90;

    fn hunter() -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(HUNTER, fixtures::BF1, 0, "Brutal Hunter", 4)
        }
    }

    fn hunting_ground(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hunter());
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
            HUNTER,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_prints_empower_and_grants_two_might_and_ganking_while_empowered() {
        assert!(std::ptr::eq(script_of("Brutal Hunter").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 3);
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
                [Grant::Might(2), Grant::Keyword(Keyword::Ganking)]
            )]
        ));
    }

    #[test]
    fn empowering_him_pays_three_and_he_is_six_might_and_walks_battlefield_to_battlefield() {
        let mut fixture = hunting_ground(3);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(HUNTER), 4);
        assert!(
            !across(&ctx),
            "736 · no Ganking, no battlefield-to-battlefield move"
        );
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == HUNTER)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {HUNTER}}}: empower (3 energy)")
        );
        activate::activate(&mut ctx, 0, HUNTER, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(!ctx.card(HUNTER).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(HUNTER));
        assert!(matches!(
            statics::grants_on(&ctx, HUNTER).as_slice(),
            [Grant::Might(2), Grant::Keyword(Keyword::Ganking)]
        ));
        assert_eq!(ctx.current_might(HUNTER), 6);
        assert!(ctx.has_keyword(HUNTER, Keyword::Ganking));
        assert!(across(&ctx));
        assert_eq!(
            activate::activate(&mut ctx, 0, HUNTER, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn two_ready_runes_refuse_the_empower_and_he_stays_home() {
        let mut fixture = hunting_ground(2);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HUNTER, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        assert!(!ctx.is_empowered(HUNTER));
        assert_eq!(ctx.current_might(HUNTER), 4);
        assert!(!across(&ctx));
    }
}

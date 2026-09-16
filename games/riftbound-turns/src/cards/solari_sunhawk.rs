use super::prelude::{empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Static};

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const MIGHT: i16 = 1;
pub const DEFLECT: u8 = 2;

pub static CARD: Card = with_statics(
    unit(
        "Solari Sunhawk",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::While(
        is_empowered,
        &[
            Grant::Might(MIGHT),
            Grant::Keyword(Keyword::Deflect(DEFLECT)),
        ],
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

    const SUNHAWK: u32 = 90;

    fn sunhawk() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(SUNHAWK, fixtures::BASE, 0, "Solari Sunhawk", 3)
        }
    }

    fn eyrie(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sunhawk());
        for (index, id) in [41, 42, 43].into_iter().enumerate() {
            fixture.table.card_mut(id).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_empower_and_grants_one_might_and_deflect_two_while_empowered() {
        assert!(std::ptr::eq(script_of("Solari Sunhawk").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 2);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(!CARD.has_keyword(Keyword::Deflect(0)));
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [Grant::Might(1), Grant::Keyword(Keyword::Deflect(2))]
            )]
        ));
    }

    #[test]
    fn empowering_it_pays_two_and_it_is_four_might_behind_two_rainbow_of_deflect() {
        let mut fixture = eyrie(2);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(SUNHAWK), 3);
        assert_eq!(ctx.deflect_of(SUNHAWK), 0);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == SUNHAWK)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {SUNHAWK}}}: empower (2 energy)")
        );
        activate::activate(&mut ctx, 0, SUNHAWK, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(!ctx.card(SUNHAWK).unwrap().exhausted);
        assert_eq!(ctx.deflect_of(SUNHAWK), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(SUNHAWK));
        assert!(matches!(
            statics::grants_on(&ctx, SUNHAWK).as_slice(),
            [Grant::Might(1), Grant::Keyword(Keyword::Deflect(2))]
        ));
        assert_eq!(ctx.current_might(SUNHAWK), 4);
        assert_eq!(ctx.deflect_of(SUNHAWK), 2);
        assert!(ctx.has_keyword(SUNHAWK, Keyword::Deflect(0)));
        assert_eq!(
            activate::activate(&mut ctx, 0, SUNHAWK, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn one_ready_rune_refuses_the_empower() {
        let mut fixture = eyrie(1);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == SUNHAWK && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, SUNHAWK, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        assert!(!ctx.is_empowered(SUNHAWK));
        assert_eq!(ctx.deflect_of(SUNHAWK), 0);
        assert_eq!(ctx.current_might(SUNHAWK), 3);
    }
}

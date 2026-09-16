use super::prelude::{empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Static};

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const DEFLECT: u8 = 1;
pub const SHIELD: u8 = 3;

pub static CARD: Card = with_statics(
    unit(
        "Serene Ascetic",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
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
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::FLAG_DEFENDER;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ASCETIC: u32 = 90;

    fn ascetic() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(ASCETIC, fixtures::BASE, 0, "Serene Ascetic", 3)
        }
    }

    fn monastery(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ascetic());
        for (index, id) in [41, 42, 43].into_iter().enumerate() {
            fixture.table.card_mut(id).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_empower_and_grants_deflect_and_shield_three_while_empowered() {
        assert!(std::ptr::eq(script_of("Serene Ascetic").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 3);
        assert!(EMPOWER.power.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
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
    fn empowering_him_pays_three_and_he_deflects_and_defends_three_harder() {
        let mut fixture = monastery(3);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(ASCETIC), 0);
        ctx.set_flag(ASCETIC, FLAG_DEFENDER, true);
        assert_eq!(ctx.current_might(ASCETIC), 3, "no Shield before Empower");
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == ASCETIC)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {ASCETIC}}}: empower (3 energy)")
        );
        activate::activate(&mut ctx, 0, ASCETIC, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(!ctx.card(ASCETIC).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(ASCETIC));
        assert!(matches!(
            statics::grants_on(&ctx, ASCETIC).as_slice(),
            [
                Grant::Keyword(Keyword::Deflect(1)),
                Grant::Keyword(Keyword::Shield(3))
            ]
        ));
        assert_eq!(ctx.deflect_of(ASCETIC), 1);
        assert!(ctx.has_keyword(ASCETIC, Keyword::Deflect(0)));
        assert_eq!(ctx.current_might(ASCETIC), 6, "Shield 3 as a defender");
        ctx.set_flag(ASCETIC, FLAG_DEFENDER, false);
        assert_eq!(ctx.current_might(ASCETIC), 3, "Shield only while defending");
        assert_eq!(
            activate::activate(&mut ctx, 0, ASCETIC, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn two_ready_runes_refuse_the_empower() {
        let mut fixture = monastery(2);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ASCETIC && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, ASCETIC, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        assert!(!ctx.is_empowered(ASCETIC));
        assert_eq!(ctx.deflect_of(ASCETIC), 0);
    }
}

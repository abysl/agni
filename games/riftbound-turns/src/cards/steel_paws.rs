use super::prelude::{empower, is_empowered, unit, with_statics};
use super::{Card, Cost, Grant, Keyword, Static};

pub const DEFLECT: u8 = 1;
pub const EMPOWER: Cost = Cost {
    energy: 7,
    power: &[],
};
pub const MIGHT: i16 = 7;

pub static CARD: Card = with_statics(
    unit(
        "Steel Paws",
        &[Keyword::Deflect(DEFLECT), Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
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

    const PAWS: u32 = 90;
    const EXTRA_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn paws() -> CardInfo {
        CardInfo {
            energy: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(PAWS, fixtures::BASE, 0, "Steel Paws", 0)
        }
    }

    fn workshop(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(paws());
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
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
    fn the_script_prints_deflect_and_empower_and_grants_seven_might_while_empowered() {
        assert!(std::ptr::eq(script_of("Steel Paws").unwrap(), &CARD));
        assert_eq!(
            CARD.keywords,
            [Keyword::Deflect(DEFLECT), Keyword::Empower(EMPOWER)]
        );
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 7);
        assert!(EMPOWER.power.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(7)])]
        ));
    }

    #[test]
    fn empowering_the_cat_pays_seven_and_it_stands_at_seven_might_deflect_printed_throughout() {
        let mut fixture = workshop(7);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(PAWS), 0);
        assert_eq!(ctx.deflect_of(PAWS), 1, "Deflect is printed");
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == PAWS)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {PAWS}}}: empower (7 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, PAWS, 0).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "seven energy from seven runes"
        );
        assert!(!ctx.card(PAWS).unwrap().exhausted);
        assert_eq!(ctx.current_might(PAWS), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(PAWS));
        assert!(matches!(
            statics::grants_on(&ctx, PAWS).as_slice(),
            [Grant::Might(7)]
        ));
        assert_eq!(ctx.current_might(PAWS), 7);
        assert_eq!(ctx.deflect_of(PAWS), 1, "Empower adds no Deflect");
        assert_eq!(
            activate::activate(&mut ctx, 0, PAWS, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn six_ready_runes_refuse_the_empower() {
        let mut fixture = workshop(6);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == PAWS && !offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 0, PAWS, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 7,
                ready: 6
            })
        );
        assert!(!ctx.is_empowered(PAWS));
        assert_eq!(ctx.current_might(PAWS), 0);
    }

    #[test]
    fn resolved_empower_publishes_seven_might_across_requests_without_double_counting() {
        use crate::engine::ctx::COUNTER_MIGHT;
        use crate::state::GameBlob;
        use crate::TurnEvent;
        use agni_plugin_sdk::decide::{Action, Request};
        use agni_plugin_sdk::table::Target;

        let mut fixture = workshop(7);
        for (seat, event) in [
            (
                0,
                TurnEvent::Activate {
                    source: PAWS,
                    ability: 0,
                },
            ),
            (0, TurnEvent::Pass),
            (1, TurnEvent::Pass),
        ] {
            let request = Request {
                plugin_state: fixture.blob.encode(),
                players: 2,
                seat,
                action: Action::Game(event.encode()),
                table: fixture.table.clone(),
            };
            let verdict = crate::engine::decide(&request, fixture.blob.clone()).unwrap();
            for effect in verdict.effects {
                fixture.table.apply(&effect, seat).unwrap();
            }
            fixture.blob = GameBlob::decode(&verdict.plugin_state.unwrap()).unwrap();
            fixture.resolve();
        }
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(PAWS));
        assert_eq!(ctx.current_might(PAWS), 7);
        assert_eq!(
            ctx.table.counter(Target::Card(PAWS), COUNTER_MIGHT),
            Some(7)
        );
        ctx.sync_might();
        assert_eq!(ctx.current_might(PAWS), 7);
        assert_eq!(
            ctx.table.counter(Target::Card(PAWS), COUNTER_MIGHT),
            Some(7)
        );
        ctx.disempower(PAWS);
        ctx.sync_might();
        assert_eq!(ctx.current_might(PAWS), 0);
        assert_eq!(
            ctx.table
                .counter(Target::Card(PAWS), COUNTER_MIGHT)
                .unwrap_or(0),
            0
        );
    }
}

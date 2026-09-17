use super::petricite_monument::every_friendly_unit;
use super::prelude::{empower, gear, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Scope, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 6,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const EMPOWERED_EXTRA: i16 = 1;

pub fn amplifier_is_empowered(ctx: &Ctx, _unit: u32, amplifier: u32) -> bool {
    ctx.is_empowered(amplifier)
}

pub static AURA: Static = Static::Aura {
    scope: Scope::FriendlyUnits,
    when: every_friendly_unit,
    grants: &[
        Grant::Might(MIGHT_BONUS),
        Grant::MightIf(amplifier_is_empowered, EMPOWERED_EXTRA),
    ],
};

pub static CARD: Card = with_statics(
    gear(
        "Rage Amplifier",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[AURA],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, statics};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const AMPLIFIER: u32 = 90;
    const ALLY: u32 = 95;
    const EMPOWER_INDEX: u8 = 0;

    fn amplifier(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::gear(AMPLIFIER, zone, seat, "Rage Amplifier", 4)
        }
    }

    fn arena(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(amplifier(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Wailer", 2));
        for id in [46, 47, 48, 49] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(AMPLIFIER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn empower_it(ctx: &mut Ctx) {
        activate::activate(ctx, 0, AMPLIFIER, EMPOWER_INDEX).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(AMPLIFIER));
    }

    #[test]
    fn the_script_prints_empower_for_six_and_a_fury_and_carries_one_aura_over_friendly_units() {
        assert!(std::ptr::eq(script_of("Rage Amplifier").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 6);
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_INDEX)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        assert!(CARD.has_aura());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(1), Grant::MightIf(_, 1)],
                ..
            }
        ));
        assert_eq!(MIGHT_BONUS + EMPOWERED_EXTRA, 2);
    }

    #[test]
    fn your_units_read_plus_one_wherever_they_stand_and_the_enemys_read_nothing() {
        let mut fixture = arena(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 4, "in the base");
        assert_eq!(ctx.current_might(ALLY), 3, "at a battlefield");
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert!(!amplifier_is_empowered(&ctx, fixtures::VI, AMPLIFIER));
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF2),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the aura is not location-bound"
        );
        assert!(ctx.set_controller(ALLY, 1, fixtures::SPRITE));
        assert_eq!(
            ctx.current_might(ALLY),
            2,
            "a unit the enemy took is no longer yours"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empowering_it_pays_six_and_a_fury_and_lifts_the_aura_to_plus_two() {
        let mut fixture = arena(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == AMPLIFIER)
            .expect("the amplifier offers its Empower");
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {AMPLIFIER}}}: empower (6 energy and 1 Fury power)")
        );
        let ready = ctx.ready_runes_of(0).len();
        empower_it(&mut ctx);
        assert!(ctx.ready_runes_of(0).len() <= ready - 6);
        assert!(amplifier_is_empowered(&ctx, fixtures::VI, AMPLIFIER));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.current_might(ALLY), 4);
        assert!(matches!(
            statics::grants_on(&ctx, ALLY).as_slice(),
            [Grant::Might(1), Grant::MightIf(_, 1)]
        ));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx.disempower(AMPLIFIER));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "+1 again once disempowered"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_reads_only_from_the_board_and_leaves_with_the_amplifier() {
        let mut fixture = arena(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "in hand it grants nothing"
        );
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        drop(ctx);
        let mut fixture = arena(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.kill(AMPLIFIER, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.location(AMPLIFIER), None);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(ALLY), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_empowered_amplifier_and_a_missing_fury_rune_are_refused() {
        let mut fixture = arena(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, AMPLIFIER, EMPOWER_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        empower_it(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, AMPLIFIER, EMPOWER_INDEX),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        drop(ctx);

        let mut broke = arena(fixtures::BASE);
        for id in [40, 41, 43, 46, 47, 48, 49] {
            let held = broke.table.card_mut(id).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, AMPLIFIER, EMPOWER_INDEX),
            Err(Refusal::NoPowerOf)
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4, "still +1 unempowered");
    }
}

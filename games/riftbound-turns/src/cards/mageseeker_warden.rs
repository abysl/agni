use super::prelude::{at_battlefield, never_suppresses, unit, with_statics};
use super::{Card, Grant, Static};
use crate::engine::ctx::Ctx;

fn enemy_unit_or_gear(ctx: &Ctx, warden: u32, card: u32) -> bool {
    ctx.at_battlefield(warden)
        && (ctx.is_unit(card) || ctx.is_gear(card))
        && ctx.controller(card) != ctx.controller(warden)
}

pub static CARD: Card = with_statics(
    unit("Mageseeker Warden", &[], &[]),
    &[
        Static::ReadySuppressed {
            by_effects: enemy_unit_or_gear,
            by_awaken: never_suppresses,
        },
        Static::While(
            at_battlefield,
            &[Grant::Static(Static::OpponentsPlayUnitsOnlyToBase)],
        ),
    ],
);

pub fn units_only_to_base(ctx: &Ctx, seat: u8) -> bool {
    ctx.units_only_to_base(seat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::state::{GameBlob, Mode, Origin, Phase};
    use agni_plugin_sdk::decide::TOP;

    const WARDEN: u32 = 90;
    const THEIR_UNIT_IN_HAND: u32 = 91;
    const THEIR_GEAR: u32 = 92;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(WARDEN, zone, 0, "Mageseeker Warden", 5));
        let mut jinx = fixtures::unit(THEIR_UNIT_IN_HAND, fixtures::HAND, 1, "Jinx", 2);
        jinx.energy = Some(1);
        jinx.domain = vec!["Mind".into()];
        fixture.table.cards.push(jinx);
        let mut gear = fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Plain Gear", 1);
        gear.exhausted = true;
        fixture.table.cards.push(gear);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Plain Field".into();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn their_turn(fixture: &mut Fixture) {
        fixture.blob = GameBlob::start(2, 1, Mode::Enforced);
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: THEIR_UNIT_IN_HAND,
            from: ctx.zones.hand,
            from_seat: 1,
            to: Some(to),
            to_seat: 1,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn both_locks_hold_only_while_he_stands_at_a_battlefield_and_only_against_his_enemies() {
        assert!(std::ptr::eq(script_of("Mageseeker Warden").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        assert!(CARD.mentions_static(Static::OpponentsPlayUnitsOnlyToBase));
        let mut afield = armed(fixtures::BF1);
        let ctx = afield.ctx();
        assert!(units_only_to_base(&ctx, 1));
        assert!(
            !units_only_to_base(&ctx, 0),
            "his own side plays where it likes"
        );
        assert!(ctx.ready_suppressed(fixtures::THEIR_UNIT));
        assert!(ctx.ready_suppressed(THEIR_GEAR));
        assert!(
            !ctx.ready_suppressed(fixtures::VI),
            "friendly units ready as usual"
        );
        assert!(
            !ctx.ready_suppressed(44),
            "runes are neither units nor gear"
        );
        let mut home = armed(fixtures::BASE);
        let ctx = home.ctx();
        assert!(!units_only_to_base(&ctx, 1));
        assert!(!ctx.ready_suppressed(fixtures::THEIR_UNIT));
        assert!(!ctx.ready_suppressed(THEIR_GEAR));
    }

    #[test]
    fn today_the_opponent_must_play_units_to_their_base() {
        let mut fixture = armed(fixtures::BF1);
        their_turn(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, fixtures::BF2)),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::UnitsOnlyToBase
            ))
        );
    }

    #[test]
    fn while_he_is_at_a_battlefield_opponents_play_units_only_to_their_base() {
        let mut fixture = armed(fixtures::BF1);
        their_turn(&mut fixture);
        let ctx = fixture.ctx();
        assert!(legal::classify(&ctx, 1, &drag(&ctx, fixtures::BF2)).is_err());
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: THEIR_UNIT_IN_HAND,
                origin: Origin::Hand,
                location: Some(Location::Base(1)),
                on_chain: false,
            })
        );
        assert_eq!(ctx.play_locations(1), [Location::Base(1)]);
    }

    #[test]
    fn while_he_is_at_a_battlefield_spells_and_abilities_cannot_ready_enemy_units_and_gear() {
        let mut fixture = armed(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(!ctx.ready(fixtures::THEIR_UNIT));
        assert!(!ctx.ready(THEIR_GEAR));
        assert!(ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.ready(fixtures::VI), "friendly units are untouched");
        assert!(
            ctx.awaken(fixtures::THEIR_UNIT, 1),
            "the turn's Awaken is not a spell or ability"
        );
    }
}

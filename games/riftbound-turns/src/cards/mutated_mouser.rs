use super::prelude::unit;
use super::{Card, Keyword};

pub const SHIELD: u8 = 2;

pub static CARD: Card = unit(
    "Mutated Mouser",
    &[Keyword::Shield(SHIELD), Keyword::Tank],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, EntryMove, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, legal, play as play_engine, settle, showdown};
    use crate::state::{GameBlob, Mode, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const MOUSER: u32 = 90;
    const BYSTANDER: u32 = 91;
    const BRUTE: u32 = 92;
    const THEIRS: u32 = 93;
    const ENERGY: u8 = 2;
    const MIGHT: u8 = 1;
    const BRUTE_MIGHT: u8 = MIGHT + SHIELD + 1;
    const BYSTANDER_MIGHT: u8 = MIGHT + 2 * SHIELD;

    fn tank(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            ..fixtures::unit(id, zone, seat, "Mutated Mouser", MIGHT)
        }
    }

    fn contested(tank_seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
            .table
            .cards
            .push(tank(MOUSER, fixtures::BF1, tank_seat));
        fixture.table.cards.push(fixtures::unit(
            BYSTANDER,
            fixtures::BF1,
            tank_seat,
            "Nobody",
            BYSTANDER_MIGHT,
        ));
        fixture.table.cards.push(fixtures::unit(
            BRUTE,
            fixtures::BF1,
            1 - tank_seat,
            "Brute",
            BRUTE_MIGHT,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MOUSER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.is_some());
    }

    fn fight(ctx: &mut Ctx) {
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "741.1.b · a lone tank and a lone neighbour leave nothing to ask: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.showdown.is_none());
    }

    fn damage_dealt(ctx: &Ctx, card: u32) -> i32 {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter,
                    delta,
                } if *held == card && *counter == COUNTER_DAMAGE && *delta > 0 => Some(*delta),
                _ => None,
            })
            .sum()
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: Some(fixtures::BASE),
            to_seat: seat,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_shield_two_tank_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Mutated Mouser").unwrap(), &CARD));
        assert_eq!(CARD.name, "Mutated Mouser");
        assert_eq!(CARD.keywords, &[Keyword::Shield(2), Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = contested(1);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(MOUSER, Keyword::Shield(2)));
        assert!(ctx.has_keyword(MOUSER, Keyword::Tank));
        assert_eq!(ctx.current_might(MOUSER), i32::from(MIGHT));
        assert_eq!(
            combat::ordered(&ctx, &[BYSTANDER, MOUSER], false),
            [MOUSER],
            "741.1.b · the tank is the only first choice"
        );
    }

    #[test]
    fn defending_it_shields_by_two_and_soaks_the_attackers_damage_before_its_neighbour() {
        let mut fixture = contested(1);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_defender(MOUSER));
        assert_eq!(
            ctx.current_might(MOUSER),
            i32::from(MIGHT + SHIELD),
            "733 · +2 while a defender"
        );
        assert_eq!(combat::lethal(&ctx, MOUSER), MIGHT + SHIELD);
        assert_eq!(
            combat::might_sum(&ctx, &[MOUSER, BYSTANDER]),
            MIGHT + SHIELD + BYSTANDER_MIGHT
        );
        fight(&mut ctx);
        assert_eq!(
            damage_dealt(&ctx, MOUSER),
            i32::from(MIGHT + SHIELD),
            "the tank soaks its lethal first"
        );
        assert_eq!(ctx.card(MOUSER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(damage_dealt(&ctx, BYSTANDER), 1);
        assert_eq!(
            ctx.location(BYSTANDER),
            Some(Location::Battlefield(fixtures::BF1)),
            "one damage is not lethal"
        );
        assert_eq!(
            damage_dealt(&ctx, BRUTE),
            i32::from(MIGHT + SHIELD + BYSTANDER_MIGHT),
            "the shield counts in the defenders' swing"
        );
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn attacking_it_has_no_shield_and_still_takes_the_defenders_damage_first() {
        let mut fixture = contested(0);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(MOUSER));
        assert_eq!(
            ctx.current_might(MOUSER),
            i32::from(MIGHT),
            "Shield is nothing to an attacker"
        );
        assert_eq!(combat::lethal(&ctx, MOUSER), MIGHT);
        fight(&mut ctx);
        assert_eq!(
            damage_dealt(&ctx, MOUSER),
            i32::from(MIGHT),
            "printed lethal only, and it went first"
        );
        assert_eq!(ctx.card(MOUSER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(damage_dealt(&ctx, BYSTANDER), i32::from(SHIELD + 1));
        assert_eq!(
            ctx.location(BYSTANDER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "the surviving attacker conquers"
        );
    }

    #[test]
    fn the_other_seat_cannot_play_it_on_this_turn_and_a_short_seat_is_refused() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tank(MOUSER, fixtures::HAND, 0));
        fixture.table.cards.push(tank(THEIRS, fixtures::HAND, 1));
        for rune in &mut fixture.table.cards {
            if rune.is_kind("Rune") && rune.owner == 0 {
                rune.exhausted = true;
            }
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIRS)),
            Err(Refusal::NotYourTurn)
        );
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, MOUSER)),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 0
            })
        );
        drop(ctx);
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for rune in runes.into_iter().take(ENERGY.into()) {
            fixture.table.card_mut(rune).unwrap().exhausted = false;
        }
        fixture.resolve();
        let action = fixtures::move_action(MOUSER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        legal::classify(&ctx, 0, &entry(&ctx, 0, MOUSER)).unwrap();
        play_engine::begin(&mut ctx, 0, MOUSER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(MOUSER), Some(Location::Base(0)));
        assert!(ctx.card(MOUSER).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.ready_runes_of(0).is_empty());
    }
}

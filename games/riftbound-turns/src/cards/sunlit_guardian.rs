use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Sunlit Guardian", &[Keyword::Shield(1), Keyword::Tank], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, legal, play as play_engine, prompts, settle, showdown};
    use crate::state::{GameBlob, Mode, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const GUARDIAN: u32 = 90;
    const BYSTANDER: u32 = 91;
    const OTHER_BYSTANDER: u32 = 94;
    const BRUTE: u32 = 92;
    const THEIR_GUARDIAN: u32 = 93;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;
    const SHIELD: i32 = 1;
    const BRUTE_MIGHT: u8 = 6;

    fn guardian(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Calm".into()],
            ..fixtures::unit(id, zone, seat, "Sunlit Guardian", MIGHT)
        }
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
    }

    fn contested(guardian_seat: u8) -> Fixture {
        let brute_seat = 1 - guardian_seat;
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(guardian(GUARDIAN, fixtures::BF1, guardian_seat));
        for bystander in [BYSTANDER, OTHER_BYSTANDER] {
            fixture.table.cards.push(fixtures::unit(
                bystander,
                fixtures::BF1,
                guardian_seat,
                "Nobody",
                MIGHT,
            ));
        }
        fixture.table.cards.push(fixtures::unit(
            BRUTE,
            fixtures::BF1,
            brute_seat,
            "Brute",
            BRUTE_MIGHT,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(
            ctx.blob.showdown.is_some(),
            "the contested battlefield opens a showdown"
        );
    }

    fn assigning(ctx: &mut Ctx) {
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
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

    fn play_to_base(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        play_engine::begin(ctx, seat, card, Origin::Hand, Some(Location::Base(seat)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    #[test]
    fn the_script_is_a_shield_one_tank_unit_with_no_abilities() {
        assert_eq!(CARD.name, "Sunlit Guardian");
        assert_eq!(CARD.keywords, &[Keyword::Shield(1), Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = contested(1);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GUARDIAN).unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(GUARDIAN, Keyword::Shield(1)));
        assert!(ctx.has_keyword(GUARDIAN, Keyword::Tank));
        assert_eq!(ctx.current_might(GUARDIAN), i32::from(MIGHT));
        assert_eq!(
            combat::ordered(&ctx, &[BYSTANDER, GUARDIAN, OTHER_BYSTANDER], false),
            [GUARDIAN],
            "741.1.b · the tank is the only first choice"
        );
    }

    #[test]
    fn defending_it_shields_to_four_and_takes_the_attackers_damage_before_its_neighbours() {
        let mut fixture = contested(1);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_defender(GUARDIAN));
        assert_eq!(ctx.current_might(GUARDIAN), i32::from(MIGHT) + SHIELD);
        assert_eq!(combat::lethal(&ctx, GUARDIAN), MIGHT + 1);
        assigning(&mut ctx);
        assert_eq!(
            combat::assigner(&ctx),
            Some((0, BRUTE_MIGHT - MIGHT - 1)),
            "the tank's lethal was assigned without a question"
        );
        assert_eq!(combat::candidates(&ctx), [BYSTANDER, OTHER_BYSTANDER]);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BYSTANDER}}} (lethal {MIGHT})"),
                format!("{{card {OTHER_BYSTANDER}}} (lethal {MIGHT})")
            ],
            "the remainder is a choice between the plain units"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Assign),
            format!(
                "assign {} damage: who takes lethal next?",
                BRUTE_MIGHT - MIGHT - 1
            )
        );
        fixtures::choose(
            &mut ctx,
            0,
            &format!("{{card {BYSTANDER}}} (lethal {MIGHT})"),
        )
        .unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(damage_dealt(&ctx, GUARDIAN), i32::from(MIGHT) + SHIELD);
        assert_eq!(ctx.card(GUARDIAN).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            damage_dealt(&ctx, BYSTANDER),
            i32::from(BRUTE_MIGHT - MIGHT - 1)
        );
        assert_eq!(
            ctx.location(BYSTANDER),
            Some(Location::Battlefield(fixtures::BF1)),
            "two damage is not lethal and is healed after combat"
        );
        assert_eq!(damage_dealt(&ctx, OTHER_BYSTANDER), 0);
        assert_eq!(
            damage_dealt(&ctx, BRUTE),
            i32::from(MIGHT) + SHIELD + 2 * i32::from(MIGHT),
            "the shield counts in the defenders' swing"
        );
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
    }

    #[test]
    fn attacking_it_has_no_shield_and_still_takes_the_defenders_damage_first() {
        let mut fixture = contested(0);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(GUARDIAN));
        assert_eq!(ctx.current_might(GUARDIAN), i32::from(MIGHT));
        assert_eq!(combat::lethal(&ctx, GUARDIAN), MIGHT);
        assigning(&mut ctx);
        assert_eq!(
            combat::assigner(&ctx),
            Some((1, BRUTE_MIGHT - MIGHT)),
            "printed lethal only: the tank went first without a question"
        );
        assert_eq!(combat::candidates(&ctx), [BYSTANDER, OTHER_BYSTANDER]);
        fixtures::choose(
            &mut ctx,
            1,
            &format!("{{card {BYSTANDER}}} (lethal {MIGHT})"),
        )
        .unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(damage_dealt(&ctx, GUARDIAN), i32::from(MIGHT));
        assert_eq!(ctx.card(GUARDIAN).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(damage_dealt(&ctx, BYSTANDER), i32::from(MIGHT));
        assert_eq!(ctx.card(BYSTANDER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(damage_dealt(&ctx, OTHER_BYSTANDER), 0);
        assert_eq!(
            ctx.location(OTHER_BYSTANDER),
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
    fn the_other_seat_cannot_play_it_on_this_turn_and_two_ready_runes_are_not_enough() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(guardian(GUARDIAN, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(guardian(THEIR_GUARDIAN, fixtures::HAND, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_GUARDIAN)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        fixture.table.card_mut(43).unwrap().exhausted = true;
        fixture.resolve();
        let action = fixtures::move_action(GUARDIAN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, GUARDIAN),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty(), "nothing was paid");
        drop(ctx);
        fixture.table.card_mut(43).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, GUARDIAN).unwrap();
        assert_eq!(ctx.location(GUARDIAN), Some(Location::Base(0)));
        assert!(ctx.card(GUARDIAN).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none());
    }
}

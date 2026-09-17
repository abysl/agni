use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit(
    "Jeweled Colossus",
    &[Keyword::Vision, Keyword::Shield(1)],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Event, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, legal, play as play_engine, settle, showdown};
    use crate::state::{GameBlob, Mode, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const COLOSSUS: u32 = 90;
    const NOBODY: u32 = 91;
    const THEIR_COLOSSUS: u32 = 92;
    const ENERGY: u8 = 5;
    const MIGHT: u8 = 5;
    const SHIELD: i32 = 1;
    const EXTRA_RUNES: [u32; 2] = [100, 101];
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const MY_TOP: u32 = 23;

    fn colossus(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, zone, seat, "Jeweled Colossus", MIGHT)
        }
    }

    fn in_hand(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(colossus(COLOSSUS, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(colossus(THEIR_COLOSSUS, fixtures::HAND, 1));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, rune) in runes.into_iter().enumerate() {
            fixture.table.card_mut(rune).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        fixture
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
    }

    fn defending_against(attacker_might: u8) -> Fixture {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(colossus(COLOSSUS, fixtures::BF1, 1));
        fixture.table.cards.push(fixtures::unit(
            NOBODY,
            fixtures::BF1,
            0,
            "Nobody",
            attacker_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn attacking_into(defender_might: u8) -> Fixture {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(colossus(COLOSSUS, fixtures::BF1, 0));
        fixture.table.cards.push(fixtures::unit(
            NOBODY,
            fixtures::BF1,
            1,
            "Nobody",
            defender_might,
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

    fn fight(ctx: &mut Ctx) {
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
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

    fn play_to_base(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        play_engine::begin(ctx, seat, card, Origin::Hand, Some(Location::Base(seat)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_vision_shield_one_unit_with_no_abilities() {
        assert_eq!(CARD.name, "Jeweled Colossus");
        assert_eq!(CARD.keywords, &[Keyword::Vision, Keyword::Shield(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = defending_against(MIGHT);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(COLOSSUS).unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(COLOSSUS, Keyword::Vision));
        assert!(ctx.has_keyword(COLOSSUS, Keyword::Shield(1)));
        assert_eq!(ctx.current_might(COLOSSUS), i32::from(MIGHT));
    }

    #[test]
    fn as_a_defender_it_has_one_more_might_and_survives_damage_equal_to_its_printed_might() {
        let mut fixture = defending_against(MIGHT);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_defender(COLOSSUS));
        assert_eq!(ctx.current_might(COLOSSUS), i32::from(MIGHT) + SHIELD);
        assert_eq!(combat::lethal(&ctx, COLOSSUS), MIGHT + 1);
        fight(&mut ctx);
        assert_eq!(damage_dealt(&ctx, COLOSSUS), i32::from(MIGHT));
        assert_eq!(
            ctx.location(COLOSSUS),
            Some(Location::Battlefield(fixtures::BF1)),
            "five damage is not lethal on six might"
        );
        assert_eq!(damage_dealt(&ctx, NOBODY), i32::from(MIGHT) + SHIELD);
        assert_eq!(ctx.card(NOBODY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
    }

    #[test]
    fn as_an_attacker_the_shield_adds_nothing() {
        let mut fixture = attacking_into(MIGHT);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(COLOSSUS));
        assert_eq!(ctx.current_might(COLOSSUS), i32::from(MIGHT));
        assert_eq!(combat::lethal(&ctx, COLOSSUS), MIGHT);
        fight(&mut ctx);
        assert_eq!(damage_dealt(&ctx, COLOSSUS), i32::from(MIGHT));
        assert_eq!(ctx.card(COLOSSUS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(NOBODY).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn played_from_hand_it_pays_five_and_enters_the_base_exhausted() {
        let mut fixture = in_hand(ENERGY.into());
        let action = fixtures::move_action(COLOSSUS, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(deck_of(&ctx, 0), MY_DECK);
        play_to_base(&mut ctx, 0, COLOSSUS).unwrap();
        assert_eq!(ctx.location(COLOSSUS), Some(Location::Base(0)));
        assert!(ctx.card(COLOSSUS).unwrap().exhausted);
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "five energy exhausts every rune"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == COLOSSUS
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn vision_looks_at_the_top_card_of_the_deck_as_it_is_played_and_may_recycle_it() {
        let mut fixture = in_hand(ENERGY.into());
        let action = fixtures::move_action(COLOSSUS, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, COLOSSUS).unwrap();
        assert_eq!(ctx.location(COLOSSUS), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "817.1.c · the trigger is the unit entering the board"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            peeks(&ctx),
            [(MY_TOP, 0)],
            "817.1.b · predict: the look reaches its controller only"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        assert_eq!(
            deck_of(&ctx, 0),
            [MY_TOP, 20, 21, 22],
            "403.1.a · recycled to the bottom of the Main Deck"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_other_seat_cannot_play_it_on_this_turn_and_four_ready_runes_are_not_enough() {
        let mut fixture = in_hand(ENERGY.into());
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_COLOSSUS)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut short = in_hand(usize::from(ENERGY) - 1);
        let action = fixtures::move_action(COLOSSUS, fixtures::BASE, 0);
        let mut ctx = short.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, COLOSSUS),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        assert!(peeks(&ctx).is_empty(), "nothing was looked at");
    }
}

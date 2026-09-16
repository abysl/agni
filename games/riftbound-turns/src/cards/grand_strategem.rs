use super::prelude::{done, friendly_units, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 5;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let units = friendly_units(ctx, seat);
    for unit in &units {
        might_this_turn(ctx, item, *unit, MIGHT, None);
    }
    ctx.narrate(format!(
        "{{seat {seat}}}'s {} units get +{MIGHT} might this turn",
        units.len()
    ));
    done()
}

pub static CARD: Card = spell("Grand Strategem", &[Keyword::Action], &[play(&[], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::Expiry;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const STRATEGEM: u32 = 90;
    const ALLY: u32 = 91;
    const ORDER_RUNES: [u32; 6] = [100, 101, 102, 103, 104, 105];

    fn strategem(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(STRATEGEM, fixtures::HAND, seat, "Grand Strategem", 6, 3);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.table.cards.push(strategem(0));
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_an_action_without_targets() {
        assert!(std::ptr::eq(script_of("Grand Strategem").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(
            CARD.abilities[0].targets.is_empty(),
            "355.10.d · friendly units are selected, not chosen"
        );
    }

    #[test]
    fn every_friendly_unit_on_the_board_gets_five_might_for_the_turn_and_enemies_none() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRATEGEM).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "six energy off the six Order runes"
        );
        assert_eq!(
            ORDER_RUNES
                .iter()
                .filter(|rune| ctx.card(**rune).unwrap().zone == Some(fixtures::RUNE_DECK))
                .count(),
            3,
            "three of them recycled for the Order power"
        );
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 8, "3 + 5 in the base");
        assert_eq!(ctx.current_might(ALLY), 6, "1 + 5 at a battlefield");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "an enemy unit is untouched"
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert_eq!(
            ctx.current_might(fixtures::CHAMPION_CARD),
            3,
            "a champion off the board is not a friendly unit"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 5);
        assert_eq!(might_counter(&ctx, ALLY), 5);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(
            ctx.state_of(ALLY).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s 2 units get +5 might this turn".to_string()));
        assert_eq!(ctx.card(STRATEGEM).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(ALLY), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn units_are_read_at_resolution_so_one_that_left_is_skipped_and_one_that_arrived_counts() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRATEGEM).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(ALLY, fixtures::TRASH, 0), 0)
            .unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::HAND_UNIT, fixtures::BASE, 0),
                0,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(might_counter(&ctx, ALLY), 0);
        assert_eq!(ctx.current_might(fixtures::HAND_UNIT), 7, "2 + 5");
        assert_eq!(ctx.current_might(fixtures::VI), 8);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s 2 units get +5 might this turn".to_string()));
    }

    #[test]
    fn it_is_refused_off_turn_and_without_six_ready_runes() {
        let mut fixture = armed();
        let theirs = {
            let mut card = strategem(1);
            card.id = 92;
            card
        };
        fixture.table.cards.push(theirs);
        fixture.resolve();
        let ctx = fixture.ctx();
        let entry = EntryMove {
            card: 92,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "an Action has no window in the other seat's neutral open state"
        );
        let mut fixture = armed();
        fixture.table.card_mut(ORDER_RUNES[0]).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            matches!(
                fixtures::play_from_hand(&mut ctx, 0, STRATEGEM),
                Err(Refusal::NotEnoughRunes { .. }) | Err(Refusal::Illegal(_))
            ),
            "five ready runes cannot pay six energy"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(STRATEGEM).unwrap().zone, Some(fixtures::HAND));
    }
}

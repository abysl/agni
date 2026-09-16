use super::prelude::unit;
use super::Card;
use crate::engine::ctx::Ctx;

pub const LOCKED_TURNS: u16 = 3;

pub fn turn_number_of(ctx: &Ctx, seat: u8) -> Option<u16> {
    if seat != ctx.turn_player() || seat >= ctx.players() {
        return None;
    }
    let players = u16::from(ctx.players());
    Some(ctx.turn().saturating_add(players - 1) / players)
}

pub fn cannot_be_played(ctx: &Ctx, seat: u8) -> bool {
    turn_number_of(ctx, seat).is_none_or(|turn| turn <= LOCKED_TURNS)
}

pub static CARD: Card = unit("Ol' Poro", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::{GameBlob, Mode, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;

    fn poro() -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(0),
            domain: vec!["Calm".into()],
            ..fixtures::unit(PORO, fixtures::HAND, 0, "Ol' Poro", 4)
        }
    }

    fn kennel(turn: u16, player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture.table.cards.push(poro());
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
        fixture
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: PORO,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.base,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_veto_is_a_named_seam() {
        assert!(std::ptr::eq(script_of("Ol' Poro").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(LOCKED_TURNS, 3);
    }

    #[test]
    fn a_seats_turn_number_counts_its_own_turns_in_a_two_player_game() {
        for (turn, player, expected) in [
            (1, 0, 1),
            (2, 1, 1),
            (3, 0, 2),
            (4, 1, 2),
            (5, 0, 3),
            (6, 1, 3),
            (7, 0, 4),
            (8, 1, 4),
        ] {
            let mut fixture = kennel(turn, player);
            let ctx = fixture.ctx();
            assert_eq!(
                turn_number_of(&ctx, player),
                Some(expected),
                "turn {turn} is seat {player}'s turn number {expected}"
            );
            assert_eq!(
                turn_number_of(&ctx, 1 - player),
                None,
                "the other seat is not taking a turn"
            );
            assert_eq!(turn_number_of(&ctx, 2), None, "no such seat");
        }
    }

    #[test]
    fn he_cannot_be_played_on_your_first_three_turns_and_can_from_the_fourth() {
        for (turn, player, locked) in [
            (1, 0, true),
            (2, 1, true),
            (3, 0, true),
            (5, 0, true),
            (6, 1, true),
            (7, 0, false),
            (8, 1, false),
            (11, 0, false),
        ] {
            let mut fixture = kennel(turn, player);
            let ctx = fixture.ctx();
            assert_eq!(
                cannot_be_played(&ctx, player),
                locked,
                "turn {turn}: seat {player} locked = {locked}"
            );
            assert!(
                cannot_be_played(&ctx, 1 - player),
                "a seat not taking its turn cannot play a unit at all"
            );
        }
    }

    #[test]
    fn today_the_legal_list_offers_him_on_turn_one_because_no_veto_is_consulted() {
        let mut fixture = kennel(1, 0);
        let ctx = fixture.ctx();
        assert!(cannot_be_played(&ctx, 0));
        assert!(
            legal::classify(&ctx, 0, &entry(&ctx)).is_ok(),
            "the seam reads true but nothing in legal::classify reads it yet"
        );
    }

    #[test]
    #[ignore = "engine gap · per-turn counters (a seat's turn number, the Obelisk of Power row: an extra turn in the opening rounds shifts turn_number_of off the turn number) plus a legal::classify consult of a Static::PlayVeto(Applies) fed by cannot_be_played, refusing the play with its own Reason on turns one to three and offering it from the fourth"]
    fn on_your_first_three_turns_the_play_is_refused_and_on_the_fourth_it_lands() {
        let mut fixture = kennel(3, 0);
        let mut ctx = fixture.ctx();
        assert!(legal::classify(&ctx, 0, &entry(&ctx)).is_err());
        assert!(fixtures::play_from_hand(&mut ctx, 0, PORO).is_err());
        assert_eq!(ctx.card(PORO).unwrap().zone, Some(fixtures::HAND));
        drop(ctx);
        let mut later = kennel(7, 0);
        let mut ctx = later.ctx();
        legal::classify(&ctx, 0, &entry(&ctx)).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, PORO).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(PORO));
    }
}

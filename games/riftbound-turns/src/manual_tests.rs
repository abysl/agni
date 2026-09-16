use crate::engine::fixtures::*;
use crate::{decide, GameBlob, Mode, Phase, TurnEvent};
use agni_plugin_sdk::decide::{Action, Effect, Request};
use agni_plugin_sdk::manual::Command;
use agni_plugin_sdk::prompt::Prompt;
use agni_plugin_sdk::table::Target;

fn request(blob: &GameBlob, seat: u8, action: Action) -> Request {
    Request {
        plugin_state: blob.encode(),
        players: 2,
        seat,
        action,
        table: table(),
    }
}

fn manual() -> GameBlob {
    let mut blob = GameBlob::start(2, 0, Mode::Enforced);
    blob.prompt = Some(Prompt::new(4, 0, 1, 1));
    blob.won = Some(0);
    blob.set_phase(Phase::Setup);
    let result = decide(&request(&blob, 1, Action::Game(Command::Disable.encode()))).unwrap();
    assert!(result.effects.is_empty());
    let freed = GameBlob::decode(&result.plugin_state.unwrap()).unwrap();
    assert!(freed.manual);
    assert_eq!(freed.mode, Mode::Free);
    assert!(freed.prompt.is_none());
    assert!(freed.won.is_none());
    assert_eq!(freed.turn_player(), 0);
    assert_eq!(freed.phase(), Some(Phase::Action));
    freed
}

#[test]
fn emergency_recovery_bypasses_prompts_winners_and_turn_ownership() {
    let blob = manual();
    assert_eq!(GameBlob::decode(&blob.encode()), Some(blob));
}

#[test]
fn emergency_recovery_works_before_setup_is_idempotent_and_reset_restores_rules() {
    let lobby = GameBlob::lobby_in(2, Mode::Enforced);
    let first = decide(&request(&lobby, 1, Action::Game(Command::Disable.encode()))).unwrap();
    let manual = GameBlob::decode(first.plugin_state.as_ref().unwrap()).unwrap();
    assert!(manual.manual);
    assert_eq!(manual.turn_player(), 1);
    let repeated = decide(&request(
        &manual,
        0,
        Action::Game(Command::Disable.encode()),
    ))
    .unwrap();
    assert_eq!(repeated.plugin_state, first.plugin_state);
    let reset = decide(&request(&manual, 0, Action::Reset)).unwrap();
    assert_eq!(reset.plugin_state, Some(Vec::new()));
    assert!(GameBlob::decode(reset.plugin_state.as_ref().unwrap()).is_none());
}

#[test]
fn manual_play_never_pays_scores_draws_or_runs_cleanup() {
    let blob = manual();
    for action in [
        move_action(HAND_UNIT, BF1, 0),
        move_action(RUNE_A, BF1, 0),
        Action::Counter {
            target: Target::Seat(1),
            counter: 0,
            delta: 5,
        },
        Action::Game(TurnEvent::EndTurn.encode()),
    ] {
        assert!(decide(&request(&blob, 0, action))
            .unwrap()
            .effects
            .is_empty());
    }
}

#[test]
fn deck_look_is_private_and_shuffle_is_repeatable() {
    let blob = manual();
    let looked = decide(&request(
        &blob,
        1,
        Action::Game(
            Command::Look {
                zone: MAIN_DECK,
                count: 1,
            }
            .encode(),
        ),
    ))
    .unwrap();
    assert_eq!(looked.effects, [Effect::Peek { card: 25, seat: 1 }]);
    let shuffled = request(
        &blob,
        0,
        Action::Game(
            Command::Shuffle {
                zone: MAIN_DECK,
                seed: 42,
            }
            .encode(),
        ),
    );
    let first = decide(&shuffled).unwrap();
    assert_eq!(first, decide(&shuffled).unwrap());
    assert_eq!(first.effects.len(), 8);
    assert!(first.effects[..4]
        .iter()
        .all(|effect| matches!(effect, Effect::Conceal { card: 20..=23 })));
    assert!(first.effects[4..].iter().all(|effect| matches!(
        effect,
        Effect::Move {
            card: 20..=23,
            zone: MAIN_DECK,
            seat: 0,
            ..
        }
    )));
}

#[test]
fn empty_and_singleton_decks_are_safe_to_look_at_and_shuffle() {
    let blob = manual();
    let mut empty = request(
        &blob,
        1,
        Action::Game(
            Command::Look {
                zone: MAIN_DECK,
                count: u32::MAX,
            }
            .encode(),
        ),
    );
    empty
        .table
        .cards
        .retain(|card| card.zone != Some(MAIN_DECK) || card.seat != 1);
    assert!(decide(&empty).unwrap().effects.is_empty());
    empty.action = Action::Game(
        Command::Shuffle {
            zone: MAIN_DECK,
            seed: 42,
        }
        .encode(),
    );
    assert!(decide(&empty).unwrap().effects.is_empty());

    let mut singleton = request(
        &blob,
        1,
        Action::Game(
            Command::Shuffle {
                zone: MAIN_DECK,
                seed: 42,
            }
            .encode(),
        ),
    );
    singleton
        .table
        .cards
        .retain(|card| card.zone != Some(MAIN_DECK) || card.seat != 1 || card.id == 25);
    assert_eq!(
        decide(&singleton).unwrap().effects,
        [
            Effect::Conceal { card: 25 },
            Effect::Move {
                card: 25,
                zone: MAIN_DECK,
                seat: 1,
                index: agni_plugin_sdk::decide::BOTTOM,
            },
        ]
    );
}

#[test]
fn manual_commands_require_recovery_and_a_real_seat() {
    let blob = GameBlob::start(2, 0, Mode::Enforced);
    assert!(decide(&request(
        &blob,
        0,
        Action::Game(
            Command::Look {
                zone: MAIN_DECK,
                count: 1
            }
            .encode()
        )
    ))
    .is_err());
    assert!(decide(&request(&blob, 2, Action::Game(Command::Disable.encode()))).is_err());
    let blob = manual();
    assert!(decide(&request(
        &blob,
        0,
        Action::Game(Command::RemoveToken { card: VI }.encode())
    ))
    .is_err());
    assert!(decide(&request(
        &blob,
        0,
        Action::Game(
            Command::Look {
                zone: BASE,
                count: 1
            }
            .encode()
        )
    ))
    .is_err());
}

#[test]
fn reveal_conceal_and_token_removal_are_owner_only() {
    let blob = manual();
    for command in [
        Command::Reveal {
            card: THEIR_HAND_CARD,
        },
        Command::Conceal {
            card: THEIR_HAND_CARD,
        },
        Command::RemoveToken { card: SPRITE },
    ] {
        assert!(decide(&request(&blob, 0, Action::Game(command.encode()))).is_err());
    }
    assert_eq!(
        decide(&request(
            &blob,
            1,
            Action::Game(
                Command::Reveal {
                    card: THEIR_HAND_CARD
                }
                .encode()
            )
        ))
        .unwrap()
        .effects,
        [Effect::Reveal {
            card: THEIR_HAND_CARD
        }]
    );
    assert_eq!(
        decide(&request(
            &blob,
            1,
            Action::Game(
                Command::Conceal {
                    card: THEIR_HAND_CARD
                }
                .encode()
            )
        ))
        .unwrap()
        .effects,
        [Effect::Conceal {
            card: THEIR_HAND_CARD
        }]
    );
    assert_eq!(
        decide(&request(
            &blob,
            1,
            Action::Game(Command::RemoveToken { card: SPRITE }.encode())
        ))
        .unwrap()
        .effects,
        [Effect::Despawn { card: SPRITE }]
    );
}

#[test]
fn the_emergency_action_is_offered_to_both_seats_during_setup_and_after_a_win() {
    for winner in [None, Some(0)] {
        let mut blob = GameBlob::start(2, 0, Mode::Enforced);
        blob.won = winner;
        blob.set_phase(Phase::Setup);
        blob.prompt = Some(Prompt::new(1, 0, 1, 1));
        for seat in 0..2 {
            let view = crate::present::present(&agni_plugin_sdk::view::Request {
                plugin_state: blob.encode(),
                players: 2,
                seat,
                zones: zones(),
                table: table(),
            });
            let index = view
                .affordances
                .iter()
                .position(|offer| offer.label == agni_plugin_sdk::manual::DISABLE)
                .unwrap();
            assert!(view.hidden.contains(&(index as u16)));
            assert_eq!(
                Command::decode(&view.affordances[index].data),
                Some(Command::Disable)
            );
        }
    }
}

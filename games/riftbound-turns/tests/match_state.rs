#[path = "../src/match_state.rs"]
mod match_state;

use match_state::{
    canonical_battlefield_name, GameResult, MatchError, MatchState, RecordOutcome, SelectionPolicy,
    StartChoice, StartPolicy, MATCH_WINS_REQUIRED,
};

fn names<'a>(first: &'a str, second: &'a str) -> [&'a str; 2] {
    [first, second]
}

#[test]
fn a_win_records_once_and_gives_the_loser_the_next_choice() {
    let mut state = MatchState::new(0).unwrap();
    assert_eq!(state.start_policy(), StartPolicy::Opening { chooser: 0 });
    assert!(!state.selection_policy().sideboard_allowed());
    assert_eq!(
        state.start_with_choice(0, StartChoice::First, names("Alpha Field", "Beta Field")),
        Ok(0)
    );
    assert_eq!(state.start_policy(), StartPolicy::Unavailable);
    assert_eq!(state.selection_policy(), SelectionPolicy::Unavailable);
    assert_eq!(
        state.start_with_choice(0, StartChoice::First, names("Gamma", "Delta")),
        Err(MatchError::GameAlreadyStarted)
    );
    assert_eq!(
        state.record_result(GameResult::Won(0)),
        Ok(RecordOutcome::Recorded)
    );
    assert_eq!(state.start_policy(), StartPolicy::Unavailable);
    assert_eq!(state.selection_policy(), SelectionPolicy::Unavailable);
    assert_eq!(
        state.start_with_choice(1, StartChoice::First, names("Gamma", "Delta")),
        Err(MatchError::GameAlreadyFinished)
    );
    assert_eq!(
        state.record_result(GameResult::Won(0)),
        Ok(RecordOutcome::AlreadyRecorded)
    );
    assert_eq!(
        state.record_result(GameResult::Draw),
        Err(MatchError::ResultAlreadyRecorded)
    );
    assert_eq!(
        state.record_result(GameResult::Won(1)),
        Err(MatchError::ResultAlreadyRecorded)
    );
    assert_eq!(state.wins, [1, 0]);
    assert_eq!(state.reset_for_next_game(), Ok(()));
    assert_eq!(state.generation, 1);
    assert_eq!(state.start_policy(), StartPolicy::Loser { chooser: 1 });
    assert_eq!(
        state.selection_policy(),
        SelectionPolicy::Unused {
            used: [vec!["alpha field".into()], vec!["beta field".into()]],
            sideboard_allowed: true,
        }
    );
    assert_eq!(
        state.start_with_choice(1, StartChoice::Last, names("Gamma Field", "Delta Field")),
        Ok(0)
    );
    assert_eq!(state.current.as_ref().map(|game| game.first), Some(0));
    assert_eq!(
        state.record_result(GameResult::Won(0)),
        Ok(RecordOutcome::Recorded)
    );
    assert!(state.complete());
    assert_eq!(state.reset_for_next_game(), Err(MatchError::MatchComplete));
    assert_eq!(state.start_policy(), StartPolicy::Unavailable);
}

#[test]
fn either_seat_can_win_and_both_presented_names_are_consumed_by_owner() {
    let mut state = MatchState::new(1).unwrap();
    assert_eq!(
        state.start_with_choice(1, StartChoice::Last, names("North", "South")),
        Ok(0)
    );
    assert_eq!(
        state.record_result(GameResult::Won(1)),
        Ok(RecordOutcome::Recorded)
    );
    assert_eq!(state.reset_for_next_game(), Ok(()));
    assert_eq!(state.start_policy(), StartPolicy::Loser { chooser: 0 });
    assert_eq!(
        state.selection_policy(),
        SelectionPolicy::Unused {
            used: [vec!["north".into()], vec!["south".into()]],
            sideboard_allowed: true,
        }
    );
    assert_eq!(
        state.start_with_choice(0, StartChoice::First, names("South", "West")),
        Ok(0)
    );
    assert_eq!(
        state.current.as_ref().unwrap().battlefields,
        ["south", "west"]
    );
}

#[test]
fn both_seats_may_present_the_same_battlefield_name() {
    let mut state = MatchState::new(0).unwrap();
    assert_eq!(
        state.start_with_choice(0, StartChoice::First, names("Shared Field", "shared field")),
        Ok(0)
    );
    assert_eq!(
        state.current.as_ref().unwrap().battlefields,
        ["shared field", "shared field"]
    );
    assert_eq!(
        canonical_battlefield_name("  Shared   Field "),
        Some("shared field".into())
    );
}

#[test]
fn a_draw_keeps_score_and_requires_the_same_owned_battlefields() {
    let mut state = MatchState::new(0).unwrap();
    assert_eq!(
        state.start_with_choice(0, StartChoice::Last, names("Alpha", "Beta")),
        Ok(1)
    );
    assert_eq!(
        state.record_result(GameResult::Draw),
        Ok(RecordOutcome::Recorded)
    );
    assert_eq!(state.reset_for_next_game(), Ok(()));
    assert_eq!(state.wins, [0, 0]);
    assert_eq!(
        state.start_policy(),
        StartPolicy::Fixed { actor: 1, first: 1 }
    );
    assert_eq!(state.start_policy().fixed_first(), Some(1));
    assert_eq!(
        state.selection_policy(),
        SelectionPolicy::Exact {
            battlefields: ["alpha".into(), "beta".into()],
            sideboard_allowed: false,
        }
    );
    assert_eq!(
        state
            .selection_policy()
            .required()
            .map(|names| names.as_slice()),
        Some(["alpha".to_owned(), "beta".to_owned()].as_slice())
    );
    assert!(!state.selection_policy().sideboard_allowed());
    let before = state.clone();
    assert_eq!(
        state.start_with_first(0, 1, names("Alpha", "Beta")),
        Err(MatchError::WrongActor)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.start_with_first(1, 0, names("Alpha", "Beta")),
        Err(MatchError::WrongFirst)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.start_with_first(1, 1, names("Alpha", "Gamma")),
        Err(MatchError::BattlefieldUsed)
    );
    assert_eq!(state, before);
    assert_eq!(state.start_with_first(1, 1, names("Alpha", "Beta")), Ok(1));
    assert_eq!(
        state.start_with_first(0, 1, names("Alpha", "Beta")),
        Err(MatchError::GameAlreadyStarted)
    );
}

#[test]
fn reset_refuses_unfinished_duplicate_and_completed_transitions() {
    let mut state = MatchState::new(0).unwrap();
    assert_eq!(state.reset_for_next_game(), Err(MatchError::UnfinishedGame));
    state
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    assert_eq!(state.reset_for_next_game(), Err(MatchError::UnfinishedGame));
    state.record_result(GameResult::Draw).unwrap();
    state.reset_for_next_game().unwrap();
    assert_eq!(state.reset_for_next_game(), Err(MatchError::DuplicateReset));
}

#[test]
fn start_rejects_wrong_actor_missing_and_used_names() {
    let mut state = MatchState::new(0).unwrap();
    assert_eq!(
        state.canonical_battlefield_selection(0, "  Alpha  "),
        Ok("alpha".into())
    );
    assert_eq!(
        state.start_with_choice(1, StartChoice::First, names("Alpha", "Beta")),
        Err(MatchError::WrongActor)
    );
    assert_eq!(
        state.start_with_first(0, 1, names("", "Alpha")),
        Err(MatchError::BattlefieldMissing)
    );
    assert_eq!(state.start_with_first(0, 1, names("Alpha", "Beta")), Ok(1));
    state.record_result(GameResult::Won(0)).unwrap();
    state.reset_for_next_game().unwrap();
    assert_eq!(
        state.canonical_battlefield_selection(0, " Alpha "),
        Err(MatchError::BattlefieldUsed)
    );
    assert_eq!(
        state.canonical_battlefield_selection(1, " Alpha "),
        Ok("alpha".into())
    );
    assert_eq!(
        state.start_with_choice(1, StartChoice::First, names(" Alpha ", "Gamma")),
        Err(MatchError::BattlefieldUsed)
    );
}

#[test]
fn opening_constructor_rejects_an_unverified_seat() {
    assert_eq!(MatchState::new(2), Err(MatchError::InvalidSeat));
}

#[test]
fn cbor_round_trip_is_deterministic_and_generation_overflow_is_refused() {
    let mut state = MatchState::new(0).unwrap();
    state
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    state.record_result(GameResult::Won(0)).unwrap();
    let bytes = state.encode();
    assert_eq!(bytes, state.encode());
    assert_eq!(MatchState::decode(&bytes), Some(state.clone()));
    assert_eq!(state.summary().result, Some(GameResult::Won(0)));

    state.reset_for_next_game().unwrap();
    state
        .start_with_choice(1, StartChoice::First, names("Gamma", "Delta"))
        .unwrap();
    state.generation = u64::MAX;
    assert_eq!(MatchState::decode(&state.encode()), Some(state.clone()));
    state.record_result(GameResult::Draw).unwrap();
    assert_eq!(MatchState::decode(&state.encode()), Some(state.clone()));
    assert_eq!(
        state.reset_for_next_game(),
        Err(MatchError::GenerationOverflow)
    );
    assert!(state.current.is_some());
    assert_eq!(state.result, Some(GameResult::Draw));
}

#[test]
fn persisted_win_draw_win_reuses_the_draw_battlefields() {
    let mut state = MatchState::new(0).unwrap();
    state
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    state.record_result(GameResult::Won(0)).unwrap();
    state = MatchState::decode(&state.encode()).unwrap();
    state.reset_for_next_game().unwrap();
    state
        .start_with_choice(1, StartChoice::First, names("Gamma", "Delta"))
        .unwrap();
    state.record_result(GameResult::Draw).unwrap();
    state = MatchState::decode(&state.encode()).unwrap();
    state.reset_for_next_game().unwrap();
    assert_eq!(state.wins, [1, 0]);
    assert_eq!(
        state.selection_policy(),
        SelectionPolicy::Exact {
            battlefields: ["gamma".into(), "delta".into()],
            sideboard_allowed: false,
        }
    );
    assert_eq!(
        state.start_policy(),
        StartPolicy::Fixed { actor: 1, first: 1 }
    );
    state
        .start_with_first(1, 1, names("Gamma", "Delta"))
        .unwrap();
    state.record_result(GameResult::Won(0)).unwrap();
    let persisted = MatchState::decode(&state.encode()).unwrap();
    assert_eq!(persisted, state);
    assert_eq!(state.wins, [2, 0]);
    assert_eq!(
        state.used,
        [
            vec![String::from("alpha"), String::from("gamma")],
            vec![String::from("beta"), String::from("delta")]
        ]
    );
}

#[test]
fn decoder_rejects_inconsistent_ledger_history() {
    let mut state = MatchState::new(0).unwrap();
    state
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    state.record_result(GameResult::Won(0)).unwrap();
    state.reset_for_next_game().unwrap();

    let mut cleared_usage = state.clone();
    cleared_usage.used = [Vec::new(), Vec::new()];
    assert!(MatchState::decode(&cleared_usage.encode()).is_none());

    let mut wrong_membership = state.clone();
    wrong_membership.used[0] = vec!["other".into()];
    assert!(MatchState::decode(&wrong_membership.encode()).is_none());

    let mut bad_score = state.clone();
    bad_score.wins[0] = MATCH_WINS_REQUIRED + 1;
    assert!(MatchState::decode(&bad_score.encode()).is_none());

    let mut both_complete = state.clone();
    both_complete.wins = [MATCH_WINS_REQUIRED, MATCH_WINS_REQUIRED];
    both_complete.used = [
        vec!["alpha".into(), "extra".into()],
        vec!["beta".into(), "extra".into()],
    ];
    assert!(MatchState::decode(&both_complete.encode()).is_none());

    let mut contradictory_generation = state.clone();
    contradictory_generation.generation = 0;
    assert!(MatchState::decode(&contradictory_generation.encode()).is_none());

    let mut wrong_completion = MatchState::new(0).unwrap();
    wrong_completion
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    wrong_completion.record_result(GameResult::Won(0)).unwrap();
    wrong_completion.reset_for_next_game().unwrap();
    wrong_completion.current = Some(match_state::CurrentGame {
        first: 1,
        battlefields: ["gamma".into(), "delta".into()],
    });
    wrong_completion.generation = 2;
    wrong_completion.result = Some(GameResult::Won(1));
    wrong_completion.wins = [2, 1];
    wrong_completion.used = [
        vec!["alpha".into(), "gamma".into(), "other".into()],
        vec!["beta".into(), "delta".into(), "other".into()],
    ];
    assert!(MatchState::decode(&wrong_completion.encode()).is_none());

    let mut missing_known_win = wrong_completion.clone();
    missing_known_win.result = Some(GameResult::Won(0));
    missing_known_win.wins = [1, 1];
    missing_known_win.used = [
        vec!["alpha".into(), "gamma".into()],
        vec!["beta".into(), "delta".into()],
    ];
    assert!(MatchState::decode(&missing_known_win.encode()).is_none());

    let mut unrecorded_extra_win = state.clone();
    unrecorded_extra_win.current = Some(match_state::CurrentGame {
        first: 1,
        battlefields: ["gamma".into(), "delta".into()],
    });
    unrecorded_extra_win.wins = [2, 0];
    unrecorded_extra_win.used = [
        vec!["alpha".into(), "gamma".into()],
        vec!["beta".into(), "delta".into()],
    ];
    assert!(MatchState::decode(&unrecorded_extra_win.encode()).is_none());

    let mut draw_policy = MatchState::new(0).unwrap();
    draw_policy
        .start_with_choice(0, StartChoice::First, names("Alpha", "Beta"))
        .unwrap();
    draw_policy.record_result(GameResult::Draw).unwrap();
    draw_policy.reset_for_next_game().unwrap();
    draw_policy.current = Some(match_state::CurrentGame {
        first: 1,
        battlefields: ["gamma".into(), "delta".into()],
    });
    assert!(MatchState::decode(&draw_policy.encode()).is_none());
}

pub mod cards;
pub mod engine;
pub mod manual;
#[cfg(test)]
mod manual_tests;
pub mod present;
pub mod rules;
pub mod state;

pub use state::{
    Control, GameBlob, Mode, Phase, Showdown, TurnCore, BLOB_VERSION, DIE_SIDES, NARRATION_LINES,
};

use agni_plugin_sdk::decide::{self, Action, Effect, Verdict};
use agni_plugin_sdk::dice::{DiceRefusal, Outcome, Roll, SECRET_LEN};
use agni_plugin_sdk::prompt::{Opt, Pick, PickRefusal, Prompt};
use agni_plugin_sdk::table::Snapshot;
use agni_plugin_sdk::turns::Window;

pub const UNREADABLE_REQUEST: &str = "unreadable request";
pub const PROMPT_WHY: &str = "choose";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEvent {
    StartGame { first_player: u8 },
    EndTurn,
    CommitRoll { commit: [u8; SECRET_LEN] },
    RevealRoll { secret: [u8; SECRET_LEN] },
    Pass,
    Pick(Pick),
    Activate { source: u32, ability: u8 },
    SetMode { mode: Mode },
    FreeTable,
    Concede,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NotStarted,
    AlreadyStarted,
    NoSuchSeat,
    NotYourTurn,
    ShowdownOpen,
    NoShowdown,
    NotYourFocus,
    BadEvent,
    RollFirst,
    NoRoll,
    NotTheWinner,
    Dice(DiceRefusal),
    NotEnoughRunes { needed: u8, ready: u8 },
    NoPowerOf,
    UnitsPlayToBase,
    Exhausted,
    PromptOpen,
    NoPrompt,
    Pick(PickRefusal),
    AlreadyPicked,
    AlreadyFree,
    AlreadyConceded,
    GameOver { winner: u8 },
    Illegal(engine::legal::Reason),
}

impl Refusal {
    pub fn label(self) -> String {
        match self {
            Refusal::NotStarted => "the game has not started".into(),
            Refusal::AlreadyStarted => "the game already started".into(),
            Refusal::NoSuchSeat => "no such seat".into(),
            Refusal::NotYourTurn => "it is not your turn".into(),
            Refusal::ShowdownOpen => "a showdown is open".into(),
            Refusal::NoShowdown => "nothing to pass: no showdown is open".into(),
            Refusal::NotYourFocus => "you do not have focus".into(),
            Refusal::BadEvent => "unreadable turn event".into(),
            Refusal::RollFirst => "roll for first player before starting".into(),
            Refusal::NoRoll => "no roll is open".into(),
            Refusal::NotTheWinner => "the roll winner chooses the mode and who goes first".into(),
            Refusal::Dice(refusal) => refusal.label().into(),
            Refusal::NotEnoughRunes { needed, ready } => {
                format!("not enough runes to pay for that: {needed} needed, {ready} ready")
            }
            Refusal::NoPowerOf => "no rune of the domain that play needs".into(),
            Refusal::UnitsPlayToBase => {
                "units are played to your base, then attack from there".into()
            }
            Refusal::Exhausted => "an exhausted unit cannot move".into(),
            Refusal::PromptOpen => "answer the open question first".into(),
            Refusal::NoPrompt => "no question is open".into(),
            Refusal::Pick(refusal) => refusal.label(),
            Refusal::AlreadyPicked => "that is already picked".into(),
            Refusal::AlreadyFree => "the table is already free".into(),
            Refusal::AlreadyConceded => "you already conceded".into(),
            Refusal::GameOver { winner } => {
                format!("the game is over: {{seat {winner}}} won · free the table to keep playing")
            }
            Refusal::Illegal(reason) => reason.label().into(),
        }
    }
}

impl TurnEvent {
    pub fn encode(self) -> Vec<u8> {
        match self {
            TurnEvent::StartGame { first_player } => vec![0, first_player],
            TurnEvent::EndTurn => vec![2],
            TurnEvent::CommitRoll { commit } => {
                let mut out = vec![6];
                out.extend(commit);
                out
            }
            TurnEvent::RevealRoll { secret } => {
                let mut out = vec![7];
                out.extend(secret);
                out
            }
            TurnEvent::Pass => vec![9],
            TurnEvent::Pick(pick) => {
                let mut out = vec![10];
                out.extend(pick.encode());
                out
            }
            TurnEvent::Activate { source, ability } => {
                let mut out = vec![11];
                out.extend(source.to_le_bytes());
                out.push(ability);
                out
            }
            TurnEvent::SetMode { mode } => vec![12, mode.code()],
            TurnEvent::FreeTable => vec![13],
            TurnEvent::Concede => vec![14],
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0, first_player] => Some(TurnEvent::StartGame {
                first_player: *first_player,
            }),
            [2] => Some(TurnEvent::EndTurn),
            [6, rest @ ..] => Some(TurnEvent::CommitRoll {
                commit: rest.try_into().ok()?,
            }),
            [7, rest @ ..] => Some(TurnEvent::RevealRoll {
                secret: rest.try_into().ok()?,
            }),
            [9] => Some(TurnEvent::Pass),
            [10, rest @ ..] => Some(TurnEvent::Pick(Pick::decode(rest)?)),
            [11, s0, s1, s2, s3, ability] => Some(TurnEvent::Activate {
                source: u32::from_le_bytes([*s0, *s1, *s2, *s3]),
                ability: *ability,
            }),
            [12, mode] => Some(TurnEvent::SetMode {
                mode: Mode::from_code(*mode)?,
            }),
            [13] => Some(TurnEvent::FreeTable),
            [14] => Some(TurnEvent::Concede),
            _ => None,
        }
    }

    pub fn commit_prefix() -> Vec<u8> {
        vec![6]
    }

    pub fn reveal_prefix() -> Vec<u8> {
        vec![7]
    }
}

pub fn options(state: &GameBlob, table: &Snapshot, prompt: &Prompt) -> Vec<Opt> {
    match state.why {
        Some(why) => {
            let mut blob = state.clone();
            let scripts = cards::Resolved::of(table);
            let ctx = engine::ctx::Ctx::fresh(table, &mut blob, &scripts, prompt.seat);
            engine::prompts::options(&ctx, prompt, why)
        }
        None => prompt.numbered(Vec::new()),
    }
}

impl GameBlob {
    pub fn offered(&self, table: &Snapshot) -> Option<Vec<Opt>> {
        self.prompt
            .as_ref()
            .map(|prompt| options(self, table, prompt))
    }

    pub fn apply(
        current: Option<GameBlob>,
        players: u8,
        seat: u8,
        event: TurnEvent,
    ) -> Result<GameBlob, Refusal> {
        let players = players.max(1);
        let mut state = current.unwrap_or_else(|| GameBlob::lobby(players));
        if !state.is_playing() {
            return state.apply_in_lobby(players, seat, event);
        }
        if let Some(core) = state.core_mut() {
            core.players = players;
        }
        match event {
            TurnEvent::StartGame { .. }
            | TurnEvent::CommitRoll { .. }
            | TurnEvent::RevealRoll { .. }
            | TurnEvent::SetMode { .. } => Err(Refusal::AlreadyStarted),
            TurnEvent::EndTurn => {
                if !state.is_turn_player(seat) {
                    return Err(Refusal::NotYourTurn);
                }
                if state.showdown.is_some() {
                    return Err(Refusal::ShowdownOpen);
                }
                if state.prompt.is_some() {
                    return Err(Refusal::PromptOpen);
                }
                if let Some(core) = state.core_mut() {
                    core.advance();
                }
                state.clear_scored();
                if let Some(proposer) = state.free_table.take() {
                    state.narrate(format!(
                        "{{seat {proposer}}}'s free table proposal expired with the turn"
                    ));
                }
                Ok(state)
            }
            TurnEvent::Concede => {
                if seat >= players {
                    return Err(Refusal::NoSuchSeat);
                }
                if !state.concede(seat) {
                    return Err(Refusal::AlreadyConceded);
                }
                state.narrate(format!("{{seat {seat}}} concedes"));
                if let Some(winner) = state.conceded_winner() {
                    state.narrate(format!("{{seat {winner}}} wins by concession"));
                }
                Ok(state)
            }
            TurnEvent::Pass => {
                let mut showdown = state.showdown.clone().ok_or(Refusal::NoShowdown)?;
                if !showdown.window.has_focus(seat) {
                    return Err(Refusal::NotYourFocus);
                }
                state.showdown = match showdown.window.pass(&state.order()) {
                    Window::Closed => None,
                    Window::Open => Some(showdown),
                };
                Ok(state)
            }
            TurnEvent::Pick(pick) => {
                let offered = state
                    .offered(&Snapshot::default())
                    .ok_or(Refusal::NoPrompt)?;
                let mut prompt = state.prompt.take().ok_or(Refusal::NoPrompt)?;
                let answer = prompt
                    .resolve(seat, pick, &offered)
                    .map_err(Refusal::Pick)?;
                match prompt.answer(answer) {
                    None => return Err(Refusal::AlreadyPicked),
                    Some(true) => {}
                    Some(false) => state.prompt = Some(prompt),
                }
                Ok(state)
            }
            TurnEvent::Activate { .. } => Ok(state),
            TurnEvent::FreeTable => {
                if state.mode == Mode::Free {
                    return Err(Refusal::AlreadyFree);
                }
                match state.free_table {
                    None => {
                        if !state.is_turn_player(seat) {
                            return Err(Refusal::NotYourTurn);
                        }
                        state.free_table = Some(seat);
                    }
                    Some(proposer) if proposer == seat => {
                        state.free_table = None;
                        state.narrate(format!("{{seat {seat}}} withdraws the free table proposal"));
                    }
                    Some(proposer) => {
                        state.free_table = None;
                        state.mode = Mode::Free;
                        state.narrate(format!(
                            "{{seat {proposer}}} and {{seat {seat}}} freed the table"
                        ));
                    }
                }
                Ok(state)
            }
        }
    }

    fn apply_in_lobby(
        mut self,
        players: u8,
        seat: u8,
        event: TurnEvent,
    ) -> Result<GameBlob, Refusal> {
        let mut roll = self
            .lobby
            .take()
            .unwrap_or_else(|| Roll::new(players, DIE_SIDES));
        match event {
            TurnEvent::CommitRoll { commit } => {
                if roll.players() != players {
                    roll = Roll::new(players, DIE_SIDES);
                }
                roll.commit(seat, commit).map_err(Refusal::Dice)?;
                self.lobby = Some(roll);
                Ok(self)
            }
            TurnEvent::RevealRoll { secret } => {
                roll.reveal(seat, secret).map_err(Refusal::Dice)?;
                self.lobby = Some(match roll.outcome() {
                    Outcome::Tie(_) => roll.again(),
                    _ => roll,
                });
                Ok(self)
            }
            TurnEvent::SetMode { mode } => {
                winner_only(&roll, seat)?;
                self.mode = mode;
                self.lobby = Some(roll);
                Ok(self)
            }
            TurnEvent::StartGame { first_player } => {
                winner_only(&roll, seat)?;
                if first_player >= players {
                    return Err(Refusal::NoSuchSeat);
                }
                Ok(GameBlob::start(players, first_player, self.mode))
            }
            _ => Err(Refusal::NotStarted),
        }
    }
}

fn winner_only(roll: &Roll, seat: u8) -> Result<(), Refusal> {
    match roll.outcome() {
        Outcome::Winner(winner) if winner == seat => Ok(()),
        Outcome::Winner(_) => Err(Refusal::NotTheWinner),
        _ => Err(Refusal::RollFirst),
    }
}

pub fn decide(request: &decide::Request) -> Result<Verdict, Refusal> {
    let current = GameBlob::decode(&request.plugin_state);
    let table = &request.table;
    if let Action::Game(bytes) = &request.action {
        if agni_plugin_sdk::manual::Command::recognizes(bytes) {
            return manual::command(request, current, bytes);
        }
    }
    if let Some(blob) = current.as_ref().filter(|blob| blob.manual) {
        return manual::decide(request, blob.clone());
    }
    if let Some(blob) = current
        .clone()
        .filter(|blob| blob.is_playing() && blob.is_enforced())
    {
        return engine::decide(request, blob);
    }
    match &request.action {
        Action::Game(data) => {
            let event = TurnEvent::decode(data).ok_or(Refusal::BadEvent)?;
            let before = current.clone().filter(GameBlob::is_playing);
            let current = current.or_else(|| {
                Some(GameBlob::lobby_in(
                    request.players,
                    rules::Options::of(table).starting_mode(),
                ))
            });
            let mut next = GameBlob::apply(current, request.players, request.seat, event)?;
            if !next.is_playing() {
                return Ok(Verdict::advance(next.encode()));
            }
            if next.is_enforced() && before.is_none() {
                return engine::start(request, next);
            }
            let effects = choreograph(table, before.as_ref(), &mut next, event, request.seat);
            Ok(Verdict::advance(next.encode()).with_effects(effects))
        }
        Action::Move { card, to, .. } => {
            let Some(mut state) = current.filter(GameBlob::is_playing) else {
                return Ok(Verdict::accept());
            };
            let mut effects = Vec::new();
            let mut changed = false;
            let was_open = state.showdown.is_some();
            if let (Some(moving), Some(to)) = (table.card(*card), *to) {
                if moving.owner == request.seat
                    && rules::is_unit_play_onto_a_battlefield(table, moving, to)
                {
                    return Err(Refusal::UnitsPlayToBase);
                }
                if moving.owner == request.seat && rules::is_play(table, moving, to) {
                    effects.extend(rules::pay(table, request.seat, moving).map_err(|refusal| {
                        match refusal {
                            rules::CostRefusal::NotEnoughRunes { needed, ready } => {
                                Refusal::NotEnoughRunes { needed, ready }
                            }
                            rules::CostRefusal::NoPowerOf { .. } => Refusal::NoPowerOf,
                        }
                    })?);
                    if rules::is_unit_play(table, moving, to) {
                        effects.push(Effect::exhaust(*card));
                    }
                }
                if moving.owner == request.seat && rules::is_march(table, moving, to) {
                    if moving.exhausted {
                        return Err(Refusal::Exhausted);
                    }
                    effects.push(Effect::exhaust(*card));
                }
                if rules::is_unit(moving) {
                    let before = state.clone();
                    rules::arrival(table, &mut state, request.seat, to);
                    changed |= state != before;
                }
            }
            if was_open || state.showdown.is_none() {
                changed |= state.note_play(request.seat);
            }
            Ok(if changed {
                Verdict::advance(state.encode()).with_effects(effects)
            } else {
                Verdict::accept().with_effects(effects)
            })
        }
        Action::Spawn { face, .. } => {
            if current.as_ref().is_some_and(GameBlob::is_enforced) {
                return Err(Refusal::Illegal(engine::legal::Reason::TokensByEffect));
            }
            Ok(Verdict::accept().with_effects(rules::on_spawn(table, &face.name)))
        }
        Action::Reset => Ok(Verdict::advance(Vec::new())),
        Action::Join => {
            let joined = request.players.saturating_add(1);
            match current {
                Some(state) if !state.is_playing() && state.players() != joined => {
                    Ok(Verdict::advance(
                        GameBlob {
                            mode: state.mode,
                            ..GameBlob::lobby(joined)
                        }
                        .encode(),
                    ))
                }
                _ => Ok(Verdict::accept()),
            }
        }
        _ => Ok(Verdict::accept()),
    }
}

fn choreograph(
    table: &Snapshot,
    before: Option<&GameBlob>,
    state: &mut GameBlob,
    event: TurnEvent,
    actor: u8,
) -> Vec<Effect> {
    match event {
        TurnEvent::StartGame { .. } => rules::begin_turn(table, state),
        TurnEvent::EndTurn => {
            let mut effects = Vec::new();
            let mut after = table.clone();
            if let Some(ending) = before {
                let mut settled = ending.clone();
                effects.extend(rules::resolve_all(table, &mut settled));
                state.control = settled.control;
                state.clear_scored();
                if after.apply_all(&effects, actor).is_err() {
                    after = table.clone();
                }
            }
            effects.extend(rules::begin_turn(&after, state));
            effects
        }
        TurnEvent::Pass => match (before.and_then(|was| was.showdown.clone()), &state.showdown) {
            (Some(showdown), None) => rules::close_showdown(table, state, showdown),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub fn decide_bytes(request: &[u8]) -> Vec<u8> {
    match decide::parse(request) {
        Some(request) => {
            decide(&request).unwrap_or_else(|refusal| Verdict::refuse(refusal.label()))
        }
        None => Verdict::refuse(UNREADABLE_REQUEST),
    }
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_plugin_sdk::turns::PassWindow;

    fn started() -> GameBlob {
        GameBlob::start(2, 0, Mode::Free)
    }

    fn with_showdown(mut state: GameBlob, focus: u8, passes: u8) -> GameBlob {
        state.showdown = Some(Showdown {
            window: PassWindow { focus, passes },
            ..Showdown::open(9, 0, 1)
        });
        state
    }

    #[test]
    fn every_event_round_trips_and_junk_is_refused() {
        for event in [
            TurnEvent::StartGame { first_player: 1 },
            TurnEvent::EndTurn,
            TurnEvent::CommitRoll { commit: [7; 8] },
            TurnEvent::RevealRoll { secret: [9; 8] },
            TurnEvent::Pass,
            TurnEvent::Pick(Pick {
                prompt: 258,
                option: 3,
            }),
            TurnEvent::Activate {
                source: 70_000,
                ability: 2,
            },
            TurnEvent::SetMode {
                mode: Mode::Enforced,
            },
            TurnEvent::SetMode { mode: Mode::Free },
            TurnEvent::FreeTable,
            TurnEvent::Concede,
        ] {
            assert_eq!(TurnEvent::decode(&event.encode()), Some(event));
        }
        assert_eq!(
            TurnEvent::Pick(Pick {
                prompt: 258,
                option: 3
            })
            .encode(),
            [10, 2, 1, 3, 0]
        );
        assert_eq!(
            TurnEvent::Activate {
                source: 70_000,
                ability: 2
            }
            .encode(),
            [11, 0x70, 0x11, 0x01, 0x00, 2]
        );
        assert_eq!(
            TurnEvent::SetMode {
                mode: Mode::Enforced
            }
            .encode(),
            [12, 1]
        );
        assert_eq!(TurnEvent::decode(&[]), None);
        assert_eq!(TurnEvent::decode(&[7]), None);
        assert_eq!(TurnEvent::decode(&[2, 1]), None);
        assert_eq!(TurnEvent::decode(&[10, 1, 2, 3]), None);
        assert_eq!(TurnEvent::decode(&[11, 1, 2, 3, 4]), None);
        assert_eq!(TurnEvent::decode(&[12, 2]), None);
        for retired in [1u8, 3, 4, 5, 8] {
            assert_eq!(TurnEvent::decode(&[retired]), None);
            assert_eq!(TurnEvent::decode(&[retired, 0, 0, 1]), None);
        }
        assert_eq!(TurnEvent::commit_prefix(), vec![6]);
        assert_eq!(TurnEvent::reveal_prefix(), vec![7]);
    }

    #[test]
    fn a_turn_ends_for_the_turn_player_only_and_hands_over() {
        let state = started();
        assert_eq!(
            GameBlob::apply(Some(state.clone()), 2, 1, TurnEvent::EndTurn),
            Err(Refusal::NotYourTurn)
        );
        let mut marked = state.clone();
        marked.mark_scored(9, 0);
        let next = GameBlob::apply(Some(marked), 2, 0, TurnEvent::EndTurn).unwrap();
        assert_eq!((next.turn(), next.turn_player()), (2, 1));
        assert_eq!(next.phase(), Some(Phase::Action));
        assert!(!next.scored(9, 0));
        let around = GameBlob::apply(Some(next), 2, 1, TurnEvent::EndTurn).unwrap();
        assert_eq!(around.turn_player(), 0);
        assert_eq!(
            GameBlob::apply(
                Some(state.clone()),
                2,
                0,
                TurnEvent::StartGame { first_player: 0 }
            ),
            Err(Refusal::AlreadyStarted)
        );
        assert_eq!(
            GameBlob::apply(
                Some(state.clone()),
                2,
                0,
                TurnEvent::SetMode {
                    mode: Mode::Enforced
                }
            ),
            Err(Refusal::AlreadyStarted)
        );
        assert_eq!(
            GameBlob::apply(Some(state.clone()), 2, 0, TurnEvent::Pass),
            Err(Refusal::NoShowdown)
        );
        let mut asked = state;
        asked.prompt = Some(Prompt::new(1, 0, 1, 1));
        assert_eq!(
            GameBlob::apply(Some(asked), 2, 0, TurnEvent::EndTurn),
            Err(Refusal::PromptOpen)
        );
    }

    #[test]
    fn a_showdown_closes_when_everyone_passes_in_sequence() {
        let open = with_showdown(started(), 0, 0);
        assert_eq!(
            GameBlob::apply(Some(open.clone()), 2, 0, TurnEvent::EndTurn),
            Err(Refusal::ShowdownOpen)
        );
        assert_eq!(
            GameBlob::apply(Some(open.clone()), 2, 1, TurnEvent::Pass),
            Err(Refusal::NotYourFocus)
        );
        let passed_once = GameBlob::apply(Some(open), 2, 0, TurnEvent::Pass).unwrap();
        assert_eq!(passed_once.showdown.as_ref().unwrap().focus(), 1);
        assert_eq!(passed_once.showdown.as_ref().unwrap().passes(), 1);
        let closed = GameBlob::apply(Some(passed_once), 2, 1, TurnEvent::Pass).unwrap();
        assert_eq!(closed.showdown, None);
        assert_eq!(closed.turn_player(), 0);
    }

    #[test]
    fn a_play_by_the_focus_holder_resets_the_pass_count_and_hands_focus_on() {
        let mut open = with_showdown(started(), 1, 1);
        assert!(!open.note_play(0));
        assert_eq!(open.showdown.as_ref().unwrap().passes(), 1);
        assert!(open.note_play(1));
        let showdown = open.showdown.clone().unwrap();
        assert_eq!((showdown.focus(), showdown.passes()), (0, 0));
        let mut quiet = started();
        assert!(!quiet.note_play(0));
    }

    #[test]
    fn focus_walks_every_seat_in_turn_order_at_four_players() {
        let mut state = GameBlob::start(4, 2, Mode::Free);
        state.showdown = Some(Showdown::open(11, 2, 0));
        let mut order = Vec::new();
        for _ in 0..3 {
            let focus = state.showdown.as_ref().unwrap().focus();
            order.push(focus);
            state = GameBlob::apply(Some(state), 4, focus, TurnEvent::Pass).unwrap();
        }
        assert_eq!(order, [2, 3, 0]);
        assert_eq!(state.showdown.as_ref().unwrap().focus(), 1);
        let closed = GameBlob::apply(Some(state), 4, 1, TurnEvent::Pass).unwrap();
        assert_eq!(closed.showdown, None);
    }

    #[test]
    fn a_pick_answers_the_open_prompt_and_stale_or_foreign_picks_are_refused() {
        assert_eq!(
            GameBlob::apply(
                Some(started()),
                2,
                0,
                TurnEvent::Pick(Pick {
                    prompt: 1,
                    option: 0
                })
            ),
            Err(Refusal::NoPrompt)
        );
        let mut asked = started();
        asked.prompt = Some(Prompt::new(4, 1, 0, 1));
        let offered = options(&asked, &Snapshot::default(), asked.prompt.as_ref().unwrap());
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].label, "skip");
        assert_eq!(
            GameBlob::apply(
                Some(asked.clone()),
                2,
                1,
                TurnEvent::Pick(Pick {
                    prompt: 3,
                    option: 0
                })
            ),
            Err(Refusal::Pick(PickRefusal::Stale { open: 4, sent: 3 }))
        );
        assert_eq!(
            GameBlob::apply(
                Some(asked.clone()),
                2,
                0,
                TurnEvent::Pick(Pick {
                    prompt: 4,
                    option: 0
                })
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        assert_eq!(
            GameBlob::apply(
                Some(asked.clone()),
                2,
                1,
                TurnEvent::Pick(Pick {
                    prompt: 4,
                    option: 5
                })
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 5,
                count: 1
            }))
        );
        let answered = GameBlob::apply(
            Some(asked),
            2,
            1,
            TurnEvent::Pick(Pick {
                prompt: 4,
                option: 0,
            }),
        )
        .unwrap();
        assert_eq!(answered.prompt, None);
        let mut shown = started();
        shown.prompt = Some(Prompt::new(5, 0, 0, 1));
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: shown.encode(),
            players: 2,
            seat: 0,
            zones: Vec::new(),
            table: Snapshot {
                players: 2,
                ..Default::default()
            },
        });
        let offered = shown.offered(&Snapshot::default()).unwrap();
        assert_eq!(strip.affordances.len(), offered.len() + 2);
        assert_eq!(
            strip.hidden,
            [offered.len() as u16, offered.len() as u16 + 1]
        );
        assert_eq!(strip.affordances[offered.len()].label, present::CONCEDE);
        assert_eq!(
            strip.affordances.last().unwrap().label,
            agni_plugin_sdk::manual::DISABLE
        );
        for (index, affordance) in strip.affordances[..offered.len()].iter().enumerate() {
            assert_eq!(affordance.label, offered[index].label);
            let event = TurnEvent::decode(&affordance.data).unwrap();
            let TurnEvent::Pick(pick) = event else {
                panic!("a prompt affordance is a pick: {event:?}");
            };
            assert_eq!(usize::from(pick.option), index);
            let closed = GameBlob::apply(Some(shown.clone()), 2, 0, event).unwrap();
            assert_eq!(closed.prompt, None);
        }
        let ignored = GameBlob::apply(
            Some(started()),
            2,
            0,
            TurnEvent::Activate {
                source: 5,
                ability: 0,
            },
        )
        .unwrap();
        assert_eq!(ignored, started());
    }

    #[test]
    fn the_free_table_needs_the_turn_player_and_a_second_seat() {
        assert_eq!(
            GameBlob::apply(Some(started()), 2, 0, TurnEvent::FreeTable),
            Err(Refusal::AlreadyFree)
        );
        let enforced = GameBlob::start(2, 0, Mode::Enforced);
        assert_eq!(
            GameBlob::apply(Some(enforced.clone()), 2, 1, TurnEvent::FreeTable),
            Err(Refusal::NotYourTurn)
        );
        let proposed = GameBlob::apply(Some(enforced), 2, 0, TurnEvent::FreeTable).unwrap();
        assert_eq!(proposed.free_table, Some(0));
        assert_eq!(proposed.mode, Mode::Enforced);
        let withdrawn =
            GameBlob::apply(Some(proposed.clone()), 2, 0, TurnEvent::FreeTable).unwrap();
        assert_eq!(withdrawn.free_table, None);
        assert_eq!(withdrawn.mode, Mode::Enforced);
        assert_eq!(
            withdrawn.log,
            ["{seat 0} withdraws the free table proposal"]
        );
        let expired = GameBlob::apply(Some(proposed.clone()), 2, 0, TurnEvent::EndTurn).unwrap();
        assert_eq!(expired.free_table, None);
        assert_eq!(expired.turn_player(), 1);
        assert_eq!(
            expired.log,
            ["{seat 0}'s free table proposal expired with the turn"]
        );
        let freed = GameBlob::apply(Some(proposed), 2, 1, TurnEvent::FreeTable).unwrap();
        assert_eq!(freed.mode, Mode::Free);
        assert_eq!(freed.free_table, None);
        assert_eq!(freed.log, ["{seat 0} and {seat 1} freed the table"]);
    }

    #[test]
    fn a_concession_hands_the_game_to_the_last_seat_standing() {
        let started = GameBlob::start(2, 0, Mode::Free);
        assert_eq!(
            GameBlob::apply(Some(started.clone()), 2, 5, TurnEvent::Concede),
            Err(Refusal::NoSuchSeat)
        );
        let conceded = GameBlob::apply(Some(started), 2, 1, TurnEvent::Concede).unwrap();
        assert_eq!(conceded.conceded, [1]);
        assert_eq!(conceded.conceded_winner(), Some(0));
        assert_eq!(
            conceded.log,
            ["{seat 1} concedes", "{seat 0} wins by concession"]
        );
        assert_eq!(
            GameBlob::apply(Some(conceded), 2, 1, TurnEvent::Concede),
            Err(Refusal::AlreadyConceded)
        );
        assert_eq!(
            GameBlob::apply(None, 2, 0, TurnEvent::Concede),
            Err(Refusal::NotStarted)
        );
    }

    #[test]
    fn every_refusal_reads_as_a_sentence() {
        for refusal in [
            Refusal::NotStarted,
            Refusal::AlreadyStarted,
            Refusal::NoSuchSeat,
            Refusal::NotYourTurn,
            Refusal::ShowdownOpen,
            Refusal::NoShowdown,
            Refusal::NotYourFocus,
            Refusal::BadEvent,
            Refusal::RollFirst,
            Refusal::NotTheWinner,
            Refusal::Dice(DiceRefusal::NotEveryoneCommitted),
            Refusal::NotEnoughRunes {
                needed: 3,
                ready: 1,
            },
            Refusal::NoPowerOf,
            Refusal::UnitsPlayToBase,
            Refusal::Exhausted,
            Refusal::PromptOpen,
            Refusal::NoPrompt,
            Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }),
            Refusal::AlreadyPicked,
            Refusal::AlreadyFree,
            Refusal::AlreadyConceded,
            Refusal::GameOver { winner: 1 },
            Refusal::Illegal(engine::legal::Reason::NotYourCard),
        ] {
            assert!(refusal.label().len() > 8, "{refusal:?}");
        }
        assert_eq!(
            Refusal::NotEnoughRunes {
                needed: 3,
                ready: 1
            }
            .label(),
            "not enough runes to pay for that: 3 needed, 1 ready"
        );
    }
}

#[cfg(test)]
mod lobby_tests {
    use super::*;
    use agni_plugin_sdk::dice;

    fn secret(seed: u8) -> [u8; SECRET_LEN] {
        [seed; SECRET_LEN]
    }

    fn rolled(players: u8) -> GameBlob {
        let mut state = None;
        for seat in 0..players {
            state = Some(
                GameBlob::apply(
                    state,
                    players,
                    seat,
                    TurnEvent::CommitRoll {
                        commit: dice::commitment(&secret(seat + 1)),
                    },
                )
                .unwrap(),
            );
        }
        for seat in 0..players {
            state = Some(
                GameBlob::apply(
                    state,
                    players,
                    seat,
                    TurnEvent::RevealRoll {
                        secret: secret(seat + 1),
                    },
                )
                .unwrap(),
            );
        }
        state.unwrap()
    }

    fn decided(players: u8) -> (GameBlob, u8) {
        let mut state = rolled(players);
        while state
            .roll()
            .is_some_and(|roll| roll.outcome() == Outcome::Pending)
        {
            state = rolled(players);
        }
        let Outcome::Winner(winner) = state.roll().expect("still in the lobby").outcome() else {
            panic!("a tie re-rolls into a fresh round");
        };
        (state, winner)
    }

    #[test]
    fn the_game_cannot_start_before_a_roll_decides_it() {
        assert_eq!(
            GameBlob::apply(None, 2, 0, TurnEvent::StartGame { first_player: 0 }),
            Err(Refusal::RollFirst)
        );
        assert_eq!(
            GameBlob::apply(
                None,
                2,
                0,
                TurnEvent::SetMode {
                    mode: Mode::Enforced
                }
            ),
            Err(Refusal::RollFirst)
        );
        for event in [
            TurnEvent::EndTurn,
            TurnEvent::Pass,
            TurnEvent::FreeTable,
            TurnEvent::Concede,
            TurnEvent::Activate {
                source: 1,
                ability: 0,
            },
            TurnEvent::Pick(Pick {
                prompt: 0,
                option: 0,
            }),
        ] {
            assert_eq!(
                GameBlob::apply(None, 2, 0, event),
                Err(Refusal::NotStarted),
                "{event:?}"
            );
        }
        let committed = GameBlob::apply(
            None,
            2,
            1,
            TurnEvent::CommitRoll {
                commit: dice::commitment(&secret(1)),
            },
        )
        .unwrap();
        assert!(committed.roll().unwrap().hands[1].commit.is_some());
        assert_eq!(
            GameBlob::apply(
                Some(committed.clone()),
                2,
                1,
                TurnEvent::RevealRoll { secret: secret(1) }
            ),
            Err(Refusal::Dice(DiceRefusal::NotEveryoneCommitted))
        );
        assert_eq!(GameBlob::decode(&committed.encode()), Some(committed));
    }

    #[test]
    fn only_the_roll_winner_starts_and_they_pick_either_seat() {
        let (state, winner) = decided(2);
        let loser = 1 - winner;
        assert_eq!(
            GameBlob::apply(
                Some(state.clone()),
                2,
                loser,
                TurnEvent::StartGame {
                    first_player: loser
                }
            ),
            Err(Refusal::NotTheWinner)
        );
        assert_eq!(
            GameBlob::apply(
                Some(state.clone()),
                2,
                winner,
                TurnEvent::StartGame { first_player: 2 }
            ),
            Err(Refusal::NoSuchSeat)
        );
        let started = GameBlob::apply(
            Some(state),
            2,
            winner,
            TurnEvent::StartGame {
                first_player: loser,
            },
        )
        .unwrap();
        assert_eq!(started.turn_player(), loser);
        assert_eq!(started.core().unwrap().first, loser);
        assert_eq!(started.mode, Mode::Free);
        assert_eq!(started.lobby, None);
        assert_eq!(
            GameBlob::apply(
                Some(started),
                2,
                winner,
                TurnEvent::CommitRoll {
                    commit: [0; SECRET_LEN]
                }
            ),
            Err(Refusal::AlreadyStarted)
        );
    }

    #[test]
    fn the_winner_picks_the_mode_before_starting() {
        let (state, winner) = decided(2);
        let loser = 1 - winner;
        assert_eq!(
            GameBlob::apply(
                Some(state.clone()),
                2,
                loser,
                TurnEvent::SetMode {
                    mode: Mode::Enforced
                }
            ),
            Err(Refusal::NotTheWinner)
        );
        let enforced = GameBlob::apply(
            Some(state),
            2,
            winner,
            TurnEvent::SetMode {
                mode: Mode::Enforced,
            },
        )
        .unwrap();
        assert_eq!(enforced.mode, Mode::Enforced);
        assert!(enforced.roll().is_some());
        assert_eq!(GameBlob::decode(&enforced.encode()), Some(enforced.clone()));
        let started = GameBlob::apply(
            Some(enforced),
            2,
            winner,
            TurnEvent::StartGame {
                first_player: winner,
            },
        )
        .unwrap();
        assert_eq!(started.mode, Mode::Enforced);
        assert_eq!(
            GameBlob::decode(&started.encode()).unwrap().mode,
            Mode::Enforced
        );
    }

    #[test]
    fn a_tie_opens_a_new_round_automatically() {
        let mut state = GameBlob {
            lobby: Some(Roll::new(2, 2)),
            ..GameBlob::default()
        };
        let mut rounds = 0;
        loop {
            let round = state.roll().unwrap().round;
            for seat in 0..2u8 {
                state = GameBlob::apply(
                    Some(state),
                    2,
                    seat,
                    TurnEvent::CommitRoll {
                        commit: dice::commitment(&secret(seat + 40)),
                    },
                )
                .unwrap();
            }
            for seat in 0..2u8 {
                state = GameBlob::apply(
                    Some(state),
                    2,
                    seat,
                    TurnEvent::RevealRoll {
                        secret: secret(seat + 40),
                    },
                )
                .unwrap();
            }
            let roll = state.roll().unwrap();
            if roll.all_revealed() {
                assert!(matches!(roll.outcome(), Outcome::Winner(_)));
                break;
            }
            assert_eq!(roll.round, round + 1);
            rounds += 1;
            assert!(rounds < 64, "a coin must decide eventually");
        }
    }

    #[test]
    fn roll_events_round_trip_and_short_ones_are_refused() {
        let commit = TurnEvent::CommitRoll {
            commit: [7; SECRET_LEN],
        };
        assert_eq!(TurnEvent::decode(&commit.encode()), Some(commit));
        let reveal = TurnEvent::RevealRoll {
            secret: [9; SECRET_LEN],
        };
        assert_eq!(TurnEvent::decode(&reveal.encode()), Some(reveal));
        assert_eq!(TurnEvent::decode(&[6, 1, 2]), None);
        assert_eq!(TurnEvent::commit_prefix(), vec![6]);
    }
}

#[cfg(test)]
mod decide_tests {
    use super::*;
    use agni_plugin_sdk::cbor::Writer;
    use agni_plugin_sdk::turns::PassWindow;

    fn request(
        plugin_state: &[u8],
        players: usize,
        seat: u8,
        action: &dyn Fn(&mut Writer),
    ) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(plugin_state);
        writer.text("state");
        writer.map(1);
        writer.text("seats");
        writer.array(players);
        for seat in 0..players {
            writer.map(1);
            writer.text("seat");
            writer.unsigned(seat as u64);
        }
        writer.text("entry");
        writer.map(2);
        writer.text("seat");
        writer.unsigned(seat as u64);
        writer.text("action");
        action(&mut writer);
        writer.finish()
    }

    fn game(data: &[u8]) -> impl Fn(&mut Writer) + '_ {
        move |writer| {
            writer.map(1);
            writer.text("Game");
            writer.map(1);
            writer.text("data");
            writer.bytes(data);
        }
    }

    fn bare(variant: &'static str) -> impl Fn(&mut Writer) {
        move |writer| {
            writer.text(variant);
        }
    }

    fn join(writer: &mut Writer) {
        writer.map(1);
        writer.text("Join");
        writer.map(1);
        writer.text("name");
        writer.text("ada");
    }

    fn a_move(writer: &mut Writer) {
        writer.map(1);
        writer.text("Move");
        writer.map(4);
        writer.text("card");
        writer.unsigned(3);
        writer.text("to");
        writer.map(1);
        writer.text("Plugin");
        writer.unsigned(8);
        writer.text("seat");
        writer.unsigned(0);
        writer.text("index");
        writer.unsigned(0);
    }

    fn verdict_of(bytes: &[u8]) -> Verdict {
        decide(&decide::parse(bytes).unwrap()).unwrap()
    }

    #[test]
    fn an_out_of_turn_event_and_an_unreadable_request_are_refused_with_a_reason() {
        let state = GameBlob::start(2, 0, Mode::Free).encode();
        let bytes = request(&state, 2, 1, &game(&TurnEvent::EndTurn.encode()));
        assert_eq!(
            decide_bytes(&bytes),
            Verdict::refuse(Refusal::NotYourTurn.label()).encode()
        );
        assert_eq!(
            decide_bytes(&request(&state, 2, 0, &game(&[42]))),
            Verdict::refuse(Refusal::BadEvent.label()).encode()
        );
        assert_eq!(
            decide_bytes(&request(&state, 2, 0, &game(&[1]))),
            Verdict::refuse(Refusal::BadEvent.label()).encode()
        );
        assert_eq!(
            decide_bytes(&[0xa1, 0x61]),
            Verdict::refuse(UNREADABLE_REQUEST).encode()
        );
        let encoded = decide_bytes(&bytes);
        assert!(encoded.windows(6).any(|window| window == b"reason"));
    }

    #[test]
    fn a_reset_returns_a_fresh_lobby_and_a_join_rebuilds_the_roll() {
        let playing = GameBlob::start(2, 0, Mode::Enforced).encode();
        let reset = verdict_of(&request(&playing, 2, 0, &bare("Reset")));
        assert!(reset.accept);
        assert_eq!(reset.plugin_state.as_deref(), Some(&[][..]));
        assert!(GameBlob::decode(&[]).is_none());
        let mut lobby = GameBlob::lobby(1);
        lobby.mode = Mode::Enforced;
        let joined = verdict_of(&request(&lobby.encode(), 1, 1, &join));
        let rebuilt = GameBlob::decode(joined.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(rebuilt.players(), 2);
        assert_eq!(rebuilt.mode, Mode::Enforced);
        assert!(rebuilt
            .roll()
            .unwrap()
            .hands
            .iter()
            .all(|hand| hand.commit.is_none()));
        let already = verdict_of(&request(&GameBlob::lobby(2).encode(), 1, 1, &join));
        assert_eq!(already.plugin_state, None);
        let mid_game = verdict_of(&request(&playing, 2, 2, &join));
        assert_eq!(mid_game.plugin_state, None);
        let fresh = verdict_of(&request(&[], 1, 1, &join));
        assert_eq!(fresh.plugin_state, None);
    }

    fn request_with_cards(
        plugin_state: &[u8],
        seat: u8,
        cards: &[(u64, u64, u64, &str, &str)],
        action: &dyn Fn(&mut Writer),
    ) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(plugin_state);
        writer.text("state");
        writer.map(3);
        writer.text("seats");
        writer.array(2);
        for seat in 0..2u64 {
            writer.map(1);
            writer.text("seat");
            writer.unsigned(seat);
        }
        writer.text("zones");
        writer.array(3);
        for (id, name, kind, owner) in [
            (0, "hand", "Hand", "PerSeat"),
            (1, "main-deck", "Deck", "PerSeat"),
            (10, "battlefield-2", "Battlefield", "Shared"),
        ] {
            writer.map(5);
            writer.text("id");
            writer.unsigned(id);
            writer.text("name");
            writer.text(name);
            writer.text("kind");
            writer.text(kind);
            writer.text("owner");
            writer.text(owner);
            writer.text("label");
            writer.text(name);
        }
        writer.text("table");
        writer.map(1);
        writer.text("cards");
        writer.array(cards.len());
        for (id, zone, owner, name, kind) in cards {
            writer.map(5);
            writer.text("id");
            writer.unsigned(*id);
            writer.text("owner");
            writer.unsigned(*owner);
            writer.text("seat");
            writer.unsigned(if *zone >= 9 { 0 } else { *owner });
            writer.text("zone");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(*zone);
            writer.text("face");
            writer.map(4);
            writer.text("name");
            writer.text(name);
            writer.text("tint");
            writer.array(3);
            for _ in 0..3 {
                writer.unsigned(128);
            }
            writer.text("foil");
            writer.bool(false);
            writer.text("kind");
            writer.text(kind);
        }
        writer.text("entry");
        writer.map(2);
        writer.text("seat");
        writer.unsigned(u64::from(seat));
        writer.text("action");
        action(&mut writer);
        writer.finish()
    }

    #[test]
    fn closing_a_showdown_by_passing_settles_control_and_scores_the_conquer() {
        let mut open = GameBlob::start(2, 0, Mode::Free);
        open.showdown = Some(Showdown {
            window: PassWindow {
                focus: 1,
                passes: 1,
            },
            ..Showdown::open(10, 1, 0)
        });
        let bytes = request_with_cards(
            &open.encode(),
            1,
            &[
                (118, 10, 0, "Proving Grounds", "Battlefield"),
                (117, 10, 1, "Vi", "Unit"),
            ],
            &game(&TurnEvent::Pass.encode()),
        );
        let verdict = verdict_of(&bytes);
        let next = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(next.showdown, None);
        assert_eq!(next.holder(10), Some(1));
        assert!(next.scored(10, 1));
        assert_eq!(
            verdict.effects,
            [Effect::score(1, rules::COUNTER_POINTS, 1)]
        );
        assert!(verdict
            .encode()
            .windows(7)
            .any(|window| window == b"effects"));
    }

    #[test]
    fn ending_the_turn_settles_the_board_and_runs_the_next_beginning_phase() {
        let playing = GameBlob::start(2, 0, Mode::Free);
        let bytes = request_with_cards(
            &playing.encode(),
            0,
            &[
                (118, 10, 0, "Proving Grounds", "Battlefield"),
                (117, 10, 0, "Vi", "Unit"),
                (20, 1, 1, "", ""),
                (21, 1, 1, "", ""),
            ],
            &game(&TurnEvent::EndTurn.encode()),
        );
        let verdict = verdict_of(&bytes);
        let next = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!((next.turn(), next.turn_player()), (2, 1));
        assert_eq!(next.holder(10), Some(0));
        assert!(!next.scored(10, 0));
        assert_eq!(
            verdict.effects,
            [
                Effect::score(0, rules::COUNTER_POINTS, 1),
                Effect::Move {
                    card: 21,
                    zone: 0,
                    seat: 1,
                    index: agni_plugin_sdk::decide::TOP
                },
            ]
        );
        let unrolled = request_with_cards(
            &GameBlob::lobby(2).encode(),
            0,
            &[(20, 1, 0, "", "")],
            &game(&TurnEvent::StartGame { first_player: 0 }.encode()),
        );
        assert_eq!(
            decide(&decide::parse(&unrolled).unwrap()),
            Err(Refusal::RollFirst)
        );
        assert_eq!(
            decide_bytes(&unrolled),
            Verdict::refuse("roll for first player before starting").encode()
        );
    }

    fn request_with_points(
        plugin_state: &[u8],
        seat: u8,
        cards: &[(u64, u64, u64, &str, &str)],
        points: &[(u64, i64)],
        action: &dyn Fn(&mut Writer),
    ) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(plugin_state);
        writer.text("state");
        writer.map(5);
        writer.text("seats");
        writer.array(2);
        for seat in 0..2u64 {
            writer.map(1);
            writer.text("seat");
            writer.unsigned(seat);
        }
        writer.text("zones");
        writer.array(4);
        for (id, name, kind, owner) in [
            (0, "hand", "Hand", "PerSeat"),
            (1, "main-deck", "Deck", "PerSeat"),
            (9, "battlefield-1", "Battlefield", "Shared"),
            (10, "battlefield-2", "Battlefield", "Shared"),
        ] {
            writer.map(5);
            writer.text("id");
            writer.unsigned(id);
            writer.text("name");
            writer.text(name);
            writer.text("kind");
            writer.text(kind);
            writer.text("owner");
            writer.text(owner);
            writer.text("label");
            writer.text(name);
        }
        writer.text("table");
        writer.map(1);
        writer.text("cards");
        writer.array(cards.len());
        for (id, zone, owner, name, kind) in cards {
            writer.map(5);
            writer.text("id");
            writer.unsigned(*id);
            writer.text("owner");
            writer.unsigned(*owner);
            writer.text("seat");
            writer.unsigned(if *zone >= 9 { 0 } else { *owner });
            writer.text("zone");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(*zone);
            writer.text("face");
            writer.map(4);
            writer.text("name");
            writer.text(name);
            writer.text("tint");
            writer.array(3);
            for _ in 0..3 {
                writer.unsigned(128);
            }
            writer.text("foil");
            writer.bool(false);
            writer.text("kind");
            writer.text(kind);
        }
        writer.text("counters");
        writer.array(points.len());
        for (seat, value) in points {
            writer.map(3);
            writer.text("target");
            writer.map(1);
            writer.text("Seat");
            writer.unsigned(*seat);
            writer.text("counter");
            writer.unsigned(u64::from(rules::COUNTER_POINTS));
            writer.text("value");
            writer.signed(*value);
        }
        writer.text("counter_table");
        writer.array(1);
        writer.map(5);
        writer.text("id");
        writer.unsigned(u64::from(rules::COUNTER_POINTS));
        writer.text("scope");
        writer.text("Seat");
        writer.text("start");
        writer.unsigned(0);
        writer.text("min");
        writer.unsigned(0);
        writer.text("max");
        writer.null();
        writer.text("entry");
        writer.map(2);
        writer.text("seat");
        writer.unsigned(u64::from(seat));
        writer.text("action");
        action(&mut writer);
        writer.finish()
    }

    #[test]
    fn a_final_point_draw_at_the_ending_and_the_next_draw_take_different_cards() {
        let playing = GameBlob::start(2, 0, Mode::Free);
        let bytes = request_with_points(
            &playing.encode(),
            0,
            &[
                (117, 10, 1, "Vi", "Unit"),
                (20, 1, 1, "", ""),
                (21, 1, 1, "", ""),
            ],
            &[(1, rules::DEFAULT_VICTORY_SCORE as i64 - 1)],
            &game(&TurnEvent::EndTurn.encode()),
        );
        let verdict = verdict_of(&bytes);
        let next = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!((next.turn(), next.turn_player()), (2, 1));
        assert_eq!(next.holder(10), Some(1));
        assert_eq!(
            verdict.effects,
            [
                Effect::Move {
                    card: 21,
                    zone: 0,
                    seat: 1,
                    index: agni_plugin_sdk::decide::TOP
                },
                Effect::score(1, rules::COUNTER_POINTS, 1),
                Effect::Move {
                    card: 20,
                    zone: 0,
                    seat: 1,
                    index: agni_plugin_sdk::decide::TOP
                },
            ]
        );
    }

    #[test]
    fn a_deal_after_the_start_is_accepted_but_the_first_turn_has_already_drawn() {
        let playing = GameBlob::start(2, 0, Mode::Free);
        let deal = |writer: &mut Writer| {
            writer.map(1);
            writer.text("Deal");
            writer.map(2);
            writer.text("cards");
            writer.array(2);
            writer.unsigned(30);
            writer.unsigned(31);
            writer.text("to");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(1);
        };
        let verdict = verdict_of(&request(&playing.encode(), 2, 0, &deal));
        assert_eq!(verdict, Verdict::accept());
        let unrolled = request_with_cards(
            &GameBlob::lobby(2).encode(),
            0,
            &[(20, 1, 0, "", "")],
            &game(&TurnEvent::StartGame { first_player: 0 }.encode()),
        );
        assert!(decide(&decide::parse(&unrolled).unwrap()).is_err());
    }

    fn move_of(card: u64, zone: u64) -> impl Fn(&mut Writer) {
        move |writer| {
            writer.map(1);
            writer.text("Move");
            writer.map(4);
            writer.text("card");
            writer.unsigned(card);
            writer.text("to");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(zone);
            writer.text("seat");
            writer.unsigned(0);
            writer.text("index");
            writer.unsigned(0);
        }
    }

    #[test]
    fn a_unit_arriving_on_a_contested_battlefield_opens_a_showdown_with_the_attackers_focus() {
        let playing = GameBlob::start(2, 0, Mode::Free);
        let verdict = verdict_of(&request_with_cards(
            &playing.encode(),
            0,
            &[(117, 1, 0, "Vi", "Unit")],
            &move_of(117, 10),
        ));
        let next = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        let showdown = next.showdown.expect("the arrival opened a showdown");
        assert_eq!(
            (
                showdown.attacker,
                showdown.defender,
                showdown.focus(),
                showdown.passes()
            ),
            (0, 1, 0, 0)
        );
        let theirs = verdict_of(&request_with_cards(
            &playing.encode(),
            1,
            &[(117, 1, 1, "Vi", "Unit")],
            &move_of(117, 10),
        ));
        assert_eq!(theirs.plugin_state, None);
    }

    #[test]
    fn only_a_play_by_the_focus_holder_changes_state_on_a_move() {
        let mut open = GameBlob::start(2, 0, Mode::Free);
        open.showdown = Some(Showdown {
            window: PassWindow {
                focus: 1,
                passes: 1,
            },
            ..Showdown::open(9, 0, 1)
        });
        assert_eq!(
            decide_bytes(&request(&open.encode(), 2, 0, &a_move)),
            Verdict::accept().encode()
        );
        let verdict = verdict_of(&request(&open.encode(), 2, 1, &a_move));
        let next = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(
            (
                next.showdown.as_ref().unwrap().focus(),
                next.showdown.as_ref().unwrap().passes()
            ),
            (0, 0)
        );
        assert_eq!(
            decide_bytes(&request(&GameBlob::lobby(2).encode(), 2, 0, &a_move)),
            Verdict::accept().encode()
        );
    }
}

#[cfg(test)]
mod enforced_tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::state::{Phase, PromptWhy, SetupStage};
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::dice::commitment;
    use agni_plugin_sdk::table::{Face, Target};

    fn request(blob: &GameBlob, table: &Snapshot, seat: u8, action: Action) -> decide::Request {
        decide::Request {
            plugin_state: blob.encode(),
            players: table.players,
            seat,
            action,
            table: table.clone(),
        }
    }

    fn game(event: TurnEvent) -> Action {
        Action::Game(event.encode())
    }

    fn step(
        blob: &mut GameBlob,
        table: &mut Snapshot,
        seat: u8,
        action: Action,
    ) -> Result<Vec<Effect>, Refusal> {
        let verdict = decide(&request(blob, table, seat, action.clone()))?;
        assert!(verdict.accept);
        table.apply_entry(&action, seat).unwrap();
        table.apply_all(&verdict.effects, seat).unwrap();
        if let Some(bytes) = verdict.plugin_state {
            *blob = GameBlob::decode(&bytes).unwrap();
        }
        strip_never_lies(blob, table);
        Ok(verdict.effects)
    }

    fn strip_never_lies(blob: &GameBlob, table: &Snapshot) {
        if !(blob.is_playing() && blob.is_enforced()) {
            return;
        }
        for seat in 0..table.players {
            let strip = present::present(&agni_plugin_sdk::view::Request {
                plugin_state: blob.encode(),
                players: table.players,
                seat,
                zones: table.zones.clone(),
                table: table.clone(),
            });
            let labels: Vec<&str> = strip
                .affordances
                .iter()
                .map(|affordance| affordance.label.as_str())
                .collect();
            for affordance in &strip.affordances {
                let verdict = decide(&request(
                    blob,
                    table,
                    seat,
                    Action::Game(affordance.data.clone()),
                ));
                if affordance.enabled {
                    assert!(
                        verdict.is_ok(),
                        "seat {seat} was offered {} but it is refused: {:?}",
                        affordance.label,
                        verdict.err()
                    );
                } else {
                    assert!(
                        verdict.is_err(),
                        "seat {seat} was shown {} greyed out but it is accepted",
                        affordance.label
                    );
                }
            }
            for (label, event) in [("pass", TurnEvent::Pass), ("end turn", TurnEvent::EndTurn)] {
                if !labels.contains(&label) {
                    assert!(
                        decide(&request(blob, table, seat, game(event))).is_err(),
                        "seat {seat} was not offered {label} but it is accepted"
                    );
                }
            }
            let picks = strip
                .affordances
                .iter()
                .filter(|affordance| {
                    matches!(
                        TurnEvent::decode(&affordance.data),
                        Some(TurnEvent::Pick(_))
                    )
                })
                .count();
            if picks == 0 {
                let prompt = blob.prompt.as_ref().map(|prompt| prompt.id).unwrap_or(1);
                let stray = game(TurnEvent::Pick(Pick { prompt, option: 0 }));
                assert!(
                    decide(&request(blob, table, seat, stray)).is_err(),
                    "seat {seat} was offered no pick but one is accepted"
                );
            }
        }
    }

    fn decided_lobby() -> GameBlob {
        let mut salt = 0u8;
        loop {
            let mut roll = Roll::new(2, DIE_SIDES);
            let secret = |seat: u8| [seat + salt + 1; 8];
            for seat in 0..2u8 {
                roll.commit(seat, commitment(&secret(seat))).unwrap();
            }
            for seat in 0..2u8 {
                roll.reveal(seat, secret(seat)).unwrap();
            }
            if let Outcome::Winner(0) = roll.outcome() {
                return GameBlob {
                    mode: Mode::Enforced,
                    lobby: Some(roll),
                    ..GameBlob::default()
                };
            }
            salt += 1;
        }
    }

    fn pick(blob: &GameBlob, option: u16) -> TurnEvent {
        TurnEvent::Pick(Pick {
            prompt: blob.prompt.as_ref().unwrap().id,
            option,
        })
    }

    #[test]
    fn a_table_opened_with_rules_enforced_starts_its_lobby_in_enforced_mode() {
        let mut table = fixtures::table();
        table.options = vec![(rules::OPTION_RULES_ENFORCED.into(), 1)];
        let secret = [7u8; 8];
        let commit = decide::Request {
            plugin_state: Vec::new(),
            players: 2,
            seat: 0,
            action: game(TurnEvent::CommitRoll {
                commit: commitment(&secret),
            }),
            table: table.clone(),
        };
        let verdict = decide(&commit).unwrap();
        let lobby = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(lobby.mode, Mode::Enforced);
        assert!(!lobby.is_playing());
        assert!(lobby.roll().unwrap().hands[0].commit.is_some());
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: Vec::new(),
            players: 2,
            seat: 0,
            zones: table.zones.clone(),
            table: table.clone(),
        });
        assert!(strip.status.contains(&"mode: rules enforced".to_string()));
        let mut free = fixtures::table();
        free.options = vec![(rules::OPTION_VICTORY_SCORE.into(), 6)];
        let verdict = decide(&decide::Request {
            table: free.clone(),
            ..commit
        })
        .unwrap();
        let lobby = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert_eq!(lobby.mode, Mode::Free);
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: Vec::new(),
            players: 2,
            seat: 0,
            zones: free.zones.clone(),
            table: free,
        });
        assert!(strip.status.contains(&"mode: free table".to_string()));
    }

    #[test]
    fn starting_an_enforced_game_runs_setup_and_the_mulligans_then_turn_one() {
        let mut blob = decided_lobby();
        let mut table = fixtures::table();
        let effects = step(
            &mut blob,
            &mut table,
            0,
            game(TurnEvent::StartGame { first_player: 0 }),
        )
        .unwrap();
        assert_eq!(blob.phase(), Some(Phase::Setup));
        assert_eq!(blob.seat(0).setup, SetupStage::Drawn);
        assert_eq!(
            blob.seat(0).chosen_champion.as_deref(),
            Some("Lillia - Fae Fawn"),
            "103.2.a.3 · the Chosen Champion is the card in the Champion Zone at setup"
        );
        assert_eq!(blob.seat(1).chosen_champion, None);
        assert_eq!(blob.why, Some(PromptWhy::Mulligan));
        assert_eq!(blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            effects.len(),
            2,
            "seat 1 is topped up to four from a two-card deck"
        );
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: blob.encode(),
            players: 2,
            seat: 0,
            zones: table.zones.clone(),
            table: table.clone(),
        });
        assert_eq!(
            strip.prompt.as_ref().unwrap().why,
            "set aside up to 2 cards to redraw"
        );
        let labels: Vec<&str> = strip
            .affordances
            .iter()
            .map(|affordance| affordance.label.as_str())
            .collect();
        assert_eq!(labels[4], "keep");
        assert_eq!(strip.primary, Some(4), "keep is the prompt's primary");
        assert_eq!(labels[labels.len() - 3], "free table");
        assert_eq!(labels[labels.len() - 2], "concede");
        assert_eq!(labels.last(), Some(&agni_plugin_sdk::manual::DISABLE));
        assert_eq!(
            strip.hidden,
            [labels.len() as u16 - 2, labels.len() as u16 - 1]
        );
        assert!(!strip.status.iter().any(|line| line == "end turn"));
        let theirs = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: blob.encode(),
            players: 2,
            seat: 1,
            zones: table.zones.clone(),
            table: table.clone(),
        });
        assert_eq!(theirs.affordances.len(), 2);
        assert_eq!(
            theirs.hidden,
            [0, 1],
            "the other seat has concession and emergency recovery in the menu"
        );
        assert_eq!(
            decide(&request(&blob, &table, 1, game(pick(&blob, 0)))),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            decide(&request(&blob, &table, 0, game(TurnEvent::EndTurn))),
            Err(Refusal::PromptOpen)
        );
        let buried = fixtures::move_to_bottom(fixtures::HAND_UNIT, fixtures::MAIN_DECK, 1);
        assert_eq!(
            decide(&request(&blob, &table, 0, buried)),
            Err(Refusal::PromptOpen),
            "a set-aside into the other seat's deck is refused"
        );
        let set_aside = fixtures::move_to_bottom(fixtures::HAND_UNIT, fixtures::MAIN_DECK, 0);
        let effects = step(&mut blob, &mut table, 0, set_aside).unwrap();
        assert!(effects.is_empty());
        assert_eq!(blob.prompt.as_ref().unwrap().picked, [fixtures::HAND_UNIT]);
        let keep = blob
            .offered(&table)
            .unwrap()
            .iter()
            .position(|opt| opt.label == "keep")
            .unwrap() as u16;
        let answer = game(pick(&blob, keep));
        let effects = step(&mut blob, &mut table, 0, answer).unwrap();
        assert_eq!(effects.len(), 1, "one redraw for the one card set aside");
        assert_eq!(blob.seat(0).setup, SetupStage::Done);
        assert_eq!(blob.prompt.as_ref().unwrap().seat, 1);
        let keep = blob.offered(&table).unwrap().len() as u16 - 1;
        let answer = game(pick(&blob, keep));
        let effects = step(&mut blob, &mut table, 1, answer).unwrap();
        assert_eq!(blob.phase(), Some(Phase::Action));
        assert_eq!((blob.turn(), blob.turn_player()), (1, 0));
        assert!(blob.prompt.is_none());
        assert!(effects.contains(&Effect::ready(fixtures::RUNE_A)));
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Move { zone, .. } if *zone == fixtures::RUNE_POOL)
        ));
        assert_eq!(blob.log.last().unwrap(), "turn 1 · {seat 0}");
        let mut empty_hands = decided_lobby();
        let mut bare = fixtures::table();
        bare.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) && card.zone != Some(fixtures::MAIN_DECK)
        });
        step(
            &mut empty_hands,
            &mut bare,
            0,
            game(TurnEvent::StartGame { first_player: 1 }),
        )
        .unwrap();
        assert_eq!(empty_hands.phase(), Some(Phase::Action));
        assert_eq!(empty_hands.turn_player(), 1);
        assert!(empty_hands.prompt.is_none());
    }

    #[test]
    fn enforced_mode_refuses_free_form_entries_with_reasons_and_accepts_reveals() {
        let fixture = Fixture::enforced();
        let (blob, table) = (fixture.blob, fixture.table);
        let refused = |action: Action, reason: Reason| {
            assert_eq!(
                decide(&request(&blob, &table, 0, action)),
                Err(Refusal::Illegal(reason))
            );
        };
        refused(
            Action::Spawn {
                face: Face::named("Sprite"),
                zone: Some(fixtures::BASE),
                seat: 0,
            },
            Reason::TokensByEffect,
        );
        refused(
            Action::Annotate {
                card: fixtures::VI,
                key: "exhausted".into(),
                value: Some(vec![1]),
            },
            Reason::AnnotationsAutomatic,
        );
        refused(
            Action::Counter {
                target: Target::Seat(0),
                counter: rules::COUNTER_POINTS,
                delta: 1,
            },
            Reason::CountersAutomatic,
        );
        refused(
            Action::Deal {
                zone: Some(fixtures::MAIN_DECK),
                count: 3,
            },
            Reason::DealIsOver,
        );
        refused(
            fixtures::move_action(fixtures::HAND_UNIT, fixtures::TRASH, 0),
            Reason::KillsAreAutomatic,
        );
        refused(
            game(TurnEvent::Activate {
                source: fixtures::LEGEND_CARD,
                ability: 1,
            }),
            Reason::NoSuchAbility,
        );
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                game(TurnEvent::Activate {
                    source: fixtures::LEGEND_CARD,
                    ability: 0,
                })
            )),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            }),
            "the legend's own ability is priced, not missing"
        );
        let reveal = decide(&request(
            &blob,
            &table,
            0,
            Action::Reveal {
                card: fixtures::HAND_HIDDEN,
                face: Face::named("Rebuke"),
            },
        ))
        .unwrap();
        assert_eq!(reveal, Verdict::accept());
        assert_eq!(
            decide(&request(&blob, &table, 0, Action::Join)).unwrap(),
            Verdict::accept()
        );
        let lobby = GameBlob::lobby_in(2, Mode::Enforced);
        assert_eq!(
            decide(&request(
                &lobby,
                &table,
                0,
                Action::Spawn {
                    face: Face::named("Sprite"),
                    zone: Some(fixtures::BASE),
                    seat: 0,
                }
            )),
            Err(Refusal::Illegal(Reason::TokensByEffect)),
            "an enforced lobby spawns nothing either"
        );
        let free_lobby = GameBlob::lobby_in(2, Mode::Free);
        assert!(decide(&request(
            &free_lobby,
            &table,
            0,
            Action::Spawn {
                face: Face::named("Sprite"),
                zone: Some(fixtures::BASE),
                seat: 0,
            }
        ))
        .is_ok());
        let reset = decide(&request(&blob, &table, 0, Action::Reset)).unwrap();
        assert_eq!(reset.plugin_state.as_deref(), Some(&[][..]));
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                1,
                fixtures::move_action(fixtures::VI, fixtures::BF1, 0)
            )),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            decide(&request(&blob, &table, 0, game(TurnEvent::Pass))),
            Err(Refusal::NoShowdown)
        );
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                game(TurnEvent::StartGame { first_player: 0 })
            )),
            Err(Refusal::AlreadyStarted)
        );
        let bytes = decide_bytes(&decide_request_bytes(&blob, &table, 0));
        assert_eq!(
            bytes,
            Verdict::refuse(Reason::KillsAreAutomatic.label()).encode()
        );
    }

    fn decide_request_bytes(blob: &GameBlob, table: &Snapshot, seat: u8) -> Vec<u8> {
        use agni_plugin_sdk::cbor::Writer;
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("plugin_state");
        writer.bytes(&blob.encode());
        writer.text("state");
        writer.map(3);
        writer.text("seats");
        writer.array(2);
        for seat in 0..2u64 {
            writer.map(1);
            writer.text("seat");
            writer.unsigned(seat);
        }
        writer.text("zones");
        writer.array(table.zones.len());
        for zone in &table.zones {
            writer.map(4);
            writer.text("id");
            writer.unsigned(u64::from(zone.id));
            writer.text("name");
            writer.text(&zone.name);
            writer.text("kind");
            writer.text(match zone.kind {
                agni_plugin_sdk::table::ZoneKind::Hand => "Hand",
                agni_plugin_sdk::table::ZoneKind::Deck => "Deck",
                agni_plugin_sdk::table::ZoneKind::Discard => "Discard",
                agni_plugin_sdk::table::ZoneKind::Stack => "Stack",
                agni_plugin_sdk::table::ZoneKind::Battlefield => "Battlefield",
                agni_plugin_sdk::table::ZoneKind::Aux => "Aux",
            });
            writer.text("owner");
            writer.text(if zone.shared { "Shared" } else { "PerSeat" });
        }
        writer.text("table");
        writer.map(1);
        writer.text("cards");
        writer.array(table.cards.len());
        for card in &table.cards {
            writer.map(5);
            writer.text("id");
            writer.unsigned(u64::from(card.id));
            writer.text("owner");
            writer.unsigned(u64::from(card.owner));
            writer.text("seat");
            writer.unsigned(u64::from(card.seat));
            writer.text("zone");
            writer.map(1);
            writer.text("Plugin");
            writer.unsigned(u64::from(card.zone.unwrap_or(0)));
            writer.text("face");
            writer.map(4);
            writer.text("name");
            writer.text(&card.name);
            writer.text("tint");
            writer.array(3);
            for _ in 0..3 {
                writer.unsigned(128);
            }
            writer.text("foil");
            writer.bool(false);
            writer.text("kind");
            writer.text(card.kind.as_deref().unwrap_or(""));
        }
        writer.text("entry");
        writer.map(2);
        writer.text("seat");
        writer.unsigned(u64::from(seat));
        writer.text("action");
        writer.map(1);
        writer.text("Move");
        writer.map(4);
        writer.text("card");
        writer.unsigned(u64::from(fixtures::HAND_UNIT));
        writer.text("to");
        writer.map(1);
        writer.text("Plugin");
        writer.unsigned(u64::from(fixtures::TRASH));
        writer.text("seat");
        writer.unsigned(0);
        writer.text("index");
        writer.unsigned(0);
        writer.finish()
    }

    #[test]
    fn a_play_a_march_a_showdown_closed_by_passes_and_a_conquer_run_through_decide() {
        let fixture = Fixture::enforced();
        let (mut blob, mut table) = (fixture.blob, fixture.table);
        let effects = step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::HAND_UNIT, fixtures::BASE, 0),
        )
        .unwrap();
        assert_eq!(
            effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::exhaust(fixtures::HAND_UNIT),
            ]
        );
        assert!(blob.seat(0).played_main);
        let effects = step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::VI, fixtures::BF1, 0),
        )
        .unwrap();
        assert_eq!(effects, [Effect::exhaust(fixtures::VI)]);
        let showdown = blob.showdown.clone().expect("the contest opens a showdown");
        assert_eq!(
            (showdown.zone, showdown.attacker, showdown.focus()),
            (fixtures::BF1, 0, 0)
        );
        assert_eq!(blob.contester(fixtures::BF1), Some(0));
        assert_eq!(
            decide(&request(&blob, &table, 0, game(TurnEvent::EndTurn))),
            Err(Refusal::ShowdownOpen)
        );
        assert_eq!(
            decide(&request(&blob, &table, 1, game(TurnEvent::Pass))),
            Err(Refusal::NotYourFocus)
        );
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                fixtures::move_action(fixtures::HAND_GEAR, fixtures::BASE, 0)
            )),
            Err(Refusal::Illegal(Reason::ShowdownTiming))
        );
        step(&mut blob, &mut table, 0, game(TurnEvent::Pass)).unwrap();
        assert_eq!(blob.showdown.as_ref().unwrap().focus(), 1);
        let effects = step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        assert_eq!(effects, [Effect::score(0, rules::COUNTER_POINTS, 1)]);
        assert!(blob.showdown.is_none());
        assert_eq!(blob.holder(fixtures::BF1), Some(0));
        assert_eq!(blob.contester(fixtures::BF1), None);
        assert!(blob.scored(fixtures::BF1, 0));
        assert_eq!(blob.log.last().unwrap(), "{seat 0} conquers {zone 9}");
        step(&mut blob, &mut table, 0, game(TurnEvent::EndTurn)).unwrap();
        assert_eq!(
            (blob.turn(), blob.turn_player(), blob.phase()),
            (2, 1, Some(Phase::Beginning)),
            "seat 1's Temporary Sprite triggers before its beginning phase goes on"
        );
        step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        let effects = step(&mut blob, &mut table, 0, game(TurnEvent::Pass)).unwrap();
        assert_eq!(blob.phase(), Some(Phase::Action));
        assert!(!blob.scored(fixtures::BF1, 0));
        assert_eq!(blob.holder(fixtures::BF1), Some(0));
        assert!(effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert_eq!(blob.holder(fixtures::BF2), None);
        assert!(effects.iter().any(|effect| matches!(effect, Effect::Move { card: 25, zone, seat: 1, index: TOP } if *zone == fixtures::HAND)));
        let effects = step(&mut blob, &mut table, 1, game(TurnEvent::EndTurn)).unwrap();
        assert_eq!((blob.turn(), blob.turn_player()), (3, 0));
        assert!(
            effects.contains(&Effect::score(0, rules::COUNTER_POINTS, 1)),
            "the held battlefield scores"
        );
        assert!(effects.contains(&Effect::ready(fixtures::VI)));
        assert_eq!(
            table.counter(Target::Seat(0), rules::COUNTER_POINTS),
            Some(2)
        );
    }

    #[test]
    fn a_location_prompt_is_answered_by_pick_and_the_free_table_drops_enforcement() {
        let fixture = Fixture::enforced();
        let (mut blob, mut table) = (fixture.blob, fixture.table);
        blob.set_holder(fixtures::BF1, Some(0));
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::HAND_UNIT, fixtures::CHAIN, 0),
        )
        .unwrap();
        assert_eq!(blob.why, Some(PromptWhy::PlayLocation { item: 1 }));
        assert_eq!(blob.queue.len(), 1);
        let offered = blob.offered(&table).unwrap();
        assert_eq!(offered[1].label, "{zone 9}");
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                game(TurnEvent::Pick(Pick {
                    prompt: 99,
                    option: 0
                }))
            )),
            Err(Refusal::Pick(PickRefusal::Stale { open: 1, sent: 99 }))
        );
        let answer = game(pick(&blob, 1));
        let effects = step(&mut blob, &mut table, 0, answer).unwrap();
        assert_eq!(
            effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::Move {
                    card: fixtures::HAND_UNIT,
                    zone: fixtures::BF1,
                    seat: 0,
                    index: TOP
                },
                Effect::exhaust(fixtures::HAND_UNIT),
            ]
        );
        assert!(blob.prompt.is_none());
        assert!(blob.queue.is_empty());
        assert!(blob.showdown.is_none());
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                fixtures::move_action(fixtures::HAND_GEAR, fixtures::CHAIN, 0)
            )),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        let mut asked = Fixture::enforced();
        asked.blob.set_holder(fixtures::BF1, Some(0));
        let (mut blob, mut table) = (asked.blob, asked.table);
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::HAND_UNIT, fixtures::CHAIN, 0),
        )
        .unwrap();
        let cancel = blob.offered(&table).unwrap().len() as u16 - 1;
        let answer = game(pick(&blob, cancel));
        let effects = step(&mut blob, &mut table, 0, answer).unwrap();
        assert_eq!(
            effects,
            [Effect::Move {
                card: fixtures::HAND_UNIT,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert!(blob.queue.is_empty());
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::HAND_UNIT, fixtures::CHAIN, 0),
        )
        .unwrap();
        assert!(blob.prompt.is_some());
        step(&mut blob, &mut table, 0, game(TurnEvent::FreeTable)).unwrap();
        assert_eq!(blob.free_table, Some(0));
        assert!(blob.prompt.is_some());
        step(&mut blob, &mut table, 1, game(TurnEvent::FreeTable)).unwrap();
        assert_eq!(blob.mode, Mode::Free);
        assert!(blob.prompt.is_none());
        assert!(blob.queue.is_empty());
        assert!(blob.why.is_none());
        assert!(
            decide(&request(
                &blob,
                &table,
                0,
                fixtures::move_action(fixtures::HAND_UNIT, fixtures::TRASH, 0)
            ))
            .unwrap()
            .accept
        );
    }

    #[test]
    fn two_contests_ask_which_showdown_opens_and_the_winner_ends_the_game() {
        let fixture = Fixture::enforced();
        let (mut blob, mut table) = (fixture.blob, fixture.table);
        blob.set_holder(fixtures::BF2, None);
        table.cards.retain(|card| card.id != fixtures::SPRITE);
        table
            .cards
            .push(fixtures::unit(90, fixtures::BASE, 0, "Jinx", 2));
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::VI, fixtures::BF1, 0),
        )
        .unwrap();
        assert_eq!(
            blob.why,
            Some(PromptWhy::GroupMove {
                unit: fixtures::VI,
                to: fixtures::BF1
            })
        );
        let done = blob.offered(&table).unwrap().len() as u16 - 1;
        let answer = game(pick(&blob, done));
        step(&mut blob, &mut table, 0, answer).unwrap();
        assert!(blob.showdown.is_some());
        assert!(blob.prompt.is_none());
        step(&mut blob, &mut table, 0, game(TurnEvent::Pass)).unwrap();
        let effects = step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        assert_eq!(effects, [Effect::score(0, rules::COUNTER_POINTS, 1)]);
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(90, fixtures::BF2, 0),
        )
        .unwrap();
        assert!(blob.showdown.is_some(), "the second contest opens at once");
        let mut staged = Fixture::enforced();
        staged.blob.set_holder(fixtures::BF2, None);
        staged
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        staged
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Jinx", 2));
        staged.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        staged.blob.set_contested(fixtures::BF1, Some(0));
        staged.blob.set_contested(fixtures::BF2, Some(0));
        let (mut blob, mut table) = (staged.blob, staged.table);
        assert_eq!(
            step(&mut blob, &mut table, 0, game(TurnEvent::EndTurn)),
            Err(Refusal::ShowdownOpen),
            "a contest never outlives the entry that made it"
        );
        let scripts = cards::Resolved::of(&table);
        let mut ctx = engine::ctx::Ctx::fresh(&table, &mut blob, &scripts, 0);
        engine::cleanup::run(&mut ctx, None);
        drop(ctx);
        assert_eq!(blob.why, Some(PromptWhy::PickStaged));
        assert_eq!(
            decide(&request(&blob, &table, 1, game(pick(&blob, 0)))),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            decide(&request(&blob, &table, 0, game(TurnEvent::Pass))),
            Err(Refusal::PromptOpen)
        );
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: blob.encode(),
            players: 2,
            seat: 0,
            zones: table.zones.clone(),
            table: table.clone(),
        });
        assert_eq!(
            strip.affordances[..2]
                .iter()
                .map(|affordance| affordance.label.as_str())
                .collect::<Vec<_>>(),
            ["{zone 9}", "{zone 10}"]
        );
        let answer = game(pick(&blob, 1));
        step(&mut blob, &mut table, 0, answer).unwrap();
        assert_eq!(blob.showdown.as_ref().unwrap().zone, fixtures::BF2);
        assert_eq!(blob.staged.len(), 1);
        step(&mut blob, &mut table, 0, game(TurnEvent::Pass)).unwrap();
        step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        assert_eq!(blob.holder(fixtures::BF2), Some(0));
        assert_eq!(blob.showdown.as_ref().unwrap().zone, fixtures::BF1);
        step(&mut blob, &mut table, 0, game(TurnEvent::Pass)).unwrap();
        step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        assert!(blob.showdown.is_none());
        assert_eq!(
            table.counter(Target::Seat(0), rules::COUNTER_POINTS),
            Some(2)
        );
        let mut almost = Fixture::enforced();
        almost.set_points(0, rules::DEFAULT_VICTORY_SCORE - 1);
        almost.blob.set_holder(fixtures::BF1, Some(0));
        almost.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        almost.blob.core_mut().unwrap().player = 1;
        let (mut blob, mut table) = (almost.blob, almost.table);
        let hand_before = table.held(fixtures::HAND, 0).count();
        let effects = step(&mut blob, &mut table, 1, game(TurnEvent::EndTurn)).unwrap();
        assert_eq!(
            effects,
            [
                Effect::ready(fixtures::RUNE_A),
                Effect::score(0, rules::COUNTER_POINTS, 1)
            ],
            "the winning hold neither channels nor draws"
        );
        assert_eq!(table.held(fixtures::HAND, 0).count(), hand_before);
        assert_eq!(rules::winner(&table), Some(0));
        assert_eq!(
            &blob.log[blob.log.len() - 2..],
            ["{seat 0} holds {zone 9}", "{seat 0} wins with 8 points"]
        );
        assert_eq!(blob.phase(), Some(Phase::Beginning));
        assert_eq!(
            decide(&request(&blob, &table, 0, game(TurnEvent::EndTurn))),
            Err(Refusal::GameOver { winner: 0 })
        );
        assert_eq!(
            decide(&request(
                &blob,
                &table,
                0,
                fixtures::move_action(fixtures::VI, fixtures::BASE, 0)
            )),
            Err(Refusal::GameOver { winner: 0 })
        );
        let strip = present::present(&agni_plugin_sdk::view::Request {
            plugin_state: blob.encode(),
            players: 2,
            seat: 0,
            zones: table.zones.clone(),
            table: table.clone(),
        });
        assert_eq!(strip.winner, Some(0));
        assert_eq!(strip.affordances.len(), 2);
        step(&mut blob, &mut table, 0, game(TurnEvent::FreeTable)).unwrap();
        step(&mut blob, &mut table, 1, game(TurnEvent::FreeTable)).unwrap();
        assert_eq!(blob.mode, Mode::Free);
    }

    #[test]
    fn a_seat_that_leaves_closes_its_prompt_its_showdown_and_hands_over_its_turn() {
        let mut blob = decided_lobby();
        let mut table = fixtures::table();
        step(
            &mut blob,
            &mut table,
            0,
            game(TurnEvent::StartGame { first_player: 0 }),
        )
        .unwrap();
        let keep = blob.offered(&table).unwrap().len() as u16 - 1;
        let answer = game(pick(&blob, keep));
        step(&mut blob, &mut table, 0, answer).unwrap();
        assert_eq!(blob.prompt.as_ref().unwrap().seat, 1);
        step(&mut blob, &mut table, 1, Action::Clear { seat: 1 }).unwrap();
        assert!(blob.prompt.is_none());
        assert_eq!(blob.phase(), Some(Phase::Action));
        assert_eq!((blob.turn(), blob.turn_player()), (1, 0));
        assert!(blob.log.contains(&"{seat 1} left the table".to_string()));
        assert!(table.cards.iter().all(|card| card.owner != 1));
        assert!(blob.cards.iter().all(|row| table.card(row.id).is_some()));
        let fixture = Fixture::enforced();
        let (mut blob, mut table) = (fixture.blob, fixture.table);
        blob.card_state_mut(fixtures::VI)
            .set(crate::state::FLAG_STUNNED, true);
        step(
            &mut blob,
            &mut table,
            0,
            fixtures::move_action(fixtures::VI, fixtures::BF1, 0),
        )
        .unwrap();
        assert!(blob.showdown.is_some());
        let effects = step(&mut blob, &mut table, 0, Action::Clear { seat: 0 }).unwrap();
        assert!(blob.showdown.is_none());
        assert!(blob.prompt.is_none());
        assert_eq!(blob.holder(fixtures::BF1), None);
        assert_eq!(blob.contester(fixtures::BF1), None);
        assert_eq!((blob.turn(), blob.turn_player()), (2, 1));
        assert!(blob.card_state(fixtures::VI).is_none());
        assert!(effects
            .iter()
            .all(|effect| !matches!(effect, Effect::Annotate { .. })));
        let effects = step(&mut blob, &mut table, 1, game(TurnEvent::Pass)).unwrap();
        assert_eq!(
            blob.holder(fixtures::BF2),
            None,
            "the sprite dies at the start of seat 1's turn"
        );
        assert!(effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Move { card: 25, zone, seat: 1, .. } if *zone == fixtures::HAND)
        ));
        step(&mut blob, &mut table, 1, game(TurnEvent::EndTurn)).unwrap();
        assert_eq!((blob.turn(), blob.turn_player()), (3, 0));
    }
}

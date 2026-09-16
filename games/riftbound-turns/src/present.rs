use crate::engine::ctx::{is_rune_face, Ctx};
use crate::engine::{activate, combat, legal};
use crate::rules::{self, COUNTER_XP, ZONE_HAND, ZONE_MAIN_DECK, ZONE_RUNE_POOL};
use crate::state::{GameBlob, InGameRoll, Mode, Phase, Showdown, ShowdownStage, TurnCore};
use crate::{TurnEvent, DIE_SIDES, PROMPT_WHY};
use agni_plugin_sdk::dice::{Outcome, Roll};
use agni_plugin_sdk::prompt::{Answer, Opt, Pick, Prompt};
use agni_plugin_sdk::table::{Snapshot, Target};
use agni_plugin_sdk::view::{self, Kind, PluginView, Request, SeatInfo, TurnInfo};

pub const ADVANCE_KEY: &str = "space";
pub const PASS_KEY: &str = "w";
pub const ESCAPE_KEY: &str = "x";
pub const CONCEDE: &str = "concede";
pub const FREE_TABLE: &str = "free table";
pub const CONFIRM_FREE_TABLE: &str = "confirm free table";
pub const WITHDRAW_FREE_TABLE: &str = "withdraw free table";

fn xp_of(table: &Snapshot, seat: u8) -> i32 {
    table.counter(Target::Seat(seat), COUNTER_XP).unwrap_or(0)
}

fn turn_info(core: &TurnCore, mode: Mode) -> TurnInfo {
    TurnInfo {
        number: u32::from(core.turn),
        seat: core.player,
        phase: core.phase.label().to_string(),
        phases: Phase::ALL
            .iter()
            .map(|phase| phase.label().to_string())
            .collect(),
        mode: mode.label().to_string(),
    }
}

fn count_at(table: &Snapshot, zone: &str, seat: u8) -> u32 {
    rules::zone_id(table, zone)
        .map(|zone| table.held(zone, seat).count())
        .unwrap_or(0) as u32
}

fn seat_info(table: &Snapshot, seat: u8, victory: i32) -> SeatInfo {
    let runes: Vec<bool> = rules::zone_id(table, ZONE_RUNE_POOL)
        .map(|pool| {
            table
                .held(pool, seat)
                .filter(|card| is_rune_face(card))
                .map(|rune| !rune.exhausted)
                .collect()
        })
        .unwrap_or_default();
    SeatInfo {
        seat,
        points: rules::points(table, seat),
        victory,
        xp: xp_of(table, seat),
        hand: count_at(table, ZONE_HAND, seat),
        deck: count_at(table, ZONE_MAIN_DECK, seat),
        runes_ready: runes.iter().filter(|ready| **ready).count() as u32,
        runes_total: runes.len() as u32,
    }
}

fn with_seats(mut view: PluginView, table: &Snapshot, players: u8) -> PluginView {
    let victory = rules::Options::of(table).victory_score;
    for seat_index in 0..players {
        view = view.seat(seat_info(table, seat_index, victory));
    }
    view
}

fn with_narration(mut view: PluginView, state: &GameBlob) -> PluginView {
    for line in &state.log {
        view = view.narrate(line.clone());
    }
    view
}

fn offer_concede(view: PluginView, state: &GameBlob, me: u8, winner: Option<u8>) -> PluginView {
    if winner.is_some() || state.has_conceded(me) || me >= state.players() {
        return view;
    }
    view.offer_hidden(CONCEDE, TurnEvent::Concede.encode())
}

fn free_table_offer(view: PluginView, state: &GameBlob, me: u8, turn_player: u8) -> PluginView {
    match state.free_table {
        None if turn_player == me => view.offer(FREE_TABLE, None, TurnEvent::FreeTable.encode()),
        Some(proposer) if proposer != me => view
            .line(format!("{} proposes a free table", seat(proposer)))
            .offer_hidden(CONFIRM_FREE_TABLE, TurnEvent::FreeTable.encode()),
        Some(_) => view
            .line("free table proposed · waiting for another seat")
            .offer_hidden(WITHDRAW_FREE_TABLE, TurnEvent::FreeTable.encode()),
        None => view,
    }
}

fn points_line(table: &Snapshot, players: u8) -> String {
    let points: Vec<String> = (0..players)
        .map(|seat_index| format!("{} {}", seat(seat_index), rules::points(table, seat_index)))
        .collect();
    let mut line = format!("points · {}", points.join(" · "));
    if (0..players).any(|seat_index| xp_of(table, seat_index) != 0) {
        let xp: Vec<String> = (0..players)
            .map(|seat_index| format!("{} {}", seat(seat_index), xp_of(table, seat_index)))
            .collect();
        line.push_str(&format!(" · xp {}", xp.join(" · ")));
    }
    line
}

fn scoreboard(request: &Request, state: &GameBlob) -> Vec<String> {
    let table = &request.table;
    let mut lines = vec![points_line(table, request.players.max(1))];
    if let Some(winner) = rules::winner(table).filter(|_| !state.manual) {
        lines.push(format!(
            "{} wins with {} points",
            seat(winner),
            rules::Options::of(table).victory_score
        ));
    }
    let held: Vec<String> = state
        .held_zones()
        .map(|(zone_id, holder)| format!("{} held by {}", zone(zone_id), seat(holder)))
        .collect();
    if !held.is_empty() {
        lines.push(held.join(" · "));
    }
    lines
}

fn seat(seat: u8) -> String {
    format!("{{seat {seat}}}")
}

fn zone(zone: u16) -> String {
    format!("{{zone {zone}}}")
}

pub fn contested<'a>(request: &'a Request) -> impl Iterator<Item = u16> + 'a {
    let keep = usize::from(request.players.max(1));
    request
        .zones
        .iter()
        .filter(|zone| zone.shared && zone.battlefield)
        .map(|zone| zone.id)
        .take(keep)
}

fn lobby(request: &Request, roll: &Roll, mode: Mode) -> PluginView {
    let me = request.seat;
    let players = request.players.max(1);
    let round = u32::from(roll.round);
    let mut view = PluginView::default().line(if roll.round > 1 {
        format!("roll for first player · tie, round {}", roll.round)
    } else {
        "roll for first player".to_string()
    });
    view = with_seats(view, &request.table, players);
    let dice = roll.dice();
    let shown: Vec<String> = (0..players)
        .map(|seat_index| {
            let hand = roll.hands.get(usize::from(seat_index));
            let mark = match (dice.get(usize::from(seat_index)).copied().flatten(), hand) {
                (Some(die), _) => die.to_string(),
                (None, Some(hand)) if hand.commit.is_some() => "rolled".to_string(),
                _ => "…".to_string(),
            };
            format!("{}: {mark}", seat(seat_index))
        })
        .collect();
    view = view.line(shown.join(" · "));
    match roll.outcome() {
        Outcome::Winner(winner) if winner == me => {
            view = view
                .line("you won the roll — who goes first?")
                .line(format!("mode: {}", mode.label()))
                .line("every deck must be dealt before the start: the first turn draws and channels at once");
            for first_player in 0..players {
                let label = if first_player == me {
                    "go first".to_string()
                } else {
                    format!("let {} go first", seat(first_player))
                };
                view = view.offer(label, None, TurnEvent::StartGame { first_player }.encode());
            }
            view = view.offer(
                format!("switch to {}", mode.other().label()),
                None,
                TurnEvent::SetMode { mode: mode.other() }.encode(),
            );
        }
        Outcome::Winner(winner) => {
            view = view
                .line(format!(
                    "{} won the roll and chooses the mode and who goes first",
                    seat(winner)
                ))
                .line(format!("mode: {}", mode.label()))
                .waiting(Some(winner), "to choose the mode and who goes first");
        }
        _ => {
            view = view.line(format!("mode: {}", mode.label()));
            let mine = roll.hands.get(usize::from(me)).copied().unwrap_or_default();
            if mine.commit.is_none() {
                view = view
                    .offer_kind(
                        "roll",
                        Kind::Commit { roll: round },
                        TurnEvent::commit_prefix(),
                    )
                    .primary_last();
            } else if roll.all_committed() && mine.secret.is_none() {
                view = view
                    .offer_kind(
                        "reveal",
                        Kind::Reveal { roll: round },
                        TurnEvent::reveal_prefix(),
                    )
                    .primary_last();
            } else if !roll.all_committed() {
                view = view
                    .line("waiting for every seat to roll")
                    .waiting(None, "every seat to roll");
            } else {
                view = view.waiting(None, "every seat to reveal");
            }
        }
    }
    view
}

fn shuffle_roll(mut view: PluginView, open: &InGameRoll, me: u8) -> PluginView {
    let roll = &open.roll;
    let players = roll.players();
    let shown: Vec<String> = (0..players)
        .map(|seat_index| {
            let hand = roll.hands.get(usize::from(seat_index));
            let mark = match hand {
                Some(hand) if hand.secret.is_some() => "revealed",
                Some(hand) if hand.commit.is_some() => "rolled",
                _ => "…",
            };
            format!("{}: {mark}", seat(seat_index))
        })
        .collect();
    view = view.line(shown.join(" · "));
    let mine = roll.hands.get(usize::from(me)).copied().unwrap_or_default();
    if mine.commit.is_none() {
        view = view
            .offer_kind(
                "roll",
                Kind::Commit { roll: open.id },
                TurnEvent::commit_prefix(),
            )
            .primary_last();
    } else if roll.all_committed() && mine.secret.is_none() {
        view = view
            .offer_kind(
                "reveal",
                Kind::Reveal { roll: open.id },
                TurnEvent::reveal_prefix(),
            )
            .primary_last();
    } else if !roll.all_committed() {
        view = view
            .line("waiting for every seat to roll")
            .waiting(None, "every seat to roll");
    } else {
        view = view
            .line("waiting for every seat to reveal")
            .waiting(None, "every seat to reveal");
    }
    view
}

fn ask(
    mut view: PluginView,
    request: &Request,
    state: &GameBlob,
    prompt: &Prompt,
    me: u8,
) -> PluginView {
    let why = match state.why {
        Some(why) => {
            let mut blob = state.clone();
            let scripts = crate::cards::Resolved::of(&request.table);
            let ctx = crate::engine::ctx::Ctx::fresh(&request.table, &mut blob, &scripts, me);
            crate::engine::prompts::status(&ctx, why)
        }
        None => PROMPT_WHY.to_string(),
    };
    view = view.prompt(prompt.summary(why.clone()));
    if prompt.seat != me {
        return view
            .line(format!("waiting for {}: {why}", seat(prompt.seat)))
            .waiting(Some(prompt.seat), why);
    }
    view = view.line(why);
    let offered = state.offered(&request.table).unwrap_or_default();
    let viable = vec![true; offered.len()];
    offer_options(view, prompt, offered, viable)
}

fn offer_options(
    mut view: PluginView,
    prompt: &Prompt,
    offered: Vec<Opt>,
    viable: Vec<bool>,
) -> PluginView {
    let escape = crate::engine::prompts::escape(&offered);
    for (index, option) in offered.into_iter().enumerate() {
        let data = TurnEvent::Pick(Pick {
            prompt: prompt.id,
            option: index as u16,
        })
        .encode();
        let hotkey = (escape == Some(index)).then_some(ESCAPE_KEY);
        let confirms = option.card.is_none() && option.answer == Answer::Done;
        let enabled = viable.get(index).copied().unwrap_or(true);
        view = match option.card {
            Some(card) => view.offer_card_enabled(option.label, card, data, enabled),
            None => view.offer(option.label, hotkey, data),
        };
        if confirms {
            view = view.primary_last();
        }
    }
    view
}

fn free(request: &Request, state: &GameBlob, core: &TurnCore) -> PluginView {
    let winner = rules::winner(&request.table).or_else(|| state.conceded_winner());
    offer_concede(free_body(request, state, core), state, request.seat, winner)
}

fn free_body(request: &Request, state: &GameBlob, core: &TurnCore) -> PluginView {
    let me = request.seat;
    let mut view = PluginView::default()
        .line(format!(
            "turn {} · {} · {} · {}",
            core.turn,
            seat(core.player),
            core.phase.label(),
            state.mode.label()
        ))
        .turn(turn_info(core, state.mode));
    view = with_seats(view, &request.table, request.players.max(1));
    for line in scoreboard(request, state) {
        view = view.line(line);
    }
    for line in &state.log {
        view = view.line(line.clone());
    }
    view = with_narration(view, state);
    if state.manual {
        return view
            .line("rules enforcement disabled · play and score manually")
            .offer("end turn", Some(ADVANCE_KEY), TurnEvent::EndTurn.encode())
            .primary_last()
            .offer_hidden(
                agni_plugin_sdk::manual::CONTROLS,
                agni_plugin_sdk::manual::Command::Disable.encode(),
            );
    }
    if let Some(winner) = rules::winner(&request.table).or_else(|| state.conceded_winner()) {
        view = view.won_by(winner);
    }
    if let Some(prompt) = &state.prompt {
        return ask(view, request, state, prompt, me);
    }
    if let Some(showdown) = state.showdown.clone() {
        view = view
            .line(format!(
                "showdown at {} · {} attacks {}",
                zone(showdown.zone),
                seat(showdown.attacker),
                seat(showdown.defender)
            ))
            .line(format!(
                "focus: {} · passes {}/{}",
                seat(showdown.focus()),
                showdown.passes(),
                state.players()
            ));
        return if state.has_focus(me) {
            view.offer("pass", Some(PASS_KEY), TurnEvent::Pass.encode())
                .primary_last()
        } else {
            view.line(format!("waiting for {}", seat(showdown.focus())))
                .waiting(Some(showdown.focus()), "pass or respond")
        };
    }
    if state.is_turn_player(me) && state.phase() != Some(Phase::Setup) {
        view = view
            .offer("end turn", Some(ADVANCE_KEY), TurnEvent::EndTurn.encode())
            .primary_last();
    } else if state.phase() == Some(Phase::Setup) {
        view = view.line("setup · mulligans");
    } else {
        view = view
            .line(format!("waiting for {}", seat(state.turn_player())))
            .waiting(Some(state.turn_player()), "");
    }
    view
}

fn control_line(ctx: &Ctx) -> Option<String> {
    let lines: Vec<String> = ctx
        .zones
        .battlefields
        .iter()
        .filter_map(|battlefield| {
            let holder = ctx.blob.holder(*battlefield);
            let contester = ctx.blob.contester(*battlefield);
            let held = holder.map(|holder| format!("held by {}", seat(holder)));
            let contested = contester.map(|contester| format!("contested by {}", seat(contester)));
            let parts: Vec<String> = held.into_iter().chain(contested).collect();
            (!parts.is_empty()).then(|| format!("{} {}", zone(*battlefield), parts.join(", ")))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join(" · "))
}

fn chain_line(state: &GameBlob) -> Option<String> {
    if state.chain.is_empty() {
        return None;
    }
    let items: Vec<String> = state
        .chain
        .iter()
        .map(|item| match item.kind.card() {
            Some(card) => format!("{{card {card}}}"),
            None => format!("{{card {}}} ability", item.kind.source()),
        })
        .collect();
    Some(format!("chain: {} (top)", items.join(" → ")))
}

fn showdown_line(showdown: &Showdown) -> String {
    let what = if showdown.combat {
        "combat"
    } else {
        "showdown"
    };
    let head = format!(
        "{what} at {} · {} against {}",
        zone(showdown.zone),
        seat(showdown.attacker),
        seat(showdown.defender)
    );
    if matches!(showdown.stage, ShowdownStage::Damage { .. }) {
        return head;
    }
    format!("{head} · focus {}", seat(showdown.focus()))
}

fn combat_lines(ctx: &Ctx, showdown: &Showdown) -> Vec<String> {
    if !showdown.combat {
        return Vec::new();
    }
    let names = |units: &[u32]| -> String {
        if units.is_empty() {
            "none".to_string()
        } else {
            units
                .iter()
                .map(|unit| format!("{{card {unit}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let attackers = combat::attackers(ctx, showdown.zone);
    let defenders = combat::defenders(ctx, showdown.zone);
    let mut lines = vec![format!(
        "attackers {} ({} might) · defenders {} ({} might)",
        names(&attackers),
        combat::might_sum(ctx, &attackers),
        names(&defenders),
        combat::might_sum(ctx, &defenders)
    )];
    if let Some((assigner, remaining)) = combat::assigner(ctx) {
        lines.push(format!("{} assigns {remaining} damage", seat(assigner)));
    }
    lines
}

fn enforced(request: &Request, state: &GameBlob, core: &TurnCore) -> PluginView {
    let me = request.seat;
    let mut blob = state.clone();
    let scripts = crate::cards::Resolved::of(&request.table);
    let ctx = Ctx::fresh(&request.table, &mut blob, &scripts, me);
    let mut view = PluginView::default()
        .line(format!(
            "turn {} · {} · {} · {}",
            core.turn,
            seat(core.player),
            core.phase.label(),
            state.mode.label()
        ))
        .turn(turn_info(core, state.mode));
    view = with_seats(view, &ctx.table, ctx.players());
    view = view.line(points_line(&ctx.table, ctx.players()));
    let winner = ctx.winner();
    if let Some(winner) = winner {
        view = if ctx.blob.won == Some(winner) {
            view.line(format!("{} wins the game", seat(winner)))
        } else if ctx.points_winner() == Some(winner) {
            view.line(format!(
                "{} wins with {} points",
                seat(winner),
                ctx.victory_score()
            ))
        } else {
            view.line(format!("{} wins by concession", seat(winner)))
        }
        .won_by(winner);
    }
    if let Some(line) = control_line(&ctx) {
        view = view.line(line);
    }
    if let Some(line) = chain_line(state) {
        view = view.line(line);
    }
    if let Some(showdown) = &state.showdown {
        view = view.line(showdown_line(showdown));
        for line in combat_lines(&ctx, showdown) {
            view = view.line(line);
        }
    }
    let turn_player = state.turn_player();
    match (&state.prompt, winner, &state.showdown) {
        (Some(prompt), _, _) => {
            let why = match state.why {
                Some(why) => crate::engine::prompts::status(&ctx, why),
                None => PROMPT_WHY.to_string(),
            };
            view = view.prompt(prompt.summary(why.clone()));
            if let Some(open) = &state.roll {
                view = shuffle_roll(view.line(why), open, me);
            } else if prompt.seat == me {
                view = view.line(why);
                let offered = crate::engine::prompts::offered(&ctx);
                let viable = crate::engine::prompts::viable(&ctx, &offered);
                view = offer_options(view, prompt, offered, viable);
            } else {
                view = view
                    .line(format!("waiting for {}: {why}", seat(prompt.seat)))
                    .waiting(Some(prompt.seat), why);
            }
        }
        (None, Some(_), _) => {}
        (None, None, _) if state.priority.is_some() => {
            let active = state.priority.map(|priority| priority.active).unwrap_or(0);
            if active == me {
                view = view
                    .offer("pass", Some(PASS_KEY), TurnEvent::Pass.encode())
                    .primary_last();
            } else {
                view = view
                    .line(format!("waiting for {}: respond or pass", seat(active)))
                    .waiting(Some(active), "respond or pass");
            }
        }
        (None, None, Some(showdown)) => {
            let focus = showdown.focus();
            if focus == me {
                view = view
                    .offer("pass", Some(PASS_KEY), TurnEvent::Pass.encode())
                    .primary_last();
            } else {
                let what = format!("pass or respond at {}", zone(showdown.zone));
                view = view
                    .line(format!("waiting for {}: {what}", seat(focus)))
                    .waiting(Some(focus), what);
            }
        }
        (None, None, None) => {
            if state.phase() == Some(Phase::Setup) {
                view = view.line("setup · mulligans");
            } else if turn_player == me && state.is_neutral_open() {
                view = view
                    .offer("end turn", Some(ADVANCE_KEY), TurnEvent::EndTurn.encode())
                    .primary_last();
            } else {
                let what = format!("their {}", core.phase.label());
                view = view
                    .line(format!("waiting for {}: {what}", seat(turn_player)))
                    .waiting(Some(turn_player), what);
            }
        }
    }
    for offer in activate::offers(&ctx, me)
        .into_iter()
        .filter(|_| winner.is_none())
    {
        view = view.offer_card_enabled(
            offer.label,
            offer.source,
            TurnEvent::Activate {
                source: offer.source,
                ability: offer.index,
            }
            .encode(),
            offer.enabled,
        );
    }
    view = free_table_offer(view, state, me, turn_player);
    view = offer_concede(view, state, me, winner);
    for line in &state.log {
        view = view.line(line.clone());
    }
    view = with_narration(view, state);
    view.legal(legal::highlights(&ctx, me))
        .arrows(legal::arrows(&ctx, me))
        .chain(legal::chain_rows(&ctx, me))
}

pub fn present(request: &Request) -> PluginView {
    let view = present_game(request);
    if request.seat < request.players
        && !GameBlob::decode(&request.plugin_state).is_some_and(|state| state.manual)
    {
        view.offer_hidden(
            agni_plugin_sdk::manual::DISABLE,
            agni_plugin_sdk::manual::Command::Disable.encode(),
        )
    } else {
        view
    }
}

fn present_game(request: &Request) -> PluginView {
    let state = GameBlob::decode(&request.plugin_state).unwrap_or_else(|| GameBlob {
        mode: rules::Options::of(&request.table).starting_mode(),
        ..GameBlob::default()
    });
    let Some(core) = state.core() else {
        let fresh;
        let roll = match state.roll() {
            Some(roll) => roll,
            None => {
                fresh = Roll::new(request.players, DIE_SIDES);
                &fresh
            }
        };
        return lobby(request, roll, state.mode);
    };
    match state.mode {
        Mode::Free => free(request, &state, core),
        Mode::Enforced => enforced(request, &state, core),
    }
}

pub fn present_bytes(request: &[u8]) -> Vec<u8> {
    match view::parse(request) {
        Some(request) => present(&request),
        None => PluginView::default(),
    }
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_plugin_sdk::dice::commitment;
    use agni_plugin_sdk::turns::PassWindow;
    use agni_plugin_sdk::view::ZoneSummary;

    fn request(state: Option<&GameBlob>, players: u8, seat: u8) -> Request {
        let zones: Vec<ZoneSummary> = (0..3)
            .map(|slot| ZoneSummary {
                id: 9 + slot,
                shared: true,
                battlefield: true,
                label: format!("Battlefield {}", slot + 1),
                ..Default::default()
            })
            .chain(std::iter::once(ZoneSummary {
                id: 8,
                shared: false,
                battlefield: true,
                label: "Base".into(),
                ..Default::default()
            }))
            .collect();
        Request {
            plugin_state: state.map(GameBlob::encode).unwrap_or_default(),
            players,
            seat,
            zones: zones.clone(),
            table: agni_plugin_sdk::table::Snapshot {
                players,
                zones,
                ..Default::default()
            },
        }
    }

    fn labels(view: &PluginView) -> Vec<&str> {
        view.affordances
            .iter()
            .enumerate()
            .filter(|(index, _)| !view.hidden.contains(&(*index as u16)))
            .map(|(_, affordance)| affordance.label.as_str())
            .collect()
    }

    fn hidden_labels(view: &PluginView) -> Vec<&str> {
        view.hidden
            .iter()
            .map(|index| view.affordances[usize::from(*index)].label.as_str())
            .filter(|label| *label != agni_plugin_sdk::manual::DISABLE)
            .collect()
    }

    fn decided() -> (GameBlob, u8) {
        let mut state = GameBlob::lobby(2);
        loop {
            let mut roll = Roll::new(2, DIE_SIDES);
            let secret = |seat: u8| [seat + state.next_prompt as u8 + 1; 8];
            for seat in 0..2u8 {
                roll.commit(seat, commitment(&secret(seat))).unwrap();
            }
            for seat in 0..2u8 {
                roll.reveal(seat, secret(seat)).unwrap();
            }
            if let Outcome::Winner(winner) = roll.outcome() {
                state.lobby = Some(roll);
                return (state, winner);
            }
            state.next_prompt += 1;
        }
    }

    #[test]
    fn the_turn_player_can_end_the_turn_and_the_other_seat_only_waits() {
        let state = GameBlob::start(2, 0, Mode::Free);
        let mine = present(&request(Some(&state), 2, 0));
        assert_eq!(labels(&mine), ["end turn"]);
        assert_eq!(mine.affordances[0].hotkey.as_deref(), Some(ADVANCE_KEY));
        assert_eq!(
            TurnEvent::decode(&mine.affordances[0].data),
            Some(TurnEvent::EndTurn)
        );
        assert_eq!(
            mine.status[0],
            "turn 1 · {seat 0} · action phase · free table"
        );
        assert_eq!(mine.status[1], "points · {seat 0} 0 · {seat 1} 0");
        assert_eq!(mine.prompt, None);
        let theirs = present(&request(Some(&state), 2, 1));
        assert!(labels(&theirs).is_empty());
        assert_eq!(theirs.status[2], "waiting for {seat 0}");
    }

    #[test]
    fn a_showdown_hands_pass_to_the_focus_holder_only() {
        let mut open = GameBlob::start(2, 0, Mode::Free);
        open.showdown = Some(Showdown {
            window: PassWindow::open(1),
            ..Showdown::open(9, 0, 1)
        });
        let attacker = present(&request(Some(&open), 2, 0));
        assert!(labels(&attacker).is_empty());
        assert_eq!(
            attacker.status[2],
            "showdown at {zone 9} · {seat 0} attacks {seat 1}"
        );
        assert_eq!(attacker.status[4], "waiting for {seat 1}");
        let defender = present(&request(Some(&open), 2, 1));
        assert_eq!(labels(&defender), ["pass"]);
        assert_eq!(defender.affordances[0].hotkey.as_deref(), Some(PASS_KEY));
        assert_eq!(
            TurnEvent::decode(&defender.affordances[0].data),
            Some(TurnEvent::Pass)
        );
        assert!(present_bytes(&[]).len() > 2);
    }

    #[test]
    fn the_points_line_grows_an_xp_tail_only_while_a_seat_has_xp() {
        use agni_plugin_sdk::table::CounterInfo;
        let state = GameBlob::start(2, 0, Mode::Enforced);
        let mut with_xp = request(Some(&state), 2, 0);
        let quiet = present(&with_xp);
        assert_eq!(quiet.status[1], "points · {seat 0} 0 · {seat 1} 0");
        with_xp.table.counters.push(CounterInfo {
            target: Target::Seat(1),
            counter: COUNTER_XP,
            value: 3,
        });
        with_xp.table.counters.push(CounterInfo {
            target: Target::Seat(0),
            counter: COUNTER_XP,
            value: 1,
        });
        with_xp.table.counters.sort();
        let view = present(&with_xp);
        assert_eq!(
            view.status[1],
            "points · {seat 0} 0 · {seat 1} 0 · xp {seat 0} 1 · {seat 1} 3"
        );
        assert_eq!(&view.status[2..], &quiet.status[2..], "nothing else moves");
        let free = GameBlob::start(2, 0, Mode::Free);
        let mut free_request = request(Some(&free), 2, 0);
        free_request.table.counters.push(CounterInfo {
            target: Target::Seat(0),
            counter: COUNTER_XP,
            value: 2,
        });
        let view = present(&free_request);
        assert!(view
            .status
            .iter()
            .any(|line| line == "points · {seat 0} 0 · {seat 1} 0 · xp {seat 0} 2 · {seat 1} 0"));
    }

    #[test]
    fn the_roll_winner_picks_the_mode_and_who_goes_first() {
        let (state, winner) = decided();
        let loser = 1 - winner;
        let mine = present(&request(Some(&state), 2, winner));
        assert_eq!(
            labels(&mine),
            [
                "go first",
                &format!("let {{seat {loser}}} go first"),
                "switch to rules enforced"
            ]
        );
        assert!(mine.status.contains(&"mode: free table".to_string()));
        assert!(mine
            .status
            .iter()
            .any(|line| line.starts_with("every deck must be dealt before the start")));
        assert_eq!(
            TurnEvent::decode(&mine.affordances[2].data),
            Some(TurnEvent::SetMode {
                mode: Mode::Enforced
            })
        );
        let mut enforced = state.clone();
        enforced.mode = Mode::Enforced;
        let switched = present(&request(Some(&enforced), 2, winner));
        assert_eq!(labels(&switched)[2], "switch to free table");
        assert!(switched
            .status
            .contains(&"mode: rules enforced".to_string()));
        let theirs = present(&request(Some(&enforced), 2, loser));
        assert!(labels(&theirs).is_empty());
        assert!(theirs.status.contains(&"mode: rules enforced".to_string()));
        let fresh = present(&request(None, 2, 0));
        assert_eq!(fresh.status[0], "roll for first player");
        assert_eq!(labels(&fresh), ["roll"]);
    }

    #[test]
    fn an_open_prompt_is_summarised_and_offered_to_its_seat_only() {
        let mut asked = GameBlob::start(2, 0, Mode::Free);
        asked.prompt = Some(Prompt::new(7, 1, 0, 2));
        let theirs = present(&request(Some(&asked), 2, 0));
        let summary = theirs.prompt.clone().expect("a summary for everyone");
        assert_eq!((summary.seat, summary.min, summary.max), (1, 0, 2));
        assert!(summary.optional);
        assert!(labels(&theirs).is_empty());
        assert_eq!(
            theirs.status.last().unwrap(),
            "waiting for {seat 1}: choose"
        );
        let mine = present(&request(Some(&asked), 2, 1));
        assert_eq!(labels(&mine), ["done", "skip"]);
        assert_eq!(mine.affordances[0].hotkey, None);
        assert_eq!(mine.affordances[1].hotkey.as_deref(), Some(ESCAPE_KEY));
        assert_eq!(
            TurnEvent::decode(&mine.affordances[1].data),
            Some(TurnEvent::Pick(Pick {
                prompt: 7,
                option: 1
            }))
        );
        assert_eq!(mine.prompt, theirs.prompt);
        let mut cancellable = GameBlob::start(2, 0, Mode::Free);
        cancellable.prompt = Some(Prompt::new(8, 1, 0, 1).cancellable());
        let escapes = present(&request(Some(&cancellable), 2, 1));
        assert_eq!(labels(&escapes), ["skip", "cancel"]);
        assert_eq!(escapes.affordances[0].hotkey, None);
        assert_eq!(escapes.affordances[1].hotkey.as_deref(), Some(ESCAPE_KEY));
    }

    #[test]
    fn the_enforced_table_offers_the_panic_button_to_the_turn_player_then_the_rest() {
        let enforced = GameBlob::start(2, 0, Mode::Enforced);
        let mine = present(&request(Some(&enforced), 2, 0));
        assert_eq!(labels(&mine), ["end turn", "free table"]);
        assert_eq!(
            mine.status[0],
            "turn 1 · {seat 0} · action phase · rules enforced"
        );
        let theirs = present(&request(Some(&enforced), 2, 1));
        assert!(labels(&theirs).is_empty());
        let mut proposed = enforced;
        proposed.free_table = Some(0);
        let waiting = present(&request(Some(&proposed), 2, 0));
        assert_eq!(labels(&waiting), ["end turn"]);
        assert_eq!(
            hidden_labels(&waiting),
            [WITHDRAW_FREE_TABLE, CONCEDE],
            "the proposer may take it back from the table menu"
        );
        assert!(waiting
            .status
            .contains(&"free table proposed · waiting for another seat".to_string()));
        let confirm = present(&request(Some(&proposed), 2, 1));
        assert!(labels(&confirm).is_empty());
        assert_eq!(
            hidden_labels(&confirm),
            [CONFIRM_FREE_TABLE, CONCEDE],
            "the confirmation is a table-menu verb, never a strip chip"
        );
        assert_eq!(
            TurnEvent::decode(&confirm.affordances[0].data),
            Some(TurnEvent::FreeTable)
        );
        assert!(confirm
            .status
            .contains(&"{seat 0} proposes a free table".to_string()));
        let mut freed = proposed;
        freed.mode = Mode::Free;
        freed.free_table = None;
        freed.narrate("{seat 0} and {seat 1} freed the table");
        let after = present(&request(Some(&freed), 2, 1));
        assert!(after
            .status
            .contains(&"{seat 0} and {seat 1} freed the table".to_string()));
        assert!(labels(&after).is_empty());
    }

    #[test]
    fn the_structured_fields_mirror_the_status_lines_on_the_enforced_fixture() {
        use crate::engine::fixtures;
        use agni_plugin_sdk::table::{CounterInfo, Target};
        let state = GameBlob::start(2, 1, Mode::Enforced);
        let mut request = enforced_request(&state, 0);
        request.table.counters.push(CounterInfo {
            target: Target::Seat(1),
            counter: COUNTER_XP,
            value: 2,
        });
        let theirs = present(&request);
        assert_eq!(
            theirs.turn,
            Some(TurnInfo {
                number: 1,
                seat: 1,
                phase: "action phase".into(),
                phases: Phase::ALL
                    .iter()
                    .map(|phase| phase.label().to_string())
                    .collect(),
                mode: "rules enforced".into(),
            })
        );
        assert_eq!(
            theirs.seats,
            [
                SeatInfo {
                    seat: 0,
                    points: 0,
                    victory: 8,
                    xp: 0,
                    hand: 4,
                    deck: 4,
                    runes_ready: 3,
                    runes_total: 4,
                },
                SeatInfo {
                    seat: 1,
                    points: 0,
                    victory: 8,
                    xp: 2,
                    hand: 1,
                    deck: 2,
                    runes_ready: 2,
                    runes_total: 2,
                }
            ]
        );
        assert_eq!(
            theirs.waiting,
            Some(view::Waiting {
                seat: Some(1),
                what: "their action phase".into()
            })
        );
        assert_eq!(theirs.primary, None);
        assert_eq!(hidden_labels(&theirs), [CONCEDE]);
        assert!(theirs.narration.is_empty());

        let mine = present(&enforced_request(&state, 1));
        assert_eq!(mine.waiting, None);
        assert_eq!(labels(&mine), ["end turn", "free table"]);
        assert_eq!(mine.primary, Some(0), "end turn is the primary");
        assert_eq!(mine.turn.as_ref().unwrap().seat, 1);

        let mut narrated = state.clone();
        narrated.narrate("{seat 1} plays {card 81}");
        narrated.narrate("{card 50} dies");
        let told = present(&enforced_request(&narrated, 0));
        assert_eq!(
            told.narration,
            ["{seat 1} plays {card 81}", "{card 50} dies"]
        );
        assert_eq!(
            &told.status[told.status.len() - 2..],
            &told.narration[..],
            "the old lines keep being emitted for one release"
        );

        let mut prompted = GameBlob::start(2, 0, Mode::Enforced);
        prompted.open_prompt(crate::state::Ask {
            prompt: Prompt::new(3, 0, 1, 1).cancellable(),
            why: crate::state::PromptWhy::PlayLocation { item: 1 },
        });
        prompted.queue.push(crate::state::Pending {
            item: crate::state::ChainItem::new(
                1,
                crate::state::ItemKind::Permanent {
                    card: fixtures::HAND_UNIT,
                },
                0,
                crate::state::Origin::Hand,
            ),
            needs: crate::state::Needs::Choices,
        });
        let asked = present(&enforced_request(&prompted, 1));
        assert_eq!(
            asked.waiting,
            Some(view::Waiting {
                seat: Some(0),
                what: "where does {card 70} enter?".into()
            })
        );
        let answering = present(&enforced_request(&prompted, 0));
        assert_eq!(answering.waiting, None);
        assert_eq!(
            answering.primary, None,
            "a location prompt has no confirm option"
        );

        let mut closed = GameBlob::start(2, 0, Mode::Enforced);
        closed.priority = Some(crate::state::Priority {
            active: 1,
            passes: 0,
        });
        let holder = present(&enforced_request(&closed, 1));
        assert_eq!(labels(&holder), ["pass"]);
        assert_eq!(holder.primary, Some(0));
        let waiting = present(&enforced_request(&closed, 0));
        assert_eq!(
            waiting.waiting,
            Some(view::Waiting {
                seat: Some(1),
                what: "respond or pass".into()
            })
        );
    }

    #[test]
    fn the_free_table_and_the_lobby_fill_the_same_fields() {
        let state = GameBlob::start(2, 0, Mode::Free);
        let mine = present(&request(Some(&state), 2, 0));
        assert_eq!(
            mine.turn.as_ref().map(|turn| turn.mode.as_str()),
            Some("free table")
        );
        assert_eq!(mine.turn.as_ref().unwrap().phases.len(), 9);
        assert_eq!(mine.seats.len(), 2);
        assert_eq!(mine.seats[1].victory, 8);
        assert_eq!(mine.primary, Some(0));
        assert_eq!(labels(&mine), ["end turn"]);
        assert_eq!(hidden_labels(&mine), [CONCEDE]);
        let theirs = present(&request(Some(&state), 2, 1));
        assert_eq!(
            theirs.waiting,
            Some(view::Waiting {
                seat: Some(0),
                what: String::new()
            })
        );
        assert_eq!(theirs.primary, None);

        let mut conceded = state.clone();
        conceded.concede(1);
        conceded.narrate("{seat 1} concedes");
        let over = present(&request(Some(&conceded), 2, 0));
        assert_eq!(over.winner, Some(0));
        assert_eq!(over.narration, ["{seat 1} concedes"]);
        assert!(
            hidden_labels(&over).is_empty(),
            "nobody concedes a finished game"
        );
        let loser = present(&request(Some(&conceded), 2, 1));
        assert!(hidden_labels(&loser).is_empty());

        let fresh = present(&request(None, 2, 0));
        assert_eq!(fresh.turn, None);
        assert_eq!(fresh.seats.len(), 2);
        assert_eq!(fresh.primary, Some(0), "roll is the lobby's primary");
        assert_eq!(fresh.waiting, None);
        let (decided, winner) = decided();
        let loser = 1 - winner;
        let chooser = present(&request(Some(&decided), 2, winner));
        assert_eq!(
            chooser.primary, None,
            "who goes first is a choice between chips, not a primary"
        );
        assert_eq!(chooser.waiting, None);
        let waits = present(&request(Some(&decided), 2, loser));
        assert_eq!(
            waits.waiting,
            Some(view::Waiting {
                seat: Some(winner),
                what: "to choose the mode and who goes first".into()
            })
        );
        assert_eq!(waits.primary, None);
    }

    fn enforced_request(state: &GameBlob, seat: u8) -> Request {
        let table = crate::engine::fixtures::table();
        Request {
            plugin_state: state.encode(),
            players: 2,
            seat,
            zones: table.zones.clone(),
            table,
        }
    }

    #[test]
    fn the_enforced_prompt_comes_first_with_x_on_the_escape_and_the_rest_waits() {
        use crate::engine::fixtures;
        use crate::state::{Ask, ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy};
        let mut asked = GameBlob::start(2, 0, Mode::Enforced);
        asked.queue.push(Pending {
            item: ChainItem::new(
                1,
                ItemKind::Permanent {
                    card: fixtures::HAND_UNIT,
                },
                0,
                Origin::Hand,
            ),
            needs: Needs::Choices,
        });
        asked.set_holder(fixtures::BF1, Some(0));
        asked.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1).cancellable(),
            why: PromptWhy::PlayLocation { item: 1 },
        });
        asked.narrate("{seat 0} did a thing");
        let mine = present(&enforced_request(&asked, 0));
        assert_eq!(
            labels(&mine),
            ["your base", "{zone 9}", "cancel", "free table"]
        );
        assert_eq!(mine.affordances[2].hotkey.as_deref(), Some(ESCAPE_KEY));
        assert_eq!(
            TurnEvent::decode(&mine.affordances[1].data),
            Some(TurnEvent::Pick(Pick {
                prompt: 3,
                option: 1
            }))
        );
        assert_eq!(
            mine.prompt.as_ref().unwrap().why,
            "where does {card 70} enter?"
        );
        assert_eq!(
            mine.status,
            [
                "turn 1 · {seat 0} · action phase · rules enforced",
                "points · {seat 0} 0 · {seat 1} 0",
                "{zone 9} held by {seat 0}",
                "where does {card 70} enter?",
                "{seat 0} did a thing"
            ]
        );
        let theirs = present(&enforced_request(&asked, 1));
        assert!(labels(&theirs).is_empty());
        assert_eq!(theirs.prompt, mine.prompt);
        assert!(theirs
            .status
            .contains(&"waiting for {seat 0}: where does {card 70} enter?".to_string()));
        assert_eq!(theirs.status.last().unwrap(), "{seat 0} did a thing");
    }

    #[test]
    fn an_activatable_ability_is_its_own_affordance_and_a_price_it_cannot_pay_is_greyed_out() {
        use crate::engine::fixtures;
        let state = GameBlob::start(2, 0, Mode::Enforced);
        let short = present(&enforced_request(&state, 0));
        let offer = short
            .affordances
            .iter()
            .find(|affordance| affordance.card == Some(fixtures::LEGEND_CARD))
            .expect("the legend's ability is on the strip");
        assert_eq!(offer.label, "{card 75}: play a Sprite (4 energy, exhaust)");
        assert!(
            !offer.enabled,
            "three ready runes cannot pay four: shown, not hidden"
        );
        let mut richer = enforced_request(&state, 0);
        richer
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        let mine = present(&richer);
        let offer = mine
            .affordances
            .iter()
            .find(|affordance| affordance.card == Some(fixtures::LEGEND_CARD))
            .expect("a fourth ready rune pays for it");
        assert!(offer.enabled);
        assert_eq!(
            TurnEvent::decode(&offer.data),
            Some(TurnEvent::Activate {
                source: fixtures::LEGEND_CARD,
                ability: 0
            })
        );
        assert_eq!(offer.hotkey, None);
        let theirs = present(&{
            let mut request = richer;
            request.seat = 1;
            request
        });
        assert!(
            !theirs
                .affordances
                .iter()
                .any(|affordance| affordance.card == Some(fixtures::LEGEND_CARD)),
            "the other seat is never offered someone else's ability"
        );
    }

    #[test]
    fn an_open_prompt_hides_every_activation_offer() {
        use crate::engine::fixtures;
        use crate::state::{Ask, PromptWhy};
        let mut asked = GameBlob::start(2, 0, Mode::Enforced);
        asked.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1),
            why: PromptWhy::Assign,
        });
        let mut request = enforced_request(&asked, 0);
        request
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        let mine = present(&request);
        assert!(
            !mine
                .affordances
                .iter()
                .any(|affordance| affordance.card == Some(fixtures::LEGEND_CARD)),
            "374: nothing is activated while a prompt is open"
        );
    }

    #[test]
    fn the_enforced_showdown_offers_pass_to_the_focus_holder_and_names_the_contest() {
        use crate::engine::fixtures;
        let mut open = GameBlob::start(2, 0, Mode::Enforced);
        open.set_holder(fixtures::BF2, Some(1));
        open.set_contested(fixtures::BF2, Some(0));
        open.showdown = Some(Showdown {
            combat: true,
            window: PassWindow::open(1),
            ..Showdown::open(fixtures::BF2, 0, 1)
        });
        let attacker = present(&enforced_request(&open, 0));
        assert_eq!(labels(&attacker), ["free table"]);
        assert!(attacker
            .status
            .contains(&"{zone 10} held by {seat 1}, contested by {seat 0}".to_string()));
        assert!(attacker.status.contains(
            &"combat at {zone 10} · {seat 0} against {seat 1} · focus {seat 1}".to_string()
        ));
        assert!(attacker
            .status
            .contains(&"waiting for {seat 1}: pass or respond at {zone 10}".to_string()));
        let defender = present(&enforced_request(&open, 1));
        assert_eq!(labels(&defender), ["pass"]);
        assert_eq!(defender.affordances[0].hotkey.as_deref(), Some(PASS_KEY));
        assert_eq!(
            TurnEvent::decode(&defender.affordances[0].data),
            Some(TurnEvent::Pass)
        );
        let mut closed = GameBlob::start(2, 0, Mode::Enforced);
        closed.priority = Some(crate::state::Priority {
            active: 1,
            passes: 0,
        });
        closed.chain.push(crate::state::ChainItem::new(
            2,
            crate::state::ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            crate::state::Origin::Hand,
        ));
        let holder = present(&enforced_request(&closed, 1));
        assert_eq!(labels(&holder), ["pass"]);
        assert!(holder
            .status
            .contains(&"chain: {card 71} (top)".to_string()));
        let waiting = present(&enforced_request(&closed, 0));
        assert_eq!(labels(&waiting), ["free table"]);
        assert!(waiting
            .status
            .contains(&"waiting for {seat 1}: respond or pass".to_string()));
    }

    #[test]
    fn the_enforced_view_carries_the_legal_list_for_its_own_seat_and_the_arrows_for_both() {
        use crate::engine::fixtures;
        use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
        use agni_plugin_sdk::view::{Arrow, ArrowKind, Legal, LegalKind, Origin as ArrowFrom};
        let mut state = GameBlob::start(2, 0, Mode::Enforced);
        state.set_phase(Phase::Action);
        state.seats = vec![Default::default(); 2];
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.targets = vec![TargetRef::Card(fixtures::THEIR_UNIT)];
        state.chain.push(item);
        state.priority = Some(crate::state::Priority {
            active: 0,
            passes: 0,
        });
        let aimed = vec![Arrow {
            from: ArrowFrom::Card(fixtures::HAND_SPELL),
            to: agni_plugin_sdk::view::TargetRef::Card(fixtures::THEIR_UNIT),
            kind: ArrowKind::Spell,
        }];
        let mine = present(&enforced_request(&state, 0));
        assert_eq!(mine.arrows, aimed);
        let theirs = present(&enforced_request(&state, 1));
        assert_eq!(theirs.arrows, aimed, "both seats see the same arrows");
        assert!(theirs.legal.is_empty(), "legality is the viewing seat's");

        let mut open = GameBlob::start(2, 0, Mode::Enforced);
        open.set_phase(Phase::Action);
        open.seats = vec![Default::default(); 2];
        let view = present(&enforced_request(&open, 0));
        assert!(view.arrows.is_empty());
        assert_eq!(
            view.legal,
            vec![
                Legal {
                    card: fixtures::VI,
                    kinds: vec![LegalKind::March],
                    zones: vec![fixtures::BF1, fixtures::BF2],
                    hidden: Vec::new(),
                },
                Legal {
                    card: fixtures::HAND_UNIT,
                    kinds: vec![LegalKind::Play { accelerate: false }],
                    zones: vec![fixtures::BASE, fixtures::CHAIN],
                    hidden: Vec::new(),
                },
                Legal {
                    card: fixtures::HAND_SPELL,
                    kinds: vec![LegalKind::Play { accelerate: false }],
                    zones: vec![fixtures::CHAIN],
                    hidden: Vec::new(),
                },
                Legal {
                    card: fixtures::HAND_GEAR,
                    kinds: vec![LegalKind::Play { accelerate: false }],
                    zones: vec![fixtures::BASE, fixtures::CHAIN],
                    hidden: Vec::new(),
                },
                Legal {
                    card: fixtures::CHAMPION_CARD,
                    kinds: vec![LegalKind::Play { accelerate: false }],
                    zones: vec![fixtures::BASE, fixtures::CHAIN],
                    hidden: Vec::new(),
                },
            ]
        );
        let free = present(&enforced_request(&GameBlob::start(2, 0, Mode::Free), 0));
        assert!(
            free.legal.is_empty() && free.arrows.is_empty(),
            "a free table enforces nothing, so it highlights nothing"
        );
    }

    #[test]
    fn the_enforced_turn_player_ends_the_turn_and_a_winner_stops_the_offers() {
        use crate::engine::fixtures;
        use agni_plugin_sdk::table::{CounterInfo, Target};
        let state = GameBlob::start(2, 1, Mode::Enforced);
        let theirs = present(&enforced_request(&state, 0));
        assert!(labels(&theirs).is_empty());
        assert_eq!(
            theirs.status,
            [
                "turn 1 · {seat 1} · action phase · rules enforced",
                "points · {seat 0} 0 · {seat 1} 0",
                "waiting for {seat 1}: their action phase"
            ]
        );
        let mine = present(&enforced_request(&state, 1));
        assert_eq!(labels(&mine), ["end turn", "free table"]);
        let mut setup = state.clone();
        setup.set_phase(Phase::Setup);
        let early = present(&enforced_request(&setup, 1));
        assert_eq!(labels(&early), ["free table"]);
        assert!(early.status.contains(&"setup · mulligans".to_string()));
        let mut request = enforced_request(&state, 1);
        request.table.counters.push(CounterInfo {
            target: Target::Seat(0),
            counter: rules::COUNTER_POINTS,
            value: rules::DEFAULT_VICTORY_SCORE,
        });
        let won = present(&request);
        assert_eq!(won.winner, Some(0));
        assert_eq!(labels(&won), ["free table"]);
        assert_eq!(won.status[1], "points · {seat 0} 8 · {seat 1} 0");
        assert_eq!(won.status[2], "{seat 0} wins with 8 points");
        let _ = fixtures::BF1;
    }
}

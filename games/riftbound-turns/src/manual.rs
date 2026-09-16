use crate::{GameBlob, Mode, Phase, Refusal, TurnCore, TurnEvent};
use agni_plugin_sdk::decide::{Action, Effect, Request, Verdict, BOTTOM};
use agni_plugin_sdk::manual::Command;
use agni_plugin_sdk::table::ZoneKind;

pub fn command(
    request: &Request,
    current: Option<GameBlob>,
    bytes: &[u8],
) -> Result<Verdict, Refusal> {
    if request.seat >= request.players {
        return Err(Refusal::NoSuchSeat);
    }
    let command = Command::decode(bytes).ok_or(Refusal::BadEvent)?;
    let mut blob =
        current.unwrap_or_else(|| GameBlob::start(request.players, request.seat, Mode::Free));
    if command == Command::Disable {
        if !blob.manual {
            disable(&mut blob, request.players, request.seat);
        }
        return Ok(Verdict::advance(blob.encode()));
    }
    if !blob.manual {
        return Err(Refusal::BadEvent);
    }
    let mut effects = Vec::new();
    let actor = request.seat;
    match command {
        Command::Disable => unreachable!(),
        Command::Look { zone, count } => {
            let deck = deck(request, zone)?;
            effects.extend(
                deck.into_iter()
                    .rev()
                    .take(count as usize)
                    .map(|card| Effect::Peek { card, seat: actor }),
            );
            blob.narrate(format!(
                "{{seat {actor}}} privately looks at {} cards in {{zone {zone}}}",
                effects.len()
            ));
        }
        Command::Shuffle { zone, seed } => {
            let deck = deck(request, zone)?;
            effects.extend(deck.iter().map(|card| Effect::Conceal { card: *card }));
            for index in crate::engine::roll::permutation(seed, deck.len()) {
                effects.push(Effect::Move {
                    card: deck[index],
                    zone,
                    seat: actor,
                    index: BOTTOM,
                });
            }
            blob.narrate(format!("{{seat {actor}}} shuffles {{zone {zone}}}"));
        }
        Command::Turn { seat, number } => {
            if seat >= request.players || number == 0 {
                return Err(Refusal::NoSuchSeat);
            }
            let core = blob.core_mut().ok_or(Refusal::NotStarted)?;
            core.player = seat;
            core.turn = number;
            blob.narrate(format!(
                "{{seat {actor}}} sets turn {number} to {{seat {seat}}}"
            ));
        }
        Command::Control { zone, seat } => {
            if seat.is_some_and(|seat| seat >= request.players)
                || !request
                    .table
                    .zone(zone)
                    .is_some_and(|zone| zone.shared && zone.battlefield)
            {
                return Err(Refusal::BadEvent);
            }
            blob.set_holder(zone, seat);
            blob.set_contested(zone, None);
            blob.narrate(format!(
                "{{seat {actor}}} changes control of {{zone {zone}}}"
            ));
        }
        Command::RemoveToken { card } => {
            if !request.table.tokens.contains(&card)
                || !request
                    .table
                    .card(card)
                    .is_some_and(|card| card.owner == actor)
            {
                return Err(Refusal::BadEvent);
            }
            effects.push(Effect::Despawn { card });
            blob.narrate(format!("{{seat {actor}}} removes token {{card {card}}}"));
        }
        Command::Reveal { card } => {
            if !request
                .table
                .card(card)
                .is_some_and(|card| card.owner == actor)
            {
                return Err(Refusal::BadEvent);
            }
            effects.push(Effect::Reveal { card });
            blob.narrate(format!("{{seat {actor}}} reveals {{card {card}}}"));
        }
        Command::Conceal { card } => {
            if !request
                .table
                .card(card)
                .is_some_and(|card| card.owner == actor)
            {
                return Err(Refusal::BadEvent);
            }
            effects.push(Effect::Conceal { card });
            blob.narrate(format!("{{seat {actor}}} turns {{card {card}}} face down"));
        }
    }
    Ok(Verdict::advance(blob.encode()).with_effects(effects))
}

fn deck(request: &Request, zone: u16) -> Result<Vec<u32>, Refusal> {
    if !request
        .table
        .zone(zone)
        .is_some_and(|zone| zone.kind == ZoneKind::Deck && !zone.shared)
    {
        return Err(Refusal::BadEvent);
    }
    Ok(request
        .table
        .held(zone, request.seat)
        .map(|card| card.id)
        .collect())
}

fn disable(blob: &mut GameBlob, players: u8, seat: u8) {
    blob.manual = true;
    blob.mode = Mode::Free;
    blob.lobby = None;
    blob.roll = None;
    blob.turn
        .get_or_insert_with(|| TurnCore::start(players, seat));
    blob.set_phase(Phase::Action);
    blob.prompt = None;
    blob.why = None;
    blob.priority = None;
    blob.showdown = None;
    blob.free_table = None;
    blob.chain.clear();
    blob.queue.clear();
    blob.staged.clear();
    blob.delayed.clear();
    blob.extra_turns.clear();
    blob.preventions.clear();
    blob.won = None;
    blob.conceded.clear();
    for control in &mut blob.control {
        control.contested = None;
    }
    blob.narrate(format!(
        "{{seat {seat}}} disabled rules enforcement · continue manually"
    ));
}

pub fn decide(request: &Request, mut blob: GameBlob) -> Result<Verdict, Refusal> {
    match &request.action {
        Action::Game(bytes) => match TurnEvent::decode(bytes).ok_or(Refusal::BadEvent)? {
            TurnEvent::EndTurn => {
                let core = blob.core_mut().ok_or(Refusal::NotStarted)?;
                core.players = request.players;
                core.advance();
                blob.narrate(format!(
                    "{{seat {}}} ends the turn · resolve the next turn manually",
                    request.seat
                ));
            }
            TurnEvent::Concede => {
                blob.concede(request.seat);
                blob.narrate(format!("{{seat {}}} concedes", request.seat));
            }
            _ => return Err(Refusal::BadEvent),
        },
        Action::Reset => return Ok(Verdict::advance(Vec::new())),
        Action::Move { card, .. } => blob.narrate(format!(
            "{{seat {}}} moves {{card {card}}} manually",
            request.seat
        )),
        Action::Counter { delta, .. } => blob.narrate(format!(
            "{{seat {}}} changes a counter by {delta:+}",
            request.seat
        )),
        Action::Annotate { card, .. } => blob.narrate(format!(
            "{{seat {}}} marks {{card {card}}} manually",
            request.seat
        )),
        _ => return Ok(Verdict::accept()),
    }
    Ok(Verdict::advance(blob.encode()))
}

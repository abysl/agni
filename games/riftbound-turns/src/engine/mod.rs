pub mod activate;
pub mod attach;
pub mod chain;
pub mod cleanup;
pub mod combat;
pub mod control;
pub mod cost;
pub mod ctx;
pub mod discard;
pub mod expiry;
pub mod hide;
pub mod kill;
pub mod legal;
pub mod march;
pub mod pay;
pub mod phases;
pub mod play;
pub mod prevent;
pub mod priority;
pub mod prompts;
pub mod roll;
pub mod showdown;
pub mod statics;
pub mod targets;
pub mod triggers;

#[cfg(test)]
pub mod fixtures;

use crate::cards::Resolved;
use crate::engine::ctx::Ctx;
use crate::engine::legal::{Intent, Reason};
use crate::engine::prompts::Answered;
use crate::state::{GameBlob, Mode, Phase, PromptWhy};
use crate::{Refusal, TurnEvent};
use agni_plugin_sdk::decide::{Action, Request, Verdict};
use agni_plugin_sdk::prompt::Answer;

const SETTLE_LIMIT: usize = 16;

pub fn start(request: &Request, mut blob: GameBlob) -> Result<Verdict, Refusal> {
    let scripts = Resolved::of(&request.table);
    let mut ctx = Ctx::fresh(&request.table, &mut blob, &scripts, request.seat);
    phases::setup(&mut ctx);
    settle(&mut ctx)?;
    finish(ctx)
}

pub fn decide(request: &Request, mut blob: GameBlob) -> Result<Verdict, Refusal> {
    let scripts = Resolved::of(&request.table);
    let seat = request.seat;
    let mut ctx = Ctx::new(&request.table, &mut blob, &scripts, seat, &request.action);
    match &request.action {
        Action::Game(data) => {
            let event = TurnEvent::decode(data).ok_or(Refusal::BadEvent)?;
            if event != TurnEvent::FreeTable {
                if let Some(winner) = ctx.winner() {
                    return Err(Refusal::GameOver { winner });
                }
            }
            game(&mut ctx, seat, event)?;
        }
        Action::Move { .. } => {
            if let Some(winner) = ctx.winner() {
                return Err(Refusal::GameOver { winner });
            }
            let entry = ctx.entry.ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
            ctx.projected_move_card = Some(entry.card);
            let intent = legal::classify(&ctx, seat, &entry)?;
            ctx.projected_move_card = None;
            act(&mut ctx, seat, intent)?;
        }
        Action::Clear { seat: gone } => {
            leave(&mut ctx, *gone);
            let _ = settle(&mut ctx);
            return Ok(finish_lenient(ctx));
        }
        Action::Reveal { card, .. } => {
            if hide::is_facedown(&ctx, *card) && ctx.controller(*card) != seat {
                return Err(Refusal::Illegal(Reason::NotYourCard));
            }
            let shown = hide::shown(&mut ctx, seat, *card);
            let awaited = discard::revealed(&mut ctx, *card)?;
            let arrived = chain::face_arrived(&mut ctx, *card)?;
            if !shown && !awaited && !arrived {
                return Ok(Verdict::accept());
            }
        }
        Action::Other => return Ok(Verdict::accept()),
        Action::Spawn { .. } => return Err(Refusal::Illegal(Reason::TokensByEffect)),
        Action::Annotate { .. } => return Err(Refusal::Illegal(Reason::AnnotationsAutomatic)),
        Action::Counter { .. } => return Err(Refusal::Illegal(Reason::CountersAutomatic)),
        Action::Deal { .. } => return Err(Refusal::Illegal(Reason::DealIsOver)),
        Action::Reset => return Ok(Verdict::advance(Vec::new())),
        Action::Join => return Ok(Verdict::accept()),
    }
    settle(&mut ctx)?;
    finish(ctx)
}

fn finish(mut ctx: Ctx) -> Result<Verdict, Refusal> {
    ctx.sync_might();
    if ctx.won.is_some() {
        hide::game_over(&mut ctx);
    }
    if ctx.fault.is_some() {
        return Err(Refusal::Illegal(Reason::EngineFault));
    }
    Ok(Verdict::advance(ctx.blob.encode()).with_effects(ctx.effects))
}

fn finish_lenient(mut ctx: Ctx) -> Verdict {
    ctx.sync_might();
    if ctx.won.is_some() {
        hide::game_over(&mut ctx);
    }
    let effects = match ctx.fault {
        Some((index, _)) => ctx.effects[..index].to_vec(),
        None => ctx.effects,
    };
    Verdict::advance(ctx.blob.encode()).with_effects(effects)
}

fn leave(ctx: &mut Ctx, seat: u8) {
    let table = &ctx.table;
    ctx.blob.cards.retain(|row| table.card(row.id).is_some());
    ctx.blob
        .queue
        .retain(|pending| pending.item.controller != seat);
    ctx.blob.chain.retain(|item| item.controller != seat);
    ctx.blob.staged.retain(|staged| staged.contester != seat);
    if ctx.blob.free_table == Some(seat) {
        ctx.blob.free_table = None;
    }
    if ctx
        .blob
        .priority
        .is_some_and(|priority| priority.active == seat)
    {
        ctx.blob.priority = None;
    }
    for zone in ctx.zones.battlefields.clone() {
        if ctx.blob.contester(zone) == Some(seat) {
            ctx.blob.set_contested(zone, None);
        }
        if ctx.blob.holder(zone) == Some(seat) {
            ctx.blob.set_holder(zone, None);
        }
    }
    ctx.narrate(format!("{{seat {seat}}} left the table"));
    if roll::is_open(ctx) {
        roll::abandon(ctx);
    }
    if ctx
        .blob
        .prompt
        .as_ref()
        .is_some_and(|prompt| prompt.seat == seat)
    {
        let why = ctx.blob.close_prompt().map(|(_, why)| why);
        if why == Some(PromptWhy::Mulligan) {
            phases::finish_mulligan(ctx, seat, &[]);
        }
    }
    let involved = ctx.blob.showdown.as_ref().is_some_and(|showdown| {
        showdown.attacker == seat || showdown.defender == seat || showdown.window.has_focus(seat)
    });
    if involved {
        showdown::abandon(ctx);
    }
    if ctx.blob.is_turn_player(seat) && ctx.blob.phase() == Some(Phase::Action) {
        for zone in ctx.zones.battlefields.clone() {
            ctx.blob.set_contested(zone, None);
        }
        ctx.blob.staged.clear();
        ctx.blob.showdown = None;
        ctx.blob.priority = None;
        ctx.blob.chain.clear();
        ctx.blob.queue.clear();
        ctx.blob.prompt = None;
        ctx.blob.why = None;
        if phases::end_turn(ctx).is_ok() {
            return;
        }
    }
    cleanup::run(ctx, None);
}

fn game(ctx: &mut Ctx, seat: u8, event: TurnEvent) -> Result<(), Refusal> {
    match event {
        TurnEvent::CommitRoll { commit } if roll::is_open(ctx) => roll::commit(ctx, seat, commit),
        TurnEvent::RevealRoll { secret } if roll::is_open(ctx) => roll::reveal(ctx, seat, secret),
        TurnEvent::StartGame { .. }
        | TurnEvent::CommitRoll { .. }
        | TurnEvent::RevealRoll { .. }
        | TurnEvent::SetMode { .. } => Err(Refusal::AlreadyStarted),
        TurnEvent::EndTurn => {
            if !ctx.blob.is_turn_player(seat) {
                return Err(Refusal::NotYourTurn);
            }
            phases::end_turn(ctx)
        }
        TurnEvent::Pass => priority::pass(ctx, seat),
        TurnEvent::Pick(pick) => match prompts::answer(ctx, seat, pick)? {
            Some(answered) => resume(ctx, &answered),
            None => Ok(()),
        },
        TurnEvent::Activate { source, ability } => activate::activate(ctx, seat, source, ability),
        TurnEvent::FreeTable => free_table(ctx, seat),
        TurnEvent::Concede => concede(ctx, seat),
    }
}

fn concede(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
    if seat >= ctx.players() {
        return Err(Refusal::NoSuchSeat);
    }
    if !ctx.blob.concede(seat) {
        return Err(Refusal::AlreadyConceded);
    }
    ctx.narrate(format!("{{seat {seat}}} concedes"));
    cleanup::win_check(ctx);
    Ok(())
}

pub fn expire_free_table(ctx: &mut Ctx) {
    if let Some(proposer) = ctx.blob.free_table.take() {
        ctx.narrate(format!(
            "{{seat {proposer}}}'s free table proposal expired with the turn"
        ));
    }
}

fn free_table(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
    if ctx.blob.mode == Mode::Free {
        return Err(Refusal::AlreadyFree);
    }
    match ctx.blob.free_table {
        None => {
            if !ctx.blob.is_turn_player(seat) {
                return Err(Refusal::NotYourTurn);
            }
            ctx.blob.free_table = Some(seat);
        }
        Some(proposer) if proposer == seat => {
            ctx.blob.free_table = None;
            ctx.narrate(format!("{{seat {seat}}} withdraws the free table proposal"));
        }
        Some(proposer) => {
            ctx.blob.free_table = None;
            ctx.blob.mode = Mode::Free;
            ctx.blob.prompt = None;
            ctx.blob.why = None;
            ctx.blob.queue.clear();
            ctx.blob.staged.clear();
            ctx.blob.priority = None;
            ctx.blob.chain.clear();
            if ctx.blob.phase() != Some(Phase::Action) {
                ctx.blob.set_phase(Phase::Action);
            }
            ctx.narrate(format!(
                "{{seat {proposer}}} and {{seat {seat}}} freed the table"
            ));
        }
    }
    Ok(())
}

pub(crate) fn act(ctx: &mut Ctx, seat: u8, intent: Intent) -> Result<(), Refusal> {
    ctx.projected_move_card = None;
    match intent {
        Intent::Play {
            card,
            origin,
            location,
            ..
        } => play::begin(ctx, seat, card, origin, location),
        Intent::StandardMove { unit, from, to } => {
            march::standard_move(ctx, seat, unit, from, to);
            Ok(())
        }
        Intent::Mulligan { card } => phases::mulligan_gesture(ctx, seat, card),
        Intent::Discard { card } => discard::gesture(ctx, seat, card),
        Intent::Hide { card, zone } => hide::hide(ctx, seat, card, zone),
        Intent::PlayFromFacedown { card } => hide::play_from_facedown(ctx, seat, card),
    }
}

pub(crate) fn resume(ctx: &mut Ctx, answered: &Answered) -> Result<(), Refusal> {
    let prompt = &answered.prompt;
    let answer = answered.answer;
    match answered.why {
        PromptWhy::Mulligan => {
            phases::finish_mulligan(ctx, prompt.seat, &prompt.picked);
            Ok(())
        }
        PromptWhy::PlayLocation { item } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            Answer::Zone(zone) => play::choose_location(ctx, item, zone),
            _ => Ok(()),
        },
        PromptWhy::OptionalCost { item, .. } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            Answer::Yes => play::choose_cost(ctx, item, true),
            _ => play::choose_cost(ctx, item, false),
        },
        PromptWhy::PayWith { item } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            Answer::Card(gold) => play::choose_payment(ctx, item, Some(gold)),
            _ => play::choose_payment(ctx, item, None),
        },
        PromptWhy::GroupMove { to, .. } => {
            march::group_move(ctx, prompt.seat, to, &prompt.picked);
            Ok(())
        }
        PromptWhy::PickStaged => match answer {
            Answer::Zone(zone) => showdown::pick(ctx, zone),
            _ => Ok(()),
        },
        PromptWhy::Assign => match answer {
            Answer::Card(unit) => combat::choose(ctx, unit),
            _ => Ok(()),
        },
        PromptWhy::Target { item, spec } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            _ => play::choose_targets(ctx, item, spec, &prompt.picked),
        },
        PromptWhy::OrderTriggers { seat } => {
            triggers::order(ctx, seat, &prompt.picked);
            Ok(())
        }
        PromptWhy::Resume { item, stage } => {
            chain::resume(ctx, item, stage, &prompt.picked, answer)
        }
        PromptWhy::Discard { item, stage } => match answer {
            Answer::Card(card) => discard::pick(ctx, prompt.seat, item, stage, card),
            _ => Ok(()),
        },
        PromptWhy::PayOrLet { item, stage } => {
            prompts::pay_or_let(ctx, prompt.seat, item, stage, answer)
        }
        PromptWhy::Mode { item, execution } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            Answer::Mode(mode) => play::choose_mode(ctx, item, execution, mode),
            _ => Ok(()),
        },
        PromptWhy::Name { item, kind, stage } => match answer {
            Answer::Cancel => {
                play::cancel(ctx, item);
                Ok(())
            }
            Answer::Name(index) if ctx.blob.pending(item).is_some() => {
                play::choose_name(ctx, item, kind, index)
            }
            Answer::Name(index) => chain::name(ctx, item, stage, kind, index),
            Answer::Done if ctx.blob.pending(item).is_none() => {
                chain::resume(ctx, item, stage, &[], answer)
            }
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

pub(crate) fn settle(ctx: &mut Ctx) -> Result<(), Refusal> {
    for _ in 0..SETTLE_LIMIT {
        chain::proceed(ctx);
        let Some(answered) = prompts::next_auto(ctx) else {
            return Ok(());
        };
        resume(ctx, &answered)?;
    }
    Ok(())
}

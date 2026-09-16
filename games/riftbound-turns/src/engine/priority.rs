use crate::engine::ctx::Ctx;
use crate::engine::legal::Reason;
use crate::engine::{activate, chain, hide, showdown};
use crate::state::Priority;
use crate::Refusal;

pub fn can_act(ctx: &Ctx, seat: u8) -> bool {
    if !ctx.hand_of(seat).is_empty() {
        return true;
    }
    let facedown: Vec<u32> = ctx.blob.cards.iter().map(|row| row.id).collect();
    if facedown
        .into_iter()
        .filter(|card| ctx.card(*card).is_some_and(|held| held.owner == seat))
        .any(|card| hide::playable(ctx, card))
    {
        return true;
    }
    !activate::offers(ctx, seat).is_empty()
}

pub fn holder(ctx: &Ctx) -> Option<u8> {
    ctx.blob.priority.map(|priority| priority.active)
}

pub fn step(ctx: &mut Ctx) {
    let Some(mut held) = ctx.blob.priority else {
        return;
    };
    held.passes = held.passes.saturating_add(1);
    if held.passes >= ctx.players() {
        ctx.blob.priority = Some(held);
        chain::resolve_top(ctx);
        return;
    }
    held.active = ctx.blob.order().next_seat(held.active);
    ctx.blob.priority = Some(Priority {
        active: held.active,
        passes: held.passes,
    });
}

pub fn pass(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
    if ctx.blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    let Some(held) = ctx.blob.priority else {
        return showdown::pass(ctx, seat);
    };
    if held.active != seat {
        return Err(Refusal::Illegal(Reason::NotYourPriority));
    }
    ctx.narrate(format!("{{seat {seat}}} passes"));
    step(ctx);
    chain::proceed(ctx);
    Ok(())
}

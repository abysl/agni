use crate::cards::{ModeTiming, NameKind, SelfCost, Stage};
use crate::engine::ctx::{Ctx, Location};
use crate::engine::{
    activate, chain, combat, cost, kill, legal, march, pay, play, showdown, targets, triggers,
};
use crate::state::{
    ChainItem, PromptWhy, ShowdownStage, TargetRef, SLOT_ACCELERATE, SLOT_ADDITIONAL,
    SLOT_PROMISED_REPEAT, SLOT_REPEAT,
};
use crate::Refusal;
use agni_plugin_sdk::prompt::{Answer, Opt, Pick, Prompt};

pub const SPLIT_QUESTION: &str = "an enemy unit at a battlefield to deal 1 to";

pub fn target_option(ctx: &Ctx, target: TargetRef) -> Opt {
    match target {
        TargetRef::Card(card) => Opt::card(format!("{{card {card}}}"), card),
        TargetRef::Item(item) => {
            let source = ctx
                .chain_item(item)
                .map(|held| held.kind.source())
                .unwrap_or(0);
            Opt {
                label: format!("{{card {source}}} on the chain"),
                card: Some(source),
                answer: Answer::Item(item),
            }
        }
        TargetRef::Zone(zone) => Opt::new(format!("{{zone {zone}}}"), Answer::Zone(zone)),
        TargetRef::Seat(seat) => Opt::new(format!("{{seat {seat}}}"), Answer::Seat(seat)),
    }
}

pub fn mode_options(modes: &[crate::cards::ModeSpec]) -> Vec<Opt> {
    modes
        .iter()
        .enumerate()
        .map(|(index, mode)| Opt::new(mode.label, Answer::Mode(index as u8)))
        .collect()
}

pub fn name_options(ctx: &Ctx, kind: NameKind) -> Vec<Opt> {
    ctx.name_options(kind)
        .into_iter()
        .enumerate()
        .map(|(index, name)| Opt::new(name, Answer::Name(index as u16)))
        .collect()
}

pub fn subject_tag(item: &ChainItem) -> String {
    match item.subject {
        Some(TargetRef::Card(card)) if card != item.kind.source() => format!(" · {{card {card}}}"),
        Some(TargetRef::Zone(zone)) => format!(" · {{zone {zone}}}"),
        _ => String::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    pub prompt: Prompt,
    pub why: PromptWhy,
    pub answer: Answer,
}

pub fn offered(ctx: &Ctx) -> Vec<Opt> {
    match (&ctx.blob.prompt, ctx.blob.why) {
        (Some(prompt), Some(why)) => options(ctx, prompt, why),
        (Some(prompt), None) => prompt.numbered(Vec::new()),
        (None, _) => Vec::new(),
    }
}

pub fn options(ctx: &Ctx, prompt: &Prompt, why: PromptWhy) -> Vec<Opt> {
    match why {
        PromptWhy::Mulligan => {
            let mut options = Vec::new();
            if !prompt.is_full() {
                options.extend(
                    ctx.hand_of(prompt.seat)
                        .into_iter()
                        .filter(|card| !prompt.is_picked(*card))
                        .map(|card| Opt::card(format!("set aside {{card {card}}}"), card)),
                );
            }
            options.push(Opt::new("keep", Answer::Done));
            options
        }
        PromptWhy::PlayLocation { item } => {
            let mut options = location_options(ctx, prompt.seat, item);
            if prompt.cancel {
                options.push(Opt::new("cancel", Answer::Cancel));
            }
            options
        }
        PromptWhy::OptionalCost { .. } => {
            let mut options = vec![Opt::new("yes", Answer::Yes), Opt::new("no", Answer::No)];
            if prompt.cancel {
                options.push(Opt::new("cancel", Answer::Cancel));
            }
            options
        }
        PromptWhy::PayOrLet { item, .. } => {
            let mut options = Vec::new();
            let affordable = pay_or_let_cost(ctx, item)
                .is_some_and(|cost| pay::affordable(ctx, prompt.seat, &cost));
            if affordable {
                options.push(Opt::new("yes", Answer::Yes));
            }
            options.push(Opt::new("no", Answer::No));
            options
        }
        PromptWhy::GroupMove { unit, to } => {
            let mut options = Vec::new();
            if let Some(destination) = Location::of_zone(to, prompt.seat, &ctx.zones) {
                if !prompt.is_full() {
                    options.extend(
                        march::companions(ctx, prompt.seat, unit, destination)
                            .into_iter()
                            .filter(|card| !prompt.is_picked(*card))
                            .map(|card| Opt::card(format!("{{card {card}}}"), card)),
                    );
                }
            }
            options.push(Opt::new("done", Answer::Done));
            options
        }
        PromptWhy::Assign => prompt.numbered(
            combat::candidates(ctx)
                .into_iter()
                .map(|unit| {
                    Opt::card(
                        format!("{{card {unit}}} (lethal {})", combat::lethal(ctx, unit)),
                        unit,
                    )
                })
                .collect(),
        ),
        PromptWhy::PickStaged => showdown::choices(ctx.blob)
            .into_iter()
            .map(|index| ctx.blob.staged[index].zone)
            .map(|zone| Opt::new(format!("{{zone {zone}}}"), Answer::Zone(zone)))
            .collect(),
        PromptWhy::PayWith { .. } => {
            let mut options: Vec<Opt> = pay::ready_golds(ctx, prompt.seat)
                .into_iter()
                .map(|gold| Opt::card(format!("kill {{card {gold}}}"), gold))
                .collect();
            options.push(Opt::new("recycle a rune", Answer::Done));
            prompt.numbered(options)
        }
        PromptWhy::Discard { .. } => prompt.numbered(
            ctx.hand_of(prompt.seat)
                .into_iter()
                .map(|card| Opt::card(format!("{{card {card}}}"), card))
                .collect(),
        ),
        PromptWhy::Target {
            item,
            spec: spec_index,
        } => {
            let candidates = ctx
                .blob
                .pending(item)
                .and_then(|pending| {
                    let specs = targets::specs_of(ctx, &pending.item);
                    let spec = specs.get(usize::from(spec_index))?;
                    let picked: Vec<TargetRef> = prompt
                        .picked
                        .iter()
                        .filter_map(|value| targets::from_answer(spec.kind, *value))
                        .collect();
                    Some(targets::candidates_with(
                        ctx,
                        &pending.item,
                        usize::from(spec_index),
                        spec,
                        &picked,
                    ))
                })
                .unwrap_or_default();
            prompt.numbered(
                candidates
                    .into_iter()
                    .map(|target| target_option(ctx, target))
                    .collect(),
            )
        }
        PromptWhy::OrderTriggers { seat } => prompt.numbered(
            triggers::batch_of(ctx, seat)
                .into_iter()
                .filter_map(|id| ctx.blob.pending(id))
                .map(|pending| {
                    let source = pending.item.kind.source();
                    let index = pending.item.kind.ability_index().unwrap_or(0);
                    let label = if index == crate::cards::IMPLICIT_TEMPORARY {
                        format!("{{card {source}}} is Temporary")
                    } else if index == crate::cards::IMPLICIT_VISION {
                        format!("{{card {source}}} has Vision")
                    } else if index == crate::cards::IMPLICIT_WEAPONMASTER {
                        format!("{{card {source}}} has Weaponmaster")
                    } else if index == 0 {
                        format!("{{card {source}}} trigger{}", subject_tag(&pending.item))
                    } else {
                        format!(
                            "{{card {source}}} trigger {}{}",
                            index + 1,
                            subject_tag(&pending.item)
                        )
                    };
                    Opt {
                        label,
                        card: Some(source),
                        answer: Answer::Item(pending.item.id),
                    }
                })
                .collect(),
        ),
        PromptWhy::Mode { item, .. } => {
            let modes = ctx
                .blob
                .pending(item)
                .map(|pending| play::modes_of(ctx, &pending.item))
                .unwrap_or(&[]);
            prompt.numbered(mode_options(modes))
        }
        PromptWhy::Name { kind, .. } => prompt.numbered(name_options(ctx, kind)),
        PromptWhy::Resume { item, stage } => {
            let held = ctx.chain_item(item);
            let ability = held.and_then(|held| targets::ability_of(ctx, held));
            if let Some(ability) = ability.filter(|ability| {
                ability.candidates.is_none() && ability.mode_timing == ModeTiming::AtResume
            }) {
                return prompt.numbered(mode_options(ability.modes));
            }
            let candidates = held
                .and_then(|held| {
                    let ability = targets::ability_of(ctx, held)?;
                    let candidates = ability.candidates?;
                    let offered = candidates(ctx, held, Stage(stage));
                    let enemy_choice = prompt.seat == held.controller;
                    Some(
                        offered
                            .into_iter()
                            .filter(|target| {
                                !enemy_choice || !targets::untargetable(ctx, held, *target)
                            })
                            .collect::<Vec<TargetRef>>(),
                    )
                })
                .unwrap_or_default();
            prompt.numbered(
                candidates
                    .into_iter()
                    .map(|target| target_option(ctx, target))
                    .collect(),
            )
        }
        PromptWhy::Shuffle { .. } => Vec::new(),
    }
}

fn location_label(location: Location, zone: u16, ambush: bool) -> Opt {
    let label = match (location, ambush) {
        (Location::Base(_), _) => "your base".to_string(),
        (Location::Battlefield(_), false) => format!("{{zone {zone}}}"),
        (Location::Battlefield(_), true) => format!("ambush {{zone {zone}}}"),
    };
    Opt::new(label, Answer::Zone(zone))
}

pub fn location_options(ctx: &Ctx, seat: u8, item: u16) -> Vec<Opt> {
    let Some(pending) = ctx.blob.pending(item) else {
        return Vec::new();
    };
    if pending.item.limited.is_some() {
        return play::limited_locations(ctx, &pending.item)
            .into_iter()
            .filter_map(|location| ctx.zone_of(location))
            .map(|(zone, _)| target_option(ctx, TargetRef::Zone(zone)))
            .collect();
    }
    let card = pending.item.kind.source();
    let plain = ctx.play_locations_for(seat, card);
    legal::locations_for(ctx, seat, card)
        .into_iter()
        .filter_map(|location| {
            let (zone, _) = ctx.zone_of(location)?;
            Some(location_label(location, zone, !plain.contains(&location)))
        })
        .collect()
}

pub fn pay_or_let_cost(ctx: &Ctx, item: u16) -> Option<cost::Cost> {
    let held = ctx.chain_item(item)?;
    let ability = targets::ability_of(ctx, held)?;
    let printed = ability.cost?;
    Some(cost::of_script(
        &printed,
        &ctx.domains_of(held.kind.source()),
    ))
}

pub fn kept_spell(ctx: &Ctx, item: u16) -> Option<u32> {
    let held = chain::execution_view(ctx, ctx.chain_item(item)?);
    held.targets.iter().find_map(|target| match target {
        TargetRef::Item(id) => ctx.chain_item(*id).and_then(|spell| spell.kind.card()),
        _ => None,
    })
}

pub fn pay_or_let(
    ctx: &mut Ctx,
    seat: u8,
    item: u16,
    stage: u8,
    answer: Answer,
) -> Result<(), Refusal> {
    let paid = answer == Answer::Yes
        && pay_or_let_cost(ctx, item)
            .and_then(|cost| pay::plan(ctx, seat, &cost).ok().map(|plan| (cost, plan)))
            .map(|(cost, plan)| {
                pay::pay(ctx, seat, &plan);
                ctx.narrate(format!("{{seat {seat}}} pays {}", cost.label()));
            })
            .is_some();
    let answer = if paid { Answer::Yes } else { Answer::No };
    chain::pay_or_let(ctx, item, stage, answer)
}

fn trigger_cost_parts(ctx: &Ctx, item: &ChainItem) -> Vec<String> {
    let mut parts = Vec::new();
    let paid = cost::of_item(ctx, item, None);
    let runes = cost::Cost {
        energy: paid.energy,
        power: paid.power.clone(),
        ..cost::Cost::default()
    };
    if !runes.is_free() {
        parts.push(format!("pay {}", runes.label()));
    }
    if paid.xp > 0 {
        parts.push(format!("spend {} XP", paid.xp));
    }
    if paid.burn > 0 {
        parts.push(format!("burn {}", paid.burn));
    }
    let source = item.kind.source();
    let self_cost = activate::self_cost_of_item(ctx, item).map(|(_, held)| held);
    match self_cost {
        Some(SelfCost::Exhaust) if runes.is_free() => {
            parts.push(format!("exhaust {{card {source}}}"));
        }
        Some(SelfCost::KillSelf) if runes.is_free() => {
            parts.push(format!("kill {{card {source}}}"));
        }
        Some(SelfCost::Disempower) if runes.is_free() => {
            parts.push(format!("exhaust and disempower {{card {source}}}"));
        }
        Some(SelfCost::BanishTarget) => {
            if let Some(TargetRef::Card(card)) = item.targets.first() {
                parts.push(format!("banish {{card {card}}}"));
            }
        }
        _ => {}
    }
    if parts.is_empty() {
        parts.push("pay nothing".to_string());
    }
    parts
}

fn optional_cost_status(ctx: &Ctx, item: u16, slot: usize) -> String {
    let pending = ctx.blob.pending(item);
    let source = pending
        .map(|pending| pending.item.kind.source())
        .unwrap_or(0);
    let card = if pending.is_some() {
        format!("{{card {source}}}")
    } else {
        "the card".to_string()
    };
    let script_cost = |printed: Option<crate::cards::Cost>| {
        printed
            .map(|printed| cost::of_script(&printed, &ctx.domains_of(source)).label())
            .unwrap_or_default()
    };
    match slot {
        SLOT_ACCELERATE => {
            let cost = pending
                .and_then(|pending| pending.item.kind.card())
                .and_then(|card| ctx.card(card))
                .map(cost::accelerate)
                .map(|cost| cost.label())
                .unwrap_or_default();
            format!("accelerate {card} for {cost}?")
        }
        SLOT_REPEAT | SLOT_PROMISED_REPEAT => {
            let cost = pending
                .and_then(|pending| {
                    if slot == SLOT_REPEAT {
                        cost::repeat_of(ctx, &pending.item)
                    } else {
                        cost::promised_repeat_of(ctx, &pending.item)
                    }
                })
                .map(|cost| cost.label())
                .unwrap_or_default();
            format!("repeat {card} for {cost}?")
        }
        SLOT_ADDITIONAL => {
            let cost = script_cost(ctx.script(source).and_then(|script| script.additional));
            format!("pay {cost} as an additional cost for {card}?")
        }
        _ => {
            let Some(pending) = pending else {
                return "pay for the trigger?".to_string();
            };
            let parts = trigger_cost_parts(ctx, &pending.item).join(" and ");
            format!(
                "{parts} for the {card} trigger{}?",
                subject_tag(&pending.item)
            )
        }
    }
}

fn split_status(ctx: &Ctx, item: &ChainItem, stage: u8) -> Option<String> {
    let ability = targets::ability_of(ctx, item)?;
    if stage == 0 || ability.question != Some(SPLIT_QUESTION) {
        return None;
    }
    let placed = i32::from(stage - 1);
    let total = item
        .targets
        .iter()
        .find_map(|target| match target {
            TargetRef::Card(card) => Some(ctx.current_might(*card)),
            _ => None,
        })
        .unwrap_or(0);
    Some(format!(
        "{{card {}}}: {} of {total} to place",
        item.kind.source(),
        (total - placed).max(0)
    ))
}

pub fn resume_questions() -> Vec<&'static str> {
    let mut questions: Vec<&'static str> = crate::cards::CARDS
        .iter()
        .flat_map(|card| card.abilities.iter().chain(card.granted_abilities()))
        .chain([&kill::CHOICE_ABILITY, &crate::cards::prelude::VISION])
        .filter_map(|ability| ability.question)
        .collect();
    questions.sort_unstable();
    questions.dedup();
    questions
}

pub fn answer_words(why: PromptWhy) -> Vec<&'static str> {
    match why {
        PromptWhy::Mulligan => vec!["set aside", "keep"],
        PromptWhy::Target { .. } => vec!["for a target", "on the chain", "an opponent"],
        PromptWhy::PlayLocation { .. } => vec!["your base"],
        PromptWhy::OptionalCost { .. } => vec!["yes or no"],
        PromptWhy::OrderTriggers { .. } => {
            vec!["trigger", "is Temporary", "has Vision", "has Weaponmaster"]
        }
        PromptWhy::PickStaged => vec!["which showdown opens"],
        PromptWhy::GroupMove { .. } => vec!["group move", "done"],
        PromptWhy::Assign => vec!["lethal"],
        PromptWhy::Resume { .. } => {
            let mut words = vec!["skip", "done"];
            words.extend(resume_questions());
            words
        }
        PromptWhy::PayWith { .. } => vec!["pay 1 power", "kill <Gold>", "recycle a rune", "cancel"],
        PromptWhy::Discard { .. } => vec!["discard a card"],
        PromptWhy::Shuffle { .. } => vec!["roll"],
        PromptWhy::PayOrLet { .. } => vec!["pay", "let it resolve"],
        PromptWhy::Mode { .. } => vec!["choose one", "cancel"],
        PromptWhy::Name {
            kind: NameKind::Spell,
            ..
        } => vec!["name a spell", "cancel"],
        PromptWhy::Name {
            kind: NameKind::Tag,
            ..
        } => vec!["name a tag", "cancel"],
    }
}

pub fn status(ctx: &Ctx, why: PromptWhy) -> String {
    let card_of = |item: u16| {
        ctx.blob
            .pending(item)
            .map(|pending| pending.item.kind.source())
            .or_else(|| ctx.chain_item(item).map(|held| held.kind.source()))
            .map(|card| format!("{{card {card}}}"))
            .unwrap_or_else(|| "the card".to_string())
    };
    let progress = || {
        ctx.blob
            .prompt
            .as_ref()
            .map(|prompt| format!(" ({} of {})", prompt.picked.len(), prompt.max))
            .unwrap_or_default()
    };
    match why {
        PromptWhy::Mulligan => "set aside up to 2 cards to redraw".to_string(),
        PromptWhy::PlayLocation { item } => format!("where does {} enter?", card_of(item)),
        PromptWhy::OptionalCost { item, cost } => {
            optional_cost_status(ctx, item, usize::from(cost))
        }
        PromptWhy::GroupMove { to, .. } => format!("move others to {{zone {to}}} too?"),
        PromptWhy::PickStaged => "which showdown opens first?".to_string(),
        PromptWhy::Target { item, spec } => {
            let label = ctx
                .blob
                .pending(item)
                .and_then(|pending| {
                    targets::specs_of(ctx, &pending.item)
                        .get(usize::from(spec))
                        .map(|spec| spec.label)
                })
                .unwrap_or("a target");
            format!("{}: choose {label}{}", card_of(item), progress())
        }
        PromptWhy::OrderTriggers { .. } => {
            "order your triggers (last placed resolves first)".to_string()
        }
        PromptWhy::Assign => {
            let remaining = match ctx.blob.showdown.as_ref().map(|showdown| &showdown.stage) {
                Some(ShowdownStage::Damage { remaining, .. }) => *remaining,
                _ => 0,
            };
            format!("assign {remaining} damage: who takes lethal next?")
        }
        PromptWhy::Resume { item, stage } => {
            let held = ctx.chain_item(item);
            if let Some(line) = held.and_then(|held| split_status(ctx, held, stage)) {
                return line;
            }
            let question = held
                .and_then(|held| targets::ability_of(ctx, held))
                .and_then(|ability| ability.question)
                .unwrap_or("a choice");
            let asked = ctx.blob.prompt.as_ref().map(|prompt| prompt.seat);
            let chooser = match (asked, held.map(|held| held.controller)) {
                (Some(seat), Some(controller)) if seat != controller => {
                    format!("{{seat {seat}}}: ")
                }
                _ => format!("{}: ", card_of(item)),
            };
            format!("{chooser}choose {question}{}", progress())
        }
        PromptWhy::PayWith { item } => {
            let power = ctx
                .blob
                .pending(item)
                .map(|pending| cost::of_item(ctx, &pending.item, None).power.len())
                .unwrap_or(1);
            format!("pay {power} power for {} with", card_of(item))
        }
        PromptWhy::Discard { .. } => "discard a card".to_string(),
        PromptWhy::Shuffle { .. } => {
            let count = ctx
                .blob
                .prompt
                .as_ref()
                .map(|prompt| prompt.picked.len())
                .unwrap_or(0);
            format!("roll to shuffle {count} recycled cards")
        }
        PromptWhy::PayOrLet { item, .. } => {
            let cost = pay_or_let_cost(ctx, item)
                .map(|cost| cost.label())
                .unwrap_or_else(|| "nothing".to_string());
            let card = kept_spell(ctx, item)
                .map(|card| format!("{{card {card}}}"))
                .unwrap_or_else(|| "the spell".to_string());
            format!("pay {cost} to keep {card}?")
        }
        PromptWhy::Mode { item, execution } => {
            let again = if execution > 0 { " for the repeat" } else { "" };
            format!("{}: choose one{again}", card_of(item))
        }
        PromptWhy::Name { item, kind, .. } => {
            let what = match kind {
                NameKind::Spell => "a spell",
                NameKind::Tag => "a tag",
            };
            format!("{}: name {what}", card_of(item))
        }
    }
}

pub fn viable(ctx: &Ctx, options: &[Opt]) -> Vec<bool> {
    let check = match ctx.blob.why {
        Some(PromptWhy::Resume { item, .. }) => ctx.chain_item(item).and_then(|held| {
            let viable = targets::ability_of(ctx, held)?.viable?;
            Some((held, viable))
        }),
        _ => None,
    };
    options
        .iter()
        .map(|option| {
            let Some((held, viable)) = check else {
                return true;
            };
            let target = match option.answer {
                Answer::Card(card) => TargetRef::Card(card),
                Answer::Zone(zone) => TargetRef::Zone(zone),
                Answer::Seat(seat) => TargetRef::Seat(seat),
                Answer::Item(item) => TargetRef::Item(item),
                _ => return true,
            };
            viable(ctx, held, target)
        })
        .collect()
}

pub fn escape(options: &[Opt]) -> Option<usize> {
    [Answer::Cancel, Answer::Skip, Answer::No]
        .into_iter()
        .find_map(|answer| options.iter().position(|option| option.answer == answer))
}

pub fn auto_answer(options: &[Opt]) -> Option<Answer> {
    match options {
        [only] if !matches!(only.answer, Answer::Skip | Answer::Cancel) => Some(only.answer),
        _ => None,
    }
}

pub fn answer(ctx: &mut Ctx, seat: u8, pick: Pick) -> Result<Option<Answered>, Refusal> {
    let offered = offered(ctx);
    let mut prompt = ctx.blob.prompt.clone().ok_or(Refusal::NoPrompt)?;
    let why = ctx.blob.why.ok_or(Refusal::NoPrompt)?;
    let answer = prompt
        .resolve(seat, pick, &offered)
        .map_err(Refusal::Pick)?;
    match prompt.answer(answer) {
        None => Err(Refusal::AlreadyPicked),
        Some(false) => {
            ctx.blob.prompt = Some(prompt);
            Ok(None)
        }
        Some(true) => {
            ctx.blob.close_prompt();
            Ok(Some(Answered {
                prompt,
                why,
                answer,
            }))
        }
    }
}

pub fn next_auto(ctx: &mut Ctx) -> Option<Answered> {
    let (Some(mut prompt), Some(why)) = (ctx.blob.prompt.clone(), ctx.blob.why) else {
        return None;
    };
    let offered = offered(ctx);
    let answer = match auto_answer(&offered) {
        Some(answer) => answer,
        None if offered.is_empty()
            && matches!(why, PromptWhy::Resume { .. } | PromptWhy::Name { .. }) =>
        {
            Answer::Done
        }
        None => return None,
    };
    prompt.answer(answer);
    ctx.blob.close_prompt();
    Some(Answered {
        prompt,
        why,
        answer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::cards::Keyword;
    use crate::cards::{Card, Flow, NameKind};
    use crate::cards::{Cost, Domain, Power};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::{STAGE_LOCATION, STAGE_PAY};
    use crate::state::SLOT_TRIGGER_COST;
    use crate::state::{
        Ask, ChainItem, Expiry, GameBlob, ItemKind, ItemStatus, Needs, Origin, Pending, Priority,
        Promise, PromiseEffect, PromiseKind, Showdown, Staged,
    };
    use agni_plugin_sdk::prompt::PickRefusal;

    const RANSOMER: u32 = 90;
    const REPEATER: u32 = 91;
    const PAYER: u32 = 92;
    const EXHAUSTER: u32 = 93;
    const BURNER: u32 = 94;
    const BANISHER: u32 = 95;
    const POUNCER: u32 = 96;
    const SPLITTER: u32 = 97;
    const RANSOM: Cost = Cost {
        energy: 2,
        power: &[],
    };

    fn ransom(ctx: &mut Ctx, item: &ChainItem, stage: Stage) -> Flow {
        if stage.0 == 1 {
            let word = if chain::paid(ctx) { "kept" } else { "let go" };
            ctx.narrate(format!("{{card {}}} {word}", item.kind.source()));
            return Flow::Done;
        }
        let Some(target) = prelude::item_target(ctx, item, 0) else {
            return Flow::Done;
        };
        let payer = prelude::item_controller(ctx, target).unwrap_or(0);
        let cost = cost::of_script(&RANSOM, &[]);
        Flow::Ask(ctx.ask_pay_or_let(item, payer, &cost, 1))
    }

    static RANSOMER_CARD: Card = prelude::spell(
        "Ransomer",
        &[],
        &[prelude::with_cost(
            prelude::play(&[prelude::a_spell("a spell to ransom")], ransom),
            RANSOM,
        )],
    );

    static REPEATER_CARD: Card = prelude::spell(
        "Repeater",
        &[Keyword::Repeat(Cost {
            energy: 1,
            power: &[Power::Own],
        })],
        &[prelude::play(&[], |_, _, _| Flow::Done)],
    );

    static GRANTED_REPEATER_CARD: Card = prelude::spell(
        "Granted Echo",
        &[],
        &[prelude::play(&[], |ctx, item, _| {
            ctx.draw(item.controller, 1);
            Flow::Done
        })],
    );

    static PAYER_CARD: Card = prelude::with_additional(
        prelude::unit("Payer", &[], &[]),
        Cost {
            energy: 0,
            power: &[Power::Domain(Domain::Body)],
        },
    );

    static EXHAUSTER_CARD: Card = prelude::unit(
        "Exhauster",
        &[],
        &[prelude::exhausting_self(prelude::optional(
            prelude::on_move(&[], |_, _, _| Flow::Done),
        ))],
    );

    static BURNER_CARD: Card = prelude::unit(
        "Burner",
        &[],
        &[prelude::burning(
            prelude::optional(prelude::on_move(&[], |_, _, _| Flow::Done)),
            1,
        )],
    );

    static BANISHER_CARD: Card = prelude::unit(
        "Banisher",
        &[],
        &[prelude::paying_with(
            prelude::optional(prelude::on_attack(
                &[prelude::a_card(
                    prelude::FRIENDLY_UNIT_IN_TRASH,
                    "a unit to banish",
                )],
                |_, _, _| Flow::Done,
            )),
            SelfCost::BanishTarget,
        )],
    );

    static POUNCER_CARD: Card = prelude::unit("Pouncer", &[Keyword::Ambush], &[]);

    static SPLITTER_CARD: Card = prelude::spell(
        "Splitter",
        &[],
        &[prelude::asking(
            prelude::with_candidates(
                prelude::play(&[prelude::a_unit("a unit to split")], |_, _, _| Flow::Done),
                |ctx, _, _| {
                    ctx.units_at(Location::Base(1))
                        .into_iter()
                        .map(TargetRef::Card)
                        .collect()
                },
            ),
            SPLIT_QUESTION,
        )],
    );

    fn pending(fixture: &mut Fixture, id: u16, kind: ItemKind, seat: u8, stage: u8) {
        let mut item = ChainItem::new(id, kind, seat, Origin::Hand);
        item.stage = stage;
        fixture.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
    }

    #[test]
    fn pay_or_let_offers_yes_only_while_the_payer_can_afford_it_and_pays_on_yes() {
        let mut fixture = Fixture::enforced();
        let mut ransomer = fixtures::spell(RANSOMER, fixtures::CHAIN, 1, "Ransomer", 2, 0);
        ransomer.domain = vec!["Chaos".into()];
        fixture.table.cards.push(ransomer);
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::CHAIN);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(RANSOMER, &RANSOMER_CARD);
        let mut spark = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        spark.status = ItemStatus::Finalized;
        let mut held = ChainItem::new(2, ItemKind::Spell { card: RANSOMER }, 1, Origin::Hand);
        held.targets = vec![TargetRef::Item(1)];
        held.status = ItemStatus::Resolving;
        held.stage = 1;
        fixture.blob.chain.push(spark);
        fixture.blob.chain.push(held);
        fixture.blob.priority = Some(Priority {
            active: 1,
            passes: 2,
        });
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1),
            why: PromptWhy::PayOrLet { item: 2, stage: 1 },
        });
        let mut ctx = fixture.ctx();
        assert_eq!(
            pay_or_let_cost(&ctx, 2).map(|cost| cost.label()),
            Some("2 energy".into())
        );
        assert_eq!(kept_spell(&ctx, 2), Some(fixtures::HAND_SPELL));
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert_eq!(
            status(&ctx, PromptWhy::PayOrLet { item: 2, stage: 1 }),
            "pay 2 energy to keep {card 71}?"
        );
        assert_eq!(
            answer_words(PromptWhy::PayOrLet { item: 2, stage: 1 }),
            ["pay", "let it resolve"]
        );
        assert_eq!(next_auto(&mut ctx), None, "two options: the payer clicks");
        let ready = ctx.ready_runes_of(0).len();
        pay_or_let(&mut ctx, 0, 2, 1, Answer::Yes).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} pays 2 energy"));
        assert!(ctx.blob.log.iter().any(|line| line == "{card 90} kept"));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the ransomer finished, Spark stays"
        );
        let mut poor = Fixture::enforced();
        for rune in [41, 42, 43] {
            poor.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ransomer = fixtures::spell(RANSOMER, fixtures::CHAIN, 1, "Ransomer", 2, 0);
        ransomer.domain = vec!["Chaos".into()];
        poor.table.cards.push(ransomer);
        poor.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::CHAIN);
        poor.resolve();
        poor.scripts = poor.scripts.clone().with_script(RANSOMER, &RANSOMER_CARD);
        let mut spark = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        spark.status = ItemStatus::Finalized;
        let mut held = ChainItem::new(2, ItemKind::Spell { card: RANSOMER }, 1, Origin::Hand);
        held.targets = vec![TargetRef::Item(1)];
        held.status = ItemStatus::Resolving;
        held.stage = 1;
        poor.blob.chain.push(spark);
        poor.blob.chain.push(held);
        poor.blob.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1),
            why: PromptWhy::PayOrLet { item: 2, stage: 1 },
        });
        let mut ctx = poor.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["no"],
            "an unaffordable ransom offers no yes"
        );
        let auto = next_auto(&mut ctx).expect("a lone no answers itself");
        assert_eq!(auto.answer, Answer::No);
        pay_or_let(&mut ctx, 0, 2, 1, auto.answer).unwrap();
        assert!(ctx.blob.log.iter().any(|line| line == "{card 90} let go"));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} pays 2 energy"));
        let mut robbed = Fixture::enforced();
        robbed.blob.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1),
            why: PromptWhy::PayOrLet { item: 9, stage: 1 },
        });
        let ctx = robbed.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["no"],
            "no item: nothing to pay for"
        );
        assert_eq!(
            status(&ctx, PromptWhy::PayOrLet { item: 9, stage: 1 }),
            "pay nothing to keep the spell?"
        );
    }

    #[test]
    fn the_optional_cost_confirm_names_each_slot_in_its_own_words() {
        let mut fixture = Fixture::enforced();
        let mut repeater = fixtures::spell(REPEATER, fixtures::CHAIN, 0, "Repeater", 1, 0);
        repeater.domain = vec!["Mind".into()];
        fixture.table.cards.push(repeater);
        let mut payer = fixtures::unit(PAYER, fixtures::CHAIN, 0, "Payer", 2);
        payer.domain = vec!["Body".into()];
        fixture.table.cards.push(payer);
        fixture
            .table
            .cards
            .push(fixtures::unit(EXHAUSTER, fixtures::BASE, 0, "Exhauster", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BURNER, fixtures::BASE, 0, "Burner", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BANISHER, fixtures::BASE, 0, "Banisher", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(REPEATER, &REPEATER_CARD)
            .with_script(PAYER, &PAYER_CARD)
            .with_script(EXHAUSTER, &EXHAUSTER_CARD)
            .with_script(BURNER, &BURNER_CARD)
            .with_script(BANISHER, &BANISHER_CARD);
        pending(
            &mut fixture,
            1,
            ItemKind::Spell { card: REPEATER },
            0,
            crate::engine::play::STAGE_REPEAT,
        );
        pending(
            &mut fixture,
            2,
            ItemKind::Permanent { card: PAYER },
            0,
            crate::engine::play::STAGE_ADDITIONAL,
        );
        for (id, source) in [(3u16, EXHAUSTER), (4, BURNER), (5, BANISHER)] {
            pending(
                &mut fixture,
                id,
                ItemKind::Trigger { source, index: 0 },
                0,
                STAGE_PAY,
            );
        }
        if let Some(held) = fixture.blob.pending_mut(5) {
            held.item.targets = vec![TargetRef::Card(fixtures::VI)];
        }
        pending(
            &mut fixture,
            6,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            crate::engine::play::STAGE_REPEAT,
        );
        fixture.blob.seat_mut(0).promises = vec![Promise {
            kind: PromiseKind::Spell,
            effect: PromiseEffect::RepeatForCost,
            until: Expiry::EndOfTurn(1),
        }];
        let ctx = fixture.ctx();
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 1,
                    cost: SLOT_REPEAT as u8
                }
            ),
            "repeat {card 91} for 1 energy and 1 Mind power?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 6,
                    cost: SLOT_PROMISED_REPEAT as u8
                }
            ),
            "repeat {card 71} for 2 energy and 1 Fury power?",
            "a promised Repeat is priced at the spell's printed cost"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 6,
                    cost: SLOT_REPEAT as u8
                }
            ),
            "repeat {card 71} for ?",
            "Spark has no printed Repeat of its own"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 2,
                    cost: SLOT_ADDITIONAL as u8
                }
            ),
            "pay 1 Body power as an additional cost for {card 92}?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 2,
                    cost: SLOT_ACCELERATE as u8
                }
            ),
            "accelerate {card 92} for 1 energy and 1 Body power?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 3,
                    cost: SLOT_TRIGGER_COST as u8
                }
            ),
            "exhaust {card 93} for the {card 93} trigger?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 4,
                    cost: SLOT_TRIGGER_COST as u8
                }
            ),
            "burn 1 for the {card 94} trigger?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 5,
                    cost: SLOT_TRIGGER_COST as u8
                }
            ),
            "banish {card 50} for the {card 95} trigger?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 9,
                    cost: SLOT_TRIGGER_COST as u8
                }
            ),
            "pay for the trigger?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::OptionalCost {
                    item: 9,
                    cost: SLOT_REPEAT as u8
                }
            ),
            "repeat the card for ?"
        );
    }

    #[test]
    fn a_reloaded_owned_repeat_is_offered_paid_and_runs_the_card_twice() {
        const CARD: u32 = 98;
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            CARD,
            fixtures::HAND,
            0,
            "Granted Echo",
            0,
            0,
        ));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CARD, &GRANTED_REPEATER_CARD);
        let encoded;
        {
            let mut ctx = fixture.ctx();
            assert!(ctx.grant(
                CARD,
                Keyword::Repeat(Cost {
                    energy: 1,
                    power: &[],
                }),
                Expiry::Permanent,
            ));
            encoded = ctx.blob.encode();
        }
        fixture.blob = GameBlob::decode(&encoded).expect("owned Repeat survives reload");

        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, CARD).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: crate::state::SLOT_REPEAT as u8,
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        let item = &ctx.blob.chain[0];
        assert_eq!(item.repeats(), 1, "the owned grant adds a second execution");
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert!(ctx.blob.chain[0].status == ItemStatus::Finalized);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob.seat(0).draws,
            2,
            "the grant actually runs both executions"
        );
    }

    #[test]
    fn a_resume_addressed_to_another_seat_is_labelled_with_that_seat() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ASKER);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.status = ItemStatus::Resolving;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 1, 1, 1),
            why: PromptWhy::Resume { item: 1, stage: 1 },
        });
        let ctx = fixture.ctx();
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{seat 1}: choose a unit at your base (0 of 1)"
        );
    }

    #[test]
    fn a_resume_with_nothing_to_offer_answers_itself_done_instead_of_stalling() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ENEMY_ASKER);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.status = ItemStatus::Resolving;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 1, 1),
            why: PromptWhy::Resume { item: 1, stage: 1 },
        });
        let mut ctx = fixture.ctx();
        assert_eq!(fixtures::labels(&ctx), ["{card 81}"]);
        assert!(ctx.shroud(fixtures::THEIR_UNIT));
        assert!(offered(&ctx).is_empty(), "the only candidate is shrouded");
        let auto = next_auto(&mut ctx).expect("an empty mandatory Resume cannot wait");
        assert_eq!(auto.answer, Answer::Done);
        assert!(auto.prompt.picked.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn a_split_in_progress_counts_the_points_left_to_place() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            SPLITTER,
            fixtures::CHAIN,
            0,
            "Splitter",
            1,
            0,
        ));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(SPLITTER, &SPLITTER_CARD);
        let mut item = ChainItem::new(1, ItemKind::Spell { card: SPLITTER }, 0, Origin::Hand);
        item.targets = vec![TargetRef::Card(fixtures::VI)];
        item.status = ItemStatus::Resolving;
        item.stage = 2;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 1, 1),
            why: PromptWhy::Resume { item: 1, stage: 2 },
        });
        let ctx = fixture.ctx();
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 2 }),
            "{card 97}: 2 of 3 to place",
            "one point placed of Vi's three"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{card 97}: 3 of 3 to place"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 4 }),
            "{card 97}: 0 of 3 to place"
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 81}"]);
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 0 }),
            format!("{{card 97}}: choose {SPLIT_QUESTION} (0 of 1)"),
            "before the first point the question reads as usual"
        );
    }

    #[test]
    fn a_granted_open_battlefield_is_a_plain_entry_under_the_units_own_timing() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            POUNCER,
            fixtures::HAND,
            0,
            "Sneaky Deckhand",
            2,
        ));
        fixture.resolve();
        pending(
            &mut fixture,
            1,
            ItemKind::Permanent { card: POUNCER },
            0,
            STAGE_LOCATION,
        );
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 1, 1).cancellable(),
            why: PromptWhy::PlayLocation { item: 1 },
        });
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["your base", "{zone 9}", "cancel"],
            "the open battlefield reads like a held one, not an ambush"
        );
        drop(ctx);
        let mut other = ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        other.status = ItemStatus::Finalized;
        fixture.blob.chain.push(other);
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 1,
        });
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "a closed state offers nothing: the grant changes where, not when"
        );
    }

    #[test]
    fn an_ambush_unit_is_offered_its_ambush_battlefields_and_only_those_in_a_closed_state() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(POUNCER, fixtures::HAND, 0, "Pouncer", 2));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(POUNCER, &POUNCER_CARD);
        pending(
            &mut fixture,
            1,
            ItemKind::Permanent { card: POUNCER },
            0,
            STAGE_LOCATION,
        );
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 1, 1).cancellable(),
            why: PromptWhy::PlayLocation { item: 1 },
        });
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["your base", "ambush {zone 9}", "cancel"],
            "the open state on its own turn lists the plain plays and the ambush"
        );
        drop(ctx);
        let mut other = ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        other.status = ItemStatus::Finalized;
        fixture.blob.chain.push(other);
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 1,
        });
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["ambush {zone 9}", "cancel"],
            "a closed state leaves only the ambush entry"
        );
        drop(ctx);
        fixture.blob.priority = Some(Priority {
            active: 1,
            passes: 0,
        });
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "without priority nothing is open"
        );
        assert_eq!(
            status(&ctx, PromptWhy::PlayLocation { item: 1 }),
            "where does {card 96} enter?"
        );
    }

    static ENEMY_ASKER: Card = prelude::spell(
        "Enemy Asker",
        &[],
        &[prelude::asking(
            prelude::with_candidates(prelude::play(&[], |_, _, _| Flow::Done), |_, _, _| {
                vec![TargetRef::Card(fixtures::THEIR_UNIT)]
            }),
            "an enemy unit",
        )],
    );

    static ASKER: Card = prelude::spell(
        "Asker",
        &[],
        &[prelude::asking(
            prelude::with_candidates(prelude::play(&[], |_, _, _| Flow::Done), |ctx, item, _| {
                ctx.units_at(crate::engine::ctx::Location::Base(item.controller))
                    .into_iter()
                    .map(TargetRef::Card)
                    .collect()
            }),
            "a unit at your base",
        )],
    );

    #[test]
    fn every_prompt_kind_names_the_words_its_answers_use() {
        let each = PromptWhy::each();
        assert!(each.len() >= 12);
        for why in each {
            let words = answer_words(why);
            assert!(!words.is_empty(), "{why:?} has answer words");
            assert!(
                words.iter().all(|word| !word.is_empty()),
                "{why:?} names no empty word"
            );
        }
        let questions = resume_questions();
        for question in [
            "the top card of your deck to recycle",
            "a unit you control here to wear it",
            "up to two runes to ready",
            "a unit you control here to kill",
            "which replacement applies",
        ] {
            assert!(
                questions.contains(&question),
                "{question} is a Resume question"
            );
        }
        assert!(answer_words(PromptWhy::Resume { item: 1, stage: 0 })
            .contains(&"which replacement applies"));
    }

    #[test]
    fn a_resume_prompt_lists_the_candidates_its_script_names() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ASKER);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.status = ItemStatus::Resolving;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 1, 1),
            why: PromptWhy::Resume { item: 1, stage: 1 },
        });
        let ctx = fixture.ctx();
        let offered = offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 50}"]
        );
        assert_eq!(offered[0].answer, Answer::Card(fixtures::VI));
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{card 71}: choose a unit at your base (0 of 1)"
        );
        let mut bare = Fixture::enforced();
        bare.blob.chain.push(ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
        bare.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 0, 1),
            why: PromptWhy::Resume { item: 1, stage: 1 },
        });
        let ctx = bare.ctx();
        assert_eq!(super::offered(&ctx).len(), 1, "only skip without a hook");
    }

    static PICKY: Card = prelude::spell(
        "Picky",
        &[],
        &[prelude::viable_if(
            prelude::asking(
                prelude::with_candidates(prelude::play(&[], |_, _, _| Flow::Done), |ctx, _, _| {
                    ctx.hand_of(0).into_iter().map(TargetRef::Card).collect()
                }),
                "a card in hand",
            ),
            |ctx, _, target| match target {
                TargetRef::Card(card) => !ctx.is_gear(card),
                _ => true,
            },
        )],
    );

    #[test]
    fn a_resume_option_the_scripts_viable_hook_rules_out_is_still_offered_but_greyed() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &PICKY);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.status = ItemStatus::Resolving;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(2, 0, 0, 1),
            why: PromptWhy::Resume { item: 1, stage: 1 },
        });
        let ctx = fixture.ctx();
        let offered = offered(&ctx);
        assert_eq!(offered.len(), 5, "four hand cards and skip");
        assert_eq!(
            viable(&ctx, &offered),
            [true, true, false, true, true],
            "the gear is greyed, never dropped · skip is always live"
        );
        drop(ctx);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ASKER);
        let ctx = fixture.ctx();
        let offered = super::offered(&ctx);
        assert_eq!(
            viable(&ctx, &offered),
            vec![true; offered.len()],
            "without a hook every option is live"
        );
    }

    #[test]
    fn two_triggers_of_one_source_are_told_apart_by_their_subject() {
        let mut fixture = Fixture::enforced();
        let legend = fixtures::LEGEND_CARD;
        for (id, subject) in [
            (1u16, TargetRef::Card(fixtures::VI)),
            (2, TargetRef::Card(fixtures::CHAMPION_CARD)),
            (3, TargetRef::Card(legend)),
        ] {
            let mut item = ChainItem::new(
                id,
                ItemKind::Trigger {
                    source: legend,
                    index: 0,
                },
                0,
                Origin::Board,
            );
            item.subject = Some(subject);
            fixture.blob.queue.push(Pending {
                item,
                needs: Needs::Order,
            });
        }
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(1, 0, 3, 3),
            why: PromptWhy::OrderTriggers { seat: 0 },
        });
        let ctx = fixture.ctx();
        let labels: Vec<String> = offered(&ctx).iter().map(|opt| opt.label.clone()).collect();
        assert_eq!(
            labels,
            [
                format!("{{card {legend}}} trigger · {{card {}}}", fixtures::VI),
                format!(
                    "{{card {legend}}} trigger · {{card {}}}",
                    fixtures::CHAMPION_CARD
                ),
                format!("{{card {legend}}} trigger"),
            ],
            "a subject that is the source itself adds nothing"
        );
        assert!(offered(&ctx).iter().all(|opt| opt.card == Some(legend)));
        assert_eq!(
            status(&ctx, PromptWhy::OptionalCost { item: 2, cost: 1 }),
            format!(
                "pay 4 energy for the {{card {legend}}} trigger · {{card {}}}?",
                fixtures::CHAMPION_CARD
            )
        );
    }

    #[test]
    fn a_prompt_without_a_why_falls_back_to_the_sdk_closers_and_singletons_auto_answer() {
        let mut fixture = Fixture::enforced();
        fixture.blob.prompt = Some(Prompt::new(1, 0, 0, 1));
        let ctx = fixture.ctx();
        let offered = offered(&ctx);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].label, "skip");
        assert_eq!(auto_answer(&offered), None);
        assert_eq!(escape(&offered), Some(0));
        assert_eq!(
            auto_answer(&[Opt::new("keep", Answer::Done)]),
            Some(Answer::Done)
        );
        assert_eq!(auto_answer(&[Opt::new("cancel", Answer::Cancel)]), None);
        let pair = [Opt::new("a", Answer::Yes), Opt::new("b", Answer::No)];
        assert_eq!(auto_answer(&pair), None);
        assert_eq!(escape(&pair), Some(1));
        assert_eq!(escape(&[Opt::new("done", Answer::Done)]), None);
        let mut none = Fixture::enforced();
        assert!(super::offered(&none.ctx()).is_empty());
        assert_eq!(next_auto(&mut none.ctx()), None);
    }

    #[test]
    fn every_prompt_has_a_status_line() {
        let mut fixture = Fixture::enforced();
        fixture.blob.showdown = Some(Showdown {
            stage: ShowdownStage::Damage {
                assigner: 0,
                remaining: 3,
                assigned: Vec::new(),
            },
            ..Showdown::open(fixtures::BF1, 0, 1)
        });
        fixture.blob.prompt = Some(Prompt::new(4, 0, 1, 2));
        let ctx = fixture.ctx();
        assert_eq!(
            status(&ctx, PromptWhy::PickStaged),
            "which showdown opens first?"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Target { item: 9, spec: 0 }),
            "the card: choose a target (0 of 2)"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Assign),
            "assign 3 damage: who takes lethal next?"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Resume { item: 1, stage: 0 }),
            "the card: choose a choice (0 of 2)"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Shuffle { why: 0 }),
            "roll to shuffle 0 recycled cards"
        );
        assert_eq!(
            status(&ctx, PromptWhy::Discard { item: 1, stage: 0 }),
            "discard a card"
        );
        assert_eq!(
            status(&ctx, PromptWhy::PayWith { item: 1 }),
            "pay 1 power for the card with"
        );
        assert_eq!(
            status(&ctx, PromptWhy::OrderTriggers { seat: 0 }),
            "order your triggers (last placed resolves first)"
        );
        assert_eq!(
            status(&ctx, PromptWhy::OptionalCost { item: 7, cost: 0 }),
            "accelerate the card for ?"
        );
        assert_eq!(
            status(&ctx, PromptWhy::GroupMove { unit: 1, to: 9 }),
            "move others to {zone 9} too?"
        );
        assert_eq!(
            status(&ctx, PromptWhy::PlayLocation { item: 1 }),
            "where does the card enter?"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::Mode {
                    item: 1,
                    execution: 0
                }
            ),
            "the card: choose one"
        );
        assert_eq!(
            status(
                &ctx,
                PromptWhy::Mode {
                    item: 1,
                    execution: 1
                }
            ),
            "the card: choose one for the repeat"
        );
        assert_eq!(
            answer_words(PromptWhy::Mode {
                item: 1,
                execution: 0
            }),
            ["choose one", "cancel"]
        );
        let modes = options(
            &ctx,
            &Prompt::new(4, 0, 1, 1).cancellable(),
            PromptWhy::Mode {
                item: 1,
                execution: 0,
            },
        );
        assert_eq!(modes.len(), 1, "no pending item: only cancel");
        assert_eq!(modes[0].answer, Answer::Cancel);
        let spell_name = PromptWhy::Name {
            item: 1,
            kind: NameKind::Spell,
            stage: 0,
        };
        let tag_name = PromptWhy::Name {
            item: 1,
            kind: NameKind::Tag,
            stage: 0,
        };
        assert_eq!(status(&ctx, spell_name), "the card: name a spell");
        assert_eq!(status(&ctx, tag_name), "the card: name a tag");
        assert_eq!(answer_words(spell_name), ["name a spell", "cancel"]);
        assert_eq!(answer_words(tag_name), ["name a tag", "cancel"]);
        let spells = options(&ctx, &Prompt::new(4, 0, 1, 1), spell_name);
        let offered: Vec<&str> = spells.iter().map(|opt| opt.label.as_str()).collect();
        assert_eq!(
            offered,
            crate::cards::spell_names(),
            "762: every spell the catalog knows, never what the table holds"
        );
        assert!(offered.contains(&"Defy"));
        assert!(
            !offered.contains(&"Spark"),
            "the fixture's Spark is in a hand, not in the catalog"
        );
        assert_eq!(spells[0].answer, Answer::Name(0));
        let tags = options(&ctx, &Prompt::new(4, 0, 1, 1).cancellable(), tag_name);
        assert_eq!(tags.len(), crate::cards::TAGS.len() + 1);
        assert_eq!(tags[0].label, "Ahri");
        assert_eq!(tags[0].answer, Answer::Name(0));
        assert_eq!(tags.last().unwrap().answer, Answer::Cancel);
        let discards = options(
            &ctx,
            &Prompt::new(4, 0, 1, 1),
            PromptWhy::Discard { item: 1, stage: 0 },
        );
        assert_eq!(discards.len(), 4);
        assert_eq!(discards[0].answer, Answer::Card(fixtures::HAND_UNIT));
    }

    #[test]
    fn a_pick_answers_the_open_prompt_by_number_and_stale_or_foreign_picks_are_refused() {
        let mut fixture = Fixture::enforced();
        fixture.blob.staged = vec![
            Staged {
                zone: fixtures::BF1,
                combat: false,
                contester: 0,
            },
            Staged {
                zone: fixtures::BF2,
                combat: false,
                contester: 0,
            },
        ];
        fixture.blob.open_prompt(Ask {
            prompt: Prompt::new(3, 0, 1, 1),
            why: PromptWhy::PickStaged,
        });
        let mut ctx = fixture.ctx();
        let pick = |prompt, option| Pick { prompt, option };
        assert_eq!(
            answer(&mut ctx, 0, pick(2, 0)),
            Err(Refusal::Pick(PickRefusal::Stale { open: 3, sent: 2 }))
        );
        assert_eq!(
            answer(&mut ctx, 1, pick(3, 0)),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            answer(&mut ctx, 0, pick(3, 2)),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        assert!(ctx.blob.prompt.is_some());
        let answered = answer(&mut ctx, 0, pick(3, 1)).unwrap().unwrap();
        assert_eq!(answered.answer, Answer::Zone(fixtures::BF2));
        assert_eq!(answered.why, PromptWhy::PickStaged);
        assert_eq!(answered.prompt.picked, [u32::from(fixtures::BF2)]);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.why.is_none());
        assert_eq!(answer(&mut ctx, 0, pick(3, 1)), Err(Refusal::NoPrompt));
        let mut multi = Fixture::enforced();
        multi.blob.open_prompt(Ask {
            prompt: Prompt::new(5, 0, 0, 2),
            why: PromptWhy::Mulligan,
        });
        let mut ctx = multi.ctx();
        assert_eq!(answer(&mut ctx, 0, pick(5, 0)).unwrap(), None);
        assert_eq!(
            ctx.blob.prompt.as_ref().unwrap().picked,
            [fixtures::HAND_UNIT]
        );
        let labels: Vec<String> = offered(&ctx).iter().map(|opt| opt.label.clone()).collect();
        assert_eq!(
            labels,
            [
                "set aside {card 71}",
                "set aside {card 72}",
                "set aside {card 73}",
                "keep"
            ]
        );
        let kept = answer(&mut ctx, 0, pick(5, 3)).unwrap().unwrap();
        assert_eq!(kept.answer, Answer::Done);
        assert_eq!(kept.prompt.picked, [fixtures::HAND_UNIT]);
        let mut lone = Fixture::enforced();
        lone.blob.staged = vec![Staged {
            zone: fixtures::BF1,
            combat: false,
            contester: 0,
        }];
        lone.blob.open_prompt(Ask {
            prompt: Prompt::new(6, 0, 1, 1),
            why: PromptWhy::PickStaged,
        });
        let mut ctx = lone.ctx();
        let auto = next_auto(&mut ctx).unwrap();
        assert_eq!(auto.answer, Answer::Zone(fixtures::BF1));
        assert!(ctx.blob.prompt.is_none());
    }
}

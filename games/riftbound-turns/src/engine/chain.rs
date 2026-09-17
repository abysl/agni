use crate::cards::{Flow, NameKind, Stage, GRANTED};
use crate::engine::ctx::{CounterDest, Ctx};
use crate::engine::legal::Reason;
use crate::engine::{activate, cleanup, kill, phases, play, priority, showdown, targets, triggers};
use crate::state::{
    ChainItem, GameBlob, ItemKind, ItemStatus, Leave, Needs, Origin, Phase, Priority, PromptWhy,
    When,
};
use crate::Refusal;
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::prompt::Answer;
use agni_plugin_sdk::table::ApplyError;

pub const PROCEED_LIMIT: usize = 64;
pub const PAID: u32 = 1;

pub fn is_open(ctx: &Ctx) -> bool {
    ctx.blob.prompt.is_none() && ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty()
}

pub fn proceed(ctx: &mut Ctx) {
    for _ in 0..PROCEED_LIMIT {
        let open = open_batches(ctx);
        let queued = ctx.blob.queue.len();
        triggers::collect(ctx);
        hold_group_batch(ctx, queued, &open);
        if ctx.blob.prompt.is_some() {
            return;
        }
        if ctx
            .blob
            .chain
            .last()
            .is_some_and(|top| top.status == ItemStatus::Resolving)
        {
            if ctx.blob.chain.last().is_some_and(kill::is_choice) {
                resolve_parked(ctx);
                continue;
            }
            return;
        }
        if let Some(first) = ctx.blob.queue.first() {
            let (id, seat, needs) = (first.item.id, first.item.controller, first.needs);
            if needs == Needs::Order {
                triggers::ask_order(ctx, seat);
                continue;
            }
            if play::advance(ctx, id).is_err() {
                play::cancel(ctx, id);
            }
            continue;
        }
        if ctx.blob.chain.is_empty() {
            if !cleanup::dying(ctx).is_empty() {
                cleanup::run(ctx, None);
                continue;
            }
            open_state(ctx);
            triggers::collect(ctx);
            if ctx.blob.prompt.is_none() && ctx.blob.queue.is_empty() {
                return;
            }
            continue;
        }
        let players = ctx.players();
        let top_controller = ctx.blob.chain.last().map(|top| top.controller).unwrap_or(0);
        let held = *ctx.blob.priority.get_or_insert(Priority {
            active: top_controller,
            passes: 0,
        });
        if held.passes >= players {
            resolve_top(ctx);
            continue;
        }
        if priority::can_act(ctx, held.active) {
            return;
        }
        ctx.narrate(format!("{{seat {}}} passes · nothing to play", held.active));
        priority::step(ctx);
    }
    ctx.narrate("the chain stalled");
}

fn open_batches(ctx: &Ctx) -> Vec<u8> {
    if ctx.blob.prompt.is_some() {
        return Vec::new();
    }
    let mut seats: Vec<u8> = ctx
        .blob
        .queue
        .iter()
        .filter(|pending| pending.needs == Needs::Order)
        .map(|pending| pending.item.controller)
        .collect();
    seats.sort_unstable();
    seats.dedup();
    seats
}

fn hold_group_batch(ctx: &mut Ctx, queued: usize, open: &[u8]) {
    let grouping = matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. }));
    for pending in ctx.blob.queue.iter_mut().skip(queued) {
        if grouping || open.contains(&pending.item.controller) {
            pending.needs = Needs::Order;
        }
    }
}

fn resolve_parked(ctx: &mut Ctx) {
    let Some(item) = ctx.blob.chain.pop() else {
        return;
    };
    ctx.picked.clear();
    let stage = item.stage;
    run(ctx, item, stage);
}

fn open_state(ctx: &mut Ctx) {
    ctx.blob.priority = None;
    match ctx.blob.phase() {
        Some(Phase::Beginning) => {
            let seat = ctx.turn_player();
            if triggers::queue_delayed(ctx, When::BeginningOf(seat)) == 0 {
                phases::continue_beginning(ctx);
            }
        }
        Some(Phase::Ending) => {
            let turn = ctx.turn();
            if triggers::queue_delayed(ctx, When::EndOfTurn(turn)) == 0 {
                phases::finish_turn(ctx);
            }
        }
        Some(Phase::Action) => {
            if let Some(showdown) = ctx.blob.showdown.as_mut() {
                showdown.initial_chain = false;
            }
            showdown::open_next(ctx);
            showdown::auto_pass(ctx);
        }
        _ => {}
    }
}

pub fn resolve_top(ctx: &mut Ctx) {
    if ctx
        .blob
        .chain
        .last()
        .is_some_and(|top| top.status == ItemStatus::Resolving)
    {
        return;
    }
    let Some(mut item) = ctx.blob.chain.pop() else {
        return;
    };
    item.status = ItemStatus::Resolving;
    ctx.picked.clear();
    let stage = item.stage;
    run(ctx, item, stage);
}

pub fn execution_view(ctx: &Ctx, item: &ChainItem) -> ChainItem {
    if !item.repeated() {
        return item.clone();
    }
    let (spec_offset, target_offset, spec_count, target_count, all_targets) =
        execution_bounds(ctx, item);
    let mut view = item.clone();
    view.targets = item
        .targets
        .iter()
        .skip(target_offset)
        .take(target_count)
        .copied()
        .collect();
    view.targets
        .extend(item.targets.iter().skip(all_targets).copied());
    view.spec_counts = item
        .spec_counts
        .iter()
        .skip(spec_offset)
        .take(spec_count)
        .copied()
        .collect();
    if item.execution > 0 {
        if let Some(mode) = item.mode_at(item.execution) {
            view.set_mode(0, mode);
        }
    }
    view
}

fn execution_bounds(ctx: &Ctx, item: &ChainItem) -> (usize, usize, usize, usize, usize) {
    let mut spec_offset = 0;
    let mut target_offset = 0;
    let mut all_targets = 0;
    let mut current = None;
    for execution in 0..=item.repeats() {
        let specs = targets::specs_of_execution(ctx, item, execution);
        let target_count = specs
            .iter()
            .enumerate()
            .map(|(local, spec)| {
                item.spec_counts
                    .get(spec_offset + local)
                    .map(|count| usize::from(*count))
                    .unwrap_or(usize::from(spec.max.max(1)))
            })
            .sum::<usize>();
        if execution == item.execution {
            current = Some((spec_offset, target_offset, specs.len(), target_count));
        }
        spec_offset += specs.len();
        target_offset += target_count;
        all_targets = target_offset;
    }
    let current = current.unwrap_or((spec_offset, target_offset, 0, 0));
    (current.0, current.1, current.2, current.3, all_targets)
}

struct Checkpoint {
    blob: GameBlob,
    effects: usize,
    events: usize,
    spawned: u32,
    faulted: bool,
}

impl Checkpoint {
    fn take(ctx: &Ctx) -> Self {
        Self {
            blob: ctx.blob.clone(),
            effects: ctx.effects.len(),
            events: ctx.events.len(),
            spawned: ctx.spawned,
            faulted: ctx.fault.is_some(),
        }
    }

    fn new_fault(&self, ctx: &Ctx) -> Option<ApplyError> {
        if self.faulted {
            return None;
        }
        ctx.fault.map(|(_, error)| error)
    }

    fn restore(self, ctx: &mut Ctx) {
        *ctx.blob = self.blob;
        ctx.effects.truncate(self.effects);
        ctx.events.truncate(self.events);
        ctx.collected = ctx.collected.min(self.events);
        ctx.spawned = self.spawned;
        ctx.fault = None;
        ctx.awaiting.clear();
        ctx.replay_table();
    }
}

fn fizzle(ctx: &mut Ctx, item: ChainItem, error: ApplyError) {
    let source = item.kind.source();
    ctx.narrate(format!(
        "{{card {source}}} fizzles · the engine could not apply its effects ({error:?})"
    ));
    if let ItemKind::Spell { card } = item.kind {
        leave(ctx, &item);
        ctx.blob.drop_card_state(card);
    }
    cleanup::run(ctx, Some(item.id));
    ctx.blob.priority = ctx.blob.chain.last().map(|top| Priority {
        active: top.controller,
        passes: 0,
    });
}

fn run(ctx: &mut Ctx, mut item: ChainItem, mut stage: u8) {
    loop {
        let view = execution_view(ctx, &item);
        let checkpoint = Checkpoint::take(ctx);
        ctx.resolving = Some(view.clone());
        ctx.defer_limited = true;
        let flow = match targets::ability_of(ctx, &view) {
            Some(ability) => (ability.run)(ctx, &view, Stage(stage)),
            None => Flow::Done,
        };
        ctx.defer_limited = false;
        ctx.resolving = None;
        ctx.picked.clear();
        let remembered = std::mem::take(&mut ctx.remembered);
        if let Some(error) = checkpoint.new_fault(ctx) {
            checkpoint.restore(ctx);
            fizzle(ctx, item, error);
            return;
        }
        match flow {
            Flow::Ask(ask) => {
                if let PromptWhy::Resume { stage, .. }
                | PromptWhy::Discard { stage, .. }
                | PromptWhy::Name { stage, .. } = ask.why
                {
                    item.stage = stage;
                }
                item.targets.extend(remembered);
                item.awaiting = std::mem::take(&mut ctx.awaiting);
                item.status = ItemStatus::Resolving;
                ctx.blob.chain.push(item);
                return;
            }
            Flow::Done => {
                if item.execution < item.repeats() {
                    let (_, _, _, _, all_targets) = execution_bounds(ctx, &item);
                    item.targets.truncate(all_targets);
                    item.awaiting.clear();
                    item.execution += 1;
                    stage = 0;
                    ctx.narrate(format!("{{card {}}} repeats", item.kind.source()));
                    continue;
                }
                finish(ctx, item);
                return;
            }
        }
    }
}

pub fn leave(ctx: &mut Ctx, item: &ChainItem) {
    let Some(card) = item.kind.card() else {
        return;
    };
    let controller = item.controller;
    if ctx.is_token(card) {
        ctx.emit(Effect::Despawn { card });
        ctx.blob.drop_card_state(card);
        return;
    }
    if ctx
        .card(card)
        .is_none_or(|held| held.zone != ctx.zones.chain)
    {
        return;
    }
    match item.origin {
        Origin::Trash {
            leave: Leave::Banish,
        } => {
            ctx.banish_by(card, item.controller);
        }
        Origin::Trash {
            leave: Leave::Recycle,
        } => {
            ctx.recycle_to_bottom(card);
            ctx.blob.drop_card_state(card);
            ctx.narrate(format!("{{card {card}}} is recycled"));
        }
        _ => {
            ctx.file_in_trash(card, controller);
            ctx.blob.drop_card_state(card);
        }
    }
}

pub fn counter(ctx: &mut Ctx, item: u16, dest: CounterDest) -> bool {
    let Some(held) = ctx.chain_item(item).cloned() else {
        return false;
    };
    let limited =
        matches!(held.kind, ItemKind::Spell { .. }) && matches!(held.origin, Origin::Trash { .. });
    if !limited {
        return ctx.counter_item(item, dest);
    }
    if ctx.refuse_counter(item) {
        return false;
    }
    let Some(index) = ctx.blob.chain.iter().position(|each| each.id == item) else {
        return false;
    };
    ctx.blob.chain.remove(index);
    let card = held.kind.source();
    ctx.set_flag(card, crate::state::FLAG_NOT_PLAYED, true);
    ctx.narrate(format!("{{card {card}}} is countered"));
    leave(ctx, &held);
    true
}

fn finish(ctx: &mut Ctx, item: ChainItem) {
    let controller = item.controller;
    match item.kind {
        ItemKind::Spell { card } => {
            ctx.raise(crate::engine::ctx::Event::PlayedSpell {
                item: item.id,
                controller,
                nth: item.slot(crate::state::SLOT_ORDINAL).unwrap_or(0),
            });
            leave(ctx, &item);
            ctx.blob.drop_card_state(card);
            ctx.note_played(controller);
            ctx.narrate(format!("{{card {card}}} resolves"));
        }
        ItemKind::Ability { source, index } => {
            ctx.raise(crate::engine::ctx::Event::Activated {
                item: item.id,
                source,
                index,
                controller,
            });
            ctx.narrate(format!("{{card {source}}} ability resolves"));
        }
        ItemKind::Lent {
            holder,
            lender,
            index,
        } => {
            let index = activate::lent_index_of(ctx, holder, lender, index).unwrap_or(GRANTED);
            ctx.raise(crate::engine::ctx::Event::Activated {
                item: item.id,
                source: holder,
                index,
                controller,
            });
            ctx.narrate(format!("{{card {holder}}} ability resolves"));
        }
        ItemKind::Trigger { .. } | ItemKind::Granted { .. } => {
            if !kill::is_choice(&item) {
                let source = item.kind.source();
                ctx.narrate(format!("{{card {source}}} ability resolves"));
            }
        }
        ItemKind::Permanent { .. } => {}
    }
    cleanup::run(ctx, Some(item.id));
    ctx.blob.priority = ctx.blob.chain.last().map(|top| Priority {
        active: top.controller,
        passes: 0,
    });
}

pub fn face_arrived(ctx: &mut Ctx, card: u32) -> Result<bool, Refusal> {
    let Some(index) = ctx
        .blob
        .chain
        .iter()
        .position(|held| held.status == ItemStatus::Resolving && held.awaiting.contains(&card))
    else {
        return Ok(false);
    };
    let held = &mut ctx.blob.chain[index];
    held.awaiting.retain(|waited| *waited != card);
    if !held.awaiting.is_empty() {
        return Ok(true);
    }
    let item = ctx.blob.chain.remove(index);
    ctx.picked.clear();
    let stage = item.stage;
    run(ctx, item, stage);
    Ok(true)
}

pub fn paid(ctx: &Ctx) -> bool {
    ctx.picks() == [PAID]
}

pub fn pay_or_let(ctx: &mut Ctx, item: u16, stage: u8, answer: Answer) -> Result<(), Refusal> {
    resume(ctx, item, stage, &[PAID], answer)
}

pub fn name(
    ctx: &mut Ctx,
    item: u16,
    stage: u8,
    kind: NameKind,
    index: u16,
) -> Result<(), Refusal> {
    let Some(source) = ctx
        .chain_item(item)
        .filter(|held| held.status == ItemStatus::Resolving)
        .map(|held| held.kind.source())
    else {
        return Err(Refusal::NoPrompt);
    };
    let Some(name) = ctx.name_options(kind).get(usize::from(index)).cloned() else {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    };
    ctx.name(source, &name);
    resume(ctx, item, stage, &[], Answer::Name(index))
}

pub fn resume(
    ctx: &mut Ctx,
    item: u16,
    stage: u8,
    picked: &[u32],
    answer: Answer,
) -> Result<(), Refusal> {
    let Some(index) = ctx
        .blob
        .chain
        .iter()
        .position(|held| held.id == item && held.status == ItemStatus::Resolving)
    else {
        return Err(Refusal::NoPrompt);
    };
    let mut held = ctx.blob.chain.remove(index);
    if let Answer::Mode(mode) = answer {
        held.set_mode(held.execution, mode);
    }
    ctx.picked = match answer {
        Answer::Skip | Answer::Cancel | Answer::No => Vec::new(),
        _ => picked.to_vec(),
    };
    run(ctx, held, stage);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        self, a_friendly_unit, a_spell, a_unit, an_enemy_unit, play as play_ability, target,
        triggered, units_up_to,
    };
    use crate::cards::TargetKind;
    use crate::cards::{Card, Keyword, Trigger};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, phases, prompts, settle};
    use crate::rules::COUNTER_POINTS;
    use crate::state::{ChainItem, Expiry, Origin, PromptWhy, TargetRef, FLAG_NOT_PLAYED};
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Answer, Pick, PickRefusal};
    use agni_plugin_sdk::table::Target;

    static NEGATE: Card = prelude::spell(
        "Negate",
        &[Keyword::Reaction],
        &[play_ability(
            &[a_spell("a spell to counter")],
            |ctx, item, _| {
                prelude::counter_spell(ctx, item, 0);
                Flow::Done
            },
        )],
    );

    static EMBOLDEN: Card = prelude::spell(
        "Embolden",
        &[Keyword::Reaction],
        &[play_ability(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                prelude::might_this_turn(ctx, item, unit, 2, None);
            }
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    static BLAST: Card = prelude::spell(
        "Blast",
        &[],
        &[play_ability(
            &[units_up_to(2, "up to two units")],
            |ctx, item, _| {
                for unit in prelude::card_targets(ctx, item) {
                    prelude::deal(ctx, item, unit, 6);
                }
                Flow::Done
            },
        )],
    );

    static STUDENT: Card = prelude::unit(
        "Student",
        &[],
        &[triggered(Trigger::YouPlaySpell, &[], |ctx, item, _| {
            let me = item.kind.source();
            prelude::might_this_turn(ctx, item, me, 1, None);
            Flow::Done
        })],
    );

    static JEWEL: Card = prelude::gear(
        "Jewel",
        &[],
        &[triggered(
            Trigger::Draw { nth: 2 },
            &[a_friendly_unit("a friendly unit")],
            |ctx, item, _| {
                if let Some(unit) = prelude::card_target(ctx, item, 0) {
                    prelude::might_this_turn(ctx, item, unit, 2, None);
                }
                Flow::Done
            },
        )],
    );

    static LATER: Card = prelude::unit(
        "Later",
        &[],
        &[triggered(Trigger::Reflexive, &[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    static NOTICED: Card = prelude::unit(
        "Noticed",
        &[],
        &[triggered(Trigger::Chosen, &[], |_, _, _| Flow::Done)],
    );

    static ECHO: Card = prelude::unit(
        "Echo",
        &[],
        &[
            triggered(Trigger::Reflexive, &[], |ctx, item, _| {
                let turn = ctx.turn();
                ctx.delay(
                    When::EndOfTurn(turn),
                    item.kind.source(),
                    item.controller,
                    1,
                    Vec::new(),
                );
                Flow::Done
            }),
            triggered(Trigger::Reflexive, &[], |ctx, item, _| {
                prelude::draw(ctx, item.controller, 1);
                Flow::Done
            }),
        ],
    );

    static HEX: Card = prelude::gear(
        "Hex",
        &[],
        &[triggered(
            Trigger::Draw { nth: 1 },
            &[an_enemy_unit("an enemy unit")],
            |ctx, item, _| {
                if let Some(unit) = prelude::card_target(ctx, item, 0) {
                    prelude::might_this_turn(ctx, item, unit, -1, None);
                }
                Flow::Done
            },
        )],
    );

    static PAIR: Card = prelude::gear(
        "Pair",
        &[],
        &[triggered(
            Trigger::Draw { nth: 1 },
            &[target(
                prelude::FRIENDLY_UNIT,
                2,
                2,
                TargetKind::Card,
                "two friendly units",
            )],
            |_, _, _| Flow::Done,
        )],
    );

    const RIPOSTE: u32 = 76;
    const THEIR_NEGATE: u32 = 82;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            RIPOSTE,
            fixtures::HAND,
            0,
            "Embolden",
            1,
            0,
        ));
        let mut negate = fixtures::spell(THEIR_NEGATE, fixtures::HAND, 1, "Negate", 1, 0);
        negate.domain = vec!["Mind".into()];
        fixture.table.cards.push(negate);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(RIPOSTE, &EMBOLDEN)
            .with_script(THEIR_NEGATE, &NEGATE);
        fixture
    }

    #[test]
    fn lethal_damage_marked_outside_a_resolution_is_cleaned_up_when_the_chain_proceeds() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.is_neutral_open());
    }

    fn with_student(mut fixture: Fixture) -> Fixture {
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &STUDENT);
        fixture
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            crate::engine::resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), crate::engine::ctx::COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn a_seat_reacts_to_its_own_spell_and_the_pass_ring_resolves_top_down() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            })
        );
        assert_eq!(
            legal::classify(
                &ctx,
                1,
                &crate::engine::ctx::EntryMove {
                    card: THEIR_NEGATE,
                    from: ctx.zones.hand,
                    from_seat: 1,
                    to: ctx.zones.chain,
                    to_seat: 0,
                    index: agni_plugin_sdk::decide::TOP,
                    hidden: false,
                }
            ),
            Err(Refusal::Illegal(crate::engine::legal::Reason::ChainClosed)),
            "the other seat waits for priority"
        );
        play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let offered = prompts::offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        assert_eq!(offered[0].card, Some(fixtures::VI));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 76}: choose a unit (0 of 1)"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::VI,
            by: 0,
            item: 2
        }));
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            }),
            "a play resets the passes"
        );
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::exhaust(43)),
            "the reaction is paid with the last ready rune"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            priority::pass(&mut ctx, 0),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotYourPriority
            ))
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.hand_of(0).len(), 4, "Embolden drew one");
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            }),
            "priority returns to the controller of the new top"
        );
        assert!(ctx.blob.queue.is_empty());
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::Move {
                card: fixtures::HAND_SPELL,
                zone: fixtures::TRASH,
                seat: 0,
                index: agni_plugin_sdk::decide::TOP
            })
        );
        assert!(ctx.blob.seat(0).played_main);
        assert!(prelude::legion(&ctx, 0));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::PlayedSpell { .. }))
                .count(),
            2
        );
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{card 71} resolves",
            "{:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.blob.chain.len(), 0);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::VI),
            counter: crate::engine::ctx::COUNTER_MIGHT,
            delta: -2
        }));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
    }

    #[test]
    fn a_countered_spell_keeps_its_cost_fires_no_play_trigger_and_never_resolves() {
        let mut fixture = with_student(armed());
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let paid = ctx.effects.clone();
        assert_eq!(
            paid,
            [
                Effect::exhaust(42),
                Effect::exhaust(41),
                Effect::Move {
                    card: fixtures::RUNE_A,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_NEGATE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let offered = prompts::offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 71} on the chain", "cancel"]
        );
        assert_eq!(offered[0].answer, Answer::Item(1));
        assert_eq!(offered[0].card, Some(fixtures::HAND_SPELL));
        let prompt_id = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 9,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::Stale {
                open: prompt_id,
                sent: 9,
            })),
            "a stale pick is refused, never misapplied"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: prompt_id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        assert!(ctx.blob.prompt.is_some());
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.priority.is_none());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(THEIR_NEGATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            ctx.effects.starts_with(&paid),
            "nothing is refunded: {:?}",
            ctx.effects
        );
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Annotate { card, .. } if *card == 41 || *card == 42
        ) && effect != &Effect::exhaust(41)
            && effect != &Effect::exhaust(42)));
        let spells: Vec<&Event> = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::PlayedSpell { .. }))
            .collect();
        assert_eq!(
            spells,
            [&Event::PlayedSpell {
                item: 2,
                controller: 1,
                nth: 1
            }],
            "only Negate was played"
        );
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "Student never saw a spell of its controller"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.blob.seat(0).played_main);
        assert!(ctx.blob.seat(1).played_main);
        assert!(!ctx.has_flag(fixtures::HAND_SPELL, FLAG_NOT_PLAYED));
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
    }

    #[test]
    fn cancel_returns_the_spell_unpaid_and_a_student_grows_when_its_controller_resolves_one() {
        let mut fixture = with_student(armed());
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        assert!(ctx.blob.prompt.as_ref().unwrap().cancel);
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        let cancel = prompts::offered(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: RIPOSTE,
                zone: fixtures::HAND,
                seat: 0,
                index: agni_plugin_sdk::decide::TOP
            }]
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} takes back {card 76}"
        );
        play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 2);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "Student's trigger went on the chain after the spell resolved"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::VI
        ));
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 1);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 50} ability resolves");
    }

    #[test]
    fn damage_from_a_spell_kills_at_cleanup_and_despawns_the_token() {
        let mut fixture = armed();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &BLAST);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.min, prompt.max), (0, 2));
        let labels = |ctx: &Ctx| {
            prompts::offered(ctx)
                .iter()
                .map(|opt| opt.label.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(&ctx),
            [
                "{card 50}",
                "{card 60}",
                "{card 81}",
                "done",
                "skip",
                "cancel"
            ]
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(labels(&ctx), ["{card 50}", "{card 81}", "done", "cancel"]);
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(
            labels(&ctx),
            ["done", "cancel"],
            "a full prompt waits for done"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: 6,
            source: Cause::Item(1)
        }));
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, .. } if *card == fixtures::SPRITE
        )));
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the dead sprite no longer holds the field"
        );
        assert!(ctx.blob.log.contains(&"{card 60} dies".to_string()));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_draw_trigger_asks_to_order_two_sources_and_the_last_placed_resolves_first() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::gear(77, fixtures::BASE, 0, "Jewel", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_GEAR, &JEWEL)
            .with_script(77, &JEWEL);
        let mut ctx = fixture.ctx();
        ctx.draw(0, 2);
        proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 2, 2));
        let offered = prompts::offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 72} trigger", "{card 77} trigger"]
        );
        assert_eq!(offered[1].card, Some(77));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::OrderTriggers { seat: 0 }),
            "order your triggers (last placed resolves first)"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the second placement and the single-candidate targets auto-answer"
        );
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[0].kind.source(), 77);
        assert_eq!(ctx.blob.chain[1].kind.source(), fixtures::HAND_GEAR);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Chosen { item, .. } if *item == 2))
                .count(),
            1,
            "an automatic single target still counts as chosen"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), 77);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, fixtures::VI), 4);
        assert!(ctx.blob.is_neutral_open());
        let mut once = armed();
        once.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        once.resolve();
        once.scripts = once
            .scripts
            .clone()
            .with_script(fixtures::HAND_GEAR, &JEWEL);
        let mut ctx = once.ctx();
        ctx.draw(0, 1);
        proceed(&mut ctx);
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
        ctx.draw(1, 2);
        proceed(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the other seat's second draw is not yours"
        );
    }

    #[test]
    fn a_delayed_trigger_runs_at_the_ending_step_and_the_turn_ends_once_it_resolves() {
        let mut fixture = armed();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &LATER);
        let mut ctx = fixture.ctx();
        ctx.delay(When::EndOfTurn(1), fixtures::VI, 0, 0, Vec::new());
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.phase(), Some(crate::state::Phase::Ending));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.delayed.is_empty());
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            phases::end_turn(&mut ctx),
            Err(Refusal::Illegal(crate::engine::legal::Reason::ChainClosed))
        );
        let hand = ctx.hand_of(0).len();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "seat 1's beginning phase puts its Sprite's Temporary trigger on the chain"
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.phase(), Some(crate::state::Phase::Action));
        assert!(ctx.blob.chain.is_empty() && ctx.blob.priority.is_none());
        assert!(
            ctx.effects.contains(&Effect::Despawn {
                card: fixtures::SPRITE
            }),
            "seat 1's beginning phase kills its temporary sprite"
        );
        assert!(!ctx.effects.contains(&Effect::score(1, COUNTER_POINTS, 1)));
    }

    #[test]
    fn the_pass_ring_passes_for_a_seat_with_nothing_to_play_and_lets_the_other_react() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 1);
        fixture.resolve();
        let mut fixture = with_student(fixture);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} passes · nothing to play".to_string()));
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "seat 1 has no hand and is passed for: {:?}",
            ctx.blob.log
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "Student's trigger waits for seat 0"
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 1, "Student grew");
        assert!(ctx.blob.is_neutral_open());
        let mut quiet = Fixture::enforced();
        let mut ctx = quiet.ctx();
        assert!(is_open(&ctx));
        ctx.might(fixtures::VI, 1, Expiry::EndOfTurn(1), None, 0);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn grant(fixture: &mut Fixture, card: u32, keyword: Keyword) {
        fixture
            .blob
            .card_state_mut(card)
            .granted
            .push((keyword, Expiry::Permanent));
    }

    #[test]
    fn a_second_deflect_target_is_not_offered_once_the_first_pick_uses_up_the_runes() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1 || [41, 42].contains(&card.id));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &BLAST);
        grant(&mut fixture, fixtures::SPRITE, Keyword::Deflect(1));
        grant(&mut fixture, fixtures::THEIR_UNIT, Keyword::Deflect(1));
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            labels(&ctx),
            [
                "{card 50}",
                "{card 60}",
                "{card 81}",
                "done",
                "skip",
                "cancel"
            ],
            "either deflect unit is affordable alone"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "done", "cancel"],
            "the pair of deflects is not: {:?}",
            ctx.blob.prompt
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::SPRITE, fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            )),
            "a direct answer with the unaffordable pair is refused before payment"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::VI]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            )),
            "each of up to two units means two different units"
        );
        assert!(ctx.blob.prompt.is_some());
        assert!(ctx.effects.is_empty());
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [1]);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::Move {
                    card: 41,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
                Effect::Move {
                    card: 42,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ],
            "two energy and the printed power plus one rainbow for the deflect"
        );
    }

    #[test]
    fn triggers_from_one_resolution_step_form_one_batch_across_events_and_seats() {
        let mut fixture = armed();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &BLAST)
            .with_script(fixtures::VI, &NOTICED)
            .with_script(fixtures::THEIR_UNIT, &NOTICED);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Card(fixtures::VI)
            ]
        );
        assert_eq!(
            ctx.blob
                .chain
                .iter()
                .map(|item| (item.kind.source(), item.controller))
                .collect::<Vec<_>>(),
            [
                (fixtures::HAND_SPELL, 0),
                (fixtures::VI, 0),
                (fixtures::THEIR_UNIT, 1)
            ],
            "the turn player places first, so the other seat's trigger is on top"
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        let mut two = armed();
        two.table.card_mut(fixtures::HAND_UNIT).unwrap().zone = Some(fixtures::BASE);
        two.resolve();
        two.scripts = two
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &BLAST)
            .with_script(fixtures::VI, &NOTICED)
            .with_script(fixtures::HAND_UNIT, &NOTICED);
        let mut ctx = two.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            labels(&ctx),
            [
                "{card 50}",
                "{card 60}",
                "{card 70}",
                "{card 81}",
                "done",
                "skip",
                "cancel"
            ]
        );
        pick(&mut ctx, 0, 0).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "two triggers of one seat from two Chosen events are ordered together"
        );
        assert_eq!(labels(&ctx), ["{card 50} trigger", "{card 70} trigger"]);
    }

    static MOVER: Card = prelude::unit(
        "Mover",
        &[],
        &[triggered(
            Trigger::Move {
                of: crate::cards::Who::Me,
                to: crate::cards::Where::Any,
            },
            &[],
            |_, _, _| Flow::Done,
        )],
    );

    static SENTRY: Card = prelude::unit(
        "Sentry",
        &[],
        &[triggered(
            Trigger::Move {
                of: crate::cards::Who::Friendly,
                to: crate::cards::Where::Battlefield,
            },
            &[],
            |_, _, _| Flow::Done,
        )],
    );

    const SECOND: u32 = 91;
    const THIRD: u32 = 92;

    fn answer(ctx: &mut Ctx, seat: u8, label: &str) {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option).unwrap();
    }

    fn marching_party() -> Fixture {
        marching_party_with(false)
    }

    fn marching_party_with(second_exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut second = fixtures::unit(SECOND, fixtures::BASE, 0, "Mover", 2);
        second.exhausted = second_exhausted;
        fixture.table.cards.push(second);
        fixture
            .table
            .cards
            .push(fixtures::unit(THIRD, fixtures::BASE, 1, "Sentry", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::VI, &MOVER)
            .with_script(SECOND, &MOVER)
            .with_script(THIRD, &SENTRY);
        fixture
    }

    fn march(fixture: &mut Fixture, unit: u32, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(unit, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn a_group_standard_move_is_one_batch_of_triggers_held_open_across_the_group_prompt() {
        let mut fixture = marching_party();
        let mut ctx = march(&mut fixture, fixtures::VI, fixtures::BF1);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove { unit, .. }) if unit == fixtures::VI
        ));
        assert_eq!(
            ctx.blob
                .queue
                .iter()
                .map(|pending| (pending.item.controller, pending.needs))
                .collect::<Vec<_>>(),
            [(0, Needs::Order)],
            "the leader's trigger waits for the rest of the group before anyone orders"
        );
        assert_eq!(
            open_batches(&ctx),
            Vec::<u8>::new(),
            "an open prompt holds no batch"
        );
        answer(&mut ctx, 0, &format!("{{card {SECOND}}}"));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "143.3 · both units moved as one declaration, so their triggers are one batch"
        );
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}} trigger", fixtures::VI),
                format!("{{card {SECOND}}} trigger")
            ]
        );
        answer(&mut ctx, 0, &format!("{{card {SECOND}}} trigger"));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob
                .chain
                .iter()
                .map(|item| item.kind.source())
                .collect::<Vec<u32>>(),
            [SECOND, fixtures::VI],
            "placed first resolves last"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn the_held_batch_is_a_lone_trigger_when_the_group_is_declined_and_a_solo_move_never_holds() {
        let mut fixture = marching_party();
        let mut ctx = march(&mut fixture, fixtures::VI, fixtures::BF1);
        answer(&mut ctx, 0, "done");
        assert!(ctx.blob.prompt.is_none(), "a batch of one orders itself");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::VI);
        let mut alone = marching_party_with(true);
        let ctx = march(&mut alone, fixtures::VI, fixtures::BF1);
        assert!(ctx.blob.prompt.is_none(), "no companion, no group prompt");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn holding_a_batch_marks_only_what_the_collect_appended_and_only_for_open_seats() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        let item = |id: u16, seat: u8| {
            ChainItem::new(
                id,
                ItemKind::Trigger {
                    source: fixtures::VI,
                    index: 0,
                },
                seat,
                Origin::Board,
            )
        };
        ctx.blob.queue.push(crate::state::Pending {
            item: item(1, 1),
            needs: Needs::Order,
        });
        ctx.blob.queue.push(crate::state::Pending {
            item: item(2, 0),
            needs: Needs::Choices,
        });
        assert_eq!(open_batches(&ctx), [1]);
        ctx.blob.queue.push(crate::state::Pending {
            item: item(3, 1),
            needs: Needs::Choices,
        });
        ctx.blob.queue.push(crate::state::Pending {
            item: item(4, 0),
            needs: Needs::Choices,
        });
        hold_group_batch(&mut ctx, 2, &[1]);
        assert_eq!(
            ctx.blob
                .queue
                .iter()
                .map(|pending| (pending.item.id, pending.needs))
                .collect::<Vec<_>>(),
            [
                (1, Needs::Order),
                (2, Needs::Choices),
                (3, Needs::Order),
                (4, Needs::Choices)
            ],
            "the appended item of the open seat joins its batch; the other seat's does not"
        );
        ctx.ask(
            0,
            0,
            1,
            false,
            PromptWhy::GroupMove {
                unit: fixtures::VI,
                to: fixtures::BF1,
            },
        );
        assert!(open_batches(&ctx).is_empty());
        hold_group_batch(&mut ctx, 3, &[]);
        assert_eq!(
            ctx.blob.queue[3].needs,
            Needs::Order,
            "while the group is being declared every new trigger is held"
        );
        assert_eq!(
            ctx.blob.queue[1].needs,
            Needs::Choices,
            "older items are left alone"
        );
    }

    #[test]
    fn a_delayed_trigger_scheduled_during_the_ending_chain_still_fires_this_turn() {
        let mut fixture = armed();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &ECHO);
        let mut ctx = fixture.ctx();
        ctx.delay(When::EndOfTurn(1), fixtures::VI, 0, 0, Vec::new());
        let hand = ctx.hand_of(0).len();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.phase(),
            Some(crate::state::Phase::Ending),
            "the echo went on the chain instead of the turn ending"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.delayed.is_empty());
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == fixtures::VI
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.phase(), Some(crate::state::Phase::Action));
    }

    #[test]
    fn a_trigger_that_incurs_a_deflect_cost_asks_before_paying_and_is_removed_when_declined() {
        let hexed = || {
            let mut fixture = armed();
            fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
            fixture.resolve();
            fixture.scripts = fixture
                .scripts
                .clone()
                .with_script(fixtures::HAND_GEAR, &HEX);
            grant(&mut fixture, fixtures::THEIR_UNIT, Keyword::Deflect(1));
            fixture
        };
        let mut fixture = hexed();
        let mut ctx = fixture.ctx();
        ctx.draw(0, 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 60}", "{card 81}"]);
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        assert_eq!(labels(&ctx), ["yes", "no"]);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "pay 1 any power for the {card 72} trigger?"
        );
        let before = ctx.effects.len();
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            &ctx.effects[before..],
            [Effect::Move {
                card: fixtures::RUNE_A,
                zone: fixtures::RUNE_DECK,
                seat: 0,
                index: BOTTOM
            }],
            "the deflect is paid with the spent rune"
        );
        let mut fixture = hexed();
        let mut declined = fixture.ctx();
        declined.draw(0, 1);
        settle(&mut declined).unwrap();
        pick(&mut declined, 0, 1).unwrap();
        let before = declined.effects.len();
        pick(&mut declined, 0, 1).unwrap();
        assert!(declined.blob.prompt.is_none());
        assert!(declined.blob.chain.is_empty() && declined.blob.queue.is_empty());
        assert_eq!(declined.effects.len(), before);
        assert_eq!(
            declined.blob.log.last().unwrap(),
            "{card 72} trigger is removed · its cost is declined"
        );
    }

    #[test]
    fn a_trigger_short_of_its_printed_minimum_is_removed_instead_of_asked_for_less() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_GEAR, &PAIR);
        let mut ctx = fixture.ctx();
        ctx.draw(0, 1);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{card 72} trigger fizzles · not enough legal targets"
        );
    }

    static REPEATER: Card = prelude::spell(
        "Echo",
        &[
            Keyword::Reaction,
            Keyword::Repeat(crate::cards::Cost {
                energy: 1,
                power: &[],
            }),
        ],
        &[play_ability(
            &[a_spell("a spell to counter")],
            |ctx, item, _| {
                prelude::counter_spell(ctx, item, 0);
                Flow::Done
            },
        )],
    );

    static SEER: Card = prelude::spell(
        "Seer",
        &[],
        &[play_ability(&[], |ctx, item, stage| match stage.0 {
            0 => {
                let faces = vec![fixtures::THEIR_HAND_CARD, 24];
                Flow::Ask(ctx.await_faces(item, &faces, 1))
            }
            _ => {
                prelude::draw(ctx, item.controller, 1);
                Flow::Done
            }
        })],
    );

    static RANSOM: Card = prelude::spell(
        "Ransom",
        &[Keyword::Reaction],
        &[play_ability(
            &[a_spell("a spell to counter unless its controller pays")],
            |ctx, item, stage| match stage.0 {
                0 => {
                    let Some(target) = prelude::item_target(ctx, item, 0) else {
                        return Flow::Done;
                    };
                    let payer = prelude::item_controller(ctx, target).unwrap_or(0);
                    let cost = crate::engine::cost::Cost {
                        energy: 2,
                        ..crate::engine::cost::Cost::free()
                    };
                    Flow::Ask(ctx.ask_pay_or_let(item, payer, &cost, 1))
                }
                _ => {
                    if paid(ctx) {
                        ctx.narrate("the ransom is paid");
                    } else {
                        prelude::counter_spell(ctx, item, 0);
                    }
                    Flow::Done
                }
            },
        )],
    );

    const ECHO_CARD: u32 = 93;
    const SEER_CARD: u32 = 94;
    const RANSOM_CARD: u32 = 95;

    fn echoing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::spell(ECHO_CARD, fixtures::HAND, 1, "Echo", 0, 0));
        fixture.table.cards.push(fixtures::spell(
            RIPOSTE,
            fixtures::HAND,
            0,
            "Embolden",
            0,
            0,
        ));
        fixture
            .table
            .cards
            .push(fixtures::spell(SEER_CARD, fixtures::HAND, 0, "Seer", 0, 0));
        fixture.table.cards.push(fixtures::spell(
            RANSOM_CARD,
            fixtures::HAND,
            1,
            "Ransom",
            0,
            0,
        ));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(ECHO_CARD, &REPEATER)
            .with_script(RIPOSTE, &EMBOLDEN)
            .with_script(SEER_CARD, &SEER)
            .with_script(RANSOM_CARD, &RANSOM);
        fixture
    }

    #[test]
    fn a_repeated_spell_runs_its_script_once_per_group_and_the_same_target_twice_is_legal() {
        let mut fixture = echoing();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 0, RIPOSTE).unwrap_err();
        play_from_hand(&mut ctx, 1, ECHO_CARD).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: play::SLOT_REPEAT as u8
            })
        );
        answer(&mut ctx, 1, "yes");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        answer(&mut ctx, 1, "{card 71} on the chain");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        answer(&mut ctx, 1, "{card 71} on the chain");
        let echo = ctx.blob.chain.last().unwrap().clone();
        assert!(echo.repeated());
        assert_eq!(echo.spec_counts, [1, 1]);
        assert_eq!(
            echo.targets,
            [TargetRef::Item(1), TargetRef::Item(1)],
            "820.2.a: the same spell twice"
        );
        let first = execution_view(&ctx, &echo);
        assert_eq!(first.targets, [TargetRef::Item(1)]);
        assert_eq!(first.spec_counts, [1]);
        let mut second = echo.clone();
        second.execution = 1;
        let second = execution_view(&ctx, &second);
        assert_eq!(second.targets, [TargetRef::Item(1)]);
        assert_eq!(second.spec_counts, [1]);
        let mut thrice = echo.clone();
        thrice.set_slot(crate::state::SLOT_PROMISED_REPEAT, 1);
        thrice.targets = vec![TargetRef::Item(1), TargetRef::Item(2), TargetRef::Item(3)];
        thrice.spec_counts = vec![1, 1, 1];
        assert_eq!(thrice.repeats(), 2, "820.3 · two instances paid");
        for execution in 0..3u8 {
            thrice.execution = execution;
            let view = execution_view(&ctx, &thrice);
            assert_eq!(view.targets, [TargetRef::Item(1 + u16::from(execution))]);
            assert_eq!(view.spec_counts, [1]);
        }
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the first execution counters, the second finds nothing"
        );
        assert!(ctx.blob.log.contains(&"{card 93} repeats".to_string()));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::PlayedSpell { .. }))
                .count(),
            1,
            "820.3.a: PlayedSpell fires once for the repeated spell"
        );
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
    }

    #[test]
    fn a_countered_repeated_spell_resolves_nothing_and_fires_no_played_spell() {
        let mut fixture = echoing();
        fixture.table.cards.push(fixtures::spell(
            THEIR_NEGATE,
            fixtures::HAND,
            0,
            "Negate",
            0,
            0,
        ));
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_NEGATE, &NEGATE);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, ECHO_CARD).unwrap();
        answer(&mut ctx, 1, "yes");
        answer(&mut ctx, 1, "{card 71} on the chain");
        answer(&mut ctx, 1, "{card 71} on the chain");
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        play_from_hand(&mut ctx, 0, THEIR_NEGATE).unwrap();
        answer(&mut ctx, 0, "{card 93} on the chain");
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(ECHO_CARD).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "the countered Echo never ran");
        assert!(!ctx.blob.log.contains(&"{card 93} repeats".to_string()));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::PlayedSpell { item: 2, .. })));
    }

    static BROKEN: Card = crate::cards::prelude::spell(
        "Broken",
        &[],
        &[play_ability(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            ctx.emit(Effect::Move {
                card: 4242,
                zone: fixtures::BASE,
                seat: 0,
                index: TOP,
            });
            ctx.narrate("this line is rolled back");
            Flow::Done
        })],
    );

    const BROKEN_CARD: u32 = 96;

    #[test]
    fn a_script_that_faults_mid_resolution_is_rolled_back_and_fizzles_instead_of_wedging_the_chain()
    {
        let mut fixture = echoing();
        fixture.table.cards.push(fixtures::spell(
            BROKEN_CARD,
            fixtures::HAND,
            0,
            "Broken",
            0,
            0,
        ));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(BROKEN_CARD, &BROKEN);
        let mut ctx = fixture.ctx();
        let hand_before = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, BROKEN_CARD).unwrap();
        let effects_before = ctx.effects.len();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.fault.is_none(),
            "the fault is contained: {:?}",
            ctx.fault
        );
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand_before - 1,
            "the draw before the faulting effect is rolled back with it"
        );
        assert!(
            ctx.effects[effects_before..]
                .iter()
                .all(|effect| !matches!(effect, Effect::Move { card: 4242, .. })),
            "{:?}",
            ctx.effects
        );
        assert_eq!(
            ctx.card(BROKEN_CARD).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell still leaves the chain"
        );
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line == "this line is rolled back"));
        assert!(ctx.blob.log.iter().any(|line| line
            == "{card 96} fizzles · the engine could not apply its effects (UnknownCard)"));
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.table.card(4242),
            None,
            "the shadow table is replayed from the origin"
        );
        assert_eq!(
            ctx.table.card(BROKEN_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
    }

    #[test]
    fn a_script_awaiting_two_faces_stays_resolving_until_the_second_arrives() {
        let mut fixture = echoing();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SEER_CARD).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        let top = ctx.blob.chain.last().expect("the Seer parks");
        assert_eq!(top.status, ItemStatus::Resolving);
        assert_eq!(top.awaiting, [fixtures::THEIR_HAND_CARD, 24]);
        assert_eq!(top.stage, 1);
        assert!(ctx.blob.prompt.is_none(), "a face debt is not a prompt");
        let _ = crate::engine::priority::pass(&mut ctx, 0);
        let _ = crate::engine::priority::pass(&mut ctx, 1);
        let top = ctx
            .blob
            .chain
            .last()
            .expect("passes resolve nothing while it waits");
        assert_eq!(top.status, ItemStatus::Resolving);
        assert_eq!(top.awaiting, [fixtures::THEIR_HAND_CARD, 24]);
        assert_eq!(face_arrived(&mut ctx, 99), Ok(false));
        assert_eq!(face_arrived(&mut ctx, fixtures::THEIR_HAND_CARD), Ok(true));
        let top = ctx.blob.chain.last().unwrap();
        assert_eq!(top.status, ItemStatus::Resolving);
        assert_eq!(top.awaiting, [24]);
        let hand_before = ctx.hand_of(0).len();
        assert_eq!(face_arrived(&mut ctx, 24), Ok(true));
        assert!(
            ctx.blob.chain.is_empty(),
            "the second face resumes the script"
        );
        assert_eq!(ctx.hand_of(0).len(), hand_before + 1);
        assert_eq!(ctx.card(SEER_CARD).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn pay_or_let_resumes_the_script_with_the_payers_answer() {
        let mut fixture = echoing();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, RANSOM_CARD).unwrap();
        answer(&mut ctx, 1, "{card 71} on the chain");
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PayOrLet { item: 2, stage: 1 })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0, "the countered spell's controller is asked");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{seat 0} may pay 2 energy to keep")));
        ctx.blob.close_prompt();
        pay_or_let(&mut ctx, 2, 1, Answer::Yes).unwrap();
        assert!(ctx.blob.log.contains(&"the ransom is paid".to_string()));
        assert_eq!(ctx.blob.chain.len(), 1, "the spell stays");
        let mut declined = echoing();
        let mut ctx = declined.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, RANSOM_CARD).unwrap();
        answer(&mut ctx, 1, "{card 71} on the chain");
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        ctx.blob.close_prompt();
        pay_or_let(&mut ctx, 2, 1, Answer::No).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
    }

    #[test]
    fn a_spell_leaves_the_chain_by_its_origin() {
        let mut fixture = echoing();
        let mut ctx = fixture.ctx();
        ctx.table
            .apply_entry(&fixtures::move_action(RIPOSTE, fixtures::TRASH, 0), 0)
            .unwrap();
        play::begin(
            &mut ctx,
            0,
            RIPOSTE,
            Origin::Trash {
                leave: crate::state::Leave::Recycle,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(RIPOSTE).unwrap().zone, Some(fixtures::CHAIN));
        let prompt = ctx.blob.prompt.clone().expect("a target prompt");
        assert!(!prompt.cancel, "a Recycle-origin play is not taken back");
        answer(&mut ctx, 0, "{card 50}");
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(RIPOSTE).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(ctx.blob.log.contains(&"{card 76} is recycled".to_string()));
        assert!(ctx.banished_of(0).is_empty());
        let mut banished = echoing();
        let mut ctx = banished.ctx();
        ctx.table
            .apply_entry(&fixtures::move_action(RIPOSTE, fixtures::TRASH, 0), 0)
            .unwrap();
        play::begin(
            &mut ctx,
            0,
            RIPOSTE,
            Origin::Trash {
                leave: crate::state::Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        answer(&mut ctx, 0, "{card 50}");
        assert!(counter(&mut ctx, 1, crate::engine::ctx::CounterDest::Hand));
        assert_eq!(
            ctx.banished_of(0),
            [RIPOSTE],
            "829.1.b.1: a Flow spell leaving the chain is banished whatever the counter says"
        );
    }
}

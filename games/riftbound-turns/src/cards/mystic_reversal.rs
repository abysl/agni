use super::prelude::{a_spell, asking, done, item_target, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::targets;
use crate::state::TargetRef;

pub const QUESTION: &str = "a new choice for the spell";

pub fn take_control(ctx: &mut Ctx, item: u16, seat: u8) -> bool {
    let Some(held) = ctx.blob.chain.iter_mut().find(|held| held.id == item) else {
        return false;
    };
    let card = held.kind.source();
    if held.controller == seat {
        ctx.narrate(format!("{{seat {seat}}} already controls {{card {card}}}"));
        return true;
    }
    held.controller = seat;
    ctx.narrate(format!("{{seat {seat}}} gains control of {{card {card}}}"));
    true
}

fn counts_of(ctx: &Ctx, item: u16) -> Vec<u8> {
    let Some(held) = ctx.chain_item(item) else {
        return Vec::new();
    };
    targets::specs_of(ctx, held)
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            held.spec_counts
                .get(index)
                .copied()
                .unwrap_or(spec.max.max(1))
        })
        .collect()
}

fn spec_of(ctx: &Ctx, item: u16, spec_index: usize) -> Option<TargetSpec> {
    let held = ctx.chain_item(item)?;
    targets::specs_of(ctx, held).get(spec_index).copied()
}

pub fn is_group(ctx: &Ctx, item: u16, spec_index: usize) -> bool {
    spec_of(ctx, item, spec_index).is_some_and(|spec| spec.max > 1)
}

pub fn new_choices(ctx: &Ctx, item: u16, spec_index: usize) -> Vec<TargetRef> {
    let Some(held) = ctx.chain_item(item) else {
        return Vec::new();
    };
    let Some(spec) = spec_of(ctx, item, spec_index) else {
        return Vec::new();
    };
    let current = targets::of_spec(ctx, held, spec_index);
    let open = targets::candidates_at(ctx, held, spec_index, &spec);
    if spec.max > 1 {
        let same =
            open.len() == current.len() && open.iter().all(|choice| current.contains(choice));
        return if same { Vec::new() } else { open };
    }
    open.into_iter()
        .filter(|choice| !current.contains(choice))
        .collect()
}

pub fn choice_of(ctx: &Ctx, item: u16, spec_index: usize, pick: u32) -> Option<TargetRef> {
    let spec = spec_of(ctx, item, spec_index)?;
    let choice = targets::from_answer(spec.kind, pick)?;
    new_choices(ctx, item, spec_index)
        .contains(&choice)
        .then_some(choice)
}

pub fn choices_of(ctx: &Ctx, item: u16, spec_index: usize, picks: &[u32]) -> Vec<TargetRef> {
    let (Some(held), Some(spec)) = (ctx.chain_item(item), spec_of(ctx, item, spec_index)) else {
        return Vec::new();
    };
    let mut chosen: Vec<TargetRef> = Vec::new();
    for pick in picks {
        let Some(choice) = targets::from_answer(spec.kind, *pick) else {
            return Vec::new();
        };
        if !targets::candidates_with(ctx, held, spec_index, &spec, &chosen).contains(&choice) {
            return Vec::new();
        }
        chosen.push(choice);
    }
    if chosen.len() < usize::from(spec.min) || chosen.len() > usize::from(spec.max.max(1)) {
        return Vec::new();
    }
    chosen
}

pub fn remake_choice(ctx: &mut Ctx, item: u16, spec_index: usize, choice: TargetRef) -> bool {
    remake_choices(ctx, item, spec_index, &[choice])
}

pub fn remake_choices(ctx: &mut Ctx, item: u16, spec_index: usize, choices: &[TargetRef]) -> bool {
    let mut counts = counts_of(ctx, item);
    if spec_index >= counts.len() {
        return false;
    }
    let offset: usize = counts
        .iter()
        .take(spec_index)
        .map(|count| usize::from(*count))
        .sum();
    let Some(held) = ctx.blob.chain.iter_mut().find(|held| held.id == item) else {
        return false;
    };
    let end = offset.saturating_add(usize::from(counts[spec_index]));
    if end > held.targets.len() {
        return false;
    }
    held.targets.splice(offset..end, choices.iter().copied());
    counts[spec_index] = u8::try_from(choices.len()).unwrap_or(u8::MAX);
    held.spec_counts = counts;
    let (by, card) = (held.controller, held.kind.source());
    for choice in choices {
        match *choice {
            TargetRef::Card(chosen) => {
                ctx.raise(Event::Chosen {
                    card: chosen,
                    by,
                    item,
                });
                ctx.narrate(format!("{{card {card}}} now chooses {{card {chosen}}}"));
            }
            TargetRef::Zone(zone) => {
                ctx.narrate(format!("{{card {card}}} now chooses {{zone {zone}}}"));
            }
            TargetRef::Item(other) => {
                ctx.narrate(format!("{{card {card}}} now chooses item {other}"));
            }
            TargetRef::Seat(seat) => {
                ctx.narrate(format!("{{card {card}}} now chooses {{seat {seat}}}"));
            }
        }
    }
    true
}

fn choices(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let (Some(target), Some(spec_index)) = (
        item_target(ctx, item, 0),
        usize::from(stage.0).checked_sub(1),
    ) else {
        return Vec::new();
    };
    new_choices(ctx, target, spec_index)
}

fn reverse(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(target) = item_target(ctx, item, 0) else {
        return done();
    };
    let next = usize::from(stage.0);
    match next.checked_sub(1) {
        None => {
            if !take_control(ctx, target, item.controller) {
                return done();
            }
        }
        Some(spec_index) if is_group(ctx, target, spec_index) => {
            if !ctx.picks().is_empty() {
                let choices = choices_of(ctx, target, spec_index, ctx.picks());
                if !choices.is_empty() {
                    remake_choices(ctx, target, spec_index, &choices);
                }
            }
        }
        Some(spec_index) => {
            if let Some(choice) = ctx
                .picks()
                .first()
                .and_then(|pick| choice_of(ctx, target, spec_index, *pick))
            {
                remake_choice(ctx, target, spec_index, choice);
            }
        }
    }
    let specs = counts_of(ctx, target).len();
    for spec_index in next..specs {
        if !new_choices(ctx, target, spec_index).is_empty() {
            let stage = u8::try_from(spec_index + 1).unwrap_or(u8::MAX);
            let max = spec_of(ctx, target, spec_index)
                .map(|spec| spec.max.max(1))
                .unwrap_or(1);
            return Flow::Ask(ctx.ask_resume(item, stage, 0, max));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Mystic Reversal",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(
            play(&[a_spell("a spell to gain control of")], reverse),
            choices,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, an_enemy_unit, SPELL_ON_CHAIN};
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::COUNTER_DAMAGE;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const REVERSAL: u32 = 90;
    const HEX: u32 = 91;
    const WINDFALL: u32 = 92;
    const VOLLEY: u32 = 93;
    const SECOND_MINE: u32 = 94;
    const CALM_RUNES: [u32; 3] = [46, 47, 48];

    static HEX_CARD: Card = prelude::spell(
        "Hex",
        &[],
        &[play(&[an_enemy_unit("an enemy unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                prelude::deal(ctx, item, unit, 2);
            }
            Flow::Done
        })],
    );

    static WINDFALL_CARD: Card = prelude::spell(
        "Windfall",
        &[],
        &[play(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 2);
            Flow::Done
        })],
    );

    fn reversal(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(REVERSAL, fixtures::HAND, seat, "Mystic Reversal", 4, 3);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(reversal(1));
        fixture
            .table
            .cards
            .push(fixtures::spell(HEX, fixtures::HAND, 0, "Hex", 1, 0));
        fixture.table.cards.push(fixtures::spell(
            WINDFALL,
            fixtures::HAND,
            0,
            "Windfall",
            1,
            0,
        ));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Calm", false));
        }
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(HEX, &HEX_CARD)
            .with_script(WINDFALL, &WINDFALL_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REVERSAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn damage_on(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn respond_with_reversal(ctx: &mut Ctx, stolen: &str) {
        priority::pass(ctx, 0).unwrap();
        fixtures::play_from_hand(ctx, 1, REVERSAL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(fixtures::labels(ctx), [stolen, "cancel"]);
        fixtures::choose(ctx, 1, stolen).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        priority::pass(ctx, 1).unwrap();
        priority::pass(ctx, 0).unwrap();
    }

    #[test]
    fn the_script_is_a_reaction_over_one_spell_with_a_new_choice_question() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Mystic Reversal").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, SPELL_ON_CHAIN);
        assert_eq!(ability.targets[0].kind, TargetKind::Item);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn it_steals_a_targeted_spell_and_the_new_controller_may_point_it_at_a_now_enemy_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.chain[0].controller, 0);
        respond_with_reversal(&mut ctx, "{card 91} on the chain");
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the Reversal waits on its question"
        );
        assert_eq!(ctx.blob.chain[1].status, ItemStatus::Resolving);
        assert_eq!(ctx.blob.chain[0].controller, 1, "Hex changed hands first");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} gains control of {card 91}".to_string()));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "skip"],
            "751.1: Vi is the one enemy unit Hex did not already choose; Jinx is now friendly to it"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item: 2, stage: 1 }),
            "{card 90}: choose a new choice for the spell (0 of 1)"
        );
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Reversal is done");
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.blob.chain[0].spec_counts, [1]);
        assert!(
            ctx.events.contains(&Event::Chosen {
                card: fixtures::VI,
                by: 1,
                item: 1
            }),
            "754: the new target is chosen now"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 91} now chooses {card 50}".to_string()));
        assert_eq!(
            priority::holder(&ctx),
            Some(1),
            "the spell's new controller holds priority over it"
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage_on(&ctx, fixtures::VI), 2, "Hex hits its new target");
        assert_eq!(damage_on(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(ctx.blob.log.contains(&"{card 91} resolves".to_string()));
        assert_eq!(ctx.card(HEX).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(REVERSAL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn keeping_the_old_choice_lets_the_stolen_spell_mistarget_its_now_friendly_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        respond_with_reversal(&mut ctx, "{card 91} on the chain");
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)],
            "753: no new choice was made"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Chosen { by: 1, .. })));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            damage_on(&ctx, fixtures::THEIR_UNIT),
            0,
            "359.3.f.4: enemy is read against the new controller, so Hex does nothing"
        );
        assert_eq!(damage_on(&ctx, fixtures::VI), 0);
        assert!(ctx.blob.log.contains(&"{card 91} resolves".to_string()));
    }

    static VOLLEY_CARD: Card = prelude::spell(
        "Volley",
        &[],
        &[play(
            &[prelude::target(
                prelude::ENEMY_UNIT,
                0,
                2,
                TargetKind::Card,
                "up to two enemy units",
            )],
            |ctx, item, _| {
                for unit in prelude::card_targets(ctx, item) {
                    prelude::deal(ctx, item, unit, 1);
                }
                Flow::Done
            },
        )],
    );

    #[test]
    fn a_stolen_group_spell_has_its_whole_group_chosen_again() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::spell(VOLLEY, fixtures::HAND, 0, "Volley", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND_MINE, fixtures::BASE, 0, "Runt", 2));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(VOLLEY, &VOLLEY_CARD);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VOLLEY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [2]);
        respond_with_reversal(&mut ctx, "{card 93} on the chain");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.seat, prompt.min, prompt.max),
            (1, 0, 2),
            "752.1 · the whole group is chosen again, up to the spec's max"
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            ["done", "skip", "{card 50}", "{card 94}"],
            "seat 1's enemies now · its own units are off the list"
        );
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 1, "{card 94}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Reversal is done");
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(SECOND_MINE)]
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [2]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: SECOND_MINE,
            by: 1,
            item: 1
        }));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage_on(&ctx, fixtures::VI), 1);
        assert_eq!(damage_on(&ctx, SECOND_MINE), 1);
        assert_eq!(damage_on(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut kept = armed();
        kept.table
            .cards
            .push(fixtures::spell(VOLLEY, fixtures::HAND, 0, "Volley", 1, 0));
        kept.resolve();
        kept.scripts = kept.scripts.clone().with_script(VOLLEY, &VOLLEY_CARD);
        let mut ctx = kept.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VOLLEY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        respond_with_reversal(&mut ctx, "{card 93} on the chain");
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)],
            "skipping keeps the old group"
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [1]);
    }

    #[test]
    fn a_spell_without_choices_simply_resolves_for_its_new_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WINDFALL).unwrap();
        respond_with_reversal(&mut ctx, "{card 92} on the chain");
        assert!(ctx.blob.prompt.is_none(), "nothing to choose anew");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].controller, 1);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 1), 2, "you means the new controller");
        assert_eq!(drew(&ctx, 0), 0);
    }

    #[test]
    fn only_spells_are_offered_and_the_reversal_never_takes_itself() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let mut trigger = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        trigger.status = ItemStatus::Finalized;
        ctx.blob.chain.push(trigger);
        let reversal = ChainItem::new(3, ItemKind::Spell { card: REVERSAL }, 1, Origin::Hand);
        let spec = &CARD.abilities[0].targets[0];
        assert_eq!(
            targets::candidates(&ctx, &reversal, spec),
            Vec::<TargetRef>::new(),
            "an ability is not a spell"
        );
        ctx.blob.chain.clear();
        fixtures::play_from_hand(&mut ctx, 0, WINDFALL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, REVERSAL).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[2]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(!take_control(&mut ctx, 99, 1), "no such item");
        assert_eq!(new_choices(&ctx, 1, 0), Vec::<TargetRef>::new());
        assert!(!remake_choice(
            &mut ctx,
            1,
            0,
            TargetRef::Card(fixtures::VI)
        ));
    }

    #[test]
    #[ignore = "engine gap · chain::leave sends a resolved spell to its controller's trash; 157 and 359.3.d say the owner's"]
    fn a_stolen_spell_resolves_into_its_owners_trash() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WINDFALL).unwrap();
        respond_with_reversal(&mut ctx, "{card 92} on the chain");
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.effects.contains(&Effect::Move {
            card: WINDFALL,
            zone: fixtures::TRASH,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.trash_of(0).contains(&WINDFALL));
        assert!(!ctx.trash_of(1).contains(&WINDFALL));
    }
}

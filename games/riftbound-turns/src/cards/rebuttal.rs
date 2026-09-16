use super::mystic_reversal::{
    choice_of, choices_of, is_group, new_choices, remake_choice, remake_choices, take_control,
    QUESTION,
};
use super::prelude::{
    an_item, asking, counter_spell, done, item_target, play, spell, with_candidates, with_cost,
    RAINBOW,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::{chain, cost, targets};
use crate::state::TargetRef;

pub const ENERGY_LIMIT: u8 = 4;
pub const RANSOM: Cost = RAINBOW;
pub const A_SPELL_COSTING_FOUR_OR_LESS: Filter =
    Filter::And(&[Filter::Spell, Filter::EnergyAtMost(ENERGY_LIMIT)]);
pub const ANSWERED: u8 = 1;
pub const FIRST_CHOICE: u8 = 2;

fn spec_index_of(stage: Stage) -> Option<usize> {
    stage.0.checked_sub(FIRST_CHOICE).map(usize::from)
}

fn stage_of(spec_index: usize) -> u8 {
    u8::try_from(spec_index)
        .unwrap_or(u8::MAX - FIRST_CHOICE)
        .saturating_add(FIRST_CHOICE)
}

fn specs_of(ctx: &Ctx, target: u16) -> Vec<super::TargetSpec> {
    ctx.chain_item(target)
        .map(|held| targets::specs_of(ctx, held))
        .unwrap_or_default()
}

fn choices(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let (Some(target), Some(spec_index)) = (item_target(ctx, item, 0), spec_index_of(stage)) else {
        return Vec::new();
    };
    new_choices(ctx, target, spec_index)
}

fn ask_next_choice(ctx: &mut Ctx, item: &Item, target: u16, from: usize) -> Flow {
    let specs = specs_of(ctx, target);
    for (spec_index, spec) in specs.iter().enumerate().skip(from) {
        if !new_choices(ctx, target, spec_index).is_empty() {
            return Flow::Ask(ctx.ask_resume(item, stage_of(spec_index), 0, spec.max.max(1)));
        }
    }
    done()
}

fn rebut(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(target) = item_target(ctx, item, 0) else {
        return done();
    };
    let seat = item.controller;
    if stage.0 == 0 {
        let ransom = cost::of_script(&RANSOM, &[]);
        let card = ctx
            .chain_item(target)
            .map(|held| held.kind.source())
            .unwrap_or(0);
        ctx.narrate(format!(
            "{{seat {seat}}} may pay {} to gain control of {{card {card}}} · otherwise it is countered",
            ransom.label()
        ));
        return Flow::Ask(ctx.ask_pay_or_let(item, seat, &ransom, ANSWERED));
    }
    if stage.0 == ANSWERED {
        if !chain::paid(ctx) {
            counter_spell(ctx, item, 0);
            return done();
        }
        if !take_control(ctx, target, seat) {
            return done();
        }
        return ask_next_choice(ctx, item, target, 0);
    }
    let Some(spec_index) = spec_index_of(stage) else {
        return done();
    };
    if is_group(ctx, target, spec_index) {
        if !ctx.picks().is_empty() {
            let choices = choices_of(ctx, target, spec_index, ctx.picks());
            if !choices.is_empty() {
                remake_choices(ctx, target, spec_index, &choices);
            }
        }
    } else if let Some(choice) = ctx
        .picks()
        .first()
        .and_then(|pick| choice_of(ctx, target, spec_index, *pick))
    {
        remake_choice(ctx, target, spec_index, choice);
    }
    ask_next_choice(ctx, item, target, spec_index + 1)
}

pub static CARD: Card = spell(
    "Rebuttal",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(
            with_cost(
                play(
                    &[an_item(
                        A_SPELL_COSTING_FOUR_OR_LESS,
                        "a spell with energy cost 4 or less",
                    )],
                    rebut,
                ),
                RANSOM,
            ),
            choices,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, an_enemy_unit};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const REBUTTAL: u32 = 90;
    const HEX: u32 = 91;
    const WINDFALL: u32 = 92;
    const THEIR_RUNES: [u32; 3] = [46, 47, 48];

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

    fn rebuttal() -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into(), "Chaos".into()],
            ..fixtures::spell(REBUTTAL, fixtures::HAND, 1, "Rebuttal", 1, 1)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rebuttal());
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
        for rune in THEIR_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Chaos", false));
        }
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(HEX, &HEX_CARD)
            .with_script(WINDFALL, &WINDFALL_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REBUTTAL).unwrap(),
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

    fn respond_with_rebuttal(ctx: &mut Ctx, spell: u32) {
        let chosen = format!("{{card {spell}}} on the chain");
        priority::pass(ctx, 0).unwrap();
        fixtures::play_from_hand(ctx, 1, REBUTTAL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(fixtures::labels(ctx), [chosen.as_str(), "cancel"]);
        fixtures::choose(ctx, 1, &chosen).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        priority::pass(ctx, 1).unwrap();
        priority::pass(ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PayOrLet {
                item: 2,
                stage: ANSWERED
            }),
            "the Rebuttal asks its own controller for the rainbow"
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} may pay 1 any power to gain control of {{card {spell}}} · otherwise it is countered"
        )));
    }

    #[test]
    fn the_script_is_a_reaction_over_a_cheap_spell_with_a_rainbow_ransom_and_a_new_choice_question()
    {
        assert!(std::ptr::eq(script_of("Rebuttal").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.cost, Some(RANSOM), "the ransom, not a play cost");
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, A_SPELL_COSTING_FOUR_OR_LESS);
        assert_eq!(ability.targets[0].kind, TargetKind::Item);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(ENERGY_LIMIT, 4);
        assert_eq!(stage_of(0), FIRST_CHOICE);
        assert_eq!(spec_index_of(Stage(FIRST_CHOICE)), Some(0));
        assert_eq!(spec_index_of(Stage(ANSWERED)), None);
    }

    #[test]
    fn paying_the_rainbow_steals_the_spell_and_the_new_controller_may_re_aim_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.chain[0].controller, 0);
        respond_with_rebuttal(&mut ctx, HEX);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        let pool = ctx.runes_of(1).len();
        fixtures::choose(&mut ctx, 1, "yes").unwrap();
        assert_eq!(
            ctx.runes_of(1).len(),
            pool - 1,
            "one rune recycles for the rainbow"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} pays 1 any power".to_string()));
        assert_eq!(ctx.blob.chain[0].controller, 1, "Hex changed hands");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} gains control of {card 91}".to_string()));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: FIRST_CHOICE
            })
        );
        assert_eq!(ctx.blob.chain[1].status, ItemStatus::Resolving);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "skip"],
            "Vi is the one enemy unit Hex did not already choose; Jinx is now friendly to it"
        );
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Rebuttal is done");
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::VI,
            by: 1,
            item: 1
        }));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage_on(&ctx, fixtures::VI), 2, "Hex hits its new target");
        assert_eq!(damage_on(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(!ctx.blob.log.contains(&"{card 91} is countered".to_string()));
        assert_eq!(ctx.card(REBUTTAL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_rainbow_counters_the_spell_and_a_targetless_steal_needs_no_question() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        respond_with_rebuttal(&mut ctx, HEX);
        let pool = ctx.runes_of(1).len();
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        assert_eq!(ctx.runes_of(1).len(), pool, "nothing paid");
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.log.contains(&"{card 91} is countered".to_string()));
        assert_eq!(ctx.card(HEX).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(damage_on(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WINDFALL).unwrap();
        respond_with_rebuttal(&mut ctx, WINDFALL);
        fixtures::choose(&mut ctx, 1, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "no choices to remake");
        assert_eq!(ctx.blob.chain[0].controller, 1);
        assert!(ctx.blob.prompt.is_none());
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(drew(&ctx, 1), 2, "the thief draws");
        assert_eq!(drew(&ctx, 0), 0);
    }

    #[test]
    fn without_a_rune_to_pay_only_no_is_offered_and_the_spell_is_countered() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![44, 45, THEIR_RUNES[1], THEIR_RUNES[2]].contains(&card.id));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HEX, &HEX_CARD);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, REBUTTAL).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 91} on the chain").unwrap();
        assert!(
            ctx.runes_of(1).is_empty(),
            "the one Chaos rune paid the Rebuttal's energy and recycled for its power"
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["no"],
            "the rainbow is out of reach"
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "no rune left to recycle: the lone 'no' answers itself"
        );
        assert!(ctx.blob.log.contains(&"{card 91} is countered".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_over_four_energy_and_itself_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table.card_mut(HEX).unwrap().energy = Some(5);
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, REBUTTAL).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "a five-energy spell is out of reach"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[1]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[2]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "never itself"
        );
        fixtures::choose(&mut ctx, 1, "cancel").unwrap();
        assert_eq!(ctx.card(REBUTTAL).unwrap().zone, Some(fixtures::HAND));
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move {
                    zone: fixtures::RUNE_DECK,
                    seat: 1,
                    ..
                }
            )),
            "a cancelled Rebuttal paid nothing"
        );
    }
}

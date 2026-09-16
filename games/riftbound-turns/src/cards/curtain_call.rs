use super::prelude::{
    asking, deal, done, draw, might_this_turn, play, spell, units_at_battlefields, units_on_board,
    with_candidates, Location, RAINBOW,
};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const REPEAT: Cost = Cost {
    energy: 1,
    power: &[],
};
pub const SECOND_REPEAT: Cost = RAINBOW;
pub const THIRD_REPEAT: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};
pub const REPEAT_STEPS: [Cost; 3] = [REPEAT, SECOND_REPEAT, THIRD_REPEAT];
pub const DRAWS: usize = 1;
pub const DAMAGE_AT_A_BATTLEFIELD: u8 = 2;
pub const DAMAGE_AT_A_BASE: u8 = 3;
pub const WEAKEN: i16 = -4;
pub const QUESTION: &str = "a mode · Draw 1 (your hand), Deal 2 (a unit at a battlefield), Deal 3 (a unit at a base) or -4 Might (a unit at a battlefield) · skip walks to the next mode";

const OFFERING_BITS: u8 = 3;
const OFFERING_MASK: u8 = (1 << OFFERING_BITS) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Draw,
    DealAtABattlefield,
    DealAtABase,
    Weaken,
}

pub const MODES: [Mode; 4] = [
    Mode::Draw,
    Mode::DealAtABattlefield,
    Mode::DealAtABase,
    Mode::Weaken,
];

impl Mode {
    fn index(self) -> usize {
        MODES.iter().position(|mode| *mode == self).unwrap_or(0)
    }

    fn bit(self) -> u8 {
        1 << self.index()
    }

    fn label(self) -> &'static str {
        match self {
            Mode::Draw => "Draw 1 · your hand",
            Mode::DealAtABattlefield => "Deal 2 to a unit at a battlefield",
            Mode::DealAtABase => "Deal 3 to a unit at a base",
            Mode::Weaken => "give a unit at a battlefield -4 Might this turn",
        }
    }
}

pub fn executions_paid(item: &Item) -> usize {
    if item.repeated() {
        2
    } else {
        1
    }
}

pub fn stage_of(chosen: u8, offering: Option<Mode>) -> Stage {
    let offering = offering.map(|mode| mode.index() as u8 + 1).unwrap_or(0);
    Stage((chosen << OFFERING_BITS) | offering)
}

pub fn decode(stage: Stage) -> (u8, Option<Mode>) {
    let chosen = stage.0 >> OFFERING_BITS;
    let offering = usize::from(stage.0 & OFFERING_MASK)
        .checked_sub(1)
        .and_then(|index| MODES.get(index).copied());
    (chosen, offering)
}

pub fn units_in_bases(ctx: &Ctx) -> Vec<u32> {
    units_on_board(ctx)
        .into_iter()
        .filter(|unit| matches!(ctx.location(*unit), Some(Location::Base(_))))
        .collect()
}

pub fn candidates_of(ctx: &Ctx, mode: Mode) -> Vec<TargetRef> {
    let mut units = match mode {
        Mode::Draw => return ctx.zones.hand.map(TargetRef::Zone).into_iter().collect(),
        Mode::DealAtABattlefield | Mode::Weaken => units_at_battlefields(ctx),
        Mode::DealAtABase => units_in_bases(ctx),
    };
    units.sort_unstable();
    units.into_iter().map(TargetRef::Card).collect()
}

fn offered(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    match decode(stage) {
        (_, Some(mode)) => candidates_of(ctx, mode),
        _ => Vec::new(),
    }
}

fn picked(ctx: &Ctx, mode: Mode, pick: u32) -> Option<TargetRef> {
    let target = match mode {
        Mode::Draw => TargetRef::Zone(u16::try_from(pick).ok()?),
        _ => TargetRef::Card(pick),
    };
    candidates_of(ctx, mode).contains(&target).then_some(target)
}

fn perform(ctx: &mut Ctx, item: &Item, mode: Mode, pick: u32) -> bool {
    let Some(target) = picked(ctx, mode, pick) else {
        return false;
    };
    let seat = item.controller;
    match (mode, target) {
        (Mode::Draw, _) => {
            ctx.narrate(format!("{{seat {seat}}} draws {DRAWS}"));
            draw(ctx, seat, DRAWS);
        }
        (Mode::DealAtABattlefield, TargetRef::Card(unit)) => {
            if deal(ctx, item, unit, DAMAGE_AT_A_BATTLEFIELD) {
                ctx.narrate(format!("{{card {unit}}} takes {DAMAGE_AT_A_BATTLEFIELD}"));
            }
        }
        (Mode::DealAtABase, TargetRef::Card(unit)) => {
            if deal(ctx, item, unit, DAMAGE_AT_A_BASE) {
                ctx.narrate(format!("{{card {unit}}} takes {DAMAGE_AT_A_BASE}"));
            }
        }
        (Mode::Weaken, TargetRef::Card(unit)) => {
            might_this_turn(ctx, item, unit, WEAKEN, None);
            ctx.narrate(format!("{{card {unit}}} gets {WEAKEN} Might this turn"));
        }
        _ => return false,
    }
    true
}

fn available_from(ctx: &Ctx, chosen: u8, from: usize) -> Vec<Mode> {
    MODES
        .iter()
        .copied()
        .skip(from)
        .filter(|mode| chosen & mode.bit() == 0 && !candidates_of(ctx, *mode).is_empty())
        .collect()
}

fn offer_next(ctx: &mut Ctx, item: &Item, chosen: u8, from: usize) -> Flow {
    let me = item.kind.source();
    let available = available_from(ctx, chosen, from);
    let Some(mode) = available.first().copied() else {
        ctx.narrate(format!("{{card {me}}} has no mode left to choose"));
        return done();
    };
    let forced = available.len() == 1;
    let suffix = if forced { "" } else { " · skip for the next" };
    ctx.narrate(format!("{{card {me}}} offers: {}{suffix}", mode.label()));
    let min = u8::from(forced);
    Flow::Ask(ctx.ask_resume(item, stage_of(chosen, Some(mode)).0, min, 1))
}

fn curtain(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if item.execution > 0 {
        return done();
    }
    let (mut chosen, offering) = decode(stage);
    let mut from = 0;
    if let Some(mode) = offering {
        match ctx.picks().first().copied() {
            Some(pick) if perform(ctx, item, mode, pick) => chosen |= mode.bit(),
            _ => from = mode.index() + 1,
        }
    }
    let performed = MODES.iter().filter(|mode| chosen & mode.bit() != 0).count();
    if performed >= executions_paid(item) {
        return done();
    }
    offer_next(ctx, item, chosen, from)
}

pub static CARD: Card = spell(
    "Curtain Call",
    &[Keyword::Repeat(REPEAT)],
    &[asking(
        with_candidates(play(&[], curtain), offered),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::{legal, priority, prompts};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const CURTAIN: u32 = 90;
    const THEIR_CURTAIN: u32 = 91;
    const BRUTE: u32 = 92;
    const MY_EXTRA: [u32; 2] = [100, 101];

    fn curtain_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Curtain Call", 4, 0);
        card.domain = vec!["Fury".into(), "Mind".into()];
        card
    }

    fn stage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(curtain_card(CURTAIN, 0));
        fixture.table.cards.push(curtain_card(THEIR_CURTAIN, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx, repeat: &str) {
        fixtures::play_from_hand(ctx, 0, CURTAIN).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            }),
            "the engine offers the first Repeat step"
        );
        fixtures::choose(ctx, 0, repeat).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no targets: the modes come on resolution"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn resume_stage(ctx: &Ctx) -> Option<Stage> {
        match ctx.blob.why {
            Some(PromptWhy::Resume { item: 1, stage }) => Some(Stage(stage)),
            _ => None,
        }
    }

    #[test]
    fn the_script_is_a_repeat_spell_whose_modes_are_walked_at_resolution_and_the_stage_carries_the_choices(
    ) {
        assert!(std::ptr::eq(script_of("Curtain Call").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(REPEAT_STEPS[0], REPEAT);
        assert_eq!(REPEAT_STEPS[1].energy, 0);
        assert_eq!(REPEAT_STEPS[1].power, &[Power::Rainbow]);
        assert_eq!(REPEAT_STEPS[2].energy, 1);
        assert_eq!(REPEAT_STEPS[2].power, &[Power::Rainbow]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        for mode in MODES {
            for chosen in 0..16u8 {
                assert_eq!(decode(stage_of(chosen, Some(mode))), (chosen, Some(mode)));
            }
        }
        assert_eq!(decode(stage_of(5, None)), (5, None));
        assert_eq!(decode(Stage(0)), (0, None));
        let fresh = Item::new(7, ItemKind::Spell { card: CURTAIN }, 0, Origin::Hand);
        assert_eq!(executions_paid(&fresh), 1);
        let mut fixture = stage();
        let ctx = fixture.ctx();
        assert_eq!(
            candidates_of(&ctx, Mode::Draw),
            [TargetRef::Zone(fixtures::HAND)]
        );
        assert_eq!(
            candidates_of(&ctx, Mode::DealAtABattlefield),
            [TargetRef::Card(fixtures::SPRITE), TargetRef::Card(BRUTE)]
        );
        assert_eq!(
            candidates_of(&ctx, Mode::Weaken),
            candidates_of(&ctx, Mode::DealAtABattlefield)
        );
        assert_eq!(
            candidates_of(&ctx, Mode::DealAtABase),
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
    }

    #[test]
    fn unrepeated_it_walks_the_modes_in_printed_order_and_one_pick_ends_it() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx, "no");
        assert_eq!(resume_stage(&ctx), Some(stage_of(0, Some(Mode::Draw))));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 0}", "skip"],
            "Draw 1 is the hand, or skip to the next mode"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {CURTAIN}}}: choose {QUESTION} (0 of 1)")
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} offers: Draw 1 · your hand · skip for the next".to_string()));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(0, Some(Mode::DealAtABattlefield)))
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 92}", "skip"]);
        assert!(ctx.blob.log.contains(
            &"{card 90} offers: Deal 2 to a unit at a battlefield · skip for the next".to_string()
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one mode, one execution");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE_AT_A_BATTLEFIELD,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "nothing drawn");
        assert!(ctx.blob.log.contains(&"{card 92} takes 2".to_string()));
        assert_eq!(ctx.card(CURTAIN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_one_energy_a_second_mode_is_walked_without_the_first_and_the_last_left_is_forced(
    ) {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx, "yes");
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(executions_paid(&ctx.blob.chain[0]), 2);
        assert_eq!(resume_stage(&ctx), Some(stage_of(0, Some(Mode::Draw))));
        fixtures::choose(&mut ctx, 0, "{zone 0}").unwrap();
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the spell left, one card came in"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        let chosen = Mode::Draw.bit();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(chosen, Some(Mode::DealAtABattlefield))),
            "the second walk starts past the mode already chosen"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(chosen, Some(Mode::DealAtABase)))
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "{card 81}", "skip"]);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(chosen, Some(Mode::Weaken)))
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.min, prompt.max),
            (1, 1),
            "the last mode left is forced"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}"],
            "no skip on the last mode"
        );
        assert!(ctx.blob.log.contains(
            &"{card 90} offers: give a unit at a battlefield -4 Might this turn".to_string()
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BRUTE), 1, "5 - 4");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets -4 Might this turn".to_string()));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "four energy and the one-energy Repeat exhausted five of the six"
        );
        assert_eq!(ctx.card(CURTAIN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_same_unit_may_take_both_battlefield_modes_and_a_bare_board_leaves_nothing_to_choose() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, "yes");
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(Mode::DealAtABattlefield.bit(), Some(Mode::Draw)))
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            resume_stage(&ctx),
            Some(stage_of(
                Mode::DealAtABattlefield.bit(),
                Some(Mode::DealAtABase)
            )),
            "Deal 2 is spent, so the walk goes on to Deal 3"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.damage_on(BRUTE), 2);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(BRUTE),
            "two damage and -4 Might on a 5-Might Brute is lethal in the cleanup"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets -4 Might this turn".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut bare = stage();
        bare.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT, BRUTE].contains(&card.id)
        });
        bare.resolve();
        let mut ctx = bare.ctx();
        cast(&mut ctx, "yes");
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.min, prompt.max),
            (1, 1),
            "Draw 1 is the only mode with a choice, so it is forced"
        );
        assert_eq!(fixtures::labels(&ctx), ["{zone 0}"]);
        fixtures::choose(&mut ctx, 0, "{zone 0}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} has no mode left to choose".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_answer_the_walk_and_cannot_play_it_off_turn() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CURTAIN)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx, "no");
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 9 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 9,
                count: 2
            }))
        );
        assert_eq!(priority::pass(&mut ctx, 0), Err(Refusal::PromptOpen));
        assert_eq!(ctx.hand_of(0).len(), 4, "nothing drawn yet");
        assert!(ctx.blob.chain.len() == 1);
    }

    #[test]
    #[ignore = "engine gap · escalating Repeat steps · play::advance prices one Repeat from the printed keyword, so only the 1-energy step is offered; 820.1.c.2 and 820.3 want the rainbow and the 1-plus-rainbow steps offered separately for a third and fourth execution, with the modes chosen at play time (820.2) rather than walked at resolution; executions_paid reads the one slot until then"]
    fn all_three_steps_paid_give_four_executions_and_four_distinct_modes() {
        let mut fixture = stage();
        for rune in [102, 103] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CURTAIN).unwrap();
        for _ in REPEAT_STEPS {
            assert!(matches!(
                ctx.blob.why,
                Some(PromptWhy::OptionalCost { item: 1, .. })
            ));
            fixtures::choose(&mut ctx, 0, "yes").unwrap();
        }
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(executions_paid(&ctx.blob.chain[0]), 4);
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, "{zone 0}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.current_might(BRUTE), 1);
    }
}

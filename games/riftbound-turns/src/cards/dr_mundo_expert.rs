use super::prelude::{asking, done, triggered, unit, with_candidates, with_statics};
use super::{Card, Flow, Grant, Item, Stage, Static, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const LADDER: usize = 48;
pub const RECYCLES: usize = 3;
pub const QUESTION: &str = "three cards from your trash to recycle";
const PICK: u8 = 1;

pub fn cards_in_my_trash(ctx: &Ctx, card: u32) -> usize {
    ctx.trash_of(ctx.controller(card)).len()
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(cards_in_my_trash(ctx, card)).unwrap_or(i16::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32, _: u32) -> bool {
    cards_in_my_trash(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static TRASH: &[Grant] = ladder!(
    1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24
    25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48
);

fn trash(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    ctx.trash_of(item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn recycle_three(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != PICK {
        let count = ctx.trash_of(seat).len().min(RECYCLES);
        if count == 0 {
            ctx.narrate(format!(
                "{{seat {seat}}} has nothing in their trash to recycle"
            ));
            return done();
        }
        let count = u8::try_from(count).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_resume(item, PICK, count, count));
    }
    let offered = trash(ctx, item, stage);
    let picked: Vec<u32> = ctx
        .picks()
        .iter()
        .copied()
        .filter(|card| offered.contains(&TargetRef::Card(*card)))
        .take(RECYCLES)
        .collect();
    for card in &picked {
        ctx.recycle_to_bottom(*card);
    }
    if !picked.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", picked.len()));
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Dr. Mundo - Expert",
        &[],
        &[asking(
            with_candidates(
                triggered(Trigger::BeginningPhase, &[], recycle_three),
                trash,
            ),
            QUESTION,
        )],
    ),
    &[Static::While(always, TRASH)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority, prompts, statics};
    use crate::state::{ItemKind, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const MUNDO: u32 = 90;
    const TRASHED: [u32; 5] = [100, 101, 102, 103, 104];

    fn expert(trashed: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            MUNDO,
            fixtures::BASE,
            0,
            "Dr. Mundo - Expert",
            6,
        ));
        for id in TRASHED.iter().take(trashed) {
            fixture
                .table
                .cards
                .push(fixtures::spell(*id, fixtures::TRASH, 0, "Spent", 1, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::spell(110, fixtures::TRASH, 1, "Theirs", 1, 0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MUNDO).unwrap(), &CARD));
        fixture
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn mundo_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == MUNDO))
            .count()
    }

    #[test]
    fn the_script_is_a_trash_counting_while_and_a_beginning_phase_recycle_that_picks_at_resolution()
    {
        assert!(std::ptr::eq(
            script_of("Dr. Mundo - Expert").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(TRASH.len(), LADDER);
        assert!(TRASH
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(
            ability.targets.is_empty(),
            "416.6 · a recycle X targets nothing"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(!ability.optional);
    }

    #[test]
    fn his_might_is_six_plus_the_cards_in_his_controllers_trash() {
        let mut fixture = expert(5);
        let mut ctx = fixture.ctx();
        assert_eq!(might_bonus(&ctx, MUNDO), 5);
        assert_eq!(statics::grants_on(&ctx, MUNDO).len(), 5);
        assert_eq!(
            ctx.current_might(MUNDO),
            11,
            "your trash · the opponent's card is not counted"
        );
        ctx.recycle_to_bottom(TRASHED[0]);
        assert_eq!(ctx.current_might(MUNDO), 10);
        ctx.trash(fixtures::HAND_SPELL);
        assert_eq!(ctx.current_might(MUNDO), 11);
        assert!(ctx.set_controller(MUNDO, 1, fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.current_might(MUNDO),
            7,
            "a control change reads the new controller's trash"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_the_start_of_his_controllers_beginning_phase_three_chosen_cards_go_to_the_bottom_of_the_deck(
    ) {
        let mut fixture = expert(5);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(mundo_items(&ctx), 1);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing is chosen while the trigger goes on the chain"
        );
        resolve_the_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 3, 3));
        assert!(!prompt.cancel);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {MUNDO}}}: choose {QUESTION} (0 of 3)")
        );
        let offered = fixtures::labels(&ctx);
        assert_eq!(offered.len(), 5, "only his controller's trash is offered");
        assert!(!offered.contains(&"{card 110}".to_string()));
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        for id in [TRASHED[4], TRASHED[1], TRASHED[2]] {
            fixtures::choose(&mut ctx, 0, &format!("{{card {id}}}")).unwrap();
        }
        assert!(
            ctx.blob.prompt.is_none(),
            "the third pick closes the prompt"
        );
        for id in [TRASHED[4], TRASHED[1], TRASHED[2]] {
            assert!(ctx.effects.contains(&Effect::Move {
                card: id,
                zone: fixtures::MAIN_DECK,
                seat: 0,
                index: BOTTOM
            }));
        }
        assert_eq!(ctx.trash_of(0), [TRASHED[0], TRASHED[3]]);
        assert_eq!(
            ctx.current_might(MUNDO),
            8,
            "the Might follows the trash down"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 3".to_string()));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_in_the_trash_he_recycles_both_and_with_none_he_says_so_and_asks_nothing() {
        let mut fixture = expert(2);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.min, prompt.max),
            (2, 2),
            "416.4 · as many as possible"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", TRASHED[0])).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the last card is the only option left, so settle picks it"
        );
        assert!(ctx.trash_of(0).is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 2".to_string()));
        assert_eq!(ctx.current_might(MUNDO), 6);
        drop(ctx);

        let mut fixture = expert(0);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(mundo_items(&ctx), 1, "the trigger still fires");
        resolve_the_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing in their trash to recycle".to_string()));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }

    #[test]
    fn the_opponents_beginning_phase_is_not_his() {
        let mut fixture = expert(3);
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(mundo_items(&ctx), 0);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.trash_of(0).len(), 3);
    }
}

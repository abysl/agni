use super::prelude::{
    asking, battlefield, done, draw, kill, location_of, triggered, when, with_candidates, Location,
};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event, Killed};
use crate::state::TargetRef;

const DRAWS: usize = 1;
const PICK: u8 = 1;

fn holder_here(ctx: &Ctx, card: u32) -> Option<u8> {
    match location_of(ctx, card) {
        Some(Location::Battlefield(zone)) => ctx.blob.holder(zone),
        _ => None,
    }
}

fn the_lab_is_controlled(ctx: &Ctx, _: &Event, source: Source) -> bool {
    holder_here(ctx, source.card).is_some()
}

fn units_here(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(at) = location_of(ctx, item.kind.source()) else {
        return Vec::new();
    };
    if !matches!(at, Location::Battlefield(_)) {
        return Vec::new();
    }
    ctx.units_at(at)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == item.controller)
        .map(TargetRef::Card)
        .collect()
}

fn picked_here(ctx: &Ctx, item: &Item) -> Option<u32> {
    let unit = *ctx.picks().first()?;
    units_here(ctx, item, Stage(PICK))
        .contains(&TargetRef::Card(unit))
        .then_some(unit)
}

fn kill_a_unit_here_to_draw(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 != PICK {
        if units_here(ctx, item, stage).is_empty() {
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, PICK, 0, 1));
    }
    let Some(unit) = picked_here(ctx, item) else {
        return done();
    };
    if kill(ctx, item, unit) == Killed::Yes {
        ctx.narrate(format!("{{card {unit}}} is killed for a card"));
        draw(ctx, item.controller, DRAWS);
    }
    done()
}

pub static CARD: Card = battlefield(
    "Dusk Rose Lab",
    &[],
    &[when(
        asking(
            with_candidates(
                triggered(Trigger::BeginningPhase, &[], kill_a_unit_here_to_draw),
                units_here,
            ),
            "a unit you control here to kill",
        ),
        the_lab_is_controlled,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority, prompts, resume, settle};
    use crate::rules::CARDS_PER_TURN;
    use crate::state::{ItemKind, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const LAB: u32 = fixtures::GROUNDS;
    const SPRITE: u32 = fixtures::SPRITE;

    fn dusk_rose_lab() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LAB).unwrap().name = "Dusk Rose Lab".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(LAB).unwrap(), &CARD));
        fixture
    }

    fn with_a_friendly_sprite() -> Fixture {
        let mut fixture = dusk_rose_lab();
        let sprite = fixture.table.card_mut(SPRITE).unwrap();
        sprite.owner = 0;
        sprite.seat = 0;
        sprite.zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        fixture
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) {
        let prompt = ctx.blob.prompt.as_ref().expect("a prompt is open").id;
        let answered = prompts::answer(ctx, seat, Pick { prompt, option }).unwrap();
        if let Some(answered) = answered {
            resume(ctx, &answered).unwrap();
        }
        settle(ctx).unwrap();
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn lab_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == LAB))
            .count()
    }

    fn trashed(ctx: &Ctx, card: u32) -> bool {
        ctx.card(card)
            .is_none_or(|held| held.zone == Some(fixtures::TRASH))
    }

    #[test]
    fn the_lab_is_a_beginning_phase_trigger_that_targets_nothing() {
        assert_eq!(CARD.name, "Dusk Rose Lab");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::BeginningPhase);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some());
        assert!(ability.timing().is_none());
        assert!(
            ability.targets.is_empty(),
            "352.10.c.1 · '[kill a unit] to [draw]' targets nothing"
        );
        assert!(ability.candidates.is_some(), "it picks at resolution");
        assert_eq!(ability.question, Some("a unit you control here to kill"));
    }

    #[test]
    fn the_holder_may_kill_a_unit_there_to_draw_and_the_kill_lands_before_the_hold_scores() {
        let mut fixture = dusk_rose_lab();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(
            ctx.blob.prompt, None,
            "352.10.c.1 · nothing is chosen while the trigger goes on the chain"
        );
        assert_eq!(lab_items(&ctx), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Chosen { .. })),
            "and nothing is chosen, so no Chosen event fires"
        );
        resolve_the_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            }),
            "the lab asks as it resolves, before anything is scored"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert!(!prompt.cancel, "a trigger cannot be taken back");
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {LAB}}}: choose a unit you control here to kill (0 of 1)")
        );
        assert!(!trashed(&ctx, fixtures::VI));
        assert!(ctx.blob.holder(fixtures::BF1) == Some(0) && !ctx.blob.scored(fixtures::BF1, 0));
        answer(&mut ctx, 0, 0);
        assert!(trashed(&ctx, fixtures::VI));
        assert_eq!(ctx.hand_of(0).len(), hand + 1 + CARDS_PER_TURN);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is killed for a card", fixtures::VI)));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the lab's only unit died before the Hold could score"
        );
        assert!(!ctx.blob.scored(fixtures::BF1, 0));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }

    #[test]
    fn the_holder_may_decline_and_the_hold_scores_as_usual() {
        let mut fixture = dusk_rose_lab();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        phases::start_turn(&mut ctx);
        assert_eq!(lab_items(&ctx), 1);
        resolve_the_chain(&mut ctx);
        answer(&mut ctx, 0, 1);
        assert!(!trashed(&ctx, fixtures::VI));
        assert_eq!(ctx.hand_of(0).len(), hand + CARDS_PER_TURN);
        assert!(ctx.blob.scored(fixtures::BF1, 0), "the Hold scored");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
    }

    #[test]
    fn the_lab_and_a_temporary_sprite_are_one_batch_the_holder_orders() {
        let mut fixture = with_a_friendly_sprite();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {LAB}}} trigger"),
                format!("{{card {SPRITE}}} is Temporary")
            ]
        );
        answer(&mut ctx, 0, 0);
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the lab was placed first, so it resolves last"
        );
        resolve_the_chain(&mut ctx);
        assert!(
            ctx.card(SPRITE).is_none(),
            "placed last, the Temporary kill resolved first"
        );
        assert!(ctx.effects.contains(&Effect::Despawn { card: SPRITE }));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()],
            "the choice is made at resolution, so the dead Sprite is never offered"
        );
        answer(&mut ctx, 0, 1);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + CARDS_PER_TURN,
            "the holder declined, so no card"
        );
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("is killed for a card")));
        assert!(ctx.blob.scored(fixtures::BF1, 0), "Vi still holds the lab");
    }

    #[test]
    fn an_uncontrolled_lab_is_silent_and_so_is_a_lab_held_by_the_other_seat() {
        let mut fixture = dusk_rose_lab();
        fixture.blob.set_holder(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "184.6.c · you refers to no one");
        assert_eq!(lab_items(&ctx), 0);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));

        let mut fixture = dusk_rose_lab();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "it is not the holder's Beginning Phase"
        );
        assert_eq!(lab_items(&ctx), 0);
    }

    #[test]
    fn only_the_holder_answers_and_only_a_unit_of_theirs_here_is_offered() {
        let mut fixture = dusk_rose_lab();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()],
            "their unit here is not one you control"
        );
        let open = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: open,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        answer(&mut ctx, 0, 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, LAB, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility)),
            "the lab's trigger is never an affordance"
        );
    }
}

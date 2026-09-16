use super::prelude::{
    battlefield, card_target, might_this_turn, target, triggered, when, FRIENDLY_UNIT_HERE,
};
use super::{Card, Flow, Item, Source, Stage, TargetKind, TargetSpec, Trigger};
use crate::engine::ctx::{Ctx, Event};

const A_UNIT_THEY_CONTROL_HERE: TargetSpec = target(
    FRIENDLY_UNIT_HERE,
    0,
    1,
    TargetKind::Card,
    "a unit you control here",
);

fn the_spell_caster_has_a_unit_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::PlayedSpell { controller, .. } = event else {
        return false;
    };
    let Some(here) = ctx.location(source.card) else {
        return false;
    };
    ctx.units_at(here)
        .into_iter()
        .any(|unit| ctx.controller(unit) == *controller)
}

fn give_the_chosen_unit_might(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, 1, None);
    }
    Flow::Done
}

pub static CARD: Card = battlefield(
    "Abandoned Hall",
    &[],
    &[when(
        triggered(
            Trigger::AnyonePlaysSpell,
            &[A_UNIT_THEY_CONTROL_HERE],
            give_the_chosen_unit_might,
        ),
        the_spell_caster_has_a_unit_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, a_spell, play as play_ability, spell};
    use crate::cards::Keyword;
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Answer, Pick, PickRefusal};
    use agni_plugin_sdk::table::Target;

    const THEIR_REACTION: u32 = 90;
    const THEIR_NEGATE: u32 = 91;
    const HALL_ITEM: u16 = 2;

    static QUICK: Card = spell("Quick", &[Keyword::Reaction], &[]);

    static NEGATE: Card = spell(
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

    fn with_the_hall_at_bf1() -> Fixture {
        let mut fixture = Fixture::enforced();
        let hall = fixture.table.card_mut(fixtures::GROUNDS).unwrap();
        hall.name = "Abandoned Hall".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut quick = fixtures::spell(THEIR_REACTION, fixtures::HAND, 1, "Quick", 1, 0);
        quick.domain = vec!["Mind".into()];
        fixture.table.cards.push(quick);
        let mut negate = fixtures::spell(THEIR_NEGATE, fixtures::HAND, 1, "Negate", 1, 0);
        negate.domain = vec!["Mind".into()];
        fixture.table.cards.push(negate);
        rescript(&mut fixture);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn rescript(fixture: &mut Fixture) {
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(THEIR_REACTION, &QUICK)
            .with_script(THEIR_NEGATE, &NEGATE);
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            _ => {}
        }
        settle(ctx)
    }

    fn pass_ring(ctx: &mut Ctx, first: u8) {
        priority::pass(ctx, first).unwrap();
        priority::pass(ctx, 1 - first).unwrap();
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn hall_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| {
                matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == fixtures::GROUNDS)
            })
            .count()
    }

    #[test]
    fn the_script_is_a_conditional_may_trigger_with_one_optional_target_here() {
        assert_eq!(CARD.name, "Abandoned Hall");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::AnyonePlaysSpell);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets.len(), 1);
        let spec = ability.targets[0];
        assert_eq!((spec.min, spec.max), (0, 1));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.filter, FRIENDLY_UNIT_HERE);
    }

    #[test]
    fn a_resolved_spell_lets_its_caster_buff_a_unit_here_and_the_buff_expires_at_end_of_turn() {
        let mut fixture = with_the_hall_at_bf1();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.queue.is_empty(), "nothing triggers on the play");
        pass_ring(&mut ctx, 0);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target {
                item: HALL_ITEM,
                spec: 0
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert!(!prompt.cancel, "a trigger cannot be taken back");
        assert_eq!(labels(&ctx), ["{card 50}", "skip"]);
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Target {
                    item: HALL_ITEM,
                    spec: 0
                }
            ),
            "{card 51}: choose a unit you control here (0 of 1)"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(hall_items(&ctx), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "not before it resolves"
        );
        pass_ring(&mut ctx, 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 1);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 51} ability resolves");
        assert!(ctx.blob.is_neutral_open());
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        crate::engine::phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
    }

    #[test]
    fn the_caster_may_decline_and_a_unit_elsewhere_or_an_enemy_unit_here_is_never_offered() {
        let mut fixture = with_the_hall_at_bf1();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        rescript(&mut fixture);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pass_ring(&mut ctx, 0);
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "skip"],
            "the sprite at the other field and Jinx here are not the caster's"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, HALL_ITEM, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play::choose_targets(&mut ctx, HALL_ITEM, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        let open = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: open,
                    option: 2
                }
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: open,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the choice belongs to the spell's caster"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(hall_items(&ctx), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        pass_ring(&mut ctx, 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 0, "declined");
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 51} ability resolves");
    }

    #[test]
    fn the_other_seats_reaction_gives_the_choice_to_that_seat_for_its_own_unit_here() {
        let mut fixture = with_the_hall_at_bf1();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        rescript(&mut fixture);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_REACTION).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        pass_ring(&mut ctx, 1);
        assert_eq!(
            ctx.card(THEIR_REACTION).unwrap().zone,
            Some(fixtures::TRASH)
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1, "the reaction's caster chooses");
        assert_eq!(labels(&ctx), ["{card 81}", "skip"]);
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].controller, 1);
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        pass_ring(&mut ctx, 1);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 1);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(ctx.blob.chain.len(), 1, "Spark is still waiting");
        assert_eq!(priority::holder(&ctx), Some(0));
        pass_ring(&mut ctx, 0);
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0, "now Spark's caster chooses");
        assert_eq!(labels(&ctx), ["{card 50}", "skip"]);
        pick(&mut ctx, 0, 0).unwrap();
        pass_ring(&mut ctx, 0);
        assert_eq!(might_counter(&ctx, fixtures::VI), 1);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 1);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn nothing_triggers_without_a_unit_of_the_caster_here_for_a_countered_spell_or_for_a_unit() {
        let mut fixture = with_the_hall_at_bf1();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        rescript(&mut fixture);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pass_ring(&mut ctx, 0);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.queue.is_empty());
        assert!(
            ctx.blob.chain.is_empty(),
            "Vi at the other field is not here"
        );
        assert!(ctx.blob.is_neutral_open());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{card 51}")));
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);

        let mut fixture = with_the_hall_at_bf1();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, THEIR_NEGATE).unwrap();
        assert_eq!(labels(&ctx), ["{card 71} on the chain", "cancel"]);
        pick(&mut ctx, 1, 0).unwrap();
        pass_ring(&mut ctx, 1);
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(
            ctx.blob.prompt.is_none(),
            "a countered spell was never played"
        );
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx.blob.is_neutral_open());

        let mut fixture = with_the_hall_at_bf1();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "a unit is not a spell");
        assert_eq!(hall_items(&ctx), 0);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
    }
}

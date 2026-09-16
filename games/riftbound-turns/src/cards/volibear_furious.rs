use super::prelude::{
    asking, card_targets, deal, done, on_attack, remember_card, remembered_cards, target, unit,
    with_candidates, ENEMY_UNIT_HERE,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DAMAGE: u8 = 5;
pub const DEFLECT: u8 = 2;
pub const QUESTION: &str = "an enemy unit here to deal 1 more to";
pub const TARGETS: TargetSpec = target(
    ENEMY_UNIT_HERE,
    0,
    DAMAGE,
    TargetKind::Card,
    "enemy units here to split 5 damage among",
);

fn struck(ctx: &Ctx, item: &Item) -> Vec<u32> {
    card_targets(ctx, item)
}

fn one_more_to(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    struck(ctx, item).into_iter().map(TargetRef::Card).collect()
}

fn strike(ctx: &mut Ctx, item: &Item, targets: &[u32], placed: &[u32]) {
    let me = item.kind.source();
    for unit in targets {
        let extra = placed.iter().filter(|held| *held == unit).count();
        let amount = u8::try_from(1 + extra).unwrap_or(DAMAGE).min(DAMAGE);
        if deal(ctx, item, *unit, amount) {
            ctx.narrate(format!("{{card {me}}} deals {amount} to {{card {unit}}}"));
        }
    }
}

fn thunder(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let targets = struck(ctx, item);
    if targets.is_empty() {
        ctx.narrate(format!("{{card {me}}}: no enemy unit here to strike"));
        return done();
    }
    let mut placed = remembered_cards(item);
    let left = if stage.0 == 0 {
        if targets.len() == 1 {
            deal(ctx, item, targets[0], DAMAGE);
            ctx.narrate(format!(
                "{{card {me}}} deals {DAMAGE} to {{card {}}}",
                targets[0]
            ));
            return done();
        }
        ctx.narrate(format!(
            "{{card {me}}} splits {DAMAGE} among {} enemy units here · 1 to each, {} more to place",
            targets.len(),
            usize::from(DAMAGE).saturating_sub(targets.len())
        ));
        DAMAGE.saturating_sub(u8::try_from(targets.len()).unwrap_or(u8::MAX))
    } else {
        if let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|unit| targets.contains(unit))
        {
            remember_card(ctx, unit);
            placed.push(unit);
        }
        stage.0 - 1
    };
    if left == 0 {
        strike(ctx, item, &targets, &placed);
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, left, 1, 1))
}

pub static CARD: Card = unit(
    "Volibear - Furious",
    &[Keyword::Deflect(DEFLECT)],
    &[asking(
        with_candidates(on_attack(&[TARGETS], thunder), one_more_to),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, play, prompts, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;

    const VOLIBEAR: u32 = 90;
    const FIRST: u32 = 91;
    const SECOND: u32 = 92;
    const THIRD: u32 = 93;
    const AWAY: u32 = 94;

    fn storm() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut volibear = fixtures::unit(VOLIBEAR, fixtures::BF1, 0, "Volibear - Furious", 9);
        volibear.domain = vec!["Fury".into()];
        volibear.energy = Some(10);
        volibear.power = Some(2);
        fixture.table.cards.push(volibear);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        for (id, might) in [(FIRST, 6), (SECOND, 6), (THIRD, 2)] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BF1, 1, "Bear Bait", might));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Away", 6));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VOLIBEAR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: VOLIBEAR });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
    }

    fn choose_all(ctx: &mut Ctx, units: &[u32]) {
        for unit in units {
            fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        }
        if ctx.blob.why == Some(PromptWhy::Target { item: 1, spec: 0 }) {
            fixtures::choose(ctx, 0, "done").unwrap();
        }
    }

    fn pick(ctx: &mut Ctx, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, 0, Pick { prompt, option })? {
            crate::engine::resume(ctx, &answered)?;
        }
        crate::engine::settle(ctx)
    }

    #[test]
    fn the_script_prints_deflect_two_and_one_attack_trigger_targeting_up_to_five_enemies_here() {
        assert!(std::ptr::eq(
            script_of("Volibear - Furious").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Deflect(DEFLECT)]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(ability.targets, &[TARGETS]);
        assert_eq!((TARGETS.min, TARGETS.max), (0, DAMAGE));
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn three_targets_each_take_one_and_the_last_two_points_are_placed_one_at_a_time() {
        let mut fixture = storm();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "done".to_string(),
                "skip".to_string(),
                format!("{{card {FIRST}}}"),
                format!("{{card {SECOND}}}"),
                format!("{{card {THIRD}}}"),
            ],
            "enemies here, not the one in its base · any number, none included"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[FIRST, AWAY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        choose_all(&mut ctx, &[FIRST, SECOND, THIRD]);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VOLIBEAR
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(FIRST),
                TargetRef::Card(SECOND),
                TargetRef::Card(THIRD)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item: 1, stage: 2 }),
            "one to each, two more to place"
        );
        assert_eq!(
            ctx.damage_on(FIRST),
            0,
            "the split is dealt once it is fully placed"
        );
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert_eq!(ctx.damage_on(THIRD), 0);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 90}: choose an enemy unit here to deal 1 more to (0 of 1)"
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {FIRST}}}"),
                format!("{{card {SECOND}}}"),
                format!("{{card {THIRD}}}"),
            ],
            "only the chosen targets take the rest"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THIRD}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item: 1, stage: 1 }),
            "one more to place"
        );
        assert_eq!(ctx.damage_on(THIRD), 0);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(FIRST),
                TargetRef::Card(SECOND),
                TargetRef::Card(THIRD),
                TargetRef::Card(THIRD)
            ],
            "the placed point rides the item"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {FIRST}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "five points placed, the trigger is done"
        );
        assert!(!ctx.on_board(THIRD), "2 on 2 Might dies at the cleanup");
        assert_eq!(ctx.damage_on(FIRST), 2);
        assert_eq!(ctx.damage_on(SECOND), 1);
        assert_eq!(ctx.damage_on(AWAY), 0);
        let dealt: Vec<(u32, u8)> = ctx
            .events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect();
        assert_eq!(
            dealt,
            [(FIRST, 2), (SECOND, 1), (THIRD, 2)],
            "one damage instance per unit, its share of the split"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_single_target_takes_all_five_without_a_prompt_and_a_target_that_left_takes_nothing() {
        let mut fixture = storm();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        choose_all(&mut ctx, &[FIRST]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(FIRST), 5);
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: FIRST,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VOLIBEAR}}} deals 5 to {{card {FIRST}}}")));
        drop(ctx);

        let mut fled = storm();
        let mut ctx = fled.ctx();
        attacks(&mut ctx);
        choose_all(&mut ctx, &[FIRST, SECOND]);
        ctx.table.card_mut(SECOND).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "the one target left takes it all"
        );
        assert_eq!(ctx.damage_on(FIRST), 5);
        assert_eq!(ctx.damage_on(SECOND), 0);
        drop(ctx);

        let mut empty = storm();
        for unit in [FIRST, SECOND, THIRD] {
            empty.table.card_mut(unit).unwrap().zone = Some(fixtures::BASE);
        }
        empty.resolve();
        let mut ctx = empty.ctx();
        ctx.raise(Event::Attacks { card: VOLIBEAR });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90}: no enemy unit here to strike".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut spared = storm();
        let mut ctx = spared.ctx();
        attacks(&mut ctx);
        assert!(
            fixtures::labels(&ctx).contains(&"skip".to_string()),
            "355.13 · any number includes none"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [FIRST, SECOND, THIRD] {
            assert_eq!(ctx.damage_on(unit), 0);
        }
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pick_outside_the_chosen_targets_is_refused_and_leaves_the_prompt_open() {
        let mut fixture = storm();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        choose_all(&mut ctx, &[FIRST, SECOND]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 3 }));
        assert_eq!(
            fixtures::labels(&ctx).len(),
            2,
            "the third enemy was never chosen"
        );
        assert!(pick(&mut ctx, 7).is_err());
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 3 }));
        for _ in 0..3 {
            fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        }
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(FIRST), 1);
        assert_eq!(ctx.damage_on(SECOND), 4);
        assert_eq!(ctx.damage_on(THIRD), 0);
    }
}

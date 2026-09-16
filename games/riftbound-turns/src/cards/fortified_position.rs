use super::prelude::{a_unit, battlefield, card_target, done, triggered, when};
use super::{Card, Flow, Item, Keyword, Source, Stage, TargetSpec, Trigger, Who};
use crate::engine::ctx::{Ctx, Event, Location};
use crate::state::{Expiry, FLAG_DEFENDER};

pub const SHIELD: u8 = 2;
pub const A_UNIT: TargetSpec = a_unit("a unit to gain Shield 2 this combat");

fn first_defender_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Defends { card } = event else {
        return false;
    };
    let here = ctx
        .card(source.card)
        .and_then(|held| held.zone)
        .map(Location::Battlefield);
    here.is_some()
        && ctx.location(*card) == here
        && ctx
            .designated(FLAG_DEFENDER)
            .into_iter()
            .find(|unit| ctx.location(*unit) == here)
            == Some(*card)
}

fn shield_a_unit_this_combat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.grant(unit, Keyword::Shield(SHIELD), Expiry::CombatEnd) {
        ctx.narrate(format!("{{card {unit}}} gains Shield {SHIELD} this combat"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Fortified Position",
    &[],
    &[when(
        triggered(
            Trigger::Defends(Who::You),
            &[A_UNIT],
            shield_a_unit_this_combat,
        ),
        first_defender_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Filter;
    use crate::engine::ctx::MoveCause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const POSITION: u32 = fixtures::GROUNDS;
    const THEIR_SECOND: u32 = 82;

    fn held_by_them() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(POSITION).unwrap().name = "Fortified Position".into();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(POSITION).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attack(ctx: &mut Ctx) {
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx
            .blob
            .showdown
            .as_ref()
            .is_some_and(|showdown| showdown.combat));
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(ctx.is_defender(fixtures::THEIR_UNIT));
    }

    fn position_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == POSITION => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 1).unwrap();
        priority::pass(ctx, 0).unwrap();
    }

    #[test]
    fn the_position_is_a_conditional_defend_trigger_that_targets_any_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Fortified Position").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Defends(Who::You));
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[A_UNIT]);
        assert_eq!(A_UNIT.filter, Filter::Unit);
        assert_eq!((A_UNIT.min, A_UNIT.max), (1, 1));
        assert!(!ability.optional);
    }

    #[test]
    fn defending_here_lets_the_holder_choose_a_unit_that_shields_two_until_the_combat_ends() {
        let mut fixture = held_by_them();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        assert_eq!(position_items(&ctx), [1], "the holder defends");
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1);
        let labels = fixtures::labels(&ctx);
        assert!(labels.contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)));
        assert!(
            labels.contains(&format!("{{card {}}}", fixtures::VI)),
            "any unit, even the attacker: {labels:?}"
        );
        assert!(labels.contains(&format!("{{card {}}}", fixtures::SPRITE)));
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        let might = ctx.current_might(fixtures::THEIR_UNIT);
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(position_items(&ctx), [1]);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            might,
            "not before it resolves"
        );
        resolve_top(&mut ctx);
        assert!(position_items(&ctx).is_empty());
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Shield(SHIELD)));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            might + i32::from(SHIELD),
            "+2 while it's a defender"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gains Shield {SHIELD} this combat",
            fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.showdown.is_some(), "the combat is still open");
        ctx.expire(Expiry::CombatEnd);
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Shield(SHIELD)));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), might);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn shield_on_the_attacker_adds_nothing_while_it_attacks() {
        let mut fixture = held_by_them();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        let might = ctx.current_might(fixtures::VI);
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Shield(SHIELD)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            might,
            "Shield only counts for a defender"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_defenders_fire_the_position_once_and_the_attacker_never_triggers_it() {
        let mut fixture = held_by_them();
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BF1, 1, "Nobody", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        assert!(ctx.is_defender(THEIR_SECOND));
        assert_eq!(position_items(&ctx), [1], "two defenders, one defend");
        drop(ctx);
        let mut mine = Fixture::enforced();
        mine.table.card_mut(POSITION).unwrap().name = "Fortified Position".into();
        mine.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        mine.blob.set_holder(fixtures::BF1, Some(0));
        mine.resolve();
        let mut ctx = mine.ctx();
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.is_defender(fixtures::VI));
        assert_eq!(position_items(&ctx), [0], "now I hold and I defend");
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
    }
}

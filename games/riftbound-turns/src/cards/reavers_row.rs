use super::prelude::{battlefield, card_target, done, optional, target, triggered, when, Location};
use super::{Card, Filter, Flow, Item, Source, Stage, TargetKind, TargetSpec, Trigger, Who};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march;
use crate::state::FLAG_DEFENDER;

pub const A_FRIENDLY_UNIT_HERE: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Friendly,
        Filter::Here,
        Filter::MovableToBase,
    ]),
    0,
    1,
    TargetKind::Card,
    "a friendly unit here to move to base",
);

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

fn may_move_a_friendly_unit_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    march::effect_move(ctx, item, unit, Location::Base(item.controller));
    done()
}

pub static CARD: Card = battlefield(
    "Reaver's Row",
    &[],
    &[when(
        optional(triggered(
            Trigger::Defends(Who::You),
            &[A_FRIENDLY_UNIT_HERE],
            may_move_a_friendly_unit_home,
        )),
        first_defender_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::MoveCause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const ROW: u32 = fixtures::GROUNDS;
    const THEIR_SECOND: u32 = 82;

    fn held_by_them() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ROW).unwrap().name = "Reaver's Row".into();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BF1, 1, "Nobody", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ROW).unwrap(), &CARD));
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

    fn row_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == ROW => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 1).unwrap();
        priority::pass(ctx, 0).unwrap();
    }

    #[test]
    fn the_row_is_an_optional_conditional_defend_trigger_over_friendly_units_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Reaver's Row").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Defends(Who::You));
        assert!(ability.optional);
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[A_FRIENDLY_UNIT_HERE]);
        assert_eq!((A_FRIENDLY_UNIT_HERE.min, A_FRIENDLY_UNIT_HERE.max), (0, 1));
    }

    #[test]
    fn defending_here_offers_the_holders_units_here_and_the_pick_goes_home() {
        let mut fixture = held_by_them();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        assert_eq!(row_items(&ctx), [1], "the holder defends");
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (1, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_SECOND}}}"),
                "skip".to_string()
            ],
            "the attacker and the units elsewhere are not offered"
        );
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
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SECOND}}}")).unwrap();
        assert_eq!(
            ctx.location(THEIR_SECOND),
            Some(Location::Battlefield(fixtures::BF1)),
            "not before it resolves"
        );
        resolve_top(&mut ctx);
        assert!(row_items(&ctx).is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_SECOND,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert_eq!(ctx.location(THEIR_SECOND), Some(Location::Base(1)));
        assert!(!ctx.in_combat(THEIR_SECOND), "home is out of the fight");
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_defender(fixtures::THEIR_UNIT));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_SECOND}}} moves to their base")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_moves_nobody_and_a_lone_defender_may_leave_the_field_to_the_attacker() {
        let mut fixture = held_by_them();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        resolve_top(&mut ctx);
        assert!(row_items(&ctx).is_empty());
        assert_eq!(
            ctx.location(THEIR_SECOND),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Move { zone, .. } if *zone == fixtures::BASE)));
        drop(ctx);
        let mut alone = held_by_them();
        alone.table.cards.retain(|card| card.id != THEIR_SECOND);
        alone.resolve();
        let mut ctx = alone.ctx();
        attack(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        resolve_top(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_attacker_never_triggers_the_row_and_two_defenders_fire_it_once() {
        let mut fixture = held_by_them();
        let mut ctx = fixture.ctx();
        attack(&mut ctx);
        assert!(ctx.is_defender(THEIR_SECOND));
        assert_eq!(row_items(&ctx), [1], "two defenders, one defend");
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        drop(ctx);
        let mut mine = Fixture::enforced();
        mine.table.card_mut(ROW).unwrap().name = "Reaver's Row".into();
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
        assert_eq!(row_items(&ctx), [0], "now I hold and I defend");
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()],
            "their attacker is not friendly"
        );
    }
}

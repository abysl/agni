use super::prelude::{a_card, battlefield, buff, card_target, done, triggered, UNIT_HERE};
use super::{Card, Flow, Item, Stage, TargetSpec, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const A_UNIT_HERE: TargetSpec = a_card(UNIT_HERE, "a unit here to buff");

fn buff_a_unit_here(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {unit}}} already has a buff"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Navori Fighting Pit",
    &[],
    &[triggered(
        Trigger::Hold(Who::You),
        &[A_UNIT_HERE],
        buff_a_unit_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Filter, TargetKind};
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const PIT: u32 = fixtures::GROUNDS;
    const SECOND: u32 = 90;

    fn pit_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(PIT).unwrap().name = "Navori Fighting Pit".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PIT).unwrap(), &CARD));
        fixture
    }

    fn with_a_second_unit_here() -> Fixture {
        let mut fixture = pit_held_by_me();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Jinx", 2));
        fixture.resolve();
        fixture
    }

    fn pit_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == PIT => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_pit_is_a_hold_trigger_that_targets_one_unit_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Navori Fighting Pit").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert_eq!(ability.targets, &[A_UNIT_HERE]);
        assert_eq!(
            A_UNIT_HERE.filter,
            Filter::And(&[Filter::Unit, Filter::Here])
        );
        assert_eq!((A_UNIT_HERE.min, A_UNIT_HERE.max), (1, 1));
        assert_eq!(A_UNIT_HERE.kind, TargetKind::Card);
        assert!(ability.condition.is_none());
        assert!(!ability.optional);
    }

    #[test]
    fn holding_with_one_unit_buffs_it_without_asking_and_the_buff_is_permanent() {
        let mut fixture = pit_held_by_me();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        hold(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "one candidate is auto-answered: {:?}",
            ctx.blob.why
        );
        assert_eq!(pit_items(&ctx), [0]);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "not before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_units_here_the_holder_picks_one_and_a_second_hold_on_a_buffed_unit_adds_nothing() {
        let mut fixture = with_a_second_unit_here();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {SECOND}}}")
            ],
            "the units here, nobody in a base"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 1
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_buffed(SECOND));
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(SECOND), 3);
        ctx.blob.clear_scored();
        hold(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(SECOND), 3, "a buff does not stack");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SECOND}}} already has a buff")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_here_is_a_legal_pick_and_a_hold_elsewhere_never_triggers_the_pit() {
        let mut fixture = pit_held_by_me();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::hold(&mut ctx, fixtures::BF1, 0);
        settle(&mut ctx).unwrap();
        assert!(fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT), "a unit here, any side");
        drop(ctx);
        let mut elsewhere = pit_held_by_me();
        let mut ctx = elsewhere.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(pit_items(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.is_buffed(fixtures::SPRITE));
    }
}

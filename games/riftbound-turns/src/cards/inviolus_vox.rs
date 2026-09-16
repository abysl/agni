use super::prelude::{a_friendly_unit, card_target, done, might_this_turn, on_conquer_me, unit};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 8;
pub const TARGET: TargetSpec = a_friendly_unit("a friendly unit to give +8 Might this turn");

fn roar(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit("Inviolus Vox", &[], &[on_conquer_me(&[TARGET], roar)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, FRIENDLY_UNIT};
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const VOX: u32 = 90;

    fn vox(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(8),
            power: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(VOX, zone, seat, "Inviolus Vox", 8)
        }
    }

    fn contesting(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vox(zone, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targeted_conquer_trigger() {
        assert!(std::ptr::eq(script_of("Inviolus Vox").unwrap(), &CARD));
        assert_eq!(CARD.name, "Inviolus Vox");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(!conquer.optional, "a friendly unit gets it");
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
        assert_eq!(conquer.targets.len(), 1);
        assert_eq!(conquer.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((conquer.targets[0].min, conquer.targets[0].max), (1, 1));
        assert_eq!(MIGHT, 8);
    }

    #[test]
    fn conquering_with_him_asks_for_a_friendly_unit_and_gives_it_eight_might_for_the_turn() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![VOX]
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {VOX}}}")
            ],
            "his controller's units, himself included"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {VOX}}}: choose {} (0 of 1)", TARGET.label)
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VOX
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the Might waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(MIGHT));
        assert_eq!(ctx.current_might(VOX), 8, "the other unit alone");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +8 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_is_not_offered_and_an_opponents_pick_is_refused() {
        let mut fixture = contesting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(!fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::SPRITE)));
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 2 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {VOX}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(VOX), 8 + i32::from(MIGHT));
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
    }

    #[test]
    fn a_conquer_without_him_and_a_hold_with_him_trigger_nothing() {
        let mut fixture = contesting(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "Vi conquered; he stayed home");
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);
        let mut fixture = contesting(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a hold is not a conquer");
        assert_eq!(ctx.points(0), 1);
    }
}

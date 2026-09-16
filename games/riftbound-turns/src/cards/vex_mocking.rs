use super::prelude::{
    asking, done, move_destinations, move_unit, optional, trigger_subject, triggered, unit, when,
    with_candidates, Location,
};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const SHIELD: u8 = 1;
pub const WHEN_YOU_STUN: Trigger = Trigger::Reflexive;
const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "the battlefield of the unit you stunned, to move me there";

pub fn you_stunned_an_enemy_unit_at_a_battlefield(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

pub fn where_it_was_stunned(ctx: &Ctx, item: &Item) -> Option<Location> {
    let me = item.kind.source();
    let stunned = trigger_subject(item)?;
    let there = ctx.location(stunned)?;
    there.battlefield()?;
    (ctx.on_board(me)
        && ctx.location(me) != Some(there)
        && move_destinations(ctx, me).contains(&there))
    .then_some(there)
}

fn taunts(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    where_it_was_stunned(ctx, item)
        .and_then(|there| ctx.zone_of(there))
        .map(|(zone, _)| vec![TargetRef::Zone(zone)])
        .unwrap_or_default()
}

fn mock(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let Some(there) = where_it_was_stunned(ctx, item) else {
                return done();
            };
            let picked = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, item.controller, &ctx.zones));
            if picked != Some(there) {
                return done();
            }
            ctx.narrate(format!("{{card {me}}} follows the stunned unit"));
            move_unit(ctx, item, me, there);
            done()
        }
        _ => {
            if where_it_was_stunned(ctx, item).is_none() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Vex - Mocking",
    &[Keyword::Shield(SHIELD), Keyword::Tank],
    &[asking(
        with_candidates(
            optional(when(
                triggered(WHEN_YOU_STUN, &[], mock),
                you_stunned_an_enemy_unit_at_a_battlefield,
            )),
            taunts,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::stun;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{priority, prompts, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const VEX: u32 = 90;

    fn vex(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(VEX, zone, seat, "Vex - Mocking", 5)
        }
    }

    fn gloom(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vex(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn queue_the_trigger(ctx: &mut Ctx, stunned: u32) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: VEX,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Card(stunned));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.prompt.is_none(), "the may is asked at resolution");
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_shield_one_and_tank_and_one_optional_stun_watch_of_her_own() {
        assert!(std::ptr::eq(script_of("Vex - Mocking").unwrap(), &CARD));
        assert_eq!(CARD.name, "Vex - Mocking");
        assert_eq!(CARD.keywords, [Keyword::Shield(1), Keyword::Tank]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let watch = &CARD.abilities[0];
        assert_eq!(watch.trigger, WHEN_YOU_STUN);
        assert_eq!(
            WHEN_YOU_STUN,
            Trigger::Reflexive,
            "the engine has no Stunned event · the trigger is queued by nothing yet"
        );
        assert!(watch.condition.is_some());
        assert!(watch.optional, "you may move me");
        assert!(watch.candidates.is_some());
        assert_eq!(watch.question, Some(QUESTION));
        assert!(watch.targets.is_empty());
        assert!(watch.cost.is_none());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_trigger_offers_the_stunned_units_battlefield_and_moves_her_there() {
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::SPRITE));
        queue_the_trigger(&mut ctx, fixtures::SPRITE);
        resolve_top(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICKED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{zone {}}}", fixtures::BF2), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {VEX}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF2)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(VEX),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(
            !ctx.card(VEX).unwrap().exhausted,
            "moved by an effect, not marched: she stays ready"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VEX}}} follows the stunned unit")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_leaves_her_where_she_was() {
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, fixtures::SPRITE);
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(VEX), Some(Location::Base(0)));
    }

    #[test]
    fn a_stun_in_a_base_or_where_she_already_stands_asks_nothing() {
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, fixtures::THEIR_UNIT);
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "a base is not a battlefield");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(VEX), Some(Location::Base(0)));
        drop(ctx);
        let mut fixture = gloom(fixtures::BF2);
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, fixtures::SPRITE);
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "already there");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(VEX),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn stunning_an_enemy_today_raises_no_event_so_nothing_queues_her() {
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::SPRITE));
        assert!(ctx.is_stunned(fixtures::SPRITE));
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
        assert!(!you_stunned_an_enemy_unit_at_a_battlefield(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            Source {
                card: VEX,
                ability: 0
            }
        ));
        assert_eq!(ctx.location(VEX), Some(Location::Base(0)));
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::stun raises no Stunned event and Trigger has no you-stun-an-enemy-unit variant (the Eclipse Herald row); with Event::Stunned { units, by } the condition reads a stunned enemy at a battlefield and the trigger queues with that unit as its subject"]
    fn stunning_an_enemy_at_a_battlefield_queues_her_and_a_friendly_stun_does_not() {
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::SPRITE));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].subject,
            Some(TargetRef::Card(fixtures::SPRITE))
        );
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF2)).unwrap();
        assert_eq!(
            ctx.location(VEX),
            Some(Location::Battlefield(fixtures::BF2))
        );
        drop(ctx);
        let mut fixture = gloom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::VI));
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "a friendly unit is not an enemy"
        );
    }
}

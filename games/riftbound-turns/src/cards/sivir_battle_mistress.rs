use super::prelude::{
    done, exhausting_self, legend, on_enemy_unit_dies, optional, ready, spawn_gold, triggered, when,
};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

const GOLD_ARRIVES_READY: bool = false;

pub const WHEN_YOU_RECYCLE_A_RUNE: Trigger = Trigger::Reflexive;

pub fn you_recycled_a_rune(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

pub fn recycled_a_rune_until_a_recycled_event_exists(
    ctx: &Ctx,
    by: u8,
    recycled: &[u32],
    source: Source,
) -> bool {
    ctx.card(source.card)
        .is_some_and(|held| ctx.face_in_play(held))
        && by == ctx.controller(source.card)
        && recycled.iter().any(|card| ctx.is_rune(*card))
}

fn she_is_exhausted(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.card(source.card).is_some_and(|held| held.exhausted)
}

fn spoils(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

fn ready_herself(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = legend(
    "Sivir - Battle Mistress",
    &[],
    &[
        when(
            optional(exhausting_self(triggered(
                WHEN_YOU_RECYCLE_A_RUNE,
                &[],
                spoils,
            ))),
            you_recycled_a_rune,
        ),
        when(on_enemy_unit_dies(&[], ready_herself), she_is_exhausted),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Who, TOKEN_GOLD};
    use crate::engine::cost::{Cost as Total, Need};
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{kill, pay, priority, prompts, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy, TargetRef};

    const SIVIR: u32 = fixtures::LEGEND_CARD;
    const PREY: u32 = 90;

    fn caravan() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SIVIR).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(PREY, fixtures::BF1, 1, "Raider", 2));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SIVIR).unwrap(), &CARD));
        fixture
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn queue_the_recycle_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: SIVIR,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Card(fixtures::RUNE_A));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn source() -> Source {
        Source {
            card: SIVIR,
            ability: 0,
        }
    }

    fn rainbow() -> Total {
        Total {
            energy: 0,
            power: vec![Need::Rainbow],
            ..Total::default()
        }
    }

    #[test]
    fn the_legend_has_a_may_recycle_trigger_that_exhausts_her_and_a_ready_me_on_enemy_deaths() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let recycle = &CARD.abilities[0];
        assert_eq!(recycle.trigger, WHEN_YOU_RECYCLE_A_RUNE);
        assert_eq!(
            WHEN_YOU_RECYCLE_A_RUNE,
            Trigger::Reflexive,
            "the engine raises no Recycled event · the trigger is queued by nothing yet"
        );
        assert!(recycle.optional);
        assert!(recycle.cost.is_none());
        assert_eq!(recycle.self_cost, SelfCost::Exhaust);
        assert!(recycle.condition.is_some());
        assert!(recycle.targets.is_empty());
        let deaths = &CARD.abilities[1];
        assert_eq!(deaths.trigger, Trigger::UnitDies(Who::Enemy));
        assert!(!deaths.optional, "ready me is not a may");
        assert_eq!(deaths.self_cost, SelfCost::Auto);
        assert!(deaths.condition.is_some());
    }

    #[test]
    fn the_recycle_reader_wants_her_controller_recycling_at_least_one_rune() {
        let mut fixture = caravan();
        let ctx = fixture.ctx();
        let seam = recycled_a_rune_until_a_recycled_event_exists;
        assert!(seam(&ctx, 0, &[fixtures::RUNE_A], source()));
        assert!(
            seam(&ctx, 0, &[fixtures::HAND_SPELL, 41], source()),
            "one rune among the cards is enough"
        );
        assert!(
            !seam(&ctx, 0, &[fixtures::HAND_SPELL], source()),
            "a card is not a rune"
        );
        assert!(!seam(&ctx, 0, &[], source()));
        assert!(
            !seam(&ctx, 1, &[44], source()),
            "an opponent's recycle is not hers"
        );
        assert!(!you_recycled_a_rune(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            source()
        ));
    }

    #[test]
    fn the_recycle_trigger_asks_to_exhaust_her_and_yes_plays_an_exhausted_gold() {
        let mut fixture = caravan();
        let mut ctx = fixture.ctx();
        queue_the_recycle_trigger(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {SIVIR}}} for the {{card {SIVIR}}} trigger · {{card {}}}?",
                fixtures::RUNE_A
            )
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(SIVIR).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(golds_of(&ctx, 0).is_empty(), "nothing until it resolves");
        resolve_top(&mut ctx);
        let gold = *golds_of(&ctx, 0).first().expect("one Gold");
        assert!(ctx.is_token(gold));
        assert!(ctx.is_gear(gold));
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut declined = caravan();
        let mut ctx = declined.ctx();
        queue_the_recycle_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(SIVIR).unwrap().exhausted);
        assert!(golds_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut spent = caravan();
        spent.table.card_mut(SIVIR).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        queue_the_recycle_trigger(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SIVIR}}} trigger is removed · its source is exhausted"
        )));
    }

    #[test]
    fn one_or_more_enemy_deaths_ready_an_exhausted_sivir_and_a_ready_one_watches_nothing() {
        let mut fixture = caravan();
        fixture.table.card_mut(SIVIR).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill::batch(
                &mut ctx,
                &[PREY, fixtures::THEIR_UNIT],
                Cause::Cleanup { last_item: None }
            ),
            [PREY, fixtures::THEIR_UNIT]
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "two deaths, two of her triggers to order · the second will find her ready"
        );
        fixtures::choose(
            &mut ctx,
            0,
            &format!("{{card {SIVIR}}} trigger 2 · {{card {PREY}}}"),
        )
        .unwrap();
        assert!(ctx.blob.prompt.is_none(), "no cost, nothing to confirm");
        assert_eq!(ctx.blob.chain.len(), 2, "{:?}", ctx.blob.log);
        assert!(ctx.blob.chain.iter().all(|held| matches!(
            held.kind,
            ItemKind::Trigger { source, index: 1 } if source == SIVIR
        )));
        assert!(
            ctx.card(SIVIR).unwrap().exhausted,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(SIVIR).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!("{{card {SIVIR}}} readies")));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == &format!("{{card {SIVIR}}} readies"))
                .count(),
            1,
            "she readies once however many died"
        );
        drop(ctx);
        let mut rested = caravan();
        let mut ctx = rested.ctx();
        assert_eq!(
            ctx.kill(PREY, Cause::Cleanup { last_item: None }),
            crate::engine::ctx::Killed::Yes
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "she is ready · nothing to ready");
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn a_friendly_death_is_not_an_enemy_death() {
        let mut fixture = caravan();
        fixture.table.card_mut(SIVIR).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.kill(fixtures::VI, Cause::Cleanup { last_item: None }),
            crate::engine::ctx::Killed::Yes
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(SIVIR).unwrap().exhausted);
    }

    #[test]
    fn paying_power_recycles_a_rune_today_without_raising_anything_for_her() {
        let mut fixture = caravan();
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &rainbow()).unwrap();
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(ctx.runes_of(0).len(), 3, "one rune recycled");
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
        assert!(golds_of(&ctx, 0).is_empty());
    }

    #[test]
    #[ignore = "engine gap · missing triggers: the pay path raises no Recycled event and Trigger has no Recycled(Who) for runes; with an Event::Recycled { by, cards } raised once per recycle instruction, WHEN_YOU_RECYCLE_A_RUNE becomes that trigger and you_recycled_a_rune reads recycled_a_rune_until_a_recycled_event_exists"]
    fn recycling_a_rune_to_pay_asks_to_exhaust_her_for_a_gold() {
        let mut fixture = caravan();
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &rainbow()).unwrap();
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(triggers::collect(&mut ctx), 1, "one rune, one trigger");
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        assert_eq!(golds_of(&ctx, 0).len(), 1);
    }
}

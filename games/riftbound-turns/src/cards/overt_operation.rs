use super::prelude::{asking, buff, done, friendly_units, play, ready, spell, with_candidates};
use super::sett_brawler::{buffed_friendly_units, spend_buff};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const PICK: u8 = 1;

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    buffed_friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn buff_all(ctx: &mut Ctx, seat: u8) {
    for unit in friendly_units(ctx, seat) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
}

fn operate(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = buffed_friendly_units(ctx, seat);
        let picked: Vec<u32> = ctx
            .picks()
            .iter()
            .copied()
            .filter(|unit| offered.contains(unit))
            .collect();
        for unit in picked {
            if spend_buff(ctx, unit) && ready(ctx, unit) {
                ctx.narrate(format!("{{card {unit}}} is readied"));
            }
        }
        buff_all(ctx, seat);
        return done();
    }
    let buffed = buffed_friendly_units(ctx, seat);
    if buffed.is_empty() {
        buff_all(ctx, seat);
        return done();
    }
    let count = u8::try_from(buffed.len()).unwrap_or(u8::MAX);
    Flow::Ask(ctx.ask_resume(item, PICK, 0, count))
}

pub static CARD: Card = spell(
    "Overt Operation",
    &[Keyword::Action],
    &[asking(
        with_candidates(play(&[], operate), candidates),
        "friendly units whose buff to spend to ready them",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const OPERATION: u32 = 90;
    const VETERAN: u32 = 91;
    const ROOKIE: u32 = 92;

    fn operation() -> CardInfo {
        let mut card = fixtures::spell(OPERATION, fixtures::HAND, 0, "Overt Operation", 5, 2);
        card.domain = vec!["Body".into()];
        card
    }

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn briefing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(operation());
        let mut veteran = fixtures::unit(VETERAN, fixtures::BF1, 0, "Legion Rearguard", 1);
        veteran.exhausted = true;
        fixture.table.cards.push(veteran);
        let mut rookie = fixtures::unit(ROOKIE, fixtures::BASE, 0, "Pit Rookie", 2);
        rookie.exhausted = true;
        fixture.table.cards.push(rookie);
        buffed(&mut fixture, VETERAN);
        buffed(&mut fixture, ROOKIE);
        buffed(&mut fixture, fixtures::THEIR_UNIT);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for rune in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn readied(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Readied { card, .. } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_an_untargeted_action_that_asks_at_resolution() {
        assert!(std::ptr::eq(script_of("Overt Operation").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            ability.targets.is_empty(),
            "355.10.c · spending buffs chooses nothing"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(
            ability.question,
            Some("friendly units whose buff to spend to ready them")
        );
    }

    #[test]
    fn each_buffed_friendly_unit_may_spend_its_buff_to_ready_then_every_friendly_unit_is_buffed() {
        let mut fixture = briefing();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, OPERATION).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to choose as it is played"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {VETERAN}}}"),
                format!("{{card {ROOKIE}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "the buffed friendly units; the buffed enemy is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {OPERATION}}}: choose friendly units whose buff to spend to ready them (0 of 2)"
            )
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        fixtures::choose(&mut ctx, 0, &format!("{{card {VETERAN}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {ROOKIE}}}"), "done".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.card(VETERAN).unwrap().exhausted,
            "its buff was spent to ready it"
        );
        assert!(ctx.card(ROOKIE).unwrap().exhausted, "its buff was kept");
        assert_eq!(readied(&ctx), [VETERAN]);
        assert!(ctx.is_buffed(VETERAN), "then buffed again");
        assert!(ctx.is_buffed(ROOKIE));
        assert!(
            ctx.is_buffed(fixtures::VI),
            "the unbuffed friendly unit is buffed"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "the enemy is nobody's friendly unit"
        );
        assert_eq!(
            ctx.table.counter(Target::Card(ROOKIE), COUNTER_BUFFED),
            Some(1),
            "702.3 · one buff at a time"
        );
        let log = &ctx.blob.log;
        let spent = log
            .iter()
            .position(|line| *line == format!("{{card {VETERAN}}}'s buff is spent"))
            .unwrap();
        let rebuffed = log
            .iter()
            .rposition(|line| *line == format!("{{card {VETERAN}}} is buffed"))
            .unwrap();
        assert!(spent < rebuffed, "spent, readied, then buffed");
        assert_eq!(ctx.card(OPERATION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_spends_nothing_and_still_buffs_every_friendly_unit() {
        let mut fixture = briefing();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, OPERATION).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(readied(&ctx).is_empty());
        assert!(ctx.card(VETERAN).unwrap().exhausted);
        assert!(ctx.is_buffed(VETERAN) && ctx.is_buffed(ROOKIE) && ctx.is_buffed(fixtures::VI));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("'s buff is spent")));
    }

    #[test]
    fn with_no_buffed_friendly_unit_nothing_is_asked_and_everyone_is_buffed() {
        let mut fixture = briefing();
        fixture
            .table
            .counters
            .retain(|counter| counter.target != Target::Card(VETERAN));
        fixture
            .table
            .counters
            .retain(|counter| counter.target != Target::Card(ROOKIE));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, OPERATION).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "055 · no choice to make");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(VETERAN) && ctx.is_buffed(ROOKIE) && ctx.is_buffed(fixtures::VI));
        assert!(ctx.card(VETERAN).unwrap().exhausted);
        assert!(readied(&ctx).is_empty());
    }

    #[test]
    fn a_pick_whose_buff_is_gone_by_the_time_the_answer_closes_readies_nothing() {
        let mut fixture = briefing();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, OPERATION).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {VETERAN}}}")).unwrap();
        assert!(spend_buff(&mut ctx, VETERAN));
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(VETERAN).unwrap().exhausted,
            "702.2.b.1 · no buff, nothing to spend, no ready"
        );
        assert!(ctx.is_buffed(VETERAN), "but the second sentence buffs it");
    }
}

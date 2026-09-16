use super::prelude::{asking, buff, done, play, ready, unit, with_candidates};
use super::sett_brawler::{buffed_friendly_units, spend_buff};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const PICK: u8 = 1;

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    buffed_friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn howl(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = buffed_friendly_units(ctx, seat);
        let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| offered.contains(picked))
        else {
            return done();
        };
        if !spend_buff(ctx, unit) {
            return done();
        }
        if buff(ctx, me) {
            ctx.narrate(format!("{{card {me}}} is buffed"));
        }
        if ready(ctx, me) {
            ctx.narrate(format!("{{card {me}}} is readied"));
        }
        return done();
    }
    if !ctx.on_board(me) || buffed_friendly_units(ctx, seat).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
}

pub static CARD: Card = unit(
    "Wildclaw Shaman",
    &[],
    &[asking(
        with_candidates(play(&[], howl), candidates),
        "a buff to spend to buff and ready me",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const SHAMAN: u32 = 90;

    fn shaman(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(SHAMAN, zone, 0, "Wildclaw Shaman", 3)
        }
    }

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn den(vi_buffed: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shaman(fixtures::HAND));
        if vi_buffed {
            buffed(&mut fixture, fixtures::VI);
        }
        buffed(&mut fixture, fixtures::THEIR_UNIT);
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_play_trigger_that_asks_for_a_buff_to_spend_at_resolution() {
        assert!(std::ptr::eq(script_of("Wildclaw Shaman").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            ability.targets.is_empty(),
            "355.10.c · a buff is not chosen"
        );
        assert!(
            !ability.optional,
            "the may is the pick, so a free trigger asks nothing before it resolves"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(
            ability.question,
            Some("a buff to spend to buff and ready me")
        );
    }

    #[test]
    fn spending_a_friendly_buff_buffs_and_readies_the_shaman_when_the_trigger_resolves() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAMAN).unwrap();
        assert!(ctx.on_board(SHAMAN));
        assert!(ctx.card(SHAMAN).unwrap().exhausted, "enters exhausted");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SHAMAN
        ));
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()],
            "the buffed friendly unit; the buffed enemy is not offered, nor the unbuffed Shaman"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SHAMAN}}}: choose a buff to spend to buff and ready me (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.is_buffed(SHAMAN));
        assert_eq!(ctx.current_might(SHAMAN), 4);
        assert!(!ctx.card(SHAMAN).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == SHAMAN
        )));
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT));
        let log = &ctx.blob.log;
        assert!(log.contains(&format!("{{card {}}}'s buff is spent", fixtures::VI)));
        assert!(log.contains(&format!("{{card {SHAMAN}}} is buffed")));
        assert!(log.contains(&format!("{{card {SHAMAN}}} is readied")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_keeps_every_buff_and_leaves_the_shaman_exhausted() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAMAN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(SHAMAN));
        assert!(ctx.card(SHAMAN).unwrap().exhausted);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
    }

    #[test]
    fn with_no_friendly_buff_to_spend_the_trigger_resolves_without_asking() {
        let mut fixture = den(false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAMAN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "702.2.b.2 · the enemy's buff is not his to spend"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(SHAMAN));
        assert!(ctx.card(SHAMAN).unwrap().exhausted);
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT));
    }

    #[test]
    fn a_shaman_gone_before_the_trigger_resolves_asks_nothing_and_a_stale_pick_spends_nothing() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAMAN).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(SHAMAN, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to buff or ready");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        drop(ctx);
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAMAN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let item = ctx.blob.chain[0].clone();
        assert!(spend_buff(&mut ctx, fixtures::VI));
        ctx.picked = vec![fixtures::VI];
        assert_eq!(howl(&mut ctx, &item, Stage(PICK)), Flow::Done);
        assert!(
            !ctx.is_buffed(SHAMAN),
            "702.2.b.1 · no buff was there to spend"
        );
        assert!(ctx.card(SHAMAN).unwrap().exhausted);
    }
}

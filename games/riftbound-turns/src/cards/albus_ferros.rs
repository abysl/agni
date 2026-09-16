use super::prelude::{asking, channel_exhausted, done, play, unit, with_candidates};
use super::sett_brawler::{buffed_friendly_units, spend_buff};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const PICK: u8 = 1;
pub const RUNES_PER_BUFF: usize = 1;

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    buffed_friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn forge(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = buffed_friendly_units(ctx, seat);
        let picked: Vec<u32> = ctx
            .picks()
            .iter()
            .copied()
            .filter(|unit| offered.contains(unit))
            .collect();
        let spent = picked
            .into_iter()
            .filter(|unit| spend_buff(ctx, *unit))
            .count();
        if spent > 0 {
            channel_exhausted(ctx, seat, spent * RUNES_PER_BUFF);
        }
        return done();
    }
    let buffed = buffed_friendly_units(ctx, seat);
    if buffed.is_empty() {
        return done();
    }
    let count = u8::try_from(buffed.len()).unwrap_or(u8::MAX);
    Flow::Ask(ctx.ask_resume(item, PICK, 0, count))
}

pub static CARD: Card = unit(
    "Albus Ferros",
    &[],
    &[asking(
        with_candidates(play(&[], forge), candidates),
        "buffs to spend for a rune each",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const ALBUS: u32 = 90;
    const VETERAN: u32 = 91;

    fn albus(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Order".into()],
            ..fixtures::unit(ALBUS, zone, 0, "Albus Ferros", 3)
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

    fn foundry() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(albus(fixtures::HAND));
        fixture.table.cards.push(fixtures::unit(
            VETERAN,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        buffed(&mut fixture, VETERAN);
        buffed(&mut fixture, fixtures::VI);
        buffed(&mut fixture, fixtures::THEIR_UNIT);
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    fn pool(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    fn rune_deck(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_DECK, seat).count()
    }

    #[test]
    fn the_script_is_a_play_trigger_that_asks_which_buffs_to_spend() {
        assert!(std::ptr::eq(script_of("Albus Ferros").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            ability.targets.is_empty(),
            "355.10.c · a buff is not chosen"
        );
        assert!(!ability.optional, "any number includes none");
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some("buffs to spend for a rune each"));
        assert_eq!(RUNES_PER_BUFF, 1);
    }

    #[test]
    fn every_buff_spent_channels_one_rune_exhausted_when_the_trigger_resolves() {
        let mut fixture = foundry();
        let mut ctx = fixture.ctx();
        let deck = rune_deck(&ctx, 0);
        let before = pool(&ctx, 0).len();
        fixtures::play_from_hand(&mut ctx, 0, ALBUS).unwrap();
        assert!(ctx.on_board(ALBUS));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ALBUS
        ));
        assert!(ctx.blob.prompt.is_none(), "the choice waits for resolution");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {VETERAN}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "his controller's buffed units, wherever they are; the enemy's is not his to spend"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.min, prompt.max), (0, 2));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {VETERAN}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "full at two, the prompt closed");
        assert!(!ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(VETERAN));
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT));
        let after = pool(&ctx, 0);
        assert_eq!(after.len(), before + 2);
        assert_eq!(rune_deck(&ctx, 0), deck - 2);
        assert!(
            after.iter().all(|(_, exhausted)| *exhausted),
            "the four that paid for him and the two channelled all sit exhausted"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 2 runes exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn spending_one_buff_channels_one_and_skipping_channels_none() {
        let mut fixture = foundry();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ALBUS).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let before = pool(&ctx, 0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {VETERAN}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "done".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool(&ctx, 0).len(), before + 1);
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(VETERAN));
        drop(ctx);
        let mut fixture = foundry();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ALBUS).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let before = pool(&ctx, 0).len();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool(&ctx, 0).len(), before);
        assert!(ctx.is_buffed(fixtures::VI) && ctx.is_buffed(VETERAN));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("channels")));
    }

    #[test]
    fn with_no_buffed_friendly_unit_the_trigger_resolves_without_asking() {
        let mut fixture = foundry();
        fixture
            .table
            .counters
            .retain(|counter| counter.target == Target::Card(fixtures::THEIR_UNIT));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ALBUS).unwrap();
        let before = pool(&ctx, 0).len();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "055 · nothing to choose");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool(&ctx, 0).len(), before);
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT), "702.2.b.2 · not his");
    }

    #[test]
    fn a_pick_whose_buff_left_before_the_answer_closed_channels_nothing_for_it() {
        let mut fixture = foundry();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ALBUS).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let before = pool(&ctx, 0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {VETERAN}}}")).unwrap();
        assert!(spend_buff(&mut ctx, VETERAN));
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            pool(&ctx, 0).len(),
            before,
            "no buff was spent by the trigger"
        );
    }
}

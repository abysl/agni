use super::prelude::{
    a_friendly_unit, asking, card_target, deal, done, gain_xp, play, spell, triggered,
    units_at_battlefields, with_candidates,
};
use super::{Card, Flow, Item, Keyword, Stage, Trigger};
use crate::engine::cleanup;
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, When};

pub const XP_PER_KILL: u8 = 1;
const XP_AFTER_KILLS: u8 = 1;

fn striker(ctx: &Ctx, item: &Item) -> Option<u32> {
    card_target(ctx, item, 0)
}

fn total_might(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn enemies_at_battlefields(ctx: &Ctx, seat: u8) -> Vec<u32> {
    units_at_battlefields(ctx)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn one_point_to(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    enemies_at_battlefields(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn split(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(unit) = striker(ctx, item) else {
        return done();
    };
    let placed = stage.0;
    if placed > 0 {
        if let Some(target) = ctx.picks().first().copied() {
            if enemies_at_battlefields(ctx, item.controller).contains(&target) {
                deal(ctx, item, target, 1);
            }
        }
    }
    let total = total_might(ctx, unit);
    if placed >= total || enemies_at_battlefields(ctx, item.controller).is_empty() {
        if placed > 0 {
            ctx.delay(
                When::AfterKillsBy(item.id),
                item.kind.source(),
                item.controller,
                XP_AFTER_KILLS,
                Vec::new(),
            );
        }
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, placed + 1, 1, 1))
}

fn xp_per_kill(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let kills = cleanup::kills_of(item);
    gain_xp(ctx, item.controller, kills.saturating_mul(XP_PER_KILL));
    done()
}

pub static CARD: Card = spell(
    "Alpha Strike",
    &[Keyword::Action],
    &[
        asking(
            with_candidates(
                play(&[a_friendly_unit("a friendly unit")], split),
                one_point_to,
            ),
            "an enemy unit at a battlefield to deal 1 to",
        ),
        triggered(Trigger::Reflexive, &[], xp_per_kill),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::ctx::{Cause, Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, prompts, settle};
    use crate::rules::COUNTER_XP;
    use crate::state::{Amount, DamageSource, Expiry, ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const STRIKE: u32 = 90;
    const YI: u32 = 91;
    const FIRST: u32 = 92;
    const SECOND: u32 = 93;
    const HOME: u32 = 94;

    fn strike(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Alpha Strike", 3, 1);
        card.domain = vec!["Calm".into(), "Body".into()];
        card
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), COUNTER_XP)
            .unwrap_or(0)
    }

    fn armed(might: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(strike(STRIKE, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(YI, fixtures::BASE, 0, "Yi", might));
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 1, "First", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF2, 1, "Second", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(HOME, fixtures::BASE, 1, "Home", 1));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(STRIKE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {YI}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn place(ctx: &mut Ctx, unit: u32) {
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
    }

    #[test]
    fn the_strike_is_an_action_that_chooses_a_friendly_unit_and_carries_its_xp_watcher() {
        assert_eq!(CARD.name, "Alpha Strike");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 2);
        let play = &CARD.abilities[0];
        assert_eq!(play.trigger, Trigger::Play);
        assert_eq!(play.targets.len(), 1);
        assert_eq!((play.targets[0].min, play.targets[0].max), (1, 1));
        assert_eq!(
            play.question,
            Some("an enemy unit at a battlefield to deal 1 to")
        );
        assert!(play.candidates.is_some());
        let after = &CARD.abilities[usize::from(XP_AFTER_KILLS)];
        assert_eq!(after.trigger, Trigger::Reflexive);
        assert!(after.targets.is_empty());
        assert_eq!(XP_PER_KILL, 1);
    }

    #[test]
    fn five_might_split_three_and_two_over_five_prompts_kills_both_and_registers_the_watcher() {
        let mut fixture = armed(5);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {FIRST}}}"), format!("{{card {SECOND}}}")],
            "enemy units at battlefields only: the one at base is never offered"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            format!("{{card {STRIKE}}}: 5 of 5 to place")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(!prompt.cancel);
        place(&mut ctx, FIRST);
        assert_eq!(
            damage_of(&ctx, FIRST),
            1,
            "each point lands as it is placed"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 2 }),
            format!("{{card {STRIKE}}}: 4 of 5 to place")
        );
        place(&mut ctx, FIRST);
        place(&mut ctx, SECOND);
        place(&mut ctx, FIRST);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 5 }));
        assert!(
            ctx.on_board(FIRST),
            "lethal damage waits for the cleanup after the spell"
        );
        place(&mut ctx, SECOND);
        assert!(ctx.blob.prompt.is_none(), "five points, five prompts");
        assert_eq!(ctx.card(FIRST).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(HOME));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::DamageDealt { source: Cause::Item(source), n: 1, .. } if *source == item
                ))
                .count(),
            5,
            "each point is dealt as the spell's own damage"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {STRIKE}}} resolves")));
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert!(
            ctx.blob.delayed.is_empty(),
            "the watcher was consumed by the cleanup attributed to the spell"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "and its trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == STRIKE && index == XP_AFTER_KILLS
        ));
        assert_eq!(cleanup::kills_of(&ctx.blob.chain[0]), 2);
        assert_eq!(xp_of(&ctx, 0), 0, "the XP waits for the trigger to resolve");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(xp_of(&ctx, 0), 2, "one XP per unit the strike killed");
        assert_eq!(xp_of(&ctx, 1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 2 XP".to_string()));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn the_watcher_scores_one_xp_per_unit_the_attributed_cleanup_killed() {
        let mut fixture = armed(5);
        fixture.table.card_mut(STRIKE).unwrap().zone = Some(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert_eq!(xp_of(&ctx, 0), 0);
        ctx.delay(When::AfterKillsBy(7), STRIKE, 0, XP_AFTER_KILLS, Vec::new());
        assert!(ctx.damage(FIRST, 3, Cause::Item(7)));
        assert!(ctx.damage(SECOND, 2, Cause::Item(7)));
        assert!(ctx.damage(HOME, 1, Cause::Item(7)));
        cleanup::run(&mut ctx, Some(7));
        assert!(ctx.blob.delayed.is_empty());
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == STRIKE && index == XP_AFTER_KILLS
        ));
        assert_eq!(cleanup::kills_of(&ctx.blob.chain[0]), 3);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(xp_of(&ctx, 0), 3);
        assert_eq!(xp_of(&ctx, 1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 3 XP".to_string()));
    }

    #[test]
    fn a_lone_candidate_takes_every_point_unasked_and_no_candidate_means_nothing_happens() {
        let mut fixture = armed(2);
        fixture.table.cards.retain(|card| card.id != SECOND);
        fixture.table.card_mut(FIRST).unwrap().might = Some(1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(fixtures::labels(&ctx), [format!("{{card {FIRST}}}")]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "a single candidate answers both points without a click"
        );
        assert_eq!(ctx.card(FIRST).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::DamageDealt { n: 1, .. }))
                .count(),
            2,
            "two Might, two points"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "the XP trigger for the one kill");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(xp_of(&ctx, 0), 1);
        let mut empty = armed(4);
        empty
            .table
            .cards
            .retain(|card| ![FIRST, SECOND].contains(&card.id));
        empty.resolve();
        let mut ctx = empty.ctx();
        cast(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "055 · nothing to split among, the spell does nothing"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.delayed.is_empty(), "no point placed, no watcher");
        assert_eq!(damage_of(&ctx, HOME), 0);
    }

    #[test]
    fn under_unyielding_spirit_nothing_is_marked_and_no_xp_is_gained() {
        let mut fixture = armed(5);
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        cast(&mut ctx);
        for _ in 0..5 {
            place(&mut ctx, FIRST);
        }
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage_of(&ctx, FIRST), 0);
        assert!(ctx.on_board(FIRST) && ctx.on_board(SECOND));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(
            ctx.blob.delayed.is_empty(),
            "nothing died in the attributed cleanup, so the watcher was dropped"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("XP")));
        assert_eq!(xp_of(&ctx, 0), 0);
    }

    #[test]
    fn the_split_prompt_belongs_to_the_caster_and_the_other_seat_cannot_play_the_spell() {
        let mut fixture = armed(5);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
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
            })),
            "no skip and no cancel: every point must be placed"
        );
        assert_eq!(priority::pass(&mut ctx, 0), Err(Refusal::PromptOpen));
        drop(ctx);
        let mut theirs = armed(5);
        theirs.table.cards.push(strike(95, 1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: 95,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "an Action on the opponent's turn outside a showdown"
        );
    }
}

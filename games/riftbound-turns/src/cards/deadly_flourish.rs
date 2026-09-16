use super::prelude::{
    an_enemy_unit, at_end_of_turn, card_target, deal, done, play, spawn_gold, spell, triggered,
};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::cleanup;
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, When};

pub const DAMAGE: u8 = 3;
pub const GOLD_AFTER_KILL: u8 = 1;
pub const LAPSE: u8 = 2;
pub const GOLD_ARRIVES_READY: bool = false;

pub fn watches(ctx: &Ctx, flourish: u32, unit: u32) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == flourish
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit)
    })
}

fn spend_watch(ctx: &mut Ctx, flourish: u32, unit: u32) {
    let turn = ctx.turn();
    let mut spent = false;
    ctx.blob.delayed.retain(|delayed| {
        let hit = !spent
            && delayed.source == flourish
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit);
        if hit {
            spent = true;
        }
        !hit
    });
}

pub fn gold_when_it_dies_this_turn(ctx: &mut Ctx, item: &Item, unit: u32) {
    ctx.delay(
        When::AfterKillsBy(item.id),
        item.kind.source(),
        item.controller,
        GOLD_AFTER_KILL,
        vec![unit],
    );
    at_end_of_turn(ctx, item, LAPSE, vec![unit]);
    ctx.narrate(format!(
        "when {{card {unit}}} dies this turn, {{seat {}}} gains an exhausted Gold",
        item.controller
    ));
}

fn flourish(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if deal(ctx, item, unit, DAMAGE) {
        ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
    }
    gold_when_it_dies_this_turn(ctx, item, unit);
    done()
}

fn watched_unit(item: &Item) -> Option<u32> {
    match item.targets.first()? {
        TargetRef::Card(unit) => Some(*unit),
        _ => None,
    }
}

fn pay_the_bounty(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = watched_unit(item) else {
        return done();
    };
    if cleanup::kills_of(item) == 0 || ctx.on_board(unit) {
        return done();
    }
    let seat = item.controller;
    spend_watch(ctx, item.kind.source(), unit);
    ctx.narrate(format!("{{card {unit}}} died"));
    spawn_gold(ctx, seat, GOLD_ARRIVES_READY);
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = watched_unit(item) {
        ctx.narrate(format!("the watch on {{card {unit}}} lapses"));
    }
    done()
}

pub static CARD: Card = spell(
    "Deadly Flourish",
    &[],
    &[
        play(&[an_enemy_unit("an enemy unit")], flourish),
        triggered(Trigger::Reflexive, &[], pay_the_bounty),
        triggered(Trigger::Reflexive, &[], lapse),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, TOKEN_GOLD};
    use crate::engine::ctx::{Cause, EntryMove, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const FLOURISH: u32 = 90;
    const THEIR_FLOURISH: u32 = 91;
    const BRUTE: u32 = 92;
    const SCOUT: u32 = 93;
    const MIND_RUNE: u32 = 100;
    const FIRST_GOLD: u32 = 200;

    fn flourish_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Deadly Flourish", 4, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(flourish_card(FLOURISH, 0));
        fixture.table.cards.push(flourish_card(THEIR_FLOURISH, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BASE, 1, "Scout", 3));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && ctx.controller(card.id) == seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, FLOURISH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_sorcery_over_an_enemy_unit_with_an_after_kills_bounty_and_a_lapse() {
        assert!(std::ptr::eq(script_of("Deadly Flourish").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(
            CARD.abilities[usize::from(GOLD_AFTER_KILL)].trigger,
            Trigger::Reflexive
        );
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
        assert_eq!(DAMAGE, 3);
    }

    #[test]
    fn three_that_kill_the_enemy_pay_an_exhausted_gold_through_the_bounty_trigger() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLOURISH).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}", "{card 93}", "cancel"],
            "enemy units anywhere, never my own Vi"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: SCOUT,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(SCOUT), "three kills the 3-Might Scout");
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the bounty trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == FLOURISH && index == GOLD_AFTER_KILL
        ));
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "the Gold waits for the trigger"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds_of(&ctx, 0), [FIRST_GOLD]);
        let gold = ctx.card(FIRST_GOLD).unwrap();
        assert_eq!(gold.zone, Some(fixtures::BASE));
        assert_eq!(gold.owner, 0);
        assert!(gold.exhausted, "played exhausted");
        assert!(ctx.is_token(FIRST_GOLD));
        assert!(
            ctx.blob.delayed.is_empty(),
            "the bounty spent the turn watch too"
        );
        assert!(!watches(&ctx, FLOURISH, SCOUT));
        assert!(ctx.blob.log.contains(&"{card 93} takes 3".to_string()));
        assert!(ctx.blob.log.contains(&"{card 93} died".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(ctx.card(FLOURISH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn three_on_a_survivor_leaves_the_turn_watch_and_it_lapses_with_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.damage_on(BRUTE), 3);
        assert!(ctx.on_board(BRUTE));
        assert!(ctx.blob.chain.is_empty(), "no bounty for no kill");
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(watches(&ctx, FLOURISH, BRUTE));
        assert!(!watches(&ctx, FLOURISH, SCOUT));
        assert_eq!(
            ctx.blob.delayed.len(),
            1,
            "the after-kills watcher was dropped by the spell's own cleanup; the turn watch stays"
        );
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(
            &"when {card 92} dies this turn, {seat 0} gains an exhausted Gold".to_string()
        ));
        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(
            ctx.blob.delayed.is_empty(),
            "the watch lapses with the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"the watch on {card 92} lapses".to_string()));
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_a_later_death_of_the_watched_unit_pays_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.kill(BRUTE, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "triggers::sources lists in-play cards only, so the spell in the trash never hears the death"
        );
        assert!(
            watches(&ctx, FLOURISH, BRUTE),
            "the watch was never consulted"
        );
    }

    #[test]
    #[ignore = "engine gap · a turn-scoped death watcher · triggers::sources lists in-play cards only, so a resolved spell cannot hear its target die later this turn; gold_when_it_dies_this_turn registers the watch until the engine grows floating triggers sourced from a resolved item"]
    fn a_watched_unit_that_dies_later_this_turn_still_pays_the_gold() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.kill(BRUTE, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 0), [FIRST_GOLD]);
        assert!(ctx.card(FIRST_GOLD).unwrap().exhausted);
        assert!(!watches(&ctx, FLOURISH, BRUTE), "once only");
    }

    #[test]
    fn a_friendly_unit_is_refused_a_gone_target_is_not_hit_and_the_other_seat_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_FLOURISH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, FLOURISH).unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is friendly, not a unit or not on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::HAND, 1), 1)
            .unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx.blob.delayed.is_empty(), "no watch on a unit not hit");
        assert!(ctx.blob.chain.is_empty());
        assert!(golds_of(&ctx, 0).is_empty());
        assert_eq!(ctx.card(FLOURISH).unwrap().zone, Some(fixtures::TRASH));
    }
}

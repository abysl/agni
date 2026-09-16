use super::prelude::{
    a_unit_at_a_battlefield, at_end_of_turn, card_target, channel_exhausted, deal, done, play,
    spell, triggered,
};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::cleanup;
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, When};

pub const DAMAGE: u8 = 4;
pub const DAMAGE_WITH_SEVEN_RUNES: u8 = 7;
pub const RUNES_REQUIRED: usize = 7;
pub const RUNES_CHANNELLED: usize = 1;
pub const CHANNEL_AFTER_KILL: u8 = 1;
pub const LAPSE: u8 = 2;

pub fn controls_enough_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() >= RUNES_REQUIRED
}

pub fn damage_for(ctx: &Ctx, seat: u8) -> u8 {
    if controls_enough_runes(ctx, seat) {
        DAMAGE_WITH_SEVEN_RUNES
    } else {
        DAMAGE
    }
}

pub fn watches(ctx: &Ctx, strike: u32, unit: u32) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == strike
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit)
    })
}

fn spend_watch(ctx: &mut Ctx, strike: u32, unit: u32) {
    let turn = ctx.turn();
    let mut spent = false;
    ctx.blob.delayed.retain(|delayed| {
        let hit = !spent
            && delayed.source == strike
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit);
        if hit {
            spent = true;
        }
        !hit
    });
}

pub fn channel_when_it_dies_this_turn(ctx: &mut Ctx, item: &Item, unit: u32) {
    ctx.delay(
        When::AfterKillsBy(item.id),
        item.kind.source(),
        item.controller,
        CHANNEL_AFTER_KILL,
        vec![unit],
    );
    at_end_of_turn(ctx, item, LAPSE, vec![unit]);
    ctx.narrate(format!(
        "when {{card {unit}}} dies this turn, {{seat {}}} channels {RUNES_CHANNELLED} rune exhausted",
        item.controller
    ));
}

fn strike(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let amount = damage_for(ctx, item.controller);
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {unit}}} takes {amount}"));
    }
    channel_when_it_dies_this_turn(ctx, item, unit);
    done()
}

fn watched_unit(item: &Item) -> Option<u32> {
    match item.targets.first()? {
        TargetRef::Card(unit) => Some(*unit),
        _ => None,
    }
}

fn siphon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = watched_unit(item) else {
        return done();
    };
    if cleanup::kills_of(item) == 0 || ctx.on_board(unit) {
        return done();
    }
    let seat = item.controller;
    spend_watch(ctx, item.kind.source(), unit);
    ctx.narrate(format!("{{card {unit}}} died"));
    channel_exhausted(ctx, seat, RUNES_CHANNELLED);
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = watched_unit(item) {
        ctx.narrate(format!("the watch on {{card {unit}}} lapses"));
    }
    done()
}

pub static CARD: Card = spell(
    "Siphoning Strike",
    &[],
    &[
        play(
            &[a_unit_at_a_battlefield("a unit at a battlefield")],
            strike,
        ),
        triggered(Trigger::Reflexive, &[], siphon),
        triggered(Trigger::Reflexive, &[], lapse),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Keyword};
    use crate::engine::ctx::{Cause, EntryMove, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const THEIR_STRIKE: u32 = 91;
    const BRUTE: u32 = 92;
    const COLOSSUS: u32 = 93;
    const MY_TOP_RUNE: u32 = 32;
    const CALM_RUNES: [u32; 2] = [100, 101];
    const MORE_RUNES: [u32; 2] = [102, 103];

    fn strike_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Siphoning Strike", 4, 0);
        card.domain = vec!["Calm".into(), "Mind".into()];
        card
    }

    fn sands() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(strike_card(STRIKE, 0));
        fixture.table.cards.push(strike_card(THEIR_STRIKE, 1));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(COLOSSUS, fixtures::BF1, 1, "Colossus", 6));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn seven_runes() -> Fixture {
        let mut fixture = sands();
        for rune in MORE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
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

    fn pool_size(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_sorcery_over_a_unit_at_a_battlefield_with_a_siphon_and_a_lapse() {
        assert!(std::ptr::eq(script_of("Siphoning Strike").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Flow(crate::cards::Cost::FREE)));
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(
            CARD.abilities[usize::from(CHANNEL_AFTER_KILL)].trigger,
            Trigger::Reflexive
        );
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
        assert_eq!((DAMAGE, DAMAGE_WITH_SEVEN_RUNES, RUNES_REQUIRED), (4, 7, 7));
        let mut fixture = sands();
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 6);
        assert!(!controls_enough_runes(&ctx, 0));
        assert_eq!(damage_for(&ctx, 0), DAMAGE);
        drop(ctx);
        let mut fixture = seven_runes();
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 8);
        assert!(controls_enough_runes(&ctx, 0));
        assert_eq!(damage_for(&ctx, 0), DAMAGE_WITH_SEVEN_RUNES);
        assert!(!controls_enough_runes(&ctx, 1));
    }

    #[test]
    fn four_that_kill_the_brute_channel_an_exhausted_rune_through_the_siphon_trigger() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx, 0);
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 60}".to_string(),
                format!("{{card {BRUTE}}}"),
                format!("{{card {COLOSSUS}}}"),
                "cancel".to_string()
            ],
            "units at battlefields, mine or theirs; Vi and Jinx in their bases are out"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "four kills the 4-Might Brute");
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the siphon trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == STRIKE && index == CHANNEL_AFTER_KILL
        ));
        assert_eq!(pool_size(&ctx, 0), pool, "the rune waits for the trigger");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool_size(&ctx, 0), pool + 1);
        let rune = ctx.card(MY_TOP_RUNE).unwrap();
        assert_eq!(rune.zone, Some(fixtures::RUNE_POOL));
        assert!(rune.exhausted, "it arrives exhausted");
        assert!(
            ctx.blob.delayed.is_empty(),
            "the siphon spent the turn watch too"
        );
        assert!(!watches(&ctx, STRIKE, BRUTE));
        assert!(ctx.blob.log.contains(&format!("{{card {BRUTE}}} takes 4")));
        assert!(ctx.blob.log.contains(&format!("{{card {BRUTE}}} died")));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_seven_runes_it_deals_seven_and_the_colossus_falls() {
        let mut fixture = seven_runes();
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx, 0);
        cast_at(&mut ctx, COLOSSUS);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: COLOSSUS,
            n: DAMAGE_WITH_SEVEN_RUNES,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(COLOSSUS));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {COLOSSUS}}} takes 7")));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool_size(&ctx, 0), pool + 1);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn four_on_a_survivor_leaves_the_turn_watch_and_it_lapses_with_the_turn() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx, 0);
        cast_at(&mut ctx, COLOSSUS);
        assert_eq!(ctx.damage_on(COLOSSUS), 4);
        assert!(ctx.on_board(COLOSSUS));
        assert!(ctx.blob.chain.is_empty(), "no siphon for no kill");
        assert_eq!(pool_size(&ctx, 0), pool);
        assert!(watches(&ctx, STRIKE, COLOSSUS));
        assert!(!watches(&ctx, STRIKE, BRUTE));
        assert_eq!(
            ctx.blob.delayed.len(),
            1,
            "the after-kills watcher was dropped by the spell's own cleanup; the turn watch stays"
        );
        assert!(ctx.blob.log.contains(&format!(
            "when {{card {COLOSSUS}}} dies this turn, {{seat 0}} channels 1 rune exhausted"
        )));
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
            .contains(&format!("the watch on {{card {COLOSSUS}}} lapses")));
        assert_eq!(pool_size(&ctx, 0), pool);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn today_a_later_death_of_the_watched_unit_channels_nothing() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx, 0);
        cast_at(&mut ctx, COLOSSUS);
        assert_eq!(ctx.kill(COLOSSUS, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            pool_size(&ctx, 0),
            pool,
            "triggers::sources lists in-play cards only, so the spell in the trash never hears the death"
        );
        assert!(
            watches(&ctx, STRIKE, COLOSSUS),
            "the watch was never consulted"
        );
    }

    #[test]
    #[ignore = "engine gap · a turn-scoped death watcher · triggers::sources lists in-play cards only, so a resolved spell cannot hear its target die later this turn; channel_when_it_dies_this_turn registers the watch until the engine grows floating triggers sourced from a resolved item (the Deadly Flourish row)"]
    fn a_watched_unit_that_dies_later_this_turn_still_channels_the_rune() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        let pool = pool_size(&ctx, 0);
        cast_at(&mut ctx, COLOSSUS);
        assert_eq!(ctx.kill(COLOSSUS, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool_size(&ctx, 0), pool + 1);
        assert!(ctx.card(MY_TOP_RUNE).unwrap().exhausted);
        assert!(!watches(&ctx, STRIKE, COLOSSUS), "once only");
    }

    #[test]
    fn a_unit_in_a_base_is_refused_a_gone_target_is_not_hit_and_the_other_seat_waits() {
        let mut fixture = sands();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STRIKE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
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
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
    }
}

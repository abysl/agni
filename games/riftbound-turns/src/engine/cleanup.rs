use crate::engine::ctx::{is_unit_face, Cause, Ctx, Event, Location, Scored};
use crate::engine::{attach, combat, control, expiry, hide, kill, play, showdown};
use crate::state::{
    ChainItem, Delayed, ItemKind, Needs, Origin, Pending, TargetRef, When, FLAG_DEFENDER, SLOTS,
    UNANSWERED,
};

pub const CLEANUP_LIMIT: usize = 8;
pub const SLOT_KILLS: usize = SLOTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Established {
    Kept(u8),
    Conquered(u8),
    Uncontrolled,
    Combat,
}

pub fn run(ctx: &mut Ctx, last_item: Option<u16>) {
    let mut dead = Vec::new();
    for _ in 0..CLEANUP_LIMIT {
        win_check(ctx);
        dead.extend(lethal_kills_by(ctx, last_item));
        if dying(ctx).is_empty() {
            break;
        }
    }
    if let Some(item) = last_item {
        after_kills(ctx, item, &dead);
    }
    attach::sync(ctx);
    control::sync(ctx);
    combat::refresh_open(ctx);
    for zone in ctx.zones.battlefields.clone() {
        settle(ctx, zone);
    }
    hide::lost_control(ctx);
    showdown::stage(ctx);
    showdown::open_next(ctx);
}

pub fn kills_of(item: &ChainItem) -> u8 {
    item.slot(SLOT_KILLS).unwrap_or(0)
}

pub fn after_kills(ctx: &mut Ctx, item: u16, dead: &[u32]) -> usize {
    let when = When::AfterKillsBy(item);
    let due: Vec<Delayed> = ctx
        .blob
        .delayed
        .iter()
        .filter(|delayed| delayed.when == when)
        .cloned()
        .collect();
    ctx.blob.delayed.retain(|delayed| delayed.when != when);
    if dead.is_empty() {
        return 0;
    }
    let kills = u8::try_from(dead.len())
        .unwrap_or(u8::MAX)
        .min(UNANSWERED - 1);
    let queued = due.len();
    for delayed in due {
        let id = ctx.blob.next_item_id();
        let mut queued = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: delayed.source,
                index: delayed.ability,
            },
            delayed.seat,
            Origin::Board,
        );
        queued.targets = delayed
            .args
            .iter()
            .map(|arg| TargetRef::Card(*arg))
            .collect();
        queued.set_slot(SLOT_KILLS, kills);
        queued.stage = play::STAGE_PAY;
        ctx.blob.queue.push(Pending {
            item: queued,
            needs: Needs::Choices,
        });
    }
    queued
}

pub fn win_check(ctx: &mut Ctx) -> Option<u8> {
    let winner = ctx.winner()?;
    if ctx.won.is_none() {
        ctx.won = Some(winner);
        let victory = ctx.victory_score();
        if ctx.blob.won == Some(winner) {
            ctx.narrate(format!("{{seat {winner}}} wins the game"));
        } else if ctx.points_winner() == Some(winner) {
            ctx.narrate(format!("{{seat {winner}}} wins with {victory} points"));
        } else {
            ctx.narrate(format!("{{seat {winner}}} wins by concession"));
        }
    }
    Some(winner)
}

pub fn dying(ctx: &Ctx) -> Vec<u32> {
    let mut dying: Vec<u32> = ctx
        .faces_on_board()
        .filter(|card| is_unit_face(card))
        .map(|card| card.id)
        .filter(|card| {
            let damage = ctx.damage_on(*card);
            damage > 0 && (ctx.lethal_damage_marked(*card) || damage >= ctx.current_might(*card))
        })
        .collect();
    dying.sort_unstable();
    dying
}

pub fn lethal_kills(ctx: &mut Ctx) -> Vec<u32> {
    lethal_kills_by(ctx, None)
}

pub fn lethal_kills_by(ctx: &mut Ctx, last_item: Option<u16>) -> Vec<u32> {
    let batch = dying(ctx);
    let dead = kill::batch(ctx, &batch, Cause::Cleanup { last_item });
    for card in &dead {
        ctx.narrate(format!("{{card {card}}} dies"));
    }
    dead
}

pub fn defender_of(ctx: &Ctx, zone: u16, attacker: u8) -> u8 {
    ctx.designated(FLAG_DEFENDER)
        .into_iter()
        .map(|unit| ctx.controller(unit))
        .find(|seat| *seat != attacker)
        .or_else(|| ctx.blob.holder(zone).filter(|holder| *holder != attacker))
        .or_else(|| {
            ctx.seats_with_units(zone)
                .into_iter()
                .find(|seat| *seat != attacker)
        })
        .unwrap_or_else(|| ctx.blob.order().next_seat(attacker))
}

pub fn combat_result(ctx: &mut Ctx, zone: u16, attacker: u8) -> Option<combat::CombatResult> {
    let defender = defender_of(ctx, zone, attacker);
    let result = combat::result(ctx, zone, attacker, defender)?;
    ctx.raise(Event::CombatWon {
        zone,
        seat: result.winner,
    });
    ctx.raise(Event::CombatLost {
        zone,
        seat: result.loser,
    });
    ctx.narrate(format!(
        "{{seat {}}} wins the combat at {{zone {zone}}}",
        result.winner
    ));
    Some(result)
}

pub fn after_combat(ctx: &mut Ctx, zone: u16, attacker: u8) -> Established {
    let here = Location::Battlefield(zone);
    win_check(ctx);
    let mut fates = Vec::new();
    let batch = dying(ctx);
    for unit in kill::batch(ctx, &batch, Cause::Cleanup { last_item: None }) {
        fates.push(format!("{{card {unit}}} dies"));
    }
    let wounded = ctx.wounded();
    ctx.heal_all();
    for (unit, damage) in wounded {
        if ctx.location(unit) == Some(here) {
            fates.push(format!("{{card {unit}}} survives ({damage} damage healed)"));
        }
    }
    let attackers = combat::attackers(ctx, zone);
    if !attackers.is_empty() && !combat::defenders(ctx, zone).is_empty() {
        if let Some(by) = ctx.tie_recalls_all(attacker) {
            fates.push(format!(
                "{{seat {attacker}}} would return to base · {{card {by}}} recalls every unit instead"
            ));
            ctx.narrate(fates.join(" · "));
            fates.clear();
            ctx.recall_all(zone);
        } else {
            for unit in attackers {
                ctx.recall(unit, false);
            }
            fates.push(format!("{{seat {attacker}}} returns to base"));
        }
    }
    if !fates.is_empty() {
        ctx.narrate(fates.join(" · "));
    }
    combat_result(ctx, zone, attacker);
    combat::clear_designations(ctx);
    expiry::at_combat_end(ctx);
    let established = establish(ctx, zone);
    match established {
        Established::Kept(seat) => ctx.narrate(format!("{{seat {seat}}} keeps {{zone {zone}}}")),
        Established::Uncontrolled => ctx.narrate(format!("{{zone {zone}}} is left empty")),
        Established::Conquered(_) | Established::Combat => {}
    }
    run(ctx, None);
    established
}

pub fn settle(ctx: &mut Ctx, zone: u16) -> Option<u8> {
    let seats = ctx.seats_with_units(zone);
    let holder = ctx.blob.holder(zone);
    match (holder, seats.as_slice()) {
        (Some(held), present) if !present.contains(&held) && ctx.blob.contester(zone).is_none() => {
            ctx.blob.set_holder(zone, None);
            None
        }
        (None, [only]) if ctx.blob.contester(zone).is_none() => {
            ctx.blob.set_holder(zone, Some(*only));
            Some(*only)
        }
        _ => None,
    }
}

pub fn establish(ctx: &mut Ctx, zone: u16) -> Established {
    let seats = ctx.seats_with_units(zone);
    let before = ctx.blob.holder(zone);
    match seats.as_slice() {
        [only] => {
            let taker = *only;
            ctx.blob.set_contested(zone, None);
            if before == Some(taker) {
                return Established::Kept(taker);
            }
            ctx.blob.set_holder(zone, Some(taker));
            if ctx.blob.scored(zone, taker) {
                ctx.narrate(format!(
                    "{{seat {taker}}} takes {{zone {zone}}} · already scored this turn"
                ));
                return Established::Kept(taker);
            }
            if ctx.no_score_here(zone, taker) {
                ctx.narrate(format!(
                    "{{seat {taker}}} takes {{zone {zone}}} · cannot score here"
                ));
                return Established::Kept(taker);
            }
            conquer(ctx, zone, taker);
            Established::Conquered(taker)
        }
        [] => {
            ctx.blob.set_holder(zone, None);
            ctx.blob.set_contested(zone, None);
            Established::Uncontrolled
        }
        _ => Established::Combat,
    }
}

pub fn conquer(ctx: &mut Ctx, zone: u16, seat: u8) -> bool {
    ctx.blob.mark_scored(zone, seat);
    let scored = ctx.score_point(seat, false);
    let units = ctx.units_at(Location::Battlefield(zone));
    ctx.raise(Event::Conquered { zone, seat, units });
    if scored == Scored::FinalPointDrawn {
        ctx.narrate(format!(
            "{{seat {seat}}} conquers {{zone {zone}}} · draws instead of the final point"
        ));
    } else {
        ctx.narrate(format!("{{seat {seat}}} conquers {{zone {zone}}}"));
    }
    scored == Scored::Point
}

pub fn hold(ctx: &mut Ctx, zone: u16, seat: u8) -> bool {
    ctx.blob.mark_scored(zone, seat);
    let scored = ctx.score_point(seat, true);
    let units = ctx.units_at(Location::Battlefield(zone));
    ctx.raise(Event::Held { zone, seat, units });
    ctx.narrate(format!("{{seat {seat}}} holds {{zone {zone}}}"));
    scored == Scored::Point
}

pub fn score_holds(ctx: &mut Ctx, seat: u8) -> Vec<u16> {
    let battlefields = ctx.zones.battlefields.clone();
    for zone in &battlefields {
        settle(ctx, *zone);
    }
    let mut held = Vec::new();
    for zone in battlefields {
        if ctx.blob.holder(zone) != Some(seat) || ctx.blob.scored(zone, seat) {
            continue;
        }
        if ctx.no_score_here(zone, seat) {
            ctx.narrate(format!(
                "{{seat {seat}}} keeps {{zone {zone}}} · cannot score here"
            ));
            continue;
        }
        hold(ctx, zone, seat);
        held.push(zone);
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, gear, spell, triggered};
    use crate::cards::{Card, Flow, Keyword, Trigger};
    use crate::engine::ctx::{Killed, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle as settle_chain;
    use crate::engine::{attach, priority, triggers};
    use crate::rules::{COUNTER_POINTS, COUNTER_XP, DEFAULT_VICTORY_SCORE};
    use crate::state::{GameBlob, Mode, PromptWhy, Showdown, FLAG_ATTACKER};
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CounterInfo, Target};

    fn fresh() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture
    }

    #[test]
    fn a_lone_contester_conquers_once_per_turn_and_a_holder_keeps_without_scoring() {
        let mut fixture = fresh();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            establish(&mut ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.blob.contester(fixtures::BF1), None);
        assert!(ctx.blob.scored(fixtures::BF1, 0));
        assert_eq!(ctx.effects, [Effect::score(0, COUNTER_POINTS, 1)]);
        assert_eq!(
            ctx.events,
            [Event::Conquered {
                zone: fixtures::BF1,
                seat: 0,
                units: vec![fixtures::VI]
            }]
        );
        assert_eq!(ctx.blob.log, ["{seat 0} conquers {zone 9}"]);
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        assert_eq!(establish(&mut ctx, fixtures::BF1), Established::Kept(0));
        assert_eq!(ctx.effects.len(), 1);
        ctx.blob.set_holder(fixtures::BF1, None);
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        assert_eq!(establish(&mut ctx, fixtures::BF1), Established::Kept(0));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.effects.len(), 1, "once per battlefield per turn");
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} takes {zone 9} · already scored this turn"
        );
        let mut empty = fresh();
        empty.blob.set_holder(fixtures::BF1, Some(0));
        empty.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = empty.ctx();
        assert_eq!(
            establish(&mut ctx, fixtures::BF1),
            Established::Uncontrolled
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert_eq!(ctx.blob.contester(fixtures::BF1), None);
        let mut both = fresh();
        both.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        both.blob.set_holder(fixtures::BF2, Some(1));
        both.blob.set_contested(fixtures::BF2, Some(0));
        let mut ctx = both.ctx();
        assert_eq!(establish(&mut ctx, fixtures::BF2), Established::Combat);
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(ctx.blob.holder(fixtures::BF2), Some(1));
    }

    #[test]
    fn contested_is_sticky_and_control_cannot_change_while_it_lasts() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                crate::engine::ctx::MoveCause::Effect
            ),
            crate::engine::ctx::Moved::Moved
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        assert_eq!(
            ctx.move_unit(
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                crate::engine::ctx::MoveCause::Effect
            ),
            crate::engine::ctx::Moved::Moved
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(0),
            "184.3.a.1 · the second arrival does not re-apply Contested"
        );
        drop(ctx);
        let mut held = fresh();
        held.blob.set_holder(fixtures::BF1, Some(0));
        held.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = held.ctx();
        assert_eq!(settle(&mut ctx, fixtures::BF1), None);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "184.3.c · control cannot change while the battlefield is contested"
        );
        ctx.blob.set_contested(fixtures::BF1, None);
        assert_eq!(settle(&mut ctx, fixtures::BF1), None);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "184.4.c · with the contest gone an empty battlefield is lost"
        );
    }

    #[test]
    fn the_final_point_needs_a_hold_or_every_battlefield_and_the_winner_is_announced_once() {
        let mut fixture = fresh();
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(!conquer(&mut ctx, fixtures::BF1, 0));
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: 23,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} conquers {zone 9} · draws instead of the final point"
        );
        assert_eq!(win_check(&mut ctx), None);
        ctx.blob.mark_scored(fixtures::BF2, 0);
        ctx.blob.slot(fixtures::BF1).scored = 0;
        assert!(conquer(&mut ctx, fixtures::BF1, 0));
        assert_eq!(ctx.points(0), DEFAULT_VICTORY_SCORE);
        assert_eq!(win_check(&mut ctx), Some(0));
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} wins with 8 points");
        let lines = ctx.blob.log.len();
        assert_eq!(win_check(&mut ctx), Some(0));
        assert_eq!(ctx.blob.log.len(), lines, "announced once per entry");
        let mut held = fresh();
        held.set_points(1, DEFAULT_VICTORY_SCORE - 1);
        held.blob.set_holder(fixtures::BF2, Some(1));
        let mut ctx = held.ctx();
        assert_eq!(score_holds(&mut ctx, 1), [fixtures::BF2]);
        assert_eq!(ctx.effects, [Effect::score(1, COUNTER_POINTS, 1)]);
        assert_eq!(ctx.winner(), Some(1));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held {
                zone,
                seat: 1,
                ..
            } if *zone == fixtures::BF2
        )));
        assert_eq!(score_holds(&mut ctx, 1), Vec::<u16>::new());
        let mut lost = fresh();
        lost.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = lost.ctx();
        assert_eq!(score_holds(&mut ctx, 0), Vec::<u16>::new());
        assert_eq!(ctx.blob.holder(fixtures::BF1), None, "no units, no hold");
    }

    #[test]
    fn a_six_point_table_ends_at_six_and_the_final_point_rule_moves_with_it() {
        let mut fixture = fresh();
        fixture.table.options = vec![("victory_score".into(), 6)];
        fixture.set_points(0, 5);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.options.victory_score, 6);
        assert!(
            !conquer(&mut ctx, fixtures::BF1, 0),
            "448.1.b at five of six"
        );
        assert_eq!(win_check(&mut ctx), None);
        ctx.blob.mark_scored(fixtures::BF2, 0);
        ctx.blob.slot(fixtures::BF1).scored = 0;
        assert!(conquer(&mut ctx, fixtures::BF1, 0));
        assert_eq!(ctx.points(0), 6);
        assert_eq!(win_check(&mut ctx), Some(0));
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} wins with 6 points");
        let mut eight = fresh();
        eight.table.options = vec![("victory_score".into(), 6)];
        eight.set_points(1, 4);
        eight.blob.set_holder(fixtures::BF2, Some(1));
        let mut ctx = eight.ctx();
        assert_eq!(score_holds(&mut ctx, 1), [fixtures::BF2]);
        assert_eq!(ctx.winner(), None, "five of six is not a win");
        let mut long = fresh();
        long.table.options = vec![("victory_score".into(), 11)];
        long.set_points(0, DEFAULT_VICTORY_SCORE);
        let mut ctx = long.ctx();
        assert_eq!(
            win_check(&mut ctx),
            None,
            "eight points do not win an eleven-point table"
        );
    }

    #[test]
    fn dying_reads_a_lethal_damage_static_of_a_card_the_marker_controls_before_might() {
        const DRAGON: u32 = 90;
        let mut fixture = fresh();
        fixture.table.cards.push(fixtures::unit(
            DRAGON,
            fixtures::BASE,
            0,
            "Elder Dragon",
            10,
        ));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.damage_by(fixtures::THEIR_UNIT, 1, Cause::Rule, Some(1)));
        assert!(dying(&ctx).is_empty());
        assert!(ctx.damage_by(fixtures::THEIR_UNIT, 1, Cause::Rule, Some(0)));
        assert_eq!(dying(&ctx), [fixtures::THEIR_UNIT]);
        assert!(ctx.damage_by(fixtures::VI, 1, Cause::Rule, Some(1)));
        assert_eq!(dying(&ctx), [fixtures::THEIR_UNIT]);
        assert!(ctx.damage_by(fixtures::VI, 2, Cause::Rule, Some(1)));
        assert_eq!(dying(&ctx), [fixtures::VI, fixtures::THEIR_UNIT]);
        assert_eq!(lethal_kills(&mut ctx), [fixtures::VI, fixtures::THEIR_UNIT]);
        assert!(ctx.on_board(fixtures::SPRITE));
    }

    #[test]
    fn a_cleanup_kills_lethal_damage_settles_control_and_opens_the_staged_showdown() {
        let mut fixture = fresh();
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::SPRITE),
            counter: crate::engine::ctx::COUNTER_DAMAGE,
            value: 3,
        });
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::THEIR_UNIT),
            counter: crate::engine::ctx::COUNTER_DAMAGE,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        run(&mut ctx, None);
        assert_eq!(
            ctx.effects,
            [Effect::Despawn {
                card: fixtures::SPRITE
            }]
        );
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(ctx.table.card(fixtures::THEIR_UNIT).is_some());
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert_eq!(
            (showdown.zone, showdown.attacker, showdown.combat),
            (fixtures::BF1, 0, false)
        );
        assert_eq!(
            ctx.blob.log,
            [
                "{card 60} dies",
                "showdown at {zone 9} · {seat 0} against {seat 1}"
            ]
        );
        let mut two = fresh();
        two.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        two.table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Jinx", 2));
        two.blob.set_holder(fixtures::BF2, Some(1));
        two.blob.set_contested(fixtures::BF1, Some(0));
        two.blob.set_contested(fixtures::BF2, Some(0));
        let mut ctx = two.ctx();
        run(&mut ctx, None);
        assert!(
            ctx.blob.showdown.is_some(),
            "showdowns open before combats (322.13)"
        );
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().zone, fixtures::BF1);
        assert_eq!(ctx.blob.staged.len(), 1);
        assert_eq!(ctx.blob.prompt, None);
        let mut open = fresh();
        open.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        open.blob.set_contested(fixtures::BF1, Some(0));
        open.blob.showdown = Some(Showdown::open(fixtures::BF2, 1, 0));
        let mut ctx = open.ctx();
        run(&mut ctx, None);
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().zone, fixtures::BF2);
        assert_eq!(
            ctx.blob.staged.len(),
            1,
            "a contest made inside a showdown waits"
        );
        assert_ne!(ctx.blob.why, Some(PromptWhy::PickStaged));
    }

    const STRIKE: u32 = 90;
    const FIRST: u32 = 91;
    const SECOND: u32 = 92;
    const WARD: u32 = 93;
    const ARMOR: u32 = 94;

    static STRIKE_CARD: Card = spell(
        "Strike",
        &[],
        &[
            prelude::play(&[], |_, _, _| Flow::Done),
            triggered(Trigger::Reflexive, &[], |ctx, item, _| {
                prelude::gain_xp(ctx, item.controller, kills_of(item));
                Flow::Done
            }),
        ],
    );

    static WARD_CARD: Card = prelude::with_replacement(
        gear("Ward", &[], &[]),
        prelude::replaces(
            |_, would, _| would.cause == Cause::Cleanup { last_item: Some(7) },
            |ctx, would, source| {
                ctx.kill(source.card, Cause::Replacement);
                ctx.heal(would.unit);
            },
        ),
    );

    static ARMOR_CARD: Card = gear("Armor", &[Keyword::Equip(crate::cards::Cost::FREE)], &[]);

    fn damaged(fixture: &mut Fixture, card: u32, damage: i32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            value: damage,
        });
        fixture.table.counters.sort();
    }

    fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), COUNTER_XP)
            .unwrap_or(0)
    }

    fn struck() -> Fixture {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::spell(STRIKE, fixtures::TRASH, 0, "Strike", 3, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 1, "First", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Second", 3));
        damaged(&mut fixture, FIRST, 2);
        damaged(&mut fixture, SECOND, 3);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(STRIKE, &STRIKE_CARD);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture
    }

    #[test]
    fn a_cleanup_attributed_to_an_item_fires_its_after_kills_watcher_with_the_dead_as_arguments() {
        let mut fixture = struck();
        let mut ctx = fixture.ctx();
        ctx.delay(When::AfterKillsBy(7), STRIKE, 0, 1, Vec::new());
        ctx.delay(When::AfterKillsBy(8), STRIKE, 0, 1, Vec::new());
        run(&mut ctx, Some(7));
        assert_eq!(ctx.card(FIRST).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.blob.delayed.len(),
            1,
            "only the watcher of item 7 is consumed"
        );
        assert_eq!(ctx.blob.delayed[0].when, When::AfterKillsBy(8));
        assert_eq!(ctx.blob.queue.len(), 1);
        let queued = &ctx.blob.queue[0].item;
        assert!(matches!(
            queued.kind,
            ItemKind::Trigger { source, index: 1 } if source == STRIKE
        ));
        assert!(
            queued.targets.is_empty(),
            "the dead are a count, never targets: a Deflect unit in the trash must not tax the trigger"
        );
        assert_eq!(kills_of(queued), 2);
        assert_eq!(queued.slot(crate::state::SLOT_TRIGGER_COST), None);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the emptied battlefield is lost"
        );
        settle_chain(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(xp_of(&ctx, 0), 2, "one XP per unit the item killed");
        assert!(ctx.blob.log.contains(&"{seat 0} gains 2 XP".to_string()));
    }

    #[test]
    fn a_cleanup_that_kills_nothing_drops_the_watcher_and_an_unattributed_cleanup_keeps_it() {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::spell(STRIKE, fixtures::TRASH, 0, "Strike", 3, 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(STRIKE, &STRIKE_CARD);
        let mut ctx = fixture.ctx();
        ctx.delay(When::AfterKillsBy(7), STRIKE, 0, 1, Vec::new());
        run(&mut ctx, None);
        assert_eq!(
            ctx.blob.delayed.len(),
            1,
            "a cleanup with no item attribution leaves every watcher alone"
        );
        assert_eq!(after_kills(&mut ctx, 9, &[]), 0);
        assert_eq!(ctx.blob.delayed.len(), 1);
        run(&mut ctx, Some(7));
        assert!(
            ctx.blob.delayed.is_empty(),
            "055 · nothing died, nothing to do"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("XP")));
    }

    #[test]
    fn a_lethal_kill_in_an_attributed_cleanup_carries_the_item_into_its_cause() {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 0, "First", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(WARD, fixtures::BASE, 0, "Ward", 1));
        damaged(&mut fixture, FIRST, 2);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(WARD, &WARD_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(dying(&ctx), [FIRST]);
        assert_eq!(
            lethal_kills_by(&mut ctx, None),
            [FIRST],
            "the ward answers only the attributed cause, so an unattributed kill is a real death"
        );
        assert_eq!(ctx.card(FIRST).unwrap().zone, Some(fixtures::TRASH));
        let mut again = fresh();
        again
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 0, "First", 2));
        again
            .table
            .cards
            .push(fixtures::gear(WARD, fixtures::BASE, 0, "Ward", 1));
        damaged(&mut again, FIRST, 2);
        again.resolve();
        again.scripts = again.scripts.clone().with_script(WARD, &WARD_CARD);
        let mut ctx = again.ctx();
        assert_eq!(
            ctx.kill(FIRST, Cause::Cleanup { last_item: Some(7) }),
            Killed::Replaced
        );
        assert!(ctx.on_board(FIRST));
        assert_eq!(ctx.damage_on(FIRST), 0);
        assert_eq!(ctx.card(WARD).unwrap().zone, Some(fixtures::TRASH));
        let mut third = fresh();
        third
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 0, "First", 2));
        third
            .table
            .cards
            .push(fixtures::gear(WARD, fixtures::BASE, 0, "Ward", 1));
        damaged(&mut third, FIRST, 2);
        third.resolve();
        third.scripts = third.scripts.clone().with_script(WARD, &WARD_CARD);
        let mut ctx = third.ctx();
        ctx.delay(When::AfterKillsBy(7), FIRST, 0, 0, Vec::new());
        run(&mut ctx, Some(7));
        assert!(
            ctx.blob.delayed.is_empty() && ctx.blob.queue.is_empty(),
            "a replaced death is not a kill the item made"
        );
        assert!(ctx.on_board(FIRST));
        assert_eq!(ctx.card(WARD).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_banished_wearers_gear_is_left_loose_and_the_next_cleanup_recalls_it() {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 0, "First", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(ARMOR, fixtures::BASE, 0, "Armor", 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ARMOR, &ARMOR_CARD);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.attach(ARMOR, FIRST), attach::Attached::Yes);
        assert_eq!(
            ctx.location(ARMOR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.banish(FIRST));
        assert!(
            !attach::is_attached(&ctx, ARMOR),
            "719.5 · the gear detaches as its wearer leaves"
        );
        assert_eq!(
            ctx.location(ARMOR),
            Some(Location::Battlefield(fixtures::BF1)),
            "and stands loose where it was until the cleanup"
        );
        run(&mut ctx, None);
        assert_eq!(ctx.location(ARMOR), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {ARMOR}}} is recalled to base")));
        assert_eq!(ctx.card(FIRST).unwrap().zone, Some(fixtures::BANISHMENT));
        assert!(ctx.deaths.is_empty(), "427.2.a · a banish is not a death");
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
    }

    #[test]
    fn the_defender_seat_is_read_from_the_designations_then_the_holder_then_the_units_then_the_order(
    ) {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 1, "First", 2));
        let mut ctx = fixture.ctx();
        ctx.mark_defender(FIRST);
        assert_eq!(defender_of(&ctx, fixtures::BF1, 0), 1);
        ctx.clear_designation(FIRST);
        ctx.blob.set_holder(fixtures::BF1, Some(1));
        assert_eq!(defender_of(&ctx, fixtures::BF1, 0), 1);
        ctx.blob.set_holder(fixtures::BF1, Some(0));
        assert_eq!(
            defender_of(&ctx, fixtures::BF1, 0),
            1,
            "a holder who is the attacker is skipped for the seat with units"
        );
        ctx.blob.set_holder(fixtures::BF1, None);
        assert_eq!(
            defender_of(&ctx, fixtures::BF3, 0),
            1,
            "the turn order's next seat"
        );
        assert_eq!(defender_of(&ctx, fixtures::BF3, 1), 0);
    }

    #[test]
    fn the_combat_result_hook_runs_after_the_recall_and_before_the_designations_clear() {
        let mut fixture = fresh();
        fixture
            .table
            .cards
            .push(fixtures::unit(FIRST, fixtures::BF1, 0, "First", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Second", 3));
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        ctx.mark_attacker(FIRST);
        ctx.mark_defender(SECOND);
        let established = after_combat(&mut ctx, fixtures::BF1, 0);
        assert_eq!(established, Established::Kept(1));
        assert_eq!(
            ctx.location(FIRST),
            Some(Location::Base(0)),
            "3d recalled the attacker"
        );
        assert!(
            ctx.designated(FLAG_ATTACKER | FLAG_DEFENDER).is_empty(),
            "2e cleared the designations after the result was read"
        );
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::CombatWon { .. } | Event::CombatLost { .. })),
            "466.3.d · a recall in 3d means No Result, so nobody won or lost"
        );
    }
}

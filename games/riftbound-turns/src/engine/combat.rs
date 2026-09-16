use crate::cards::Keyword;
use crate::engine::ctx::{Cause, Ctx, Location};
use crate::engine::legal::Reason;
use crate::engine::{cleanup, prevent, triggers};
use crate::state::{PromptWhy, Showdown, ShowdownStage, FLAG_ATTACKER, FLAG_DEFENDER};
use crate::Refusal;

pub fn designate(ctx: &mut Ctx, showdown: &Showdown) -> usize {
    triggers::collect(ctx);
    refresh(ctx, showdown);
    let order = ctx.blob.order();
    let mut seats = vec![showdown.attacker];
    let mut seat = order.next_seat(showdown.attacker);
    for _ in 1..ctx.players() {
        if seat != showdown.attacker && seat != showdown.defender {
            seats.push(seat);
        }
        seat = order.next_seat(seat);
    }
    if showdown.defender != showdown.attacker {
        seats.push(showdown.defender);
    }
    triggers::collect_ordered(ctx, &seats)
}

pub fn refresh(ctx: &mut Ctx, showdown: &Showdown) {
    let here = Location::Battlefield(showdown.zone);
    for unit in ctx.designated(FLAG_ATTACKER | FLAG_DEFENDER) {
        if !ctx.is_unit(unit) || ctx.location(unit) != Some(here) {
            ctx.clear_designation(unit);
        }
    }
    for unit in ctx.units_at(here) {
        let controller = ctx.controller(unit);
        if controller == showdown.attacker {
            ctx.mark_attacker(unit);
        } else if controller == showdown.defender {
            ctx.mark_defender(unit);
        } else {
            ctx.clear_designation(unit);
        }
    }
}

pub fn refresh_open(ctx: &mut Ctx) {
    let Some(showdown) = ctx.blob.showdown.clone() else {
        return;
    };
    if showdown.combat && showdown.stage == ShowdownStage::Open {
        refresh(ctx, &showdown);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatResult {
    pub zone: u16,
    pub winner: u8,
    pub loser: u8,
}

pub fn recalled(ctx: &Ctx, zone: u16) -> bool {
    let here = Location::Battlefield(zone);
    ctx.designated(FLAG_ATTACKER)
        .into_iter()
        .any(|unit| ctx.on_board(unit) && ctx.location(unit) != Some(here))
}

pub fn result(ctx: &Ctx, zone: u16, attacker: u8, defender: u8) -> Option<CombatResult> {
    if attacker == defender || recalled(ctx, zone) {
        return None;
    }
    let seats = ctx.seats_with_units(zone);
    let (winner, loser) = match (seats.contains(&attacker), seats.contains(&defender)) {
        (true, false) => (attacker, defender),
        (false, true) => (defender, attacker),
        _ => return None,
    };
    (seats.len() == 1).then_some(CombatResult {
        zone,
        winner,
        loser,
    })
}

pub fn clear_designations(ctx: &mut Ctx) {
    for unit in ctx.designated(FLAG_ATTACKER | FLAG_DEFENDER) {
        ctx.clear_designation(unit);
    }
}

pub fn attackers(ctx: &Ctx, zone: u16) -> Vec<u32> {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.is_attacker(*unit))
        .collect()
}

pub fn defenders(ctx: &Ctx, zone: u16) -> Vec<u32> {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.is_defender(*unit))
        .collect()
}

fn needed_for(ctx: &Ctx, unit: u32, damage_assigner: Option<u8>) -> u8 {
    if damage_assigner.is_some_and(|seat| ctx.any_damage_is_lethal(seat, unit)) {
        return 1;
    }
    let needed = ctx.current_might(unit) - ctx.damage_on(unit);
    u8::try_from(needed.max(1)).unwrap_or(u8::MAX)
}

pub fn exempt(ctx: &Ctx, unit: u32) -> bool {
    ctx.no_damage(unit, Cause::Combat) || prevent::unbounded(ctx, unit, Cause::Combat)
}

pub fn lethal(ctx: &Ctx, unit: u32) -> u8 {
    lethal_for(ctx, unit, assigner(ctx).map(|(seat, _)| seat))
}

fn lethal_for(ctx: &Ctx, unit: u32, damage_assigner: Option<u8>) -> u8 {
    let needed = needed_for(ctx, unit, damage_assigner);
    if exempt(ctx, unit) {
        return needed;
    }
    let assigned = needed.div_ceil(ctx.damage_multiplier(unit));
    assigned.saturating_add(prevent::held(ctx, unit, Cause::Combat))
}

pub fn might_sum(ctx: &Ctx, units: &[u32]) -> u8 {
    let total: i32 = units
        .iter()
        .copied()
        .filter(|unit| ctx.deals_combat_damage(*unit))
        .map(|unit| ctx.current_might(unit))
        .sum();
    u8::try_from(total.max(0)).unwrap_or(u8::MAX)
}

pub fn ordered(ctx: &Ctx, units: &[u32], plain_taken: bool) -> Vec<u32> {
    ordered_for(ctx, units, plain_taken, false)
}

pub fn ordered_for(ctx: &Ctx, units: &[u32], plain_taken: bool, tank_ignored: bool) -> Vec<u32> {
    let tank = |unit: &u32| !tank_ignored && ctx.has_keyword(*unit, Keyword::Tank);
    let back = |unit: &u32| ctx.has_keyword(*unit, Keyword::Backline);
    let pick = |wanted: fn(bool, bool) -> bool| -> Vec<u32> {
        units
            .iter()
            .copied()
            .filter(|unit| wanted(tank(unit), back(unit)))
            .collect()
    };
    let tank_only = pick(|tank, back| tank && !back);
    let plain = pick(|tank, back| !tank && !back);
    let back_only = pick(|tank, back| !tank && back);
    let both = pick(|tank, back| tank && back);
    let mut chosen = if !tank_only.is_empty() {
        tank_only
    } else if !plain.is_empty() {
        plain
    } else {
        back_only
    };
    if !plain_taken || units.len() == 1 {
        chosen.extend(both);
    }
    chosen.sort_unstable_by_key(|unit| units.iter().position(|held| held == unit));
    chosen
}

fn opposing(ctx: &Ctx, showdown: &Showdown, assigner: u8) -> Vec<u32> {
    let side = if assigner == showdown.attacker {
        defenders(ctx, showdown.zone)
    } else {
        attackers(ctx, showdown.zone)
    };
    side.into_iter()
        .filter(|unit| !exempt(ctx, *unit))
        .collect()
}

fn unassigned(ctx: &Ctx, showdown: &Showdown, assigner: u8, assigned: &[(u32, u8)]) -> Vec<u32> {
    opposing(ctx, showdown, assigner)
        .into_iter()
        .filter(|unit| !assigned.iter().any(|(done, _)| done == unit))
        .collect()
}

pub fn candidates(ctx: &Ctx) -> Vec<u32> {
    let Some(showdown) = ctx.blob.showdown.as_ref() else {
        return Vec::new();
    };
    let ShowdownStage::Damage {
        assigner, assigned, ..
    } = &showdown.stage
    else {
        return Vec::new();
    };
    let opposing = opposing(ctx, showdown, *assigner);
    let tank_ignored = ctx.ignores_tank(*assigner, showdown.zone).is_some();
    let plain_taken = opposing
        .iter()
        .filter(|unit| assigned.iter().any(|(done, _)| done == *unit))
        .any(|unit| tank_ignored || !ctx.has_keyword(*unit, Keyword::Tank));
    let left = opposing
        .into_iter()
        .filter(|unit| !assigned.iter().any(|(done, _)| done == unit))
        .collect::<Vec<u32>>();
    ordered_for(ctx, &left, plain_taken, tank_ignored)
}

pub fn assigner(ctx: &Ctx) -> Option<(u8, u8)> {
    match ctx.blob.showdown.as_ref().map(|showdown| &showdown.stage) {
        Some(ShowdownStage::Damage {
            assigner,
            remaining,
            ..
        }) => Some((*assigner, *remaining)),
        _ => None,
    }
}

pub fn close(ctx: &mut Ctx, mut showdown: Showdown) {
    refresh(ctx, &showdown);
    let zone = showdown.zone;
    let attackers = attackers(ctx, zone);
    let defenders = defenders(ctx, zone);
    if attackers.is_empty() || defenders.is_empty() {
        ctx.narrate(format!(
            "combat at {{zone {zone}}} · no damage: only one side remains"
        ));
        resolve(ctx, &showdown);
        return;
    }
    let attack = might_sum(ctx, &attackers);
    let defend = might_sum(ctx, &defenders);
    ctx.narrate(format!(
        "attackers {attack} might vs defenders {defend} might"
    ));
    showdown.stage = ShowdownStage::Damage {
        assigner: showdown.attacker,
        remaining: attack,
        assigned: Vec::new(),
    };
    ctx.blob.showdown = Some(showdown);
    proceed(ctx);
}

pub fn proceed(ctx: &mut Ctx) {
    loop {
        let Some(showdown) = ctx.blob.showdown.clone() else {
            return;
        };
        let ShowdownStage::Damage {
            assigner,
            remaining,
            assigned,
        } = showdown.stage.clone()
        else {
            return;
        };
        let candidates = candidates(ctx);
        if remaining == 0 || candidates.is_empty() {
            if assigner == showdown.attacker && showdown.defender != showdown.attacker {
                let defend = might_sum(ctx, &defenders(ctx, showdown.zone));
                if let Some(open) = ctx.blob.showdown.as_mut() {
                    open.stage = ShowdownStage::Damage {
                        assigner: showdown.defender,
                        remaining: defend,
                        assigned,
                    };
                }
                continue;
            }
            deal(ctx);
            return;
        }
        if candidates.len() == 1 {
            assign(ctx, candidates[0]);
            continue;
        }
        ctx.ask(assigner, 1, 1, false, PromptWhy::Assign);
        return;
    }
}

pub fn assign(ctx: &mut Ctx, unit: u32) -> bool {
    let Some(showdown) = ctx.blob.showdown.clone() else {
        return false;
    };
    let ShowdownStage::Damage {
        assigner,
        remaining,
        mut assigned,
    } = showdown.stage.clone()
    else {
        return false;
    };
    if !candidates(ctx).contains(&unit) {
        return false;
    }
    let left = unassigned(ctx, &showdown, assigner, &assigned).len();
    let amount = if left <= 1 {
        remaining
    } else {
        lethal(ctx, unit).min(remaining)
    };
    assigned.push((unit, amount));
    if let Some(open) = ctx.blob.showdown.as_mut() {
        open.stage = ShowdownStage::Damage {
            assigner,
            remaining: remaining - amount,
            assigned,
        };
    }
    true
}

pub fn choose(ctx: &mut Ctx, unit: u32) -> Result<(), Refusal> {
    if !assign(ctx, unit) {
        return Err(Refusal::Illegal(Reason::NotALegalTarget));
    }
    proceed(ctx);
    Ok(())
}

fn deal(ctx: &mut Ctx) {
    let Some(showdown) = ctx.blob.showdown.take() else {
        return;
    };
    let assigned = match &showdown.stage {
        ShowdownStage::Damage { assigned, .. } => assigned.clone(),
        _ => Vec::new(),
    };
    record_excess(ctx, &showdown, &assigned);
    let mut dealt = Vec::new();
    for (unit, amount) in assigned {
        let marker = if ctx.controller(unit) == showdown.attacker {
            showdown.defender
        } else {
            showdown.attacker
        };
        if ctx.damage_by(unit, amount, Cause::Combat, Some(marker)) {
            dealt.push(format!("{{card {unit}}} takes {amount}"));
        }
    }
    if dealt.is_empty() {
        ctx.narrate("no damage is dealt");
    } else {
        ctx.narrate(dealt.join(" · "));
    }
    resolve(ctx, &showdown);
}

fn record_excess(ctx: &mut Ctx, showdown: &Showdown, assigned: &[(u32, u8)]) {
    for assigner in [showdown.attacker, showdown.defender] {
        let mut excess: Option<u8> = None;
        for (unit, amount) in assigned {
            let by = if ctx.is_defender(*unit) {
                showdown.attacker
            } else {
                showdown.defender
            };
            if by != assigner {
                continue;
            }
            let over = amount.saturating_sub(lethal_for(ctx, *unit, Some(assigner)));
            excess = Some(excess.unwrap_or(0).saturating_add(over));
        }
        if let Some(excess) = excess {
            ctx.record_excess_damage(assigner, showdown.zone, excess);
        }
    }
}

pub fn resolve(ctx: &mut Ctx, showdown: &Showdown) {
    if ctx.blob.why == Some(PromptWhy::Assign) {
        ctx.blob.close_prompt();
    }
    ctx.blob.showdown = None;
    cleanup::after_combat(ctx, showdown.zone, showdown.attacker);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, enemy_alone_at, gain_xp, might_this_turn, when};
    use crate::cards::{Card, Flow, Grant, Item, Scope, Source, Stage, Static};
    use crate::engine::ctx::{Event, MoveCause, Moved, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, march, prompts, settle, showdown, triggers};
    use crate::rules::{COUNTER_POINTS, COUNTER_XP};
    use crate::state::{Amount, DamageSource, Expiry, GameBlob, Mode, FLAG_STUNNED};
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    static TANK: Card = prelude::unit("Tanky", &[Keyword::Tank], &[]);
    static BACKLINE: Card = prelude::unit("Patroller", &[Keyword::Backline], &[]);
    static BOTH: Card = prelude::unit("Tanky Patroller", &[Keyword::Tank, Keyword::Backline], &[]);

    static VOIDREAVER: Card = prelude::legend(
        "Voidreaver",
        &[],
        &[prelude::on_combat_won(&[], |ctx, item, _| {
            gain_xp(ctx, item.controller, 1);
            prelude::done()
        })],
    );

    fn one_on_one(ctx: &Ctx, _: &Event, who: Source) -> bool {
        ctx.alone_at(who.card) && enemy_alone_at(ctx, who.card)
    }

    fn double_might(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        let me = item.kind.source();
        let now = i16::try_from(ctx.current_might(me)).unwrap_or(i16::MAX);
        ctx.might(me, now, Expiry::CombatEnd, None, item.id);
        prelude::done()
    }

    static FIORA: Card = prelude::unit(
        "Peerless",
        &[],
        &[
            when(prelude::on_attack(&[], double_might), one_on_one),
            when(prelude::on_defend(&[], double_might), one_on_one),
        ],
    );

    fn enemy_alone(ctx: &Ctx, _: &Event, who: Source) -> bool {
        enemy_alone_at(ctx, who.card)
    }

    fn mutate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        let me = item.kind.source();
        might_this_turn(ctx, item, me, 2, None);
        gain_xp(ctx, item.controller, 2);
        prelude::done()
    }

    static HORROR: Card = prelude::unit(
        "Horror",
        &[Keyword::Ambush],
        &[
            when(prelude::on_attack(&[], mutate), enemy_alone),
            when(prelude::on_defend(&[], mutate), enemy_alone),
        ],
    );

    fn defending_alone(ctx: &Ctx, _: u32, unit: u32) -> bool {
        ctx.is_defender(unit) && ctx.alone_at(unit)
    }

    static BLADESMAN: Card = prelude::with_statics(
        prelude::legend("Bladesman", &[], &[]),
        &[Static::Aura {
            scope: Scope::FriendlyUnits,
            when: defending_alone,
            grants: &[Grant::Might(2)],
        }],
    );

    static WASTE: Card = prelude::with_statics(
        prelude::battlefield("Waste", &[], &[]),
        &[Static::Aura {
            scope: Scope::UnitsHere,
            when: defending_alone,
            grants: &[Grant::Might(-2)],
        }],
    );

    const ATTACKER: u32 = 90;
    const A: u32 = 94;
    const B: u32 = 95;
    const C: u32 = 96;
    const D: u32 = 97;
    const E: u32 = 98;

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
    }

    fn put(fixture: &mut Fixture, id: u32, zone: u16, seat: u8, might: u8) {
        fixture
            .table
            .cards
            .push(fixtures::unit(id, zone, seat, "Nobody", might));
    }

    fn token(fixture: &mut Fixture, id: u32, zone: u16, seat: u8, might: u8) {
        fixture.table.cards.push(CardInfo {
            ..fixtures::unit(id, zone, seat, "Sprite", might)
        });
        fixture.table.tokens.push(id);
    }

    fn script(fixture: &mut Fixture, card: u32, held: &'static Card) {
        fixture.scripts = fixture.scripts.clone().with_script(card, held);
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn answer(ctx: &mut Ctx, unit: u32) {
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        ctx.blob.close_prompt();
        choose(ctx, unit).unwrap();
    }

    #[test]
    fn damage_is_ordered_tank_first_backline_last_and_a_unit_with_both_is_always_a_choice() {
        let mut fixture = arena();
        for id in [A, B, C, D] {
            put(&mut fixture, id, fixtures::BF1, 1, 3);
        }
        fixture.resolve();
        script(&mut fixture, A, &TANK);
        script(&mut fixture, B, &BACKLINE);
        script(&mut fixture, D, &BOTH);
        let ctx = fixture.ctx();
        assert_eq!(
            ordered(&ctx, &[A, B, C, D], false),
            [A, D],
            "the tank goes first and the tank/backline unit may fill either duty"
        );
        assert_eq!(
            ordered(&ctx, &[B, C, D], false),
            [C, D],
            "with no plain tank left the plain unit comes before the backline one"
        );
        assert_eq!(
            ordered(&ctx, &[B, C, D], true),
            [C],
            "743.1.d.7 · once a plain unit has taken damage the tank/backline unit is neither first nor last"
        );
        assert_eq!(
            ordered(&ctx, &[B, D], false),
            [B, D],
            "a backline unit is a candidate once nothing else remains"
        );
        assert_eq!(ordered(&ctx, &[B], false), [B]);
        assert_eq!(ordered(&ctx, &[D], false), [D]);
        assert_eq!(
            ordered(&ctx, &[D], true),
            [D],
            "the last unassigned unit still fulfils backline"
        );
        assert_eq!(ordered(&ctx, &[], false), Vec::<u32>::new());
        assert_eq!(lethal(&ctx, A), 3);
    }

    #[test]
    fn a_tank_and_backline_unit_is_offered_first_or_last_but_never_in_between() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 20);
        for id in [A, B, C, D, E] {
            put(&mut fixture, id, fixtures::BF1, 1, 2);
        }
        fixture.resolve();
        script(&mut fixture, A, &TANK);
        script(&mut fixture, D, &BOTH);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            candidates(&ctx),
            [A, D],
            "741.1.b · a tank or the both-unit"
        );
        answer(&mut ctx, A);
        assert_eq!(
            candidates(&ctx),
            [B, C, D, E],
            "no tank duty is outstanding and nothing plain has taken damage yet"
        );
        answer(&mut ctx, B);
        assert_eq!(
            candidates(&ctx),
            [C, E],
            "443.1.d.7 · the both-unit can be neither first nor last from here"
        );
        answer(&mut ctx, C);
        assert!(ctx.blob.showdown.is_none());
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Counter {
                    target: Target::Card(card),
                    counter,
                    delta,
                } if *card == D && *counter == COUNTER_DAMAGE && *delta == 12
            )),
            "the last unassigned unit fulfils backline and absorbs the remainder"
        );
    }

    #[test]
    fn five_damage_across_four_three_might_units_is_lethal_three_then_the_remaining_two() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 5);
        for id in [A, B, C, D] {
            put(&mut fixture, id, fixtures::BF1, 1, 3);
        }
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            prompts::offered(&ctx)
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            [
                "{card 94} (lethal 3)",
                "{card 95} (lethal 3)",
                "{card 96} (lethal 3)",
                "{card 97} (lethal 3)"
            ]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Assign),
            "assign 5 damage: who takes lethal next?"
        );
        assert_eq!(assigner(&ctx), Some((0, 5)));
        answer(&mut ctx, B);
        assert_eq!(assigner(&ctx), Some((0, 2)));
        assert_eq!(candidates(&ctx), [A, C, D], "95 already has its lethal");
        answer(&mut ctx, D);
        assert!(ctx.blob.showdown.is_none());
        let dealt: Vec<(u32, i32)> = ctx
            .effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(card),
                    counter,
                    delta,
                } if *counter == COUNTER_DAMAGE && *delta > 0 => Some((*card, *delta)),
                _ => None,
            })
            .collect();
        assert_eq!(
            dealt,
            [(B, 3), (D, 2), (ATTACKER, 12)],
            "lethal in full to one, the remainder to a second, nothing split"
        );
        assert_eq!(
            ctx.table.card(B).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.table.card(D).and_then(|card| card.zone),
            Some(fixtures::BF1),
            "two damage is not lethal on three might"
        );
        assert_eq!(damage_of(&ctx, D), 0, "2c heals the survivor");
        assert_eq!(
            ctx.table.card(ATTACKER).and_then(|card| card.zone),
            Some(fixtures::TRASH),
            "twelve defending might is more than enough"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert!(!ctx.blob.scored(fixtures::BF1, 1));
    }

    #[test]
    fn a_stunned_unit_contributes_no_might_but_still_needs_its_full_lethal() {
        let mut fixture = arena();
        put(&mut fixture, A, fixtures::BF1, 1, 5);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(might_sum(&ctx, &[A]), 5);
        assert_eq!(lethal(&ctx, A), 5);
        assert!(ctx.stun(A));
        assert_eq!(might_sum(&ctx, &[A]), 0, "410.1.b");
        assert_eq!(lethal(&ctx, A), 5, "410.1.c");
        assert!(
            !ctx.stun(A),
            "410.1.a.1 · a stunned unit cannot be stunned again"
        );
        ctx.damage(A, 2, Cause::Combat);
        assert_eq!(lethal(&ctx, A), 3, "damage already marked counts");
    }

    #[test]
    fn attackers_are_recalled_and_a_surviving_defender_re_establishes_without_scoring() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BASE, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 5);
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.card_state_mut(A).set(FLAG_STUNNED, true);
        let action = fixtures::move_action(ATTACKER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            ATTACKER,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        assert!(ctx.is_attacker(ATTACKER) && ctx.is_defender(A));
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(damage_of(&ctx, A), 0, "three damage was healed at 2c");
        assert_eq!(
            damage_of(&ctx, ATTACKER),
            0,
            "a stunned defender deals none"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: ATTACKER,
            zone: fixtures::BASE,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert!(
            !ctx.blob.scored(fixtures::BF1, 1),
            "re-establishing is not a conquer"
        );
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Counter { counter, .. } if *counter == COUNTER_POINTS
        )));
        assert!(
            !ctx.is_attacker(ATTACKER) && !ctx.is_defender(A),
            "2e clears the designations"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} keeps {zone 9}"));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 0 might"));
    }

    #[test]
    fn the_sole_survivor_conquers_and_the_dead_are_trashed_or_despawned() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 1);
        token(&mut fixture, B, fixtures::BF1, 1, 1);
        fixture.resolve();
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        answer(&mut ctx, A);
        assert!(ctx.blob.showdown.is_none());
        assert!(ctx.effects.contains(&Effect::Move {
            card: A,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.effects.contains(&Effect::Despawn { card: B }));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.blob.scored(fixtures::BF1, 0));
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)));
        assert_eq!(damage_of(&ctx, ATTACKER), 0, "2c heals the survivor");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} conquers {zone 9}"));
    }

    #[test]
    fn a_seat_leaving_during_the_damage_step_takes_its_prompt_with_it() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 5);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 3);
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(prompts::offered(&ctx).len(), 2);
        crate::engine::leave(&mut ctx, 1);
        assert!(ctx.blob.showdown.is_none());
        assert!(
            ctx.blob.prompt.is_none(),
            "the assign prompt cannot outlive its combat"
        );
        assert_eq!(ctx.blob.why, None);
        assert!(prompts::next_auto(&mut ctx).is_none());
        assert_eq!(crate::engine::phases::end_turn(&mut ctx), Ok(()));
    }

    #[test]
    fn an_effect_move_stages_a_contest_and_the_combat_designates_both_sides() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BASE, 0, 2);
        put(&mut fixture, A, fixtures::BF1, 1, 2);
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                ATTACKER,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card,
                cause: MoveCause::Effect,
                ..
            } if *card == ATTACKER
        )));
        cleanup::run(&mut ctx, None);
        triggers::collect(&mut ctx);
        showdown::open_next(&mut ctx);
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the contest stages a combat");
        assert!(showdown.combat);
        assert_eq!((showdown.attacker, showdown.defender), (0, 1));
        assert_eq!(attackers(&ctx, fixtures::BF1), [ATTACKER]);
        assert_eq!(defenders(&ctx, fixtures::BF1), [A]);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Attacks { card } if *card == ATTACKER)));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Defends { card } if *card == A)));
        assert!(
            !showdown.initial_chain,
            "no card in the pool has an attack or defend trigger yet"
        );
        clear_designations(&mut ctx);
        assert!(!ctx.in_combat(ATTACKER) && !ctx.in_combat(A));
    }

    fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), COUNTER_XP)
            .unwrap_or(0)
    }

    fn opened(fixture: &mut Fixture) -> Ctx<'_> {
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        let showdown = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(showdown.combat);
        assert_eq!((showdown.attacker, showdown.defender), (0, 1));
        ctx
    }

    fn legend(fixture: &mut Fixture, id: u32, seat: u8, held: &'static Card) {
        fixture.table.cards.push(fixtures::card(
            id,
            fixtures::LEGEND,
            seat,
            held.name,
            "Legend",
        ));
        script(fixture, id, held);
    }

    #[test]
    fn the_result_is_the_designated_seat_left_alone_at_the_battlefield() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 2);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            None,
            "466.3.d · both players have units present"
        );
        assert_eq!(result(&ctx, fixtures::BF1, 0, 0), None);
        ctx.damage(A, 2, Cause::Combat);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [A]);
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            Some(CombatResult {
                zone: fixtures::BF1,
                winner: 0,
                loser: 1
            }),
            "466.3.a · the attacker is the only player with units remaining"
        );
        assert_eq!(
            result(&ctx, fixtures::BF2, 0, 1),
            None,
            "nobody is designated at another battlefield"
        );
        ctx.damage(ATTACKER, 3, Cause::Combat);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [ATTACKER]);
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            None,
            "466.3.d · neither player has units present"
        );
    }

    #[test]
    fn a_defender_whose_attackers_died_wins_and_recalled_attackers_leave_no_result() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 5);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        ctx.damage(ATTACKER, 3, Cause::Combat);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [ATTACKER]);
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            Some(CombatResult {
                zone: fixtures::BF1,
                winner: 1,
                loser: 0
            }),
            "466.3.b · the attacker is the only player without units remaining"
        );
        let mut both = arena();
        put(&mut both, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut both, A, fixtures::BF1, 1, 5);
        both.resolve();
        let mut ctx = opened(&mut both);
        ctx.recall(ATTACKER, false);
        assert_eq!(
            ctx.location(ATTACKER),
            Some(Location::Base(0)),
            "3d · attackers present are recalled while defenders remain"
        );
        assert!(recalled(&ctx, fixtures::BF1));
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            None,
            "466.3.d · units were recalled during step 3d, read before the designations clear"
        );
        clear_designations(&mut ctx);
        assert!(!recalled(&ctx, fixtures::BF1));
    }

    #[test]
    fn a_lone_attacker_moved_away_during_the_showdown_hands_the_defender_the_win_without_damage() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 2);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert_eq!(
            ctx.move_unit(ATTACKER, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        assert!(
            ctx.is_attacker(ATTACKER),
            "the designation outlives the move until the next cleanup"
        );
        cleanup::run(&mut ctx, None);
        assert!(
            !ctx.is_attacker(ATTACKER),
            "464.2.c.3.a · the cleanup refreshes the open combat's designations"
        );
        assert!(ctx.blob.showdown.is_some(), "the showdown is still open");
        assert!(!recalled(&ctx, fixtures::BF1), "a Charm is not a recall");
        assert_eq!(
            result(&ctx, fixtures::BF1, 0, 1),
            Some(CombatResult {
                zone: fixtures::BF1,
                winner: 1,
                loser: 0
            })
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "combat at {zone 9} · no damage: only one side remains"));
        assert_eq!(damage_of(&ctx, A), 0);
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert!(!ctx.blob.scored(fixtures::BF1, 1));
    }

    #[test]
    fn a_combat_won_trigger_gains_xp_for_the_winning_seat_and_scores_only_through_the_conquer() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 5);
        fixture.resolve();
        legend(&mut fixture, 200, 1, &VOIDREAVER);
        legend(&mut fixture, 201, 0, &VOIDREAVER);
        let mut ctx = opened(&mut fixture);
        ctx.damage(ATTACKER, 3, Cause::Combat);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [ATTACKER]);
        let won = result(&ctx, fixtures::BF1, 0, 1).unwrap();
        ctx.raise(Event::CombatWon {
            zone: won.zone,
            seat: won.winner,
        });
        ctx.raise(Event::CombatLost {
            zone: won.zone,
            seat: won.loser,
        });
        clear_designations(&mut ctx);
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Kept(1)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "only the winner's legend triggers");
        assert_eq!(ctx.blob.chain[0].controller, 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(xp_of(&ctx, 1), 1);
        assert_eq!(xp_of(&ctx, 0), 0);
        assert_eq!(ctx.points(1), 0, "a defender who wins does not score");
        let mut attackers = arena();
        put(&mut attackers, ATTACKER, fixtures::BF1, 0, 5);
        put(&mut attackers, A, fixtures::BF1, 1, 2);
        attackers.resolve();
        legend(&mut attackers, 200, 1, &VOIDREAVER);
        legend(&mut attackers, 201, 0, &VOIDREAVER);
        let mut ctx = opened(&mut attackers);
        ctx.damage(A, 2, Cause::Combat);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [A]);
        let won = result(&ctx, fixtures::BF1, 0, 1).unwrap();
        assert_eq!((won.winner, won.loser), (0, 1));
        ctx.raise(Event::CombatWon {
            zone: won.zone,
            seat: won.winner,
        });
        clear_designations(&mut ctx);
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(xp_of(&ctx, 0), 1);
        assert_eq!(ctx.points(0), 1, "the attacker who wins conquers");
        assert_eq!(xp_of(&ctx, 1), 0);
    }

    #[test]
    fn fiora_doubles_her_might_one_on_one_until_the_combat_ends_and_not_against_two() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 2);
        fixture.resolve();
        script(&mut fixture, ATTACKER, &FIORA);
        let mut ctx = opened(&mut fixture);
        assert!(
            ctx.blob.showdown.as_ref().unwrap().initial_chain,
            "her attack trigger forms the initial chain"
        );
        assert_eq!(ctx.current_might(ATTACKER), 3, "nothing until it resolves");
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(ATTACKER), 6, "740.2.b · one on one");
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 6 might vs defenders 2 might"));
        assert_eq!(
            ctx.table.card(A).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.current_might(ATTACKER),
            3,
            "466.7.c · this combat effects expire as it ends"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        let mut two = arena();
        put(&mut two, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut two, A, fixtures::BF1, 1, 2);
        put(&mut two, B, fixtures::BF1, 1, 2);
        two.resolve();
        script(&mut two, ATTACKER, &FIORA);
        let mut ctx = opened(&mut two);
        assert!(
            !ctx.blob.showdown.as_ref().unwrap().initial_chain,
            "two enemies · no trigger"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(ATTACKER), 3);
    }

    #[test]
    fn an_ambusher_is_designated_at_the_next_cleanup_and_its_trigger_reads_the_board_then() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 6);
        put(&mut fixture, B, fixtures::BASE, 0, 4);
        put(&mut fixture, C, fixtures::BASE, 1, 2);
        fixture.resolve();
        script(&mut fixture, B, &HORROR);
        let mut ctx = opened(&mut fixture);
        assert!(!ctx.blob.showdown.as_ref().unwrap().initial_chain);
        assert_eq!(
            ctx.move_unit(B, Location::Battlefield(fixtures::BF1), MoveCause::Effect),
            Moved::Moved
        );
        assert!(!ctx.in_combat(B), "not until the cleanup");
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.is_attacker(B),
            "464.2.c.3.a · the cleanup after the arrival designates it"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Attacks { card } if *card == B)));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "one enemy is alone here");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(B), 6);
        assert_eq!(xp_of(&ctx, 0), 2);
        assert_eq!(
            ctx.move_unit(C, Location::Battlefield(fixtures::BF1), MoveCause::Effect),
            Moved::Moved
        );
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_defender(C));
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "383.4.e.2.b · the designation was gained once; a second enemy changes nothing"
        );
        assert_eq!(xp_of(&ctx, 0), 2);
        let mut crowded = arena();
        put(&mut crowded, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut crowded, A, fixtures::BF1, 1, 6);
        put(&mut crowded, C, fixtures::BF1, 1, 2);
        put(&mut crowded, B, fixtures::BASE, 0, 4);
        crowded.resolve();
        script(&mut crowded, B, &HORROR);
        let mut ctx = opened(&mut crowded);
        ctx.move_unit(B, Location::Battlefield(fixtures::BF1), MoveCause::Effect);
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_attacker(B));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "it faced two");
        assert_eq!(ctx.current_might(B), 4);
        assert_eq!(xp_of(&ctx, 0), 0);
    }

    #[test]
    fn a_lone_defender_under_the_bladesman_deals_two_more_until_a_friend_arrives() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 4);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        fixture.resolve();
        legend(&mut fixture, 200, 1, &BLADESMAN);
        let mut ctx = opened(&mut fixture);
        assert_eq!(ctx.current_might(A), 5, "defending alone");
        assert_eq!(ctx.current_might(ATTACKER), 4, "not a friendly unit");
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 4 might vs defenders 5 might"));
        assert_eq!(
            ctx.table.card(ATTACKER).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.table.card(A).and_then(|card| card.zone),
            Some(fixtures::BF1),
            "four damage is not lethal on five"
        );
        assert_eq!(
            ctx.current_might(A),
            3,
            "466.7.a · no designation, no bonus"
        );
        let mut joined = arena();
        put(&mut joined, ATTACKER, fixtures::BF1, 0, 4);
        put(&mut joined, A, fixtures::BF1, 1, 3);
        put(&mut joined, B, fixtures::BASE, 1, 1);
        joined.resolve();
        legend(&mut joined, 200, 1, &BLADESMAN);
        let mut ctx = opened(&mut joined);
        assert_eq!(ctx.current_might(A), 5);
        ctx.move_unit(B, Location::Battlefield(fixtures::BF1), MoveCause::Effect);
        assert_eq!(ctx.current_might(A), 3, "no longer alone");
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_defender(B));
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 4 might vs defenders 4 might"));
        let mut at_base = arena();
        put(&mut at_base, A, fixtures::BASE, 1, 3);
        at_base.resolve();
        legend(&mut at_base, 200, 1, &BLADESMAN);
        let ctx = at_base.ctx();
        assert_eq!(ctx.current_might(A), 3, "nothing at base");
    }

    #[test]
    fn the_waste_drops_a_lone_defender_to_nothing_and_one_damage_kills_it() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 1);
        put(&mut fixture, A, fixtures::BF1, 1, 2);
        fixture.resolve();
        script(&mut fixture, fixtures::GROUNDS, &WASTE);
        let mut ctx = opened(&mut fixture);
        assert_eq!(ctx.current_might(A), 0);
        assert_eq!(lethal(&ctx, A), 1, "lethal is never less than one");
        assert_eq!(
            ctx.current_might(ATTACKER),
            1,
            "the attacker is not defending"
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 1 might vs defenders 0 might"));
        assert_eq!(
            ctx.table.card(A).and_then(|card| card.zone),
            Some(fixtures::TRASH),
            "one damage on zero might is lethal"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        let mut elsewhere = arena();
        put(&mut elsewhere, ATTACKER, fixtures::BF2, 0, 1);
        put(&mut elsewhere, A, fixtures::BF2, 1, 2);
        elsewhere.resolve();
        script(&mut elsewhere, fixtures::GROUNDS, &WASTE);
        elsewhere.blob.set_holder(fixtures::BF2, Some(1));
        elsewhere.blob.set_contested(fixtures::BF2, Some(0));
        let mut ctx = elsewhere.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_defender(A));
        assert_eq!(ctx.current_might(A), 2, "the Waste only reads units here");
    }

    #[test]
    fn a_doubled_unit_is_assigned_the_least_that_doubles_to_lethal() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 5);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 4);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert!(ctx.multiply_damage_this_turn(A, 2));
        assert!(ctx.multiply_damage_this_turn(B, 2));
        assert_eq!(lethal(&ctx, A), 2);
        assert_eq!(lethal(&ctx, B), 2);
        assert!(!exempt(&ctx, A));
        ctx.prevent_on(A, DamageSource::Combat, Amount::N(2), Expiry::CombatEnd);
        assert_eq!(lethal(&ctx, A), 4);
        ctx.blob.preventions.clear();
        assert!(ctx.multiply_damage_this_turn(B, 2));
        assert_eq!(lethal(&ctx, B), 1);
        ctx.prevent(DamageSource::Combat, Amount::All, Expiry::CombatEnd);
        assert!(exempt(&ctx, A));
        assert_eq!(lethal(&ctx, A), 3);
    }

    #[test]
    fn an_assigner_whose_damage_is_lethal_assigns_one_per_unit_and_the_hit_units_die() {
        const DRAGON: u32 = 96;
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 9);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 3);
        fixture.table.cards.push(fixtures::unit(
            DRAGON,
            fixtures::BASE,
            0,
            "Elder Dragon",
            10,
        ));
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert_eq!(lethal(&ctx, A), 3);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(assigner(&ctx), Some((0, 9)));
        assert_eq!(lethal(&ctx, A), 1);
        assert_eq!(lethal(&ctx, ATTACKER), 9);
        assert_eq!(
            prompts::offered(&ctx)
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 94} (lethal 1)", "{card 95} (lethal 1)"]
        );
        choose(&mut ctx, A).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(!ctx.on_board(A));
        assert!(!ctx.on_board(B));
        assert!(ctx.on_board(ATTACKER));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 1, source: Cause::Combat } if *card == A
        )));
    }

    #[test]
    fn elder_dragon_lethal_is_used_when_excess_is_recorded() {
        const DRAGON: u32 = 96;
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 9);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 5);
        fixture.table.cards.push(fixtures::unit(
            DRAGON,
            fixtures::BASE,
            0,
            "Elder Dragon",
            10,
        ));
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        choose(&mut ctx, A).unwrap();
        assert_eq!(ctx.excess_damage_in_attack(0, fixtures::BF1), Some(7));
    }

    #[test]
    fn finite_prevention_and_lotus_multiplier_are_included_in_excess_threshold() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 9);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 3);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert!(ctx.multiply_damage_this_turn(B, 2));
        ctx.prevent_on(B, DamageSource::Combat, Amount::N(1), Expiry::CombatEnd);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        choose(&mut ctx, A).unwrap();
        assert_eq!(ctx.excess_damage_in_attack(0, fixtures::BF1), Some(3));
    }

    #[test]
    fn both_combat_assigners_keep_distinct_excess_records() {
        const OTHER_ATTACKER: u32 = 97;
        const OTHER_DRAGON: u32 = 98;
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, OTHER_ATTACKER, fixtures::BF1, 0, 3);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 3);
        fixture
            .table
            .cards
            .push(fixtures::unit(96, fixtures::BASE, 0, "Elder Dragon", 10));
        fixture.table.cards.push(fixtures::unit(
            OTHER_DRAGON,
            fixtures::BASE,
            1,
            "Elder Dragon",
            10,
        ));
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        choose(&mut ctx, A).unwrap();
        choose(&mut ctx, ATTACKER).unwrap();
        assert_eq!(ctx.excess_damage_in_attack(0, fixtures::BF1), Some(4));
        assert_eq!(ctx.excess_damage_in_attack(1, fixtures::BF1), Some(4));
    }

    #[test]
    fn wholly_prevented_combat_damage_exempts_a_unit_and_a_numeric_prevention_raises_its_lethal() {
        let mut fixture = arena();
        put(&mut fixture, ATTACKER, fixtures::BF1, 0, 5);
        put(&mut fixture, A, fixtures::BF1, 1, 3);
        put(&mut fixture, B, fixtures::BF1, 1, 3);
        fixture.resolve();
        let mut ctx = opened(&mut fixture);
        assert_eq!(lethal(&ctx, A), 3);
        assert!(!exempt(&ctx, A));
        ctx.prevent(DamageSource::Combat, Amount::N(2), Expiry::CombatEnd);
        assert_eq!(
            lethal(&ctx, A),
            5,
            "465.2.c.5 · the assignment carries the prevented points"
        );
        assert!(!exempt(&ctx, A), "it can still be dealt damage");
        ctx.blob.preventions.clear();
        ctx.prevent(DamageSource::Combat, Amount::N(3), Expiry::CombatEnd);
        assert!(!exempt(&ctx, A), "a finite shield still needs assignment");
        assert_eq!(lethal(&ctx, A), 6, "the full finite shield is assigned");
        ctx.blob.preventions.clear();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert_eq!(
            lethal(&ctx, A),
            3,
            "a spell prevention does not read combat"
        );
        assert!(!exempt(&ctx, A));
        ctx.blob.preventions.clear();
        ctx.prevent(DamageSource::Combat, Amount::All, Expiry::CombatEnd);
        assert!(exempt(&ctx, A) && exempt(&ctx, B) && exempt(&ctx, ATTACKER));
        assert_eq!(lethal(&ctx, A), 3, "465.2.c.10 · no amount is lethal");
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.showdown.is_none(),
            "nobody is a candidate, so nothing is asked"
        );
        assert!(ctx.blob.log.iter().any(|line| line == "no damage is dealt"));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        for unit in [ATTACKER, A, B] {
            assert_eq!(damage_of(&ctx, unit), 0);
        }
        assert_eq!(
            ctx.location(ATTACKER),
            Some(Location::Base(0)),
            "3d · both sides remain, so the attackers are recalled"
        );
        assert!(
            ctx.blob.preventions.is_empty(),
            "the combat-end entry is dropped as the combat ends"
        );
        let mut half = arena();
        put(&mut half, ATTACKER, fixtures::BF1, 0, 5);
        put(&mut half, A, fixtures::BF1, 1, 3);
        put(&mut half, B, fixtures::BF1, 1, 3);
        half.resolve();
        let mut ctx = opened(&mut half);
        ctx.prevent(DamageSource::Combat, Amount::N(1), Expiry::CombatEnd);
        assert_eq!(
            prompts::offered(&ctx).len(),
            0,
            "the showdown is still open"
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(
            prompts::offered(&ctx)
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 94} (lethal 4)", "{card 95} (lethal 4)"]
        );
        assert_eq!(assigner(&ctx), Some((0, 5)));
        answer(&mut ctx, A);
        assert!(
            ctx.blob.showdown.is_none(),
            "the remainder goes to the last unit and the defender has one candidate"
        );
    }
}

use super::{
    Ability, Adder, Candidates, Card, Condition, Cost, Domain, ExtraCost, Filter, Flow, Grant,
    Item, Keyword, LevelGate, ModeSpec, ModeTiming, NameKind, Once, Power, Rel, Replacement,
    ReplacementApplies, ReplacementRun, Run, SelfCost, Stage, Static, TargetKind, TargetSpec,
    Timing, Trigger, Usable, Viable, Where, Who, KIND_BATTLEFIELD, KIND_GEAR, KIND_LEGEND,
    KIND_SPELL, KIND_UNIT,
};
use crate::engine::combat;
use crate::engine::ctx::{is_unit_face, Cause, CounterDest, Ctx, Event, Killed};
use crate::engine::hide;
use crate::engine::targets;
use crate::engine::{attach, chain, cost, discard, march, pay};
use crate::state::{
    Ask, CardState, Expiry, Noted, PlayLock, Pool, Pooled, TargetRef, When, FLAG_REVEALING,
};
use agni_plugin_sdk::decide::{Effect, TOP};

pub use crate::engine::attach::Attached;
pub use crate::engine::ctx::{Location, MoveCause, Moved, Token};
pub use crate::engine::march::Swapped;
pub use crate::engine::play::LimitedPlay;
pub use crate::state::Price;
pub use crate::state::{Promise, PromiseEffect, PromiseKind};

pub const fn card(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    Card {
        name,
        keywords,
        abilities,
        statics: &[],
        replacement: None,
        additional: None,
        names: None,
        kind: None,
        adds: None,
    }
}

pub const fn adding(card: Card, adds: Adder) -> Card {
    Card {
        adds: Some(adds),
        ..card
    }
}

const fn of_kind(card: Card, kind: &'static str) -> Card {
    Card {
        kind: Some(kind),
        ..card
    }
}

pub const fn with_additional(card: Card, cost: Cost) -> Card {
    Card {
        additional: Some(cost),
        ..card
    }
}

pub const fn naming(card: Card, kind: NameKind) -> Card {
    Card {
        names: Some(kind),
        ..card
    }
}

pub const fn spell(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    of_kind(card(name, keywords, abilities), KIND_SPELL)
}

pub const fn unit(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    of_kind(card(name, keywords, abilities), KIND_UNIT)
}

pub const fn gear(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    of_kind(card(name, keywords, abilities), KIND_GEAR)
}

pub const fn battlefield(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    of_kind(card(name, keywords, abilities), KIND_BATTLEFIELD)
}

pub const fn legend(
    name: &'static str,
    keywords: &'static [Keyword],
    abilities: &'static [Ability],
) -> Card {
    of_kind(card(name, keywords, abilities), KIND_LEGEND)
}

pub const fn with_replacement(card: Card, replacement: Replacement) -> Card {
    Card {
        replacement: Some(replacement),
        ..card
    }
}

pub const fn replaces(applies: ReplacementApplies, run: ReplacementRun) -> Replacement {
    Replacement { applies, run }
}

pub const fn with_statics(card: Card, statics: &'static [Static]) -> Card {
    Card { statics, ..card }
}

pub const fn triggered(trigger: Trigger, targets: &'static [TargetSpec], run: Run) -> Ability {
    Ability {
        trigger,
        optional: false,
        cost: None,
        extra: None,
        condition: None,
        targets,
        modes: &[],
        mode_timing: ModeTiming::AtPlay,
        run,
        candidates: None,
        viable: None,
        question: None,
        label: None,
        self_cost: SelfCost::Auto,
        once: Once::Never,
        xp: 0,
        burn: 0,
        usable: None,
    }
}

pub const fn spending_xp(ability: Ability, xp: u8) -> Ability {
    Ability { xp, ..ability }
}

pub const fn burning(ability: Ability, burn: u8) -> Ability {
    Ability { burn, ..ability }
}

pub const fn usable_if(ability: Ability, usable: Usable) -> Ability {
    Ability {
        usable: Some(usable),
        ..ability
    }
}

pub const fn once_per_seat_each_turn(ability: Ability) -> Ability {
    Ability {
        once: Once::PerSeatPerTurn,
        ..ability
    }
}

pub const fn play(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Play, targets, run)
}

pub const fn optional(ability: Ability) -> Ability {
    Ability {
        optional: true,
        ..ability
    }
}

pub const fn when(ability: Ability, condition: Condition) -> Ability {
    Ability {
        condition: Some(condition),
        ..ability
    }
}

pub const fn with_candidates(ability: Ability, candidates: Candidates) -> Ability {
    Ability {
        candidates: Some(candidates),
        ..ability
    }
}

pub const fn viable_if(ability: Ability, viable: Viable) -> Ability {
    Ability {
        viable: Some(viable),
        ..ability
    }
}

pub const fn asking(ability: Ability, question: &'static str) -> Ability {
    Ability {
        question: Some(question),
        ..ability
    }
}

pub const fn mode(label: &'static str, targets: &'static [TargetSpec], run: Run) -> ModeSpec {
    ModeSpec {
        label,
        targets,
        run,
    }
}

pub const fn choosing(ability: Ability, modes: &'static [ModeSpec]) -> Ability {
    Ability {
        modes,
        mode_timing: ModeTiming::AtResume,
        ..ability
    }
}

pub const fn modal(modes: &'static [ModeSpec]) -> Ability {
    Ability {
        modes,
        mode_timing: ModeTiming::AtPlay,
        ..play(&[], run_mode)
    }
}

pub fn chosen_mode(item: &Item) -> Option<u8> {
    item.mode()
}

pub fn mode_spec(ctx: &Ctx, item: &Item) -> Option<&'static ModeSpec> {
    let ability = targets::ability_of(ctx, item)?;
    ability.modes.get(usize::from(chosen_mode(item)?))
}

pub fn run_mode(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match mode_spec(ctx, item) {
        Some(mode) => (mode.run)(ctx, item, stage),
        None => {
            ctx.narrate(format!(
                "{{card {}}} has no mode chosen · nothing happens",
                item.kind.source()
            ));
            Flow::Done
        }
    }
}

pub const fn activated(
    timing: Timing,
    cost: Cost,
    targets: &'static [TargetSpec],
    run: Run,
) -> Ability {
    Ability {
        cost: Some(cost),
        ..triggered(Trigger::Activated(timing), targets, run)
    }
}

pub const fn costing(ability: Ability, extra: ExtraCost) -> Ability {
    Ability {
        extra: Some(extra),
        ..ability
    }
}

pub const fn named(ability: Ability, label: &'static str) -> Ability {
    Ability {
        label: Some(label),
        ..ability
    }
}

pub const fn once_each_turn(ability: Ability) -> Ability {
    Ability {
        once: Once::PerTurn,
        ..ability
    }
}

pub const fn paying_with(ability: Ability, self_cost: SelfCost) -> Ability {
    Ability {
        self_cost,
        ..ability
    }
}

pub const fn on_play_from_facedown(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::PlayFromFacedown, targets, run)
}

pub const fn reaction(cost: Cost, targets: &'static [TargetSpec], run: Run) -> Ability {
    activated(Timing::Reaction, cost, targets, run)
}

pub const fn on_move(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(
        Trigger::Move {
            of: Who::Me,
            to: Where::Any,
        },
        targets,
        run,
    )
}

pub const fn on_move_to_battlefield(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(
        Trigger::Move {
            of: Who::Me,
            to: Where::Battlefield,
        },
        targets,
        run,
    )
}

pub const fn on_attack(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Attacks(Who::Me), targets, run)
}

pub const fn on_defend(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Defends(Who::Me), targets, run)
}

pub const fn deathknell(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Death, targets, run)
}

pub const fn on_opponent_plays_unit(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::OpponentPlaysUnit, targets, run)
}

pub const fn on_friendly_unit_dies(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::UnitDies(Who::Friendly), targets, run)
}

pub const fn on_enemy_unit_dies(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::UnitDies(Who::Enemy), targets, run)
}

pub const fn on_chosen(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Chosen, targets, run)
}

pub const fn on_readied(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Readied(Who::Me), targets, run)
}

pub const fn on_friendly_unit_readied(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Readied(Who::Friendly), targets, run)
}

pub const fn on_friendly_unit_chosen(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::ChosenFriendly(Who::Friendly), targets, run)
}

pub const fn on_any_unit_chosen(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::ChosenFriendly(Who::Any), targets, run)
}

pub const fn on_conquer(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Conquer(Who::You), targets, run)
}

pub const fn on_conquer_me(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Conquer(Who::Me), targets, run)
}

pub const fn on_hold_me(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Hold(Who::Me), targets, run)
}

pub const fn on_empowered(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Empowered, targets, run)
}

pub const fn on_you_empower(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::YouEmpower, targets, run)
}

pub const fn on_you_banish(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Banished(Who::You), targets, run)
}

pub const fn on_combat_won(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::CombatWon(Who::You), targets, run)
}

pub const fn on_activated(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::Activation { of: Who::You }, targets, run)
}

pub const fn on_you_play_card(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::YouPlayCard, targets, run)
}

pub const fn on_unit_played_here(targets: &'static [TargetSpec], run: Run) -> Ability {
    triggered(Trigger::UnitPlayedHere, targets, run)
}

fn empower_run(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.empower_by(item.kind.source(), item.controller);
    Flow::Done
}

fn not_empowered(ctx: &Ctx, source: super::Source) -> bool {
    !ctx.is_empowered(source.card)
}

pub fn empowered(ctx: &Ctx, source: super::Source) -> bool {
    ctx.is_empowered(source.card)
}

pub fn when_empowered(ctx: &Ctx, _: &Event, source: super::Source) -> bool {
    ctx.is_empowered(source.card)
}

pub const fn empower(cost: Cost) -> Ability {
    usable_if(
        named(
            paying_with(
                activated(Timing::Sorcery, cost, &[], empower_run),
                SelfCost::Free,
            ),
            "empower",
        ),
        not_empowered,
    )
}

pub const WEAPONMASTER_TARGET: TargetSpec = target(
    FRIENDLY_EQUIPMENT,
    0,
    1,
    TargetKind::Card,
    "an Equipment you control to attach",
);

pub fn weaponmaster_cost(ctx: &Ctx, gear: u32) -> Option<cost::Cost> {
    let printed = ctx.script(gear)?.equip_cost()?;
    let domains = ctx.domains_of(gear);
    let mut total = cost::of_script(&printed, &domains);
    if let Some(rainbow) = total
        .power
        .iter()
        .position(|need| *need == cost::Need::Rainbow)
    {
        total.power.remove(rainbow);
    }
    Some(total)
}

fn weaponmaster_run(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(gear) = card_target(ctx, item, 0) else {
        return Flow::Done;
    };
    let Some(total) = weaponmaster_cost(ctx, gear) else {
        return Flow::Done;
    };
    let seat = item.controller;
    let Ok(plan) = pay::plan(ctx, seat, &total) else {
        ctx.narrate(format!(
            "{{card {gear}}} stays where it is · its Equip cost can't be paid"
        ));
        return Flow::Done;
    };
    pay::pay(ctx, seat, &plan);
    attach_gear(ctx, gear, me);
    Flow::Done
}

pub static WEAPONMASTER: Ability = named(
    optional(play(&[WEAPONMASTER_TARGET], weaponmaster_run)),
    "weaponmaster",
);

pub const PREDICT_RECYCLE: u8 = 1;
pub const PREDICT_QUESTION: &str = "the top card of your deck to recycle";

pub fn predicted_top(ctx: &Ctx, seat: u8) -> Option<u32> {
    let deck = ctx.zones.main_deck?;
    ctx.top_of(deck, seat, 1).first().copied()
}

pub fn top_of_deck(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    predicted_top(ctx, item.controller)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

pub fn predict(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PREDICT_RECYCLE {
        let top = predicted_top(ctx, seat);
        match ctx.picks().first().copied() {
            Some(card) if top == Some(card) => {
                ctx.recycle_to_bottom(card);
                ctx.narrate(format!(
                    "{{seat {seat}}} recycles the top card of their deck"
                ));
            }
            _ => ctx.narrate(format!("{{seat {seat}}} keeps the top card of their deck")),
        }
        return Flow::Done;
    }
    if ctx.peek_top(seat).is_none() {
        ctx.narrate(format!("{{seat {seat}}} has no card to predict"));
        return Flow::Done;
    }
    Flow::Ask(ctx.ask_resume(item, PREDICT_RECYCLE, 0, 1))
}

pub static VISION: Ability = named(
    asking(
        with_candidates(play(&[], predict), top_of_deck),
        PREDICT_QUESTION,
    ),
    "vision",
);

pub const fn exhausting_self(ability: Ability) -> Ability {
    paying_with(ability, SelfCost::Exhaust)
}

pub const fn disempowering_self(ability: Ability) -> Ability {
    paying_with(ability, SelfCost::Disempower)
}

pub const fn with_cost(ability: Ability, cost: Cost) -> Ability {
    Ability {
        cost: Some(cost),
        ..ability
    }
}

pub const RAINBOW: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow],
};

pub const ONE_ENERGY: Cost = Cost {
    energy: 1,
    power: &[],
};

pub const CHAOS: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Chaos)],
};

pub const EQUIP_TARGET: TargetSpec = a_card(FRIENDLY_UNIT, "a unit you control to equip");

fn equip_run(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        attach_gear(ctx, item.kind.source(), unit);
    }
    Flow::Done
}

pub const fn equip(cost: Cost) -> Ability {
    named(
        paying_with(
            activated(Timing::Sorcery, cost, &[EQUIP_TARGET], equip_run),
            SelfCost::Free,
        ),
        "equip",
    )
}

pub const fn while_attached(grants: &'static [Grant]) -> Static {
    Static::WhileAttached(grants)
}

pub const MIGHTY: i32 = 5;

pub const HIDDEN: &[Keyword] = &[Keyword::Hidden];

pub const UNIT: Filter = Filter::Unit;
pub const UNIT_WITH_THE_NAMED_TAG: Filter = Filter::And(&[Filter::Unit, Filter::NamedTag]);
pub const GEAR: Filter = Filter::Gear;
pub const ENEMY_GEAR: Filter = Filter::And(&[Filter::Gear, Filter::Enemy]);
pub const GEAR_COSTING_ONE_OR_LESS: Filter = Filter::And(&[Filter::Gear, Filter::EnergyAtMost(1)]);
pub const TEMPORARY_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Temporary]);
pub const FRIENDLY_TEMPORARY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Temporary]);
pub const FRIENDLY_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Friendly]);
pub const ENEMY_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Enemy]);
pub const UNIT_AT_BATTLEFIELD: Filter = Filter::And(&[Filter::Unit, Filter::AtBattlefield]);
pub const UNIT_HERE: Filter = Filter::And(&[Filter::Unit, Filter::Here]);
pub const AT_HIDING_BATTLEFIELD: Filter = Filter::Here;
pub const UNIT_AT_HIDING_BATTLEFIELD: Filter = Filter::And(&[Filter::Unit, Filter::Here]);
pub const UNIT_AT_ANOTHER_LOCATION: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Not(&Filter::Here)]);
pub const FRIENDLY_UNIT_HERE: Filter = Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Here]);
pub const ANOTHER_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::NotSame(0)]);
pub const ENEMY_UNIT_HERE: Filter = Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Here]);
pub const ATTACKING_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Attacker]);
pub const DEFENDING_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Defender]);
pub const UNIT_IN_COMBAT: Filter = Filter::And(&[Filter::Unit, Filter::InCombat]);
pub const FRIENDLY_UNIT_IN_COMBAT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::InCombat]);
pub const ENEMY_UNIT_IN_COMBAT: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::InCombat]);
pub const MOVABLE_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Movable]);
pub const MOVABLE_FRIENDLY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Movable]);
pub const MOVABLE_ENEMY_UNIT: Filter = Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Movable]);
pub const SPELL_ON_CHAIN: Filter = Filter::Spell;
pub const ENEMY_ITEM_CHOOSING_FRIENDLY: Filter = Filter::And(&[
    Filter::ItemOnChain,
    Filter::ItemControlledBy(Rel::Enemy),
    Filter::ItemTargetsFriendly,
]);
pub const FRIENDLY_UNIT_OR_GEAR: Filter =
    Filter::And(&[Filter::Or(&[Filter::Unit, Filter::Gear]), Filter::Friendly]);
pub const ENEMY_ITEM_CHOOSING_FRIENDLY_UNIT_OR_GEAR: Filter = Filter::And(&[
    Filter::ItemOnChain,
    Filter::ItemControlledBy(Rel::Enemy),
    Filter::ItemTargets(&FRIENDLY_UNIT_OR_GEAR),
]);
pub const ATTACHED_GEAR: Filter = Filter::And(&[Filter::Gear, Filter::Attached]);
pub const UNATTACHED_GEAR: Filter = Filter::And(&[Filter::Gear, Filter::Unattached]);
pub const FRIENDLY_UNATTACHED_GEAR: Filter =
    Filter::And(&[Filter::Gear, Filter::Friendly, Filter::Unattached]);
pub const UNIT_ELSEWHERE_THAN_FIRST: Filter =
    Filter::And(&[Filter::Unit, Filter::DifferentLocationFrom(0)]);
pub const FRIENDLY_UNIT_ELSEWHERE_THAN_FIRST: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::DifferentLocationFrom(0),
]);
pub const ANOTHER_UNIT_WITH_FIRST: Filter = Filter::And(&[
    Filter::Unit,
    Filter::NotSame(0),
    Filter::SameLocationAs(0),
    Filter::AtBattlefield,
]);
pub const IN_TRASH: Filter = Filter::InTrash;
pub const ELSEWHERE_THAN_ME: Filter = Filter::Not(&Filter::Here);
pub const ANOTHER_UNIT_THAN_ME: Filter = Filter::And(&[Filter::Unit, Filter::NotSelf]);
pub const ANOTHER_FRIENDLY_UNIT_THAN_ME: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::NotSelf]);
pub const FRIENDLY_EQUIPMENT: Filter =
    Filter::And(&[Filter::Gear, Filter::Friendly, Filter::Equipment]);
pub const FRIENDLY_UNIT_IN_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Friendly]);
pub const FRIENDLY_SPELL_IN_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_SPELL), Filter::InTrash, Filter::Friendly]);

pub const fn target(
    filter: Filter,
    min: u8,
    max: u8,
    kind: TargetKind,
    label: &'static str,
) -> TargetSpec {
    TargetSpec {
        filter,
        min,
        max,
        kind,
        label,
        min_at_level: None,
    }
}

pub const fn at_level(xp: u8, min: u8, spec: TargetSpec) -> TargetSpec {
    TargetSpec {
        min_at_level: Some(LevelGate { xp, min }),
        ..spec
    }
}

pub const fn a_card(filter: Filter, label: &'static str) -> TargetSpec {
    target(filter, 1, 1, TargetKind::Card, label)
}

pub const fn a_unit(label: &'static str) -> TargetSpec {
    a_card(UNIT, label)
}

pub const fn a_unit_with_the_named_tag(label: &'static str) -> TargetSpec {
    a_card(UNIT_WITH_THE_NAMED_TAG, label)
}

pub const fn a_friendly_unit(label: &'static str) -> TargetSpec {
    a_card(FRIENDLY_UNIT, label)
}

pub const fn an_enemy_unit(label: &'static str) -> TargetSpec {
    a_card(ENEMY_UNIT, label)
}

pub const fn a_unit_at_a_battlefield(label: &'static str) -> TargetSpec {
    a_card(UNIT_AT_BATTLEFIELD, label)
}

pub const fn another_unit(label: &'static str) -> TargetSpec {
    a_card(ANOTHER_UNIT, label)
}

pub const fn units_up_to(count: u8, label: &'static str) -> TargetSpec {
    target(UNIT, 0, count, TargetKind::Card, label)
}

pub const fn an_item(filter: Filter, label: &'static str) -> TargetSpec {
    target(filter, 1, 1, TargetKind::Item, label)
}

pub const fn a_spell(label: &'static str) -> TargetSpec {
    an_item(SPELL_ON_CHAIN, label)
}

pub const fn a_play_location(label: &'static str) -> TargetSpec {
    target(Filter::Any, 1, 1, TargetKind::Zone, label)
}

pub const fn a_battlefield(label: &'static str) -> TargetSpec {
    target(Filter::AtBattlefield, 1, 1, TargetKind::Zone, label)
}

pub const BATTLEFIELD_WITH_YOUR_UNITS: Filter =
    Filter::And(&[Filter::AtBattlefield, Filter::ZoneWithUnits(Rel::Friendly)]);

pub const fn a_battlefield_with_your_units(label: &'static str) -> TargetSpec {
    target(BATTLEFIELD_WITH_YOUR_UNITS, 1, 1, TargetKind::Zone, label)
}

pub fn this_turn(ctx: &Ctx) -> Expiry {
    Expiry::EndOfTurn(ctx.turn())
}

pub fn promise(ctx: &mut Ctx, seat: u8, kind: PromiseKind, effect: PromiseEffect, until: Expiry) {
    ctx.promise(seat, kind, effect, until);
}

pub fn promise_this_turn(ctx: &mut Ctx, seat: u8, kind: PromiseKind, effect: PromiseEffect) {
    let until = this_turn(ctx);
    ctx.promise(seat, kind, effect, until);
}

pub fn discount_of(energy: u8, rainbow: u8) -> PromiseEffect {
    PromiseEffect::Discount(Pool {
        energy,
        power: vec![Pooled::Rainbow; usize::from(rainbow)],
    })
}

pub fn might_this_turn(ctx: &mut Ctx, item: &Item, unit: u32, delta: i16, min: Option<i32>) {
    let until = this_turn(ctx);
    ctx.might(unit, delta, until, min, item.id);
}

pub fn draw(ctx: &mut Ctx, seat: u8, count: usize) -> usize {
    ctx.draw(seat, count)
}

pub fn forget_revealing(ctx: &mut Ctx, card: u32) {
    ctx.set_flag(card, FLAG_REVEALING, false);
    if ctx.state_of(card).is_some_and(CardState::is_default) {
        ctx.blob.drop_card_state(card);
    }
}

pub fn draw_revealed(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    forget_revealing(ctx, card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat,
        index: TOP,
    });
    let nth = {
        let row = ctx.blob.seat_mut(seat);
        row.draws = row.draws.saturating_add(1);
        row.draws
    };
    ctx.raise(Event::Drew { seat, nth });
    ctx.narrate(format!("{{seat {seat}}} draws {{card {card}}}"));
    true
}

pub fn card_target(ctx: &Ctx, item: &Item, index: usize) -> Option<u32> {
    match item.targets.get(index)? {
        TargetRef::Card(card) if targets::valid(ctx, item, index) => Some(*card),
        _ => None,
    }
}

pub fn chosen_card(item: &Item, index: usize) -> Option<u32> {
    match item.targets.get(index)? {
        TargetRef::Card(card) => Some(*card),
        _ => None,
    }
}

pub fn card_targets(ctx: &Ctx, item: &Item) -> Vec<u32> {
    targets::valid_cards(ctx, item)
}

pub fn remember_card(ctx: &mut Ctx, card: u32) {
    ctx.remember(TargetRef::Card(card));
}

pub fn remembered_cards(item: &Item) -> Vec<u32> {
    let chosen: usize = item
        .spec_counts
        .iter()
        .map(|count| usize::from(*count))
        .sum();
    item.targets
        .iter()
        .skip(chosen)
        .filter_map(|target| match target {
            TargetRef::Card(card) => Some(*card),
            _ => None,
        })
        .collect()
}

pub fn item_target(ctx: &Ctx, item: &Item, index: usize) -> Option<u16> {
    match item.targets.get(index)? {
        TargetRef::Item(id) if targets::valid(ctx, item, index) => Some(*id),
        _ => None,
    }
}

pub fn zone_target(item: &Item, index: usize) -> Option<u16> {
    match item.targets.get(index)? {
        TargetRef::Zone(zone) => Some(*zone),
        _ => None,
    }
}

pub fn seat_target(item: &Item, index: usize) -> Option<u8> {
    match item.targets.get(index)? {
        TargetRef::Seat(seat) => Some(*seat),
        _ => None,
    }
}

pub fn item_controller(ctx: &Ctx, item: u16) -> Option<u8> {
    ctx.chain_item(item).map(|held| held.controller)
}

pub fn counter_spell(ctx: &mut Ctx, item: &Item, index: usize) -> Option<u8> {
    let target = item_target(ctx, item, index)?;
    let controller = item_controller(ctx, target)?;
    chain::counter(ctx, target, CounterDest::Trash).then_some(controller)
}

pub fn counter_to_hand(ctx: &mut Ctx, item: &Item, index: usize) -> Option<u8> {
    let target = item_target(ctx, item, index)?;
    let controller = item_controller(ctx, target)?;
    chain::counter(ctx, target, CounterDest::Hand).then_some(controller)
}

pub fn kill(ctx: &mut Ctx, item: &Item, unit: u32) -> Killed {
    ctx.kill(unit, Cause::Item(item.id))
}

pub fn deal(ctx: &mut Ctx, item: &Item, unit: u32, amount: u8) -> bool {
    ctx.damage(unit, amount, Cause::Item(item.id))
}

pub fn bounce(ctx: &mut Ctx, card: u32) -> bool {
    ctx.bounce(card)
}

pub fn stun(ctx: &mut Ctx, unit: u32) -> bool {
    let stunned = ctx.stun(unit);
    if stunned {
        ctx.narrate(format!("{{card {unit}}} is stunned"));
    }
    stunned
}

pub fn lock_move(ctx: &mut Ctx, unit: u32) {
    ctx.lock_move(unit);
}

pub fn stun_and_lock(ctx: &mut Ctx, unit: u32) -> bool {
    let stunned = stun(ctx, unit);
    lock_move(ctx, unit);
    stunned
}

pub fn excess_damage_assigned_in_my_attack(ctx: &Ctx, seat: u8, zone: u16) -> Option<u8> {
    ctx.excess_damage_in_attack(seat, zone)
}

pub fn location_of(ctx: &Ctx, unit: u32) -> Option<Location> {
    ctx.location(unit)
}

pub fn at_battlefield(ctx: &Ctx, unit: u32) -> bool {
    ctx.at_battlefield(unit)
}

pub fn never_suppresses(_: &Ctx, _: u32, _: u32) -> bool {
    false
}

pub fn suppresses_itself(_: &Ctx, source: u32, card: u32) -> bool {
    source == card
}

pub fn a_card_not_a_token(ctx: &Ctx, event: &Event) -> bool {
    match event {
        Event::Played { card, .. } => !ctx.is_token(*card),
        Event::PlayedSpell { .. } => true,
        _ => false,
    }
}

pub fn in_combat(ctx: &Ctx, unit: u32) -> bool {
    ctx.in_combat(unit)
}

pub fn move_unit(ctx: &mut Ctx, item: &Item, unit: u32, to: Location) -> Option<Moved> {
    march::effect_move(ctx, item, unit, to)
}

pub fn move_destinations(ctx: &Ctx, unit: u32) -> Vec<Location> {
    crate::engine::march::effect_destinations(ctx, unit)
}

pub fn move_options(ctx: &Ctx, unit: u32) -> Vec<TargetRef> {
    move_destinations(ctx, unit)
        .into_iter()
        .filter_map(|to| ctx.zone_of(to))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

pub fn picked_move(ctx: &Ctx, unit: u32) -> Option<Location> {
    let zone = u16::try_from(*ctx.picks().first()?).ok()?;
    let to = Location::of_zone(zone, ctx.controller(unit), &ctx.zones)?;
    move_destinations(ctx, unit).contains(&to).then_some(to)
}

pub fn move_to_location_of(ctx: &mut Ctx, item: &Item, unit: u32, other: u32) -> Option<Moved> {
    let to = ctx.location(other)?;
    move_unit(ctx, item, unit, to)
}

pub fn recall(ctx: &mut Ctx, unit: u32, exhausted: bool) {
    ctx.recall(unit, exhausted);
}

pub fn ready(ctx: &mut Ctx, unit: u32) -> bool {
    ctx.ready(unit)
}

pub fn exhaust(ctx: &mut Ctx, unit: u32) -> bool {
    ctx.exhaust(unit)
}

pub fn grant_this_turn(ctx: &mut Ctx, unit: u32, keyword: Keyword) -> bool {
    let until = this_turn(ctx);
    ctx.grant(unit, keyword, until)
}

pub fn spawn(ctx: &mut Ctx, owner: u8, token: Token, at: Location, ready: bool) -> Option<u32> {
    ctx.spawn(owner, token, at, ready)
}

pub fn spawn_gold(ctx: &mut Ctx, owner: u8, ready: bool) -> Option<u32> {
    let gold = ctx.spawn(owner, Token::Gold, Location::Base(owner), ready)?;
    ctx.narrate(format!("{{seat {owner}}} gains a Gold"));
    Some(gold)
}

pub fn attackers_at(ctx: &Ctx, zone: u16) -> Vec<u32> {
    combat::attackers(ctx, zone)
}

pub fn defenders_at(ctx: &Ctx, zone: u16) -> Vec<u32> {
    combat::defenders(ctx, zone)
}

pub fn trigger_subject(item: &Item) -> Option<u32> {
    item.subject_card()
}

pub fn enemy_units(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| is_unit_face(card) && ctx.controller(card.id) != seat)
        .map(|card| card.id)
        .filter(|card| !ctx.is_pending_play(*card))
        .collect()
}

pub fn lock_spells(ctx: &mut Ctx, seat: u8) {
    ctx.blob.seat_mut(seat).play_lock |= PlayLock::SPELLS;
    ctx.narrate(format!("{{seat {seat}}} can't play spells this turn"));
}

pub fn lock_cards(ctx: &mut Ctx, seat: u8) {
    ctx.blob.seat_mut(seat).play_lock = PlayLock::CARDS;
    ctx.narrate(format!("{{seat {seat}}} can't play cards this turn"));
}

pub fn legion(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.seat(seat).played_main
}

pub fn choosable(ctx: &Ctx, item: &Item, card: u32) -> bool {
    let target = TargetRef::Card(card);
    !targets::untargetable(ctx, item, target) && targets::deflect_affordable(ctx, item, target)
}

pub fn deflect_tax(ctx: &Ctx, item: &Item, card: u32) -> cost::Cost {
    let target = TargetRef::Card(card);
    let taxed = cost::deflect(ctx, item, Some(target));
    let already = cost::deflect(ctx, item, None);
    cost::Cost {
        power: vec![cost::Need::Rainbow; taxed.saturating_sub(already)],
        ..cost::Cost::default()
    }
}

pub fn pay_deflect(ctx: &mut Ctx, item: &Item, card: u32) -> bool {
    let tax = deflect_tax(ctx, item, card);
    if tax.power.is_empty() {
        return true;
    }
    let seat = item.controller;
    let Ok(plan) = pay::plan(ctx, seat, &tax) else {
        ctx.narrate(format!(
            "{{seat {seat}}} cannot pay the Deflect of {{card {card}}}"
        ));
        return false;
    };
    pay::pay(ctx, seat, &plan);
    ctx.narrate(format!(
        "{{seat {seat}}} pays {} for the Deflect of {{card {card}}}",
        tax.label()
    ));
    true
}

pub fn friendly_units(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| is_unit_face(card) && ctx.controller(card.id) == seat)
        .map(|card| card.id)
        .filter(|card| !ctx.is_pending_play(*card))
        .collect()
}

pub fn is_open_battlefield(ctx: &Ctx, zone: u16) -> bool {
    ctx.blob.holder(zone).is_none() && ctx.units_at(Location::Battlefield(zone)).is_empty()
}

pub fn open_battlefields(ctx: &Ctx) -> Vec<Location> {
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| is_open_battlefield(ctx, *zone) && ctx.units_played_here(*zone))
        .map(Location::Battlefield)
        .collect()
}

pub fn units_at_battlefields(ctx: &Ctx) -> Vec<u32> {
    ctx.zones
        .battlefields
        .iter()
        .flat_map(|zone| ctx.units_at(Location::Battlefield(*zone)))
        .collect()
}

pub fn alone_there(ctx: &Ctx, unit: u32) -> bool {
    ctx.alone_at(unit)
}

pub fn enemy_alone_at(ctx: &Ctx, unit: u32) -> bool {
    let Some(at) = ctx.location(unit) else {
        return false;
    };
    let seat = ctx.controller(unit);
    ctx.units_at(at)
        .into_iter()
        .filter(|other| ctx.controller(*other) != seat)
        .count()
        == 1
}

pub fn died_alone(item: &Item) -> bool {
    item.noted.is_some_and(|noted| noted.alone)
}

pub fn paid_additional(item: &Item) -> bool {
    item.paid_additional()
}

pub fn units_on_board(ctx: &Ctx) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| is_unit_face(card))
        .map(|card| card.id)
        .filter(|card| !ctx.is_pending_play(*card))
        .collect()
}

pub fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
    ctx.xp(seat)
}

pub fn gain_xp(ctx: &mut Ctx, seat: u8, amount: u8) {
    ctx.score_xp(seat, i32::from(amount));
    ctx.narrate(format!("{{seat {seat}}} gains {amount} XP"));
}

pub fn banish(ctx: &mut Ctx, card: u32) -> bool {
    ctx.banish(card)
}

pub fn banish_by(ctx: &mut Ctx, card: u32, by: u8) -> bool {
    ctx.banish_by(card, by)
}

pub fn burn(ctx: &mut Ctx, seat: u8, count: usize) -> usize {
    ctx.burn(seat, count)
}

pub fn burn_cards(ctx: &mut Ctx, seat: u8, count: usize) -> Vec<u32> {
    ctx.burn_cards(seat, count)
}

pub fn faceless(ctx: &Ctx, cards: &[u32]) -> Vec<u32> {
    cards
        .iter()
        .copied()
        .filter(|card| ctx.kind_of(*card).is_none())
        .collect()
}

pub fn heal(ctx: &mut Ctx, unit: u32) -> bool {
    ctx.heal(unit)
}

pub fn shroud(ctx: &mut Ctx, unit: u32) -> bool {
    ctx.shroud(unit)
}

pub fn channel_exhausted(ctx: &mut Ctx, seat: u8, count: usize) -> usize {
    ctx.channel_exhausted(seat, count)
}

pub fn queue_turn(ctx: &mut Ctx, seat: u8) {
    ctx.queue_turn(seat);
}

pub fn score_point(ctx: &mut Ctx, seat: u8) {
    ctx.score_effect(seat);
}

pub fn win_the_game(ctx: &mut Ctx, seat: u8, source: u32) {
    ctx.narrate(format!("{{seat {seat}}} wins the game · {{card {source}}}"));
    ctx.win(seat);
}

pub fn a_gear(label: &'static str) -> TargetSpec {
    a_card(GEAR, label)
}

pub fn noted_of(item: &Item) -> Option<Noted> {
    item.noted
}

pub fn noted_might(item: &Item) -> i32 {
    item.noted.map(|noted| noted.might).unwrap_or(0)
}

pub fn was_mighty(item: &Item) -> bool {
    noted_might(item) >= MIGHTY
}

pub fn is_mighty(ctx: &Ctx, card: u32) -> bool {
    ctx.current_might(card) >= MIGHTY
}

pub fn became_mighty(before: i32, after: i32) -> bool {
    before < MIGHTY && after >= MIGHTY
}

pub fn buff(ctx: &mut Ctx, card: u32) -> bool {
    ctx.buff(card)
}

pub fn disempower(ctx: &mut Ctx, card: u32) -> bool {
    ctx.disempower(card)
}

pub fn is_empowered(ctx: &Ctx, card: u32) -> bool {
    ctx.is_empowered(card)
}

pub fn is_temporary(ctx: &Ctx, card: u32) -> bool {
    ctx.is_temporary(card)
}

pub fn temporary_units(ctx: &Ctx, seat: u8) -> usize {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ctx.is_temporary(*unit))
        .count()
}

pub fn friendly_gear(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| card.is_kind(KIND_GEAR) && ctx.controller(card.id) == seat)
        .map(|card| card.id)
        .collect()
}

pub fn at_end_of_turn(ctx: &mut Ctx, item: &Item, ability: u8, args: Vec<u32>) {
    let turn = ctx.turn();
    ctx.delay(
        When::EndOfTurn(turn),
        item.kind.source(),
        item.controller,
        ability,
        args,
    );
}

pub fn at_next_beginning_phase(ctx: &mut Ctx, item: &Item, ability: u8, args: Vec<u32>) {
    let seat = item.controller;
    ctx.delay(
        When::BeginningOf(seat),
        item.kind.source(),
        seat,
        ability,
        args,
    );
}

pub fn ready_runes(ctx: &mut Ctx, seat: u8, count: usize) -> usize {
    let exhausted: Vec<u32> = ctx
        .runes_of(seat)
        .into_iter()
        .filter(|rune| rune.exhausted)
        .map(|rune| rune.id)
        .take(count)
        .collect();
    exhausted
        .into_iter()
        .filter(|rune| ctx.ready(*rune))
        .count()
}

pub fn from_facedown(item: &Item) -> bool {
    hide::facedown_zone(item).is_some()
}

pub fn hiding_battlefield(ctx: &Ctx, item: &Item) -> Option<u16> {
    hide::facedown_zone(item).or_else(|| match ctx.location(item.kind.source()) {
        Some(Location::Battlefield(zone)) => Some(zone),
        _ => None,
    })
}

pub fn cards_at_hiding_battlefield(ctx: &Ctx, item: &Item) -> Vec<TargetRef> {
    let Some(zone) = hiding_battlefield(ctx, item) else {
        return Vec::new();
    };
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn granted_reaction(ctx: &Ctx, card: u32) -> bool {
    hide::reacts(ctx, card)
}

pub fn unit_priced_in_hand(ctx: &Ctx, seat: u8, card: u32, price: Price) -> bool {
    if ctx.kind_of(card) != Some(KIND_UNIT) {
        return false;
    }
    let item = cost::priced_item(ctx, seat, card, price);
    pay::affordable_for(
        ctx,
        seat,
        &cost::of_item(ctx, &item, None),
        super::Paying::Item(&item),
    )
}

pub fn hand_card_may_be_a_priced_unit(ctx: &Ctx, seat: u8, card: u32, price: Price) -> bool {
    ctx.card(card).is_none_or(|held| held.is_hidden())
        || unit_priced_in_hand(ctx, seat, card, price)
}

pub fn hand_cards_that_may_be_a_priced_unit(ctx: &Ctx, seat: u8, price: Price) -> Vec<u32> {
    ctx.hand_of(seat)
        .into_iter()
        .filter(|card| {
            !ctx.table.is_revealed(*card) || unit_priced_in_hand(ctx, seat, *card, price)
        })
        .collect()
}

pub fn facedown_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    hide::of_seat(ctx, seat)
}

pub fn done() -> Flow {
    Flow::Done
}

pub fn attach_gear(ctx: &mut Ctx, gear: u32, unit: u32) -> Attached {
    attach::attach(ctx, gear, unit)
}

pub fn detach_gear(ctx: &mut Ctx, gear: u32) -> bool {
    attach::detach(ctx, gear)
}

pub fn detach_all(ctx: &mut Ctx, unit: u32) -> usize {
    attach::detach_all(ctx, unit)
}

pub fn is_attached(ctx: &Ctx, gear: u32) -> bool {
    attach::is_attached(ctx, gear)
}

pub fn attached_to(ctx: &Ctx, gear: u32) -> Option<u32> {
    attach::attached_to(ctx, gear)
}

pub fn lender_of(item: &Item) -> Option<u32> {
    item.kind.lender()
}

pub fn attachments_of(ctx: &Ctx, unit: u32) -> Vec<u32> {
    attach::attachments_of(ctx, unit)
}

pub fn equipment_of(ctx: &Ctx, unit: u32) -> Vec<u32> {
    attachments_of(ctx, unit)
        .into_iter()
        .filter(|gear| {
            ctx.script(*gear)
                .is_some_and(|script| script.is_equipment())
        })
        .collect()
}

pub fn swap_units(ctx: &mut Ctx, first: u32, second: u32) -> Swapped {
    march::swap_units(ctx, first, second)
}

pub fn swap_might_this_turn(ctx: &mut Ctx, item: &Item, first: u32, second: u32) -> bool {
    if first == second || !ctx.is_unit(first) || !ctx.is_unit(second) {
        return false;
    }
    let first_might = ctx.current_might(first);
    let second_might = ctx.current_might(second);
    let Ok(first_delta) = i16::try_from(second_might - first_might) else {
        return false;
    };
    let Ok(second_delta) = i16::try_from(first_might - second_might) else {
        return false;
    };
    might_this_turn(ctx, item, first, first_delta, None);
    might_this_turn(ctx, item, second, second_delta, None);
    ctx.narrate(format!(
        "{{card {first}}} and {{card {second}}} swap Might this turn · {second_might} and {first_might}"
    ));
    true
}

pub fn ask_discard(ctx: &mut Ctx, item: &Item, stage: u8) -> Option<Ask> {
    discard::ask(ctx, item, stage)
}

pub fn discarded(ctx: &Ctx) -> Option<u32> {
    ctx.picks().first().copied()
}

pub fn discarded_kind<'c>(ctx: &'c Ctx) -> Option<&'c str> {
    ctx.kind_of(discarded(ctx)?)
}

pub fn kind_of<'c>(ctx: &'c Ctx, card: u32) -> Option<&'c str> {
    ctx.kind_of(card)
}

pub fn chosen_unit(item: &Item) -> Option<u32> {
    item.subject_card()
}

pub const CHARM_DESTINATION: TargetSpec = target(
    Filter::DifferentLocationFrom(0),
    1,
    1,
    TargetKind::Zone,
    "where it goes",
);

pub fn charm_destination(ctx: &Ctx, item: &Item, unit: u32, index: usize) -> Option<Location> {
    let TargetRef::Zone(zone) = *item.targets.get(index)? else {
        return None;
    };
    if !targets::valid(ctx, item, index) {
        return None;
    }
    let to = Location::of_zone(zone, ctx.controller(unit), &ctx.zones)?;
    move_destinations(ctx, unit).contains(&to).then_some(to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as plays, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::Target;

    const JINX: u32 = 90;
    const FERVENT: u32 = 91;

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn a_might_swap_is_two_this_turn_deltas_that_compose_with_a_later_stupefy() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BF1, 1, "Jinx", 5));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let switcheroo = Item::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(!swap_might_this_turn(
            &mut ctx,
            &switcheroo,
            fixtures::VI,
            fixtures::VI
        ));
        assert!(swap_might_this_turn(
            &mut ctx,
            &switcheroo,
            fixtures::VI,
            JINX
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.current_might(JINX), 3);
        assert_eq!(
            ctx.effects,
            [
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: COUNTER_MIGHT,
                    delta: 2
                },
                Effect::Counter {
                    target: Target::Card(JINX),
                    counter: COUNTER_MIGHT,
                    delta: -2
                },
            ],
            "two deltas over the layered Might, mirrored for kai"
        );
        let stupefy = Item::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &stupefy, fixtures::VI, -1, Some(1));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "Switcheroo then Stupefy composes: the -1 lands on the swapped value"
        );
        might_this_turn(&mut ctx, &stupefy, JINX, -4, Some(1));
        assert_eq!(
            ctx.current_might(JINX),
            1,
            "454.3.b · the minimum is read against the swapped value"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 1);
        assert_eq!(might_counter(&ctx, JINX), -4);
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(JINX), 5);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(might_counter(&ctx, JINX), 0);
        assert!(ctx.fault.is_none());
    }

    static BLADE_DANCER: Card = legend(
        "Blade Dancer",
        &[],
        &[
            optional(exhausting_self(with_cost(
                on_friendly_unit_chosen(&[], |ctx, item, _| {
                    if let Some(unit) = chosen_unit(item) {
                        ready(ctx, unit);
                    }
                    Flow::Done
                }),
                RAINBOW,
            ))),
            optional(with_cost(
                on_conquer(&[], |ctx, item, _| {
                    ready(ctx, item.kind.source());
                    Flow::Done
                }),
                ONE_ENERGY,
            )),
        ],
    );

    static FERVENT_CARD: Card = unit(
        "Fervent",
        &[Keyword::Deflect(1)],
        &[
            on_chosen(&[], |ctx, item, _| {
                might_this_turn(ctx, item, item.kind.source(), 1, None);
                Flow::Done
            }),
            on_readied(&[], |ctx, item, _| {
                might_this_turn(ctx, item, item.kind.source(), 1, None);
                Flow::Done
            }),
        ],
    );

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fervent = fixtures::unit(FERVENT, fixtures::BASE, 0, "Fervent", 4);
        fervent.exhausted = true;
        fervent.domain = vec!["Calm".into()];
        fixture.table.cards.push(fervent);
        let spell = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == fixtures::HAND_SPELL)
            .unwrap();
        fixture.table.cards[spell] =
            fixtures::spell(fixtures::HAND_SPELL, fixtures::HAND, 0, "Discipline", 2, 0);
        fixture.table.cards[spell].domain = vec!["Calm".into()];
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &BLADE_DANCER)
            .with_script(FERVENT, &FERVENT_CARD);
        fixture
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn play_discipline(ctx: &mut Ctx) {
        let entry = crate::engine::ctx::EntryMove {
            card: fixtures::HAND_SPELL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        legal::classify(ctx, 0, &entry).unwrap();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::HAND_SPELL, chain, 0), 0)
            .unwrap();
        plays::begin(ctx, 0, fixtures::HAND_SPELL, Origin::Hand, None).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn irelias_legend_readies_a_discipline_target_and_fervent_gains_one_twice_on_a_self_choose() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}}")).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == FERVENT
        )));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "Fervent's Chosen and the legend's ChosenFriendly form one batch"
        );
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}} trigger")).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost: 1, .. })),
            "392.2 · the legend's may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "pay 1 any power for the {card 75} trigger · {card 91}?"
        );
        choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted,
            "exhausting her is part of the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 3, "the spell and both triggers");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            !ctx.card(FERVENT).unwrap().exhausted,
            "the legend's trigger readies the chosen unit"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == FERVENT
        )));
        assert_eq!(
            ctx.blob.chain.len(),
            3,
            "Readied queues Fervent's second trigger onto the chain"
        );
        for _ in 0..3 {
            priority::pass(&mut ctx, 0).unwrap();
            priority::pass(&mut ctx, 1).unwrap();
        }
        assert!(ctx.blob.chain.is_empty());
        let mods = &ctx.state_of(FERVENT).unwrap().might;
        assert_eq!(
            mods.iter().map(|held| held.delta).collect::<Vec<i16>>(),
            [1, 1, 2],
            "+1 when chosen, +1 when readied, +2 from Discipline"
        );
        assert_eq!(ctx.current_might(FERVENT), 8);
        assert_eq!(might_counter(&ctx, FERVENT), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_legend_cannot_pay_its_trigger_and_it_is_removed_without_asking() {
        let mut fixture = dojo();
        fixture
            .table
            .card_mut(fixtures::LEGEND_CARD)
            .unwrap()
            .exhausted = true;
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}}")).unwrap();
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}} trigger")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no cost confirm for a cost that can't be paid"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the spell and Fervent's own trigger"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 75} trigger is removed · its source is exhausted"));
    }
}

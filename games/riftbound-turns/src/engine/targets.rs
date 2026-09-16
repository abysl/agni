use crate::cards::prelude::{self, triggered};
use crate::cards::{
    base_name, generic, is_granted, Ability, Filter, Flow, Item, ModeTiming, Paying, Rel, Stage,
    Static, TargetKind, TargetSpec, Trigger, IMPLICIT_HUNT, IMPLICIT_TEMPORARY, IMPLICIT_VISION,
    IMPLICIT_WEAPONMASTER,
};
use crate::engine::ctx::{Ctx, Location};
use crate::engine::{activate, attach, cost, hide, kill, march, pay};
use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, TargetRef, FLAG_SHROUDED};

fn hunt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let source = item.kind.source();
    let value = ctx.hunt_value(source);
    if value > 0 {
        ctx.score_xp(item.controller, i32::from(value));
        ctx.narrate(format!(
            "{{card {source}}} hunts · {{seat {}}} gains {value} XP",
            item.controller
        ));
    }
    Flow::Done
}

pub static HUNT_ABILITY: Ability = triggered(Trigger::Reflexive, &[], hunt);

pub fn ability_at(ctx: &Ctx, source: u32, index: u8) -> Option<&'static Ability> {
    if is_granted(index) {
        return activate::lent_at(ctx, source, index).map(|lent| lent.ability);
    }
    ctx.script(source)?.abilities.get(usize::from(index))
}

pub fn ability_of_kind(ctx: &Ctx, kind: ItemKind) -> Option<&'static Ability> {
    match kind {
        ItemKind::Spell { card } => {
            let script = ctx.script(card)?;
            script
                .abilities
                .iter()
                .find(|ability| ability.trigger == Trigger::Play)
                .or_else(|| script.abilities.first())
        }
        ItemKind::Permanent { .. } => None,
        ItemKind::Ability { source, index } | ItemKind::Trigger { source, index } => {
            if index == IMPLICIT_TEMPORARY {
                return Some(&generic::TEMPORARY);
            }
            if index == IMPLICIT_HUNT {
                return Some(&HUNT_ABILITY);
            }
            if index == IMPLICIT_VISION {
                return Some(&prelude::VISION);
            }
            if index == IMPLICIT_WEAPONMASTER {
                return Some(&prelude::WEAPONMASTER);
            }
            if index == kill::CHOICE {
                return Some(&kill::CHOICE_ABILITY);
            }
            ability_at(ctx, source, index)
        }
        ItemKind::Granted { lender, index, .. } | ItemKind::Lent { lender, index, .. } => {
            activate::lent_text(ctx, lender, index)
        }
    }
}

pub fn specs_of_execution(ctx: &Ctx, item: &ChainItem, execution: u8) -> &'static [TargetSpec] {
    let Some(ability) = ability_of(ctx, item) else {
        return &[];
    };
    if ability.mode_timing != ModeTiming::AtPlay || ability.modes.is_empty() {
        return ability.targets;
    }
    item.mode_at(execution)
        .or_else(|| (execution > 0).then(|| item.mode_at(0)).flatten())
        .and_then(|mode| ability.modes.get(usize::from(mode)))
        .map(|mode| mode.targets)
        .unwrap_or(&[])
}
pub fn ability_of(ctx: &Ctx, item: &ChainItem) -> Option<&'static Ability> {
    ability_of_kind(ctx, item.kind)
}
pub fn specs_of(ctx: &Ctx, item: &ChainItem) -> Vec<TargetSpec> {
    let executions = 1 + usize::from(item.repeats());
    (0..executions)
        .flat_map(|execution| {
            specs_of_execution(ctx, item, execution as u8)
                .iter()
                .copied()
        })
        .collect()
}

pub fn spec_at(specs: &[TargetSpec], counts: &[u8], index: usize) -> Option<(usize, TargetSpec)> {
    let mut seen = 0usize;
    for (position, spec) in specs.iter().enumerate() {
        seen += counts
            .get(position)
            .map(|count| usize::from(*count))
            .unwrap_or(usize::from(spec.max.max(1)));
        if index < seen {
            return Some((position, *spec));
        }
    }
    None
}

pub fn spec_of_index(specs: &[TargetSpec], counts: &[u8], index: usize) -> Option<TargetSpec> {
    spec_at(specs, counts, index).map(|(_, spec)| spec)
}

pub fn group_start(ctx: &Ctx, item: &ChainItem, position: usize) -> usize {
    if !item.repeated() {
        return 0;
    }
    let mut spec_offset = 0;
    let mut target_offset = 0;
    let executions = 1 + usize::from(item.repeats());
    for execution in 0..executions {
        let specs = specs_of_execution(ctx, item, execution as u8);
        if position < spec_offset + specs.len() {
            return target_offset;
        }
        for (local, spec) in specs.iter().enumerate() {
            let index = spec_offset + local;
            target_offset += item
                .spec_counts
                .get(index)
                .map(|count| usize::from(*count))
                .unwrap_or(usize::from(spec.max.max(1)));
        }
        spec_offset += specs.len();
    }
    target_offset
}

pub fn of_spec(ctx: &Ctx, item: &ChainItem, spec_index: usize) -> Vec<TargetRef> {
    let specs = specs_of(ctx, item);
    let mut start = 0usize;
    for (position, spec) in specs.iter().enumerate() {
        let count = item
            .spec_counts
            .get(position)
            .map(|count| usize::from(*count))
            .unwrap_or(usize::from(spec.max.max(1)));
        if position == spec_index {
            return item
                .targets
                .iter()
                .skip(start)
                .take(count)
                .copied()
                .collect();
        }
        start += count;
    }
    Vec::new()
}

pub fn source_location(ctx: &Ctx, item: &ChainItem) -> Option<Location> {
    match item.origin {
        Origin::Facedown { zone } => Some(Location::Battlefield(zone)),
        _ => ctx.location(item.kind.source()),
    }
}

fn card_of_ref(ctx: &Ctx, target: TargetRef) -> Option<u32> {
    match target {
        TargetRef::Card(card) => Some(card),
        TargetRef::Item(item) => ctx.chain_item(item).and_then(|held| held.kind.card()),
        _ => None,
    }
}

fn controller_of_ref(ctx: &Ctx, target: TargetRef) -> Option<u8> {
    match target {
        TargetRef::Card(card) => ctx.card(card).map(|_| ctx.controller(card)),
        TargetRef::Item(item) => ctx.chain_item(item).map(|held| held.controller),
        TargetRef::Seat(seat) => Some(seat),
        TargetRef::Zone(zone) => ctx.blob.holder(zone),
    }
}

fn location_of_ref(ctx: &Ctx, target: TargetRef) -> Option<Location> {
    match target {
        TargetRef::Card(card) => ctx.location(card),
        TargetRef::Zone(zone) if ctx.zones.is_battlefield(zone) => {
            Some(Location::Battlefield(zone))
        }
        _ => None,
    }
}

pub fn anchor_of(filter: &Filter) -> Option<u8> {
    match filter {
        Filter::SameLocationAs(index)
        | Filter::DifferentLocationFrom(index)
        | Filter::ToOrFromBaseOf(index)
        | Filter::ZoneWithUnits(Rel::SameControllerAs(index)) => Some(*index),
        Filter::Not(inner) => anchor_of(inner),
        Filter::And(all) | Filter::Or(all) => all.iter().find_map(anchor_of),
        _ => None,
    }
}

fn anchor_card(ctx: &Ctx, item: &ChainItem, base: usize, spec: &TargetSpec) -> Option<u32> {
    let index = anchor_of(&spec.filter)?;
    match item.targets.get(base + usize::from(index))? {
        TargetRef::Card(card) => ctx.card(*card).map(|_| *card),
        _ => None,
    }
}

fn location_relative_to(ctx: &Ctx, anchor: TargetRef, target: TargetRef) -> Option<Location> {
    match target {
        TargetRef::Zone(zone) if ctx.zones.base == Some(zone) => {
            controller_of_ref(ctx, anchor).map(Location::Base)
        }
        _ => location_of_ref(ctx, target),
    }
}

pub fn zone_location(
    ctx: &Ctx,
    item: &ChainItem,
    base: usize,
    spec: &TargetSpec,
    zone: u16,
) -> Option<Location> {
    let anchor = anchor_of(&spec.filter)
        .and_then(|index| item.targets.get(base + usize::from(index)).copied());
    match anchor {
        Some(anchor) => location_relative_to(ctx, anchor, TargetRef::Zone(zone)),
        None => Location::of_zone(zone, item.controller, &ctx.zones),
    }
}

pub fn admits_facedown(filter: &Filter) -> bool {
    match filter {
        Filter::Facedown => true,
        Filter::Not(inner) => admits_facedown(inner),
        Filter::And(all) | Filter::Or(all) => all.iter().any(admits_facedown),
        _ => false,
    }
}

fn in_universe(ctx: &Ctx, item: &ChainItem, spec: &TargetSpec, target: TargetRef) -> bool {
    match (spec.kind, target) {
        (TargetKind::Card, TargetRef::Card(card)) if ctx.is_facedown(card) => {
            admits_facedown(&spec.filter)
                && ctx.card(card).is_some()
                && ctx.controller(card) == item.controller
                && !ctx.is_pending_play(card)
        }
        (TargetKind::Card, TargetRef::Card(card)) => {
            let held = ctx.card(card);
            held.is_some_and(|held| !held.is_hidden())
                && !ctx.is_pending_play(card)
                && (ctx.on_board(card)
                    || ctx.is_legend(card)
                    || ctx.in_trash(card)
                    || ctx.in_banishment(card)
                    || ctx.in_champion_zone(card)
                    || (ctx.in_hand(card) && ctx.table.is_revealed(card)))
        }
        (TargetKind::Item, TargetRef::Item(id)) => {
            id != item.id
                && ctx
                    .chain_item(id)
                    .is_some_and(|held| held.status == ItemStatus::Finalized)
        }
        (TargetKind::Zone, TargetRef::Zone(zone)) => {
            ctx.zones.is_battlefield(zone) || ctx.zones.base == Some(zone)
        }
        (TargetKind::Seat, TargetRef::Seat(seat)) => seat < ctx.players(),
        _ => false,
    }
}

fn universe(ctx: &Ctx, item: &ChainItem, base: usize, spec: &TargetSpec) -> Vec<TargetRef> {
    match spec.kind {
        TargetKind::Card => ctx
            .table
            .cards
            .iter()
            .map(|card| TargetRef::Card(card.id))
            .filter(|target| in_universe(ctx, item, spec, *target))
            .collect(),
        TargetKind::Item => ctx
            .blob
            .chain
            .iter()
            .map(|held| TargetRef::Item(held.id))
            .filter(|target| in_universe(ctx, item, spec, *target))
            .collect(),
        TargetKind::Zone => match (spec.filter, anchor_card(ctx, item, base, spec)) {
            (Filter::Any, _) => ctx
                .play_locations(item.controller)
                .into_iter()
                .filter_map(|location| ctx.zone_of(location).map(|(zone, _)| zone))
                .map(TargetRef::Zone)
                .collect(),
            (_, Some(unit)) if ctx.is_unit(unit) => {
                let from = ctx.location(unit);
                let mut open = vec![Location::Base(ctx.controller(unit))];
                open.extend(
                    ctx.zones
                        .battlefields
                        .iter()
                        .copied()
                        .map(Location::Battlefield),
                );
                open.retain(|to| {
                    Some(*to) == from
                        || (!ctx.destination_capped(unit, *to)
                            && !from.is_some_and(|from| march::base_closed(ctx, unit, from, *to)))
                });
                open.into_iter()
                    .filter_map(|location| ctx.zone_of(location).map(|(zone, _)| zone))
                    .map(TargetRef::Zone)
                    .collect()
            }
            _ => ctx
                .zones
                .battlefields
                .iter()
                .map(|zone| TargetRef::Zone(*zone))
                .collect(),
        },
        TargetKind::Seat => (0..ctx.players()).map(TargetRef::Seat).collect(),
    }
}

pub fn matches(ctx: &Ctx, item: &ChainItem, filter: &Filter, target: TargetRef) -> bool {
    matches_from(ctx, item, 0, filter, target)
}

fn matches_from(
    ctx: &Ctx,
    item: &ChainItem,
    base: usize,
    filter: &Filter,
    target: TargetRef,
) -> bool {
    let card = card_of_ref(ctx, target);
    let on_board = match target {
        TargetRef::Card(card) if ctx.on_board(card) => Some(card),
        _ => None,
    };
    let chain_kind = |spell: bool| match target {
        TargetRef::Item(id) => ctx
            .chain_item(id)
            .is_some_and(|held| matches!(held.kind, ItemKind::Spell { .. }) == spell),
        _ => false,
    };
    let printed = |pick: fn(&agni_plugin_sdk::table::CardInfo) -> Option<u8>| {
        card.and_then(|card| ctx.card(card))
            .and_then(pick)
            .unwrap_or(0)
    };
    let earlier = |index: u8| item.targets.get(base + usize::from(index)).copied();
    match filter {
        Filter::Any => true,
        Filter::Unit => on_board.is_some_and(|card| ctx.is_unit(card)),
        Filter::Gear => on_board.is_some_and(|card| ctx.is_gear(card)),
        Filter::Rune => on_board.is_some_and(|card| ctx.is_rune(card)),
        Filter::Legend => matches!(target, TargetRef::Card(card) if ctx.is_legend(card)),
        Filter::Spell => chain_kind(true),
        Filter::Ability => chain_kind(false),
        Filter::ItemOnChain => matches!(target, TargetRef::Item(id) if ctx.is_on_chain(id)),
        Filter::Friendly => controller_of_ref(ctx, target) == Some(item.controller),
        Filter::Enemy => {
            controller_of_ref(ctx, target).is_some_and(|controller| controller != item.controller)
        }
        Filter::AtBattlefield => {
            location_of_ref(ctx, target).is_some_and(|at| at.battlefield().is_some())
        }
        Filter::InBase => location_of_ref(ctx, target).is_some_and(Location::is_base),
        Filter::Here => {
            let here = source_location(ctx, item);
            here.is_some() && location_of_ref(ctx, target) == here
        }
        Filter::HiddenBattlefield => match target {
            TargetRef::Zone(zone) => ctx.blob.cards.iter().any(|row| {
                row.hidden_at == Some(zone)
                    && ctx
                        .card(row.id)
                        .is_some_and(|held| held.owner == item.controller)
            }),
            _ => false,
        },
        Filter::Facedown => card.is_some_and(|card| ctx.is_facedown(card)),
        Filter::SameLocationAs(index) => {
            let anchor = earlier(*index);
            let other = anchor.and_then(|other| location_of_ref(ctx, other));
            let mine = anchor.and_then(|anchor| location_relative_to(ctx, anchor, target));
            other.is_some() && mine == other
        }
        Filter::DifferentLocationFrom(index) => {
            let anchor = earlier(*index);
            let other = anchor.and_then(|other| location_of_ref(ctx, other));
            let mine = anchor.and_then(|anchor| location_relative_to(ctx, anchor, target));
            other.is_some() && mine.is_some() && mine != other
        }
        Filter::ToOrFromBaseOf(index) => {
            let anchor = earlier(*index);
            let from = anchor.and_then(|other| location_of_ref(ctx, other));
            let to = anchor.and_then(|anchor| location_relative_to(ctx, anchor, target));
            matches!(
                (from, to),
                (Some(Location::Base(_)), Some(Location::Battlefield(_)))
                    | (Some(Location::Battlefield(_)), Some(Location::Base(_)))
            )
        }
        Filter::NotSame(index) => earlier(*index) != Some(target),
        Filter::SameControllerAs(index) => {
            let anchor = earlier(*index).and_then(|anchor| controller_of_ref(ctx, anchor));
            anchor.is_some() && controller_of_ref(ctx, target) == anchor
        }
        Filter::MightLessThan(index) => {
            let anchor = earlier(*index).and_then(|anchor| match anchor {
                TargetRef::Card(unit) if ctx.on_board(unit) && ctx.is_unit(unit) => {
                    Some(ctx.current_might(unit))
                }
                _ => None,
            });
            on_board.is_some_and(|card| {
                ctx.is_unit(card) && anchor.is_some_and(|might| ctx.current_might(card) < might)
            })
        }
        Filter::ItemTargetsOnly(index) => match (earlier(*index), target) {
            (Some(TargetRef::Card(unit)), TargetRef::Item(id)) => ctx
                .chain_item(id)
                .is_some_and(|held| chooses_only(ctx, item.controller, held, unit)),
            _ => false,
        },
        Filter::Domain(domain) => card.is_some_and(|card| ctx.domains_of(card).contains(domain)),
        Filter::EnergyAtMost(limit) => printed(|face| face.energy) <= *limit,
        Filter::PowerAtMost(limit) => printed(|face| face.power) <= *limit,
        Filter::Temporary => card.is_some_and(|card| ctx.is_temporary(card)),
        Filter::Attacker => on_board.is_some_and(|card| ctx.is_attacker(card)),
        Filter::Defender => on_board.is_some_and(|card| ctx.is_defender(card)),
        Filter::InCombat => on_board.is_some_and(|card| ctx.in_combat(card)),
        Filter::Movable => card.is_some_and(|card| {
            (ctx.controller(card) != item.controller
                || !ctx.has_flag(card, crate::state::FLAG_NO_MOVE_BY_OWNER))
                && !march::item_cannot_move(ctx, item, card)
        }),
        Filter::Empowered => on_board.is_some_and(|card| ctx.is_empowered(card)),
        Filter::Exhausted => card
            .and_then(|card| ctx.card(card))
            .is_some_and(|held| held.exhausted),
        Filter::Ready => card
            .and_then(|card| ctx.card(card))
            .is_some_and(|held| !held.exhausted),
        Filter::Attached => on_board.is_some_and(|card| attach::is_attached(ctx, card)),
        Filter::Unattached => on_board.is_some_and(|card| !attach::is_attached(ctx, card)),
        Filter::ItemTargetsFriendly => match target {
            TargetRef::Item(id) => ctx.chain_item(id).is_some_and(|held| {
                held.targets.iter().any(|chosen| match chosen {
                    TargetRef::Card(card) => {
                        ctx.card(*card).is_some() && ctx.controller(*card) == item.controller
                    }
                    _ => false,
                })
            }),
            _ => false,
        },
        Filter::ItemTargets(inner) => match target {
            TargetRef::Item(id) => ctx.chain_item(id).is_some_and(|held| {
                held.targets
                    .iter()
                    .any(|chosen| matches_from(ctx, item, base, inner, *chosen))
            }),
            _ => false,
        },
        Filter::ItemControlledBy(rel) => match (rel, target) {
            (Rel::Any, TargetRef::Item(id)) => ctx.is_on_chain(id),
            (Rel::Enemy, TargetRef::Item(id)) => ctx
                .chain_item(id)
                .is_some_and(|held| held.controller != item.controller),
            (Rel::Friendly, TargetRef::Item(id)) => ctx
                .chain_item(id)
                .is_some_and(|held| held.controller == item.controller),
            (Rel::SameControllerAs(index), TargetRef::Item(id)) => {
                let anchor = earlier(*index).and_then(|anchor| controller_of_ref(ctx, anchor));
                anchor.is_some()
                    && ctx
                        .chain_item(id)
                        .is_some_and(|held| Some(held.controller) == anchor)
            }
            _ => false,
        },
        Filter::ZoneWithUnits(rel) => {
            let TargetRef::Zone(zone) = target else {
                return false;
            };
            let anchor = match rel {
                Rel::SameControllerAs(index) => earlier(*index),
                _ => None,
            };
            let at = match anchor {
                Some(anchor) => location_relative_to(ctx, anchor, target),
                None => Location::of_zone(zone, item.controller, &ctx.zones),
            };
            let wanted = match rel {
                Rel::SameControllerAs(_) => {
                    anchor.and_then(|anchor| controller_of_ref(ctx, anchor))
                }
                _ => None,
            };
            let anchor_card = anchor.and_then(|anchor| card_of_ref(ctx, anchor));
            at.is_some_and(|at| {
                ctx.units_at(at).into_iter().any(|unit| match rel {
                    Rel::Any => true,
                    Rel::Friendly => ctx.controller(unit) == item.controller,
                    Rel::Enemy => ctx.controller(unit) != item.controller,
                    Rel::SameControllerAs(_) => {
                        anchor_card != Some(unit)
                            && wanted.is_some()
                            && Some(ctx.controller(unit)) == wanted
                    }
                })
            })
        }
        Filter::InShowdown => ctx.blob.showdown.as_ref().is_some_and(|showdown| {
            location_of_ref(ctx, target) == Some(Location::Battlefield(showdown.zone))
        }),
        Filter::InTrash => card.is_some_and(|card| ctx.in_trash(card)),
        Filter::InBanishment => card.is_some_and(|card| ctx.in_banishment(card)),
        Filter::InHand => card.is_some_and(|card| ctx.in_hand(card)),
        Filter::InChampionZone => card.is_some_and(|card| ctx.in_champion_zone(card)),
        Filter::Owned => card.is_some_and(|card| ctx.owner(card) == item.controller),
        Filter::Champion(champion) => card
            .and_then(|card| ctx.card(card))
            .and_then(|held| held.name.strip_prefix(champion))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(" - ")),
        Filter::MightAtMost(limit) => on_board
            .is_some_and(|card| ctx.is_unit(card) && ctx.current_might(card) <= i32::from(*limit)),
        Filter::Equipment => {
            card.is_some_and(|card| ctx.script(card).is_some_and(|script| script.is_equipment()))
        }
        Filter::SameLocationAsPicks | Filter::DifferentLocationFromPicks => true,
        Filter::TotalMightAtMost(limit) => on_board
            .is_some_and(|card| ctx.is_unit(card) && ctx.current_might(card) <= i32::from(*limit)),
        Filter::MovableToBase => on_board.is_some_and(|card| ctx.movable_to_base(card)),
        Filter::InCombatWith(inner) => on_board.is_some_and(|card| {
            ctx.in_combat(card)
                && ctx.location(card).is_some_and(|at| {
                    ctx.units_at(at).into_iter().any(|other| {
                        other != card
                            && ctx.in_combat(other)
                            && matches_from(ctx, item, base, inner, TargetRef::Card(other))
                    })
                })
        }),
        Filter::ChosenByEnemyItem(inner) => card.is_some_and(|card| {
            ctx.blob.chain.iter().any(|held| {
                held.controller != item.controller
                    && held.targets.contains(&TargetRef::Card(card))
                    && matches_from(ctx, item, base, inner, TargetRef::Item(held.id))
            })
        }),
        Filter::Kind(kind) => card.is_some_and(|card| ctx.kind_of(card) == Some(kind)),
        Filter::Named(name) => card
            .and_then(|card| ctx.card(card))
            .is_some_and(|held| base_name(&held.name) == *name),
        Filter::NamedTag => card.is_some_and(|card| {
            ctx.named(item.kind.source())
                .is_some_and(|tag| ctx.bears_tag(card, tag))
        }),
        Filter::NotSelf => card != Some(item.kind.source()),
        Filter::And(all) => all
            .iter()
            .all(|held| matches_from(ctx, item, base, held, target)),
        Filter::Or(any) => any
            .iter()
            .any(|held| matches_from(ctx, item, base, held, target)),
        Filter::Not(inner) => {
            if matches!(**inner, Filter::Here) && source_location(ctx, item).is_none() {
                return false;
            }
            !matches_from(ctx, item, base, inner, target)
        }
    }
}

pub fn chooses_only(ctx: &Ctx, controller: u8, held: &ChainItem, unit: u32) -> bool {
    let mut chooses_it = false;
    for chosen in &held.targets {
        let TargetRef::Card(card) = chosen else {
            continue;
        };
        if *card == unit {
            chooses_it = true;
        } else if ctx.is_unit(*card) && ctx.controller(*card) == controller {
            return false;
        }
    }
    chooses_it
}

pub fn untargetable(ctx: &Ctx, item: &ChainItem, target: TargetRef) -> bool {
    let TargetRef::Card(card) = target else {
        return false;
    };
    if ctx.controller(card) == item.controller {
        return false;
    }
    if ctx.has_flag(card, FLAG_SHROUDED) {
        return true;
    }
    ctx.script(card).is_some_and(|script| {
        script.statics.iter().any(|held| match held {
            Static::Untargetable(applies) => applies(ctx, card),
            _ => false,
        })
    }) || ctx.projected_statics(card).iter().any(|held| match held {
        Static::Untargetable(applies) => applies(ctx, card),
        _ => false,
    })
}

pub fn deflect_affordable(ctx: &Ctx, item: &ChainItem, target: TargetRef) -> bool {
    let TargetRef::Card(card) = target else {
        return true;
    };
    if ctx.deflect_of(card) == 0 || ctx.controller(card) == item.controller {
        return true;
    }
    if cost::ignores_deflect(ctx, item) {
        return true;
    }
    let total = cost::of_item(ctx, item, Some(target));
    pay::affordable_for(ctx, item.controller, &total, Paying::Item(item))
}

pub fn from_facedown(ctx: &Ctx, item: &ChainItem, spec: &TargetSpec, target: TargetRef) -> bool {
    let Some(zone) = hide::facedown_zone(item) else {
        return true;
    };
    let specs = specs_of(ctx, item);
    let lifted = match specs.iter().position(|held| held == spec) {
        Some(position) => hide::lifts_at(&specs, position),
        None => hide::lifted_by(&spec.filter),
    };
    if lifted {
        return true;
    }
    match (spec.kind, target) {
        (TargetKind::Card, _) => location_of_ref(ctx, target) == Some(Location::Battlefield(zone)),
        (TargetKind::Zone, TargetRef::Zone(picked)) => picked == zone,
        _ => true,
    }
}

pub fn min_of(ctx: &Ctx, item: &ChainItem, spec: &TargetSpec) -> Option<u8> {
    match spec.min_at_level {
        Some(gate) if ctx.xp(item.controller) < i32::from(gate.xp) => None,
        Some(gate) => Some(spec.min.max(gate.min)),
        None => Some(spec.min),
    }
}

pub fn fillable(ctx: &Ctx, item: &ChainItem, spec: &TargetSpec) -> bool {
    min_of(ctx, item, spec)
        .is_none_or(|min| min == 0 || candidates(ctx, item, spec).len() >= usize::from(min))
}

pub fn first_spec_fillable(ctx: &Ctx, item: &ChainItem) -> bool {
    specs_of(ctx, item)
        .first()
        .is_none_or(|spec| fillable(ctx, item, spec))
}

pub fn candidates(ctx: &Ctx, item: &ChainItem, spec: &TargetSpec) -> Vec<TargetRef> {
    candidates_at(ctx, item, 0, spec)
}

pub fn candidates_at(
    ctx: &Ctx,
    item: &ChainItem,
    position: usize,
    spec: &TargetSpec,
) -> Vec<TargetRef> {
    let base = group_start(ctx, item, position);
    universe(ctx, item, base, spec)
        .into_iter()
        .filter(|target| matches_from(ctx, item, base, &spec.filter, *target))
        .filter(|target| from_facedown(ctx, item, spec, *target))
        .filter(|target| !untargetable(ctx, item, *target))
        .filter(|target| deflect_affordable(ctx, item, *target))
        .collect()
}

pub fn wants_same_location(filter: &Filter) -> bool {
    match filter {
        Filter::SameLocationAsPicks => true,
        Filter::Not(inner) => wants_same_location(inner),
        Filter::And(all) | Filter::Or(all) => all.iter().any(wants_same_location),
        _ => false,
    }
}

pub fn wants_different_location(filter: &Filter) -> bool {
    match filter {
        Filter::DifferentLocationFromPicks => true,
        Filter::Not(inner) => wants_different_location(inner),
        Filter::And(all) | Filter::Or(all) => all.iter().any(wants_different_location),
        _ => false,
    }
}

pub fn total_might_limit(filter: &Filter) -> Option<u8> {
    match filter {
        Filter::TotalMightAtMost(limit) => Some(*limit),
        Filter::Not(inner) => total_might_limit(inner),
        Filter::And(all) | Filter::Or(all) => all.iter().find_map(total_might_limit),
        _ => None,
    }
}

fn might_of_ref(ctx: &Ctx, target: TargetRef) -> i32 {
    match target {
        TargetRef::Card(card) if ctx.on_board(card) && ctx.is_unit(card) => ctx.current_might(card),
        _ => 0,
    }
}

pub fn candidates_with(
    ctx: &Ctx,
    item: &ChainItem,
    position: usize,
    spec: &TargetSpec,
    picked: &[TargetRef],
) -> Vec<TargetRef> {
    if picked.is_empty() {
        return candidates_at(ctx, item, position, spec);
    }
    let mut so_far = item.clone();
    so_far.targets.extend(picked.iter().copied());
    let anchor = if wants_same_location(&spec.filter) {
        picked
            .first()
            .and_then(|first| location_of_ref(ctx, *first))
    } else {
        None
    };
    let taken: Vec<Location> = if wants_different_location(&spec.filter) {
        picked
            .iter()
            .filter_map(|held| location_of_ref(ctx, *held))
            .collect()
    } else {
        Vec::new()
    };
    let room = total_might_limit(&spec.filter).map(|limit| {
        i32::from(limit)
            - picked
                .iter()
                .map(|held| might_of_ref(ctx, *held))
                .sum::<i32>()
    });
    candidates_at(ctx, &so_far, position, spec)
        .into_iter()
        .filter(|target| !picked.contains(target))
        .filter(|target| anchor.is_none() || location_of_ref(ctx, *target) == anchor)
        .filter(|target| !location_of_ref(ctx, *target).is_some_and(|at| taken.contains(&at)))
        .filter(|target| room.is_none_or(|room| might_of_ref(ctx, *target) <= room))
        .collect()
}

pub fn valid(ctx: &Ctx, item: &ChainItem, index: usize) -> bool {
    let Some(target) = item.targets.get(index).copied() else {
        return false;
    };
    let Some((position, spec)) = spec_at(&specs_of(ctx, item), &item.spec_counts, index) else {
        return matches!(target, TargetRef::Zone(_) | TargetRef::Seat(_));
    };
    let base = group_start(ctx, item, position);
    in_universe(ctx, item, &spec, target)
        && matches_from(ctx, item, base, &spec.filter, target)
        && !untargetable(ctx, item, target)
}

pub fn valid_cards(ctx: &Ctx, item: &ChainItem) -> Vec<u32> {
    item.targets
        .iter()
        .enumerate()
        .filter(|(index, _)| valid(ctx, item, *index))
        .filter_map(|(_, target)| match target {
            TargetRef::Card(card) => Some(*card),
            _ => None,
        })
        .collect()
}

pub fn from_answer(kind: TargetKind, value: u32) -> Option<TargetRef> {
    Some(match kind {
        TargetKind::Card => TargetRef::Card(value),
        TargetKind::Item => TargetRef::Item(u16::try_from(value).ok()?),
        TargetKind::Zone => TargetRef::Zone(u16::try_from(value).ok()?),
        TargetKind::Seat => TargetRef::Seat(u8::try_from(value).ok()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::cards::{Card, Keyword, Static, TargetKind};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Expiry, GameBlob, SLOT_PROMISED_REPEAT, SLOT_REPEAT};

    static PIERCING: Card = Card {
        name: "Piercing",
        keywords: &[],
        abilities: &[],
        statics: &[Static::IgnoresDeflect],
        replacement: None,
        additional: None,
        names: None,
        kind: None,
        adds: None,
    };

    static SHY: Card = Card {
        name: "Shy",
        keywords: &[],
        abilities: &[],
        statics: &[Static::Untargetable(|_, _| true)],
        replacement: None,
        additional: None,
        names: None,
        kind: None,
        adds: None,
    };

    static SWEEP: Card = prelude::spell(
        "Sweep",
        &[],
        &[prelude::play(
            &[
                prelude::units_up_to(2, "up to two units"),
                prelude::a_spell("a spell"),
            ],
            |_, _, _| crate::cards::Flow::Done,
        )],
    );

    static THIRD_ANCHORED: Card = prelude::spell(
        "Third Anchored",
        &[],
        &[prelude::play(
            &[
                prelude::a_unit("an anchor"),
                prelude::target(
                    Filter::MightLessThan(0),
                    0,
                    1,
                    TargetKind::Card,
                    "a unit with less Might",
                ),
            ],
            |_, _, _| crate::cards::Flow::Done,
        )],
    );

    fn spell_item(ctx: &Ctx, controller: u8) -> ChainItem {
        let _ = ctx;
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            controller,
            Origin::Hand,
        )
    }

    const UNIT_ELSEWHERE: Filter = Filter::And(&[Filter::Unit, Filter::Not(&Filter::Here)]);

    #[test]
    fn not_here_fails_closed_when_the_source_has_no_location() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let ctx = fixture.ctx();
        let in_hand = ChainItem::new(
            11,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert_eq!(
            source_location(&ctx, &in_hand),
            None,
            "a card still in hand has no location for Here to read"
        );
        assert!(
            cards(&candidates(
                &ctx,
                &in_hand,
                &prelude::a_card(UNIT_ELSEWHERE, "elsewhere")
            ))
            .is_empty(),
            "Not(Here) over an unresolvable Here offers nothing rather than everything"
        );
        let landed = ChainItem::new(
            12,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(
            source_location(&ctx, &landed),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            cards(&candidates(
                &ctx,
                &landed,
                &prelude::a_card(UNIT_ELSEWHERE, "elsewhere")
            )),
            [fixtures::SPRITE, fixtures::THEIR_UNIT],
            "with a real location Not(Here) reads as printed"
        );
    }

    fn cards(refs: &[TargetRef]) -> Vec<u32> {
        refs.iter()
            .filter_map(|target| match target {
                TargetRef::Card(card) => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn filters_pick_units_by_side_location_and_printed_cost() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let ctx = fixture.ctx();
        let mine = spell_item(&ctx, 0);
        let all = |filter: Filter, ctx: &Ctx, item: &ChainItem| {
            cards(&candidates(
                ctx,
                item,
                &prelude::target(filter, 1, 1, TargetKind::Card, "x"),
            ))
        };
        assert_eq!(
            all(Filter::Unit, &ctx, &mine),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(all(prelude::FRIENDLY_UNIT, &ctx, &mine), [fixtures::VI]);
        assert_eq!(
            all(prelude::ENEMY_UNIT, &ctx, &mine),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(
            all(prelude::UNIT_AT_BATTLEFIELD, &ctx, &mine),
            [fixtures::VI, fixtures::SPRITE]
        );
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::InBase]), &ctx, &mine),
            [fixtures::THEIR_UNIT]
        );
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::Temporary]), &ctx, &mine),
            [fixtures::SPRITE]
        );
        assert_eq!(
            all(
                Filter::And(&[Filter::Unit, Filter::Domain(crate::cards::Domain::Fury)]),
                &ctx,
                &mine
            ),
            [fixtures::VI, fixtures::THEIR_UNIT]
        );
        assert_eq!(all(Filter::Gear, &ctx, &mine), Vec::<u32>::new());
        assert_eq!(all(Filter::Rune, &ctx, &mine).len(), 6);
        assert_eq!(all(Filter::Legend, &ctx, &mine), [fixtures::LEGEND_CARD]);
        assert_eq!(
            all(
                Filter::And(&[Filter::Unit, Filter::EnergyAtMost(1)]),
                &ctx,
                &mine
            ),
            [fixtures::SPRITE],
            "a token has no printed cost"
        );
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::Exhausted]), &ctx, &mine),
            Vec::<u32>::new()
        );
        let mut chosen = mine.clone();
        chosen.targets.push(TargetRef::Card(fixtures::VI));
        assert_eq!(
            all(prelude::ANOTHER_UNIT, &ctx, &chosen),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(
            all(
                Filter::And(&[Filter::Unit, Filter::SameLocationAs(0)]),
                &ctx,
                &chosen
            ),
            [fixtures::VI]
        );
        assert_eq!(
            all(
                Filter::And(&[Filter::Unit, Filter::DifferentLocationFrom(0)]),
                &ctx,
                &chosen
            ),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert!(
            !valid(&ctx, &chosen, 0),
            "a generic spell has no spec to validate a card target against"
        );
        assert!(!valid(&ctx, &chosen, 1));
        let hall = ChainItem::new(
            8,
            ItemKind::Trigger {
                source: fixtures::GROUNDS,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(
            cards(&candidates(
                &ctx,
                &hall,
                &prelude::a_card(prelude::UNIT_HERE, "here")
            )),
            [fixtures::VI]
        );
        assert_eq!(
            source_location(&ctx, &hall),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let facedown = ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Facedown {
                zone: fixtures::BF2,
            },
        );
        assert_eq!(
            cards(&candidates(
                &ctx,
                &facedown,
                &prelude::a_card(prelude::UNIT_HERE, "here")
            )),
            [fixtures::SPRITE]
        );
        let zones = candidates(&ctx, &mine, &prelude::a_battlefield("where"));
        assert_eq!(
            zones,
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BF2)
            ]
        );
        let seats = candidates(
            &ctx,
            &mine,
            &prelude::target(Filter::Enemy, 1, 1, TargetKind::Seat, "an opponent"),
        );
        assert_eq!(seats, [TargetRef::Seat(1)]);
        let base = candidates(
            &ctx,
            &mine,
            &prelude::target(Filter::Any, 1, 1, TargetKind::Zone, "where"),
        );
        assert_eq!(base, [TargetRef::Zone(fixtures::BASE)]);
        assert_eq!(from_answer(TargetKind::Seat, 300), None);
        assert_eq!(from_answer(TargetKind::Item, 3), Some(TargetRef::Item(3)));
        ctx.blob.set_holder(fixtures::BF1, Some(0));
        ctx.blob.card_state_mut(fixtures::HAND_HIDDEN).hidden_at = Some(fixtures::BF1);
        let hidden = candidates(
            &ctx,
            &mine,
            &prelude::target(Filter::HiddenBattlefield, 1, 1, TargetKind::Zone, "where"),
        );
        assert_eq!(hidden, [TargetRef::Zone(fixtures::BF1)]);
    }

    #[test]
    fn a_facedown_card_joins_a_card_universe_only_when_the_filter_asks_for_it() {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let ctx = fixture.ctx();
        ctx.blob.card_state_mut(fixtures::HAND_HIDDEN).hidden_at = Some(fixtures::BF1);
        let mine = spell_item(&ctx, 0);
        let theirs = spell_item(&ctx, 1);
        const FRIENDLY_PIECE_OR_FACEDOWN: Filter = Filter::And(&[
            Filter::Or(&[Filter::Unit, Filter::Facedown]),
            Filter::Friendly,
        ]);
        assert!(admits_facedown(&FRIENDLY_PIECE_OR_FACEDOWN));
        assert!(!admits_facedown(&Filter::And(&[
            Filter::Unit,
            Filter::Friendly
        ])));
        assert_eq!(
            cards(&candidates(
                &ctx,
                &mine,
                &prelude::a_card(FRIENDLY_PIECE_OR_FACEDOWN, "a piece or hidden card")
            )),
            [fixtures::VI, fixtures::HAND_HIDDEN],
            "the controller's facedown card stands beside the units"
        );
        assert!(
            !cards(&candidates(
                &ctx,
                &mine,
                &prelude::a_card(Filter::Any, "anything")
            ))
            .contains(&fixtures::HAND_HIDDEN),
            "a filter without Facedown never sees it"
        );
        assert!(
            !cards(&candidates(
                &ctx,
                &theirs,
                &prelude::a_card(Filter::Facedown, "a hidden card")
            ))
            .contains(&fixtures::HAND_HIDDEN),
            "an opponent's item never reaches another seat's facedown card"
        );
    }

    #[test]
    fn a_zone_spec_anchored_to_a_chosen_unit_offers_where_that_unit_may_move() {
        let mut fixture = Fixture::enforced();
        let ctx = fixture.ctx();
        let mut charm = spell_item(&ctx, 0);
        charm.targets.push(TargetRef::Card(fixtures::SPRITE));
        assert_eq!(anchor_of(&prelude::CHARM_DESTINATION.filter), Some(0));
        assert_eq!(
            anchor_of(&Filter::And(&[
                Filter::Unit,
                Filter::Not(&Filter::SameLocationAs(3))
            ])),
            Some(3)
        );
        assert_eq!(anchor_of(&Filter::AtBattlefield), None);
        assert_eq!(
            candidates(&ctx, &charm, &prelude::CHARM_DESTINATION),
            [
                TargetRef::Zone(fixtures::BASE),
                TargetRef::Zone(fixtures::BF1)
            ],
            "its own base and the other battlefield, never where it stands"
        );
        assert_eq!(
            zone_location(&ctx, &charm, 0, &prelude::CHARM_DESTINATION, fixtures::BASE),
            Some(Location::Base(1)),
            "the base zone is the anchored unit's controller's base"
        );
        assert_eq!(
            zone_location(
                &ctx,
                &charm,
                0,
                &prelude::a_battlefield("where"),
                fixtures::BASE
            ),
            Some(Location::Base(0)),
            "without an anchor it is the item controller's"
        );
        let with_it = prelude::target(
            Filter::SameLocationAs(0),
            1,
            1,
            TargetKind::Zone,
            "where it stands",
        );
        assert_eq!(
            candidates(&ctx, &charm, &with_it),
            [TargetRef::Zone(fixtures::BF2)]
        );
        let mut home = spell_item(&ctx, 0);
        home.targets.push(TargetRef::Card(fixtures::THEIR_UNIT));
        assert_eq!(
            candidates(&ctx, &home, &prelude::CHARM_DESTINATION),
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BF2)
            ],
            "a unit in its base is offered only the battlefields"
        );
        assert_eq!(
            candidates(&ctx, &home, &with_it),
            [TargetRef::Zone(fixtures::BASE)],
            "and the base zone reads as its own base"
        );
        let across = prelude::target(
            Filter::ToOrFromBaseOf(0),
            1,
            1,
            TargetKind::Zone,
            "to or from its base",
        );
        assert_eq!(anchor_of(&across.filter), Some(0));
        assert_eq!(
            candidates(&ctx, &charm, &across),
            [TargetRef::Zone(fixtures::BASE)],
            "a unit at a battlefield crosses only to its base"
        );
        assert_eq!(
            candidates(&ctx, &home, &across),
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BF2)
            ],
            "a unit in its base crosses only to a battlefield"
        );
        let unanchored = spell_item(&ctx, 0);
        assert_eq!(
            candidates(&ctx, &unanchored, &prelude::CHARM_DESTINATION),
            Vec::<TargetRef>::new(),
            "no unit chosen yet, no destination"
        );
        let mut done = charm.clone();
        done.targets.push(TargetRef::Zone(fixtures::BASE));
        done.spec_counts = vec![1, 1];
        let scripts = ctx
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &crate::cards::charm::CARD);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        let mut ctx = Ctx::fresh(&table, &mut blob, &scripts, 0);
        assert!(valid(&ctx, &done, 1));
        ctx.move_unit(
            fixtures::SPRITE,
            Location::Base(1),
            crate::engine::ctx::MoveCause::Effect,
        );
        assert!(
            !valid(&ctx, &done, 1),
            "once the unit stands in its base the base is no longer a different location"
        );
    }

    #[test]
    fn items_on_the_chain_are_targets_by_kind_controller_and_what_they_chose() {
        let mut fixture = Fixture::enforced();
        let ctx = fixture.ctx();
        let mut theirs = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        theirs.status = ItemStatus::Finalized;
        theirs.targets.push(TargetRef::Card(fixtures::VI));
        let mut ability = ChainItem::new(
            2,
            ItemKind::Trigger {
                source: fixtures::THEIR_UNIT,
                index: 0,
            },
            1,
            Origin::Board,
        );
        ability.status = ItemStatus::Finalized;
        ctx.blob.chain.push(theirs);
        ctx.blob.chain.push(ability);
        let mine = ChainItem::new(3, ItemKind::Spell { card: 99 }, 0, Origin::Hand);
        let items =
            |filter: Filter, ctx: &Ctx| candidates(ctx, &mine, &prelude::an_item(filter, "x"));
        assert_eq!(items(Filter::Spell, &ctx), [TargetRef::Item(1)]);
        assert_eq!(items(Filter::Ability, &ctx), [TargetRef::Item(2)]);
        assert_eq!(
            items(Filter::ItemOnChain, &ctx),
            [TargetRef::Item(1), TargetRef::Item(2)]
        );
        assert_eq!(
            items(prelude::ENEMY_ITEM_CHOOSING_FRIENDLY, &ctx),
            [TargetRef::Item(1)]
        );
        assert_eq!(
            items(Filter::And(&[Filter::Spell, Filter::EnergyAtMost(1)]), &ctx),
            Vec::<TargetRef>::new()
        );
        assert_eq!(
            items(
                Filter::And(&[
                    Filter::Spell,
                    Filter::EnergyAtMost(2),
                    Filter::PowerAtMost(1)
                ]),
                &ctx
            ),
            [TargetRef::Item(1)]
        );
        assert_eq!(items(Filter::Friendly, &ctx), Vec::<TargetRef>::new());
        assert_eq!(
            items(Filter::ItemControlledBy(Rel::Any), &ctx),
            [TargetRef::Item(1), TargetRef::Item(2)]
        );
        let own = ChainItem::new(1, ItemKind::Spell { card: 99 }, 1, Origin::Hand);
        assert_eq!(
            candidates(&ctx, &own, &prelude::a_spell("x")),
            Vec::<TargetRef>::new(),
            "an item never targets itself"
        );
        ctx.blob.chain[0].status = ItemStatus::Resolving;
        assert_eq!(items(Filter::Spell, &ctx), Vec::<TargetRef>::new());
        let mut chosen = mine.clone();
        chosen.targets.push(TargetRef::Item(2));
        assert!(valid(&ctx, &chosen, 0) || specs_of(&ctx, &chosen).is_empty());
    }

    #[test]
    fn deflect_hides_unaffordable_candidates_unless_the_spell_ignores_it_and_untargetable_hides_all(
    ) {
        let mut fixture = Fixture::enforced();
        fixture
            .blob
            .card_state_mut(fixtures::THEIR_UNIT)
            .granted
            .push((Keyword::Deflect(1), Expiry::Permanent));
        for rune in [41, 42] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        let spell = spell_item(&ctx, 0);
        assert_eq!(ctx.deflect_of(fixtures::THEIR_UNIT), 1);
        let spec = prelude::a_unit("a unit");
        assert_eq!(
            cards(&candidates(&ctx, &spell, &spec)),
            [fixtures::VI, fixtures::SPRITE],
            "two ready runes pay the spell but not the deflect"
        );
        let with = cost::of_item(&ctx, &spell, Some(TargetRef::Card(fixtures::THEIR_UNIT)));
        assert_eq!(with.power.len(), 2);
        assert_eq!(with.power[1], cost::Need::Rainbow);
        let mine = cost::of_item(&ctx, &spell, Some(TargetRef::Card(fixtures::VI)));
        assert_eq!(mine.power.len(), 1, "no deflect on your own unit");
        ctx.table.card_mut(41).unwrap().exhausted = false;
        assert_eq!(
            cards(&candidates(&ctx, &spell, &spec)),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT],
            "a third rune makes the deflect payable"
        );
        ctx.table.card_mut(41).unwrap().exhausted = true;
        let scripts = ctx
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &PIERCING);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        let ctx = Ctx::fresh(&table, &mut blob, &scripts, 0);
        assert_eq!(
            cards(&candidates(&ctx, &spell, &spec)),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(
            cost::of_item(&ctx, &spell, Some(TargetRef::Card(fixtures::THEIR_UNIT)))
                .power
                .len(),
            1
        );
        let scripts = scripts.with_script(fixtures::THEIR_UNIT, &SHY);
        let ctx = Ctx::fresh(&table, &mut blob, &scripts, 0);
        assert_eq!(
            cards(&candidates(&ctx, &spell, &spec)),
            [fixtures::VI, fixtures::SPRITE]
        );
        let theirs = spell_item(&ctx, 1);
        assert!(
            cards(&candidates(&ctx, &theirs, &spec)).contains(&fixtures::THEIR_UNIT),
            "untargetable only against opponents"
        );
    }

    static EQUIPMENT: Card =
        prelude::gear("Blade", &[Keyword::Equip(crate::cards::Cost::FREE)], &[]);

    #[test]
    fn the_m9_filters_read_zones_kinds_might_and_the_item_source() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::gear(90, fixtures::BASE, 0, "Blade", 1));
        fixture
            .table
            .cards
            .push(fixtures::spell(91, fixtures::TRASH, 0, "Spark", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(92, fixtures::TRASH, 1, "Dead", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(93, fixtures::BANISHMENT, 0, "Gone", 2));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(90, &EQUIPMENT);
        let ctx = fixture.ctx();
        let mine = spell_item(&ctx, 0);
        let all = |filter: Filter, item: &ChainItem| {
            cards(&candidates(
                &ctx,
                item,
                &prelude::target(filter, 1, 1, TargetKind::Card, "x"),
            ))
        };
        assert_eq!(
            all(Filter::Unit, &mine),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT],
            "the wider universe leaks nothing into Unit"
        );
        assert_eq!(all(Filter::InTrash, &mine), [91, 92]);
        assert_eq!(
            all(Filter::And(&[Filter::Spell, Filter::InTrash]), &mine),
            Vec::<u32>::new(),
            "Spell reads the chain, not the trash"
        );
        assert_eq!(
            all(prelude::FRIENDLY_SPELL_IN_TRASH, &mine),
            [91],
            "the prelude constant reads the printed kind"
        );
        assert_eq!(
            all(prelude::FRIENDLY_UNIT_IN_TRASH, &mine),
            Vec::<u32>::new()
        );
        let theirs = spell_item(&ctx, 1);
        assert_eq!(all(prelude::FRIENDLY_UNIT_IN_TRASH, &theirs), [92]);
        assert_eq!(
            all(Filter::InChampionZone, &mine),
            [fixtures::CHAMPION_CARD],
            "the Champion Zone is public and in the universe"
        );
        assert_eq!(
            all(
                Filter::And(&[Filter::Champion("Lillia"), Filter::Kind("Unit")]),
                &mine
            ),
            [fixtures::CHAMPION_CARD],
            "a champion unit is named after its champion"
        );
        assert_eq!(all(Filter::Champion("Lil"), &mine), Vec::<u32>::new());
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::Owned]), &mine),
            [fixtures::VI],
            "the Sprite is seat 1's"
        );
        assert_eq!(
            all(
                Filter::And(&[Filter::InTrash, Filter::Friendly, Filter::Kind("Spell")]),
                &mine
            ),
            [91]
        );
        assert_eq!(
            all(Filter::And(&[Filter::InTrash, Filter::Kind("Unit")]), &mine),
            [92]
        );
        assert_eq!(all(Filter::InBanishment, &mine), [93]);
        assert_eq!(
            all(Filter::InHand, &mine),
            Vec::<u32>::new(),
            "an unrevealed hand is outside the universe"
        );
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::MightAtMost(2)]), &mine),
            [fixtures::THEIR_UNIT]
        );
        assert_eq!(all(prelude::FRIENDLY_EQUIPMENT, &mine), [90]);
        assert_eq!(
            all(
                Filter::And(&[Filter::Gear, Filter::Not(&Filter::Equipment)]),
                &mine
            ),
            Vec::<u32>::new()
        );
        let trigger = ChainItem::new(
            9,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(
            all(prelude::ANOTHER_FRIENDLY_UNIT_THAN_ME, &trigger),
            Vec::<u32>::new(),
            "Vi is the only friendly unit"
        );
        assert_eq!(
            all(prelude::ANOTHER_UNIT_THAN_ME, &trigger),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(
            all(Filter::And(&[Filter::Unit, Filter::MovableToBase]), &mine),
            [fixtures::SPRITE],
            "only a unit at a battlefield can move to base"
        );
        drop(ctx);
        let mut revealed = fixture.ctx();
        revealed.table.revealed.push(fixtures::HAND_UNIT);
        revealed.table.revealed.sort_unstable();
        assert_eq!(
            cards(&candidates(
                &revealed,
                &mine,
                &prelude::target(Filter::InHand, 1, 1, TargetKind::Card, "x")
            )),
            [fixtures::HAND_UNIT],
            "a revealed hand card joins the universe"
        );
    }

    #[test]
    fn same_location_as_picks_narrows_later_picks_to_the_first_picks_location() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 1, "Jinx", 2));
        let ctx = fixture.ctx();
        let bellows = spell_item(&ctx, 0);
        let spec = prelude::target(
            Filter::And(&[Filter::Unit, Filter::SameLocationAsPicks]),
            0,
            3,
            TargetKind::Card,
            "up to three units at one location",
        );
        assert!(wants_same_location(&spec.filter));
        assert!(!wants_same_location(&prelude::UNIT));
        assert_eq!(
            cards(&candidates_with(&ctx, &bellows, 0, &spec, &[])),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT, 90]
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &bellows,
                0,
                &spec,
                &[TargetRef::Card(fixtures::VI)]
            )),
            [90]
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &bellows,
                0,
                &spec,
                &[TargetRef::Card(fixtures::THEIR_UNIT)]
            )),
            Vec::<u32>::new()
        );
    }

    #[test]
    fn total_might_at_most_refuses_a_pick_that_would_break_the_group_total() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(91, fixtures::BF2, 1, "Colossus", 6));
        let ctx = fixture.ctx();
        let item = spell_item(&ctx, 0);
        let spec = prelude::target(
            Filter::And(&[Filter::Unit, Filter::Enemy, Filter::TotalMightAtMost(5)]),
            0,
            5,
            TargetKind::Card,
            "enemy units with total Might 5 or less",
        );
        assert_eq!(total_might_limit(&spec.filter), Some(5));
        assert_eq!(total_might_limit(&prelude::ENEMY_UNIT), None);
        assert_eq!(
            cards(&candidates_with(&ctx, &item, 0, &spec, &[])),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, 90],
            "the 6-Might Colossus is over the limit on its own"
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &item,
                0,
                &spec,
                &[TargetRef::Card(fixtures::SPRITE)]
            )),
            [fixtures::THEIR_UNIT],
            "3 used: the 2 fits, the 4 does not"
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &item,
                0,
                &spec,
                &[
                    TargetRef::Card(fixtures::SPRITE),
                    TargetRef::Card(fixtures::THEIR_UNIT)
                ]
            )),
            Vec::<u32>::new(),
            "5 used: nothing else fits"
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &item,
                0,
                &spec,
                &[TargetRef::Card(90)]
            )),
            Vec::<u32>::new(),
            "4 used: neither the 3 nor the 2 fits"
        );
    }

    #[test]
    fn different_location_from_picks_refuses_a_second_pick_where_one_already_stands() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 1, "Brute", 4));
        let ctx = fixture.ctx();
        let item = spell_item(&ctx, 0);
        let spec = prelude::target(
            Filter::And(&[prelude::ENEMY_UNIT, Filter::DifferentLocationFromPicks]),
            0,
            7,
            TargetKind::Card,
            "up to one enemy unit at each location",
        );
        assert!(wants_different_location(&spec.filter));
        assert!(!wants_different_location(&prelude::ENEMY_UNIT));
        assert_eq!(
            cards(&candidates_with(&ctx, &item, 0, &spec, &[])),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, 90]
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &item,
                0,
                &spec,
                &[TargetRef::Card(fixtures::SPRITE)]
            )),
            [fixtures::THEIR_UNIT],
            "the Brute shares the Sprite's battlefield"
        );
        assert_eq!(
            cards(&candidates_with(
                &ctx,
                &item,
                0,
                &spec,
                &[TargetRef::Card(fixtures::THEIR_UNIT), TargetRef::Card(90)]
            )),
            Vec::<u32>::new(),
            "their base and battlefield 2 are both taken"
        );
    }

    #[test]
    fn a_repeated_item_doubles_its_specs_and_spec_of_index_returns_a_copy() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &SWEEP);
        let ctx = fixture.ctx();
        let mut sweep = spell_item(&ctx, 0);
        assert_eq!(specs_of(&ctx, &sweep).len(), 2);
        sweep.set_slot(crate::state::SLOT_REPEAT, 1);
        let doubled = specs_of(&ctx, &sweep);
        assert_eq!(doubled.len(), 4);
        assert_eq!(doubled[0], doubled[2]);
        assert_eq!(
            spec_of_index(&doubled, &[2, 1, 1, 1], 3).map(|spec| spec.kind),
            Some(TargetKind::Card)
        );
        assert_eq!(
            spec_of_index(&doubled, &[2, 1, 1, 1], 4).map(|spec| spec.kind),
            Some(TargetKind::Item)
        );
        assert_eq!(spec_of_index(&doubled, &[2, 1, 1, 1], 5), None);
        sweep.set_slot(crate::state::SLOT_REPEAT, 0);
        assert_eq!(specs_of(&ctx, &sweep).len(), 2);
    }

    #[test]
    fn a_target_after_a_variable_spec_is_validated_against_its_own_spec() {
        let mut fixture = Fixture::enforced();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &SWEEP);
        let ctx = fixture.ctx();
        let mut theirs = ChainItem::new(1, ItemKind::Spell { card: 99 }, 1, Origin::Hand);
        theirs.status = ItemStatus::Finalized;
        ctx.blob.chain.push(theirs);
        let mut sweep = spell_item(&ctx, 0);
        sweep.targets = vec![TargetRef::Card(fixtures::VI), TargetRef::Item(1)];
        assert!(valid(&ctx, &sweep, 0));
        assert!(
            !valid(&ctx, &sweep, 1),
            "without a record the spell is read as the second unit"
        );
        sweep.spec_counts = vec![1, 1];
        assert!(valid(&ctx, &sweep, 0));
        assert!(valid(&ctx, &sweep, 1));
        assert_eq!(of_spec(&ctx, &sweep, 0), [TargetRef::Card(fixtures::VI)]);
        assert_eq!(of_spec(&ctx, &sweep, 1), [TargetRef::Item(1)]);
        assert_eq!(of_spec(&ctx, &sweep, 2), Vec::<TargetRef>::new());
        assert_eq!(valid_cards(&ctx, &sweep), [fixtures::VI]);
        let mut none = spell_item(&ctx, 0);
        none.targets = vec![TargetRef::Item(1)];
        none.spec_counts = vec![0, 1];
        assert!(valid(&ctx, &none, 0));
        assert_eq!(of_spec(&ctx, &none, 1), [TargetRef::Item(1)]);
        let spec = prelude::units_up_to(2, "units");
        let picked = [TargetRef::Card(fixtures::VI)];
        assert_eq!(
            cards(&candidates_with(&ctx, &none, 0, &spec, &picked)),
            [fixtures::SPRITE, fixtures::THEIR_UNIT],
            "a pick already made is not offered again"
        );
    }

    #[test]
    fn a_third_repeated_group_reads_its_own_anchor_after_reconstruction() {
        let mut fixture = Fixture::enforced();
        for (id, might) in [(110, 1), (111, 2), (112, 4)] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BASE, 1, "Anchor Unit", might));
        }
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &THIRD_ANCHORED);
        let ctx = fixture.ctx();
        let mut item = spell_item(&ctx, 0);
        item.set_slot(SLOT_REPEAT, 1);
        item.set_slot(SLOT_PROMISED_REPEAT, 1);
        item.spec_counts = vec![1, 1, 1, 1, 1];
        item.targets = vec![
            TargetRef::Card(112),
            TargetRef::Card(110),
            TargetRef::Card(111),
            TargetRef::Card(110),
            TargetRef::Card(112),
        ];
        let table = ctx.table.clone();
        let mut blob = GameBlob::decode(&ctx.blob.encode()).unwrap();
        blob.chain.push(item);
        blob = GameBlob::decode(&blob.encode()).unwrap();
        let scripts = ctx.scripts.clone();
        drop(ctx);
        let ctx = Ctx::fresh(&table, &mut blob, &scripts, 0);
        let item = ctx.blob.chain[0].clone();
        let specs = specs_of(&ctx, &item);
        assert_eq!(specs.len(), 6);
        assert_eq!(group_start(&ctx, &item, 5), 4);
        let candidates = cards(&candidates_with(&ctx, &item, 5, &specs[5], &[]));
        assert!(
            candidates.contains(&111),
            "the third anchor is 112, so 111 is legal"
        );
        assert!(
            !candidates.contains(&112),
            "the anchor itself is forbidden by MightLessThan"
        );
    }
}

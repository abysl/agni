use crate::cards::{Keyword, Paying, KIND_GEAR, KIND_SPELL, KIND_UNIT};
use crate::engine::ctx::{Ctx, EntryMove, Location};
use crate::engine::{activate, combat, cost, hide, march, pay, play, prompts, statics};
use crate::state::{
    ChainItem, ItemKind, Leave, Origin, Phase, PlayLock, PromptWhy, TargetRef,
    FLAG_NO_MOVE_BY_OWNER, SLOT_ACCELERATE,
};
use crate::Refusal;
use agni_plugin_sdk::decide::TOP;
use agni_plugin_sdk::prompt::PickRefusal;
use agni_plugin_sdk::view::{
    Arrow, ArrowKind, ChainRow, Legal, LegalKind, Origin as ArrowFrom, TargetRef as Aim,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Play {
        card: u32,
        origin: Origin,
        location: Option<Location>,
        on_chain: bool,
    },
    StandardMove {
        unit: u32,
        from: Location,
        to: Location,
    },
    Hide {
        card: u32,
        zone: u16,
    },
    PlayFromFacedown {
        card: u32,
    },
    Mulligan {
        card: u32,
    },
    Discard {
        card: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NoSuchCard,
    NotYourCard,
    UnknownDestination,
    InSetup,
    TooManySetAside,
    ChainResolvesItself,
    RunesArePaid,
    DrawsAreAutomatic,
    KillsAreAutomatic,
    TrashIsFinal,
    LegendStays,
    BattlefieldStays,
    SideboardStays,
    GearStays,
    ToHand,
    Unplayable,
    Unrevealed,
    HideFromHand,
    NotYourBase,
    SpellsToChain,
    GearToBase,
    NotHeld,
    NoUnitsPlayedHere,
    ChampionToBoard,
    NotActionPhase,
    ChainClosed,
    NotYourPriority,
    NotALegalTarget,
    ShowdownTiming,
    ClosedTiming,
    NoSpells,
    NoUnits,
    NoGear,
    OnlyUnitsMove,
    Locked,
    NeedsGanking,
    SameLocation,
    TwoOtherSeats,
    HideNeedsHold,
    OneFacedown,
    HiddenThisTurn,
    NotFacedown,
    Facedown,
    NoHiddenKeyword,
    NoLegalTargets,
    NoSuchAbility,
    NotInPlay,
    AlreadyActivated,
    Attached,
    TokensByEffect,
    AnnotationsAutomatic,
    CountersAutomatic,
    DealIsOver,
    EngineFault,
    NotEnoughXp,
    AlreadyEmpowered,
    NotEmpowered,
    NoMoveToBase,
    UnitsOnlyToBase,
}

impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Reason::NoSuchCard => "no such card on the table",
            Reason::NotYourCard => "that is not your card",
            Reason::UnknownDestination => "that is not a place on the table",
            Reason::InSetup => "the game is still in setup: finish the mulligan first",
            Reason::TooManySetAside => "you may set aside up to two cards",
            Reason::ChainResolvesItself => "the chain resolves itself",
            Reason::RunesArePaid => "runes are paid for you",
            Reason::DrawsAreAutomatic => "draws and recycles are automatic",
            Reason::KillsAreAutomatic => "kills and discards are automatic",
            Reason::TrashIsFinal => "cards leave the trash only through effects",
            Reason::LegendStays => "the legend stays in its zone",
            Reason::BattlefieldStays => "battlefields stay where they are",
            Reason::SideboardStays => "the sideboard is not in play",
            Reason::GearStays => "gear stays where it was played",
            Reason::ToHand => "cards return to hand only through effects",
            Reason::Unplayable => "that card is not playable",
            Reason::Unrevealed => "a hidden card is played only face down at a battlefield",
            Reason::HideFromHand => "only a card in hand or your champion is hidden",
            Reason::NotYourBase => "that is not your base",
            Reason::SpellsToChain => "spells are played to the chain",
            Reason::GearToBase => "gear is played to your base",
            Reason::NotHeld => "you must hold that battlefield to play a unit there",
            Reason::NoUnitsPlayedHere => "units can't be played at that battlefield",
            Reason::ChampionToBoard => "the champion is played to your base or a held battlefield",
            Reason::NotActionPhase => "wait for the action phase",
            Reason::ChainClosed => "wait for the chain to resolve",
            Reason::NotYourPriority => "another seat holds priority",
            Reason::NotALegalTarget => "that is not a legal target",
            Reason::ShowdownTiming => "a showdown is open: only Action and Reaction cards play now",
            Reason::ClosedTiming => "only Reaction cards play while the chain is closed",
            Reason::NoSpells => "you can't play spells this turn",
            Reason::NoUnits => "you can't play units this turn",
            Reason::NoGear => "you can't play gear this turn",
            Reason::OnlyUnitsMove => "only units move",
            Reason::Locked => "you can't move that unit this turn",
            Reason::NeedsGanking => "only a Ganking unit moves from battlefield to battlefield",
            Reason::SameLocation => "that unit is already there",
            Reason::TwoOtherSeats => "that battlefield already holds two other players' units",
            Reason::HideNeedsHold => "you must hold that battlefield to hide a card there",
            Reason::OneFacedown => "that battlefield already has a facedown card",
            Reason::HiddenThisTurn => "a card hidden this turn is played from the next turn on",
            Reason::NotFacedown => "that card is not hidden at a battlefield",
            Reason::Facedown => "a facedown card has no abilities to activate",
            Reason::NoHiddenKeyword => {
                "that card has no Hidden, so it stays face down where it was hidden"
            }
            Reason::NoLegalTargets => {
                "a hidden spell needs a legal target at the battlefield it was hidden at"
            }
            Reason::NoSuchAbility => "that card has no such activated ability",
            Reason::NotInPlay => "that card is not in play",
            Reason::AlreadyActivated => "that ability is once each turn",
            Reason::Attached => "an attached gear's own text is inactive",
            Reason::TokensByEffect => "tokens are played by card effects",
            Reason::AnnotationsAutomatic => "marks on cards are automatic",
            Reason::CountersAutomatic => "counters are automatic",
            Reason::DealIsOver => "the deal happened before the start",
            Reason::EngineFault => "the engine could not apply its own effects",
            Reason::NotEnoughXp => "you don't have enough XP to spend",
            Reason::AlreadyEmpowered => "that card is already Empowered",
            Reason::NotEmpowered => "that card is not Empowered",
            Reason::NoMoveToBase => "units can't move from that battlefield to base",
            Reason::UnitsOnlyToBase => "you can only play units to your base right now",
        }
    }
}

fn illegal<T>(reason: Reason) -> Result<T, Refusal> {
    Err(Refusal::Illegal(reason))
}

pub fn unlocked(ctx: &Ctx, seat: u8, kind: &str) -> Result<(), Refusal> {
    let lock = PlayLock::of_kind(kind);
    if ctx.blob.seat(seat).play_lock.contains(lock) {
        return illegal(match lock {
            PlayLock::SPELLS => Reason::NoSpells,
            PlayLock::GEAR => Reason::NoGear,
            _ => Reason::NoUnits,
        });
    }
    Ok(())
}

pub fn classify(ctx: &Ctx, seat: u8, mv: &EntryMove) -> Result<Intent, Refusal> {
    if ctx.card(mv.card).is_none() {
        return illegal(Reason::NoSuchCard);
    }
    if ctx.controller(mv.card) != seat {
        return illegal(Reason::NotYourCard);
    }
    let Some(to) = mv.to else {
        return illegal(Reason::UnknownDestination);
    };
    let zones = &ctx.zones;
    let from = mv.from;
    if let Some(gesture) = gesture(ctx, seat, mv, to)? {
        return Ok(gesture);
    }
    if ctx.blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    if ctx.blob.phase() == Some(Phase::Setup) {
        return illegal(Reason::InSetup);
    }
    if from == Some(to) && !zones.is_battlefield(to) && zones.base != Some(to) {
        return illegal(Reason::SameLocation);
    }
    let is = |zone: Option<u16>| zone.is_some() && zone == from;
    if is(zones.chain) {
        return illegal(Reason::ChainResolvesItself);
    }
    if is(zones.main_deck) {
        return illegal(Reason::DrawsAreAutomatic);
    }
    if is(zones.rune_deck) || is(zones.rune_pool) {
        return illegal(Reason::RunesArePaid);
    }
    if is(zones.trash) {
        return from_trash(ctx, seat, mv, to);
    }
    if is(zones.legend) {
        return illegal(Reason::LegendStays);
    }
    if is(zones.sideboard) {
        return illegal(Reason::SideboardStays);
    }
    if is(zones.hand) {
        return from_hand(ctx, seat, mv, to);
    }
    if mv.hidden && !is(zones.champion) {
        return illegal(Reason::HideFromHand);
    }
    if is(zones.champion) {
        return from_champion(ctx, seat, mv, to);
    }
    let on_board = from.is_some_and(|zone| zones.base == Some(zone) || zones.is_battlefield(zone));
    if on_board {
        return from_board(ctx, seat, mv, to);
    }
    illegal(Reason::UnknownDestination)
}

fn gesture(ctx: &Ctx, seat: u8, mv: &EntryMove, to: u16) -> Result<Option<Intent>, Refusal> {
    let (Some(prompt), Some(why)) = (&ctx.blob.prompt, ctx.blob.why) else {
        return Ok(None);
    };
    let zones = &ctx.zones;
    let from_hand = mv.from.is_some() && mv.from == zones.hand;
    match why {
        PromptWhy::Mulligan if from_hand && zones.main_deck == Some(to) && mv.to_seat == seat => {
            if prompt.seat != seat {
                return Err(Refusal::Pick(PickRefusal::NotYourPrompt {
                    seat: prompt.seat,
                }));
            }
            if prompt.is_picked(mv.card) {
                return Err(Refusal::AlreadyPicked);
            }
            if prompt.is_full() {
                return illegal(Reason::TooManySetAside);
            }
            Ok(Some(Intent::Mulligan { card: mv.card }))
        }
        PromptWhy::Discard { .. } if from_hand && zones.trash == Some(to) && mv.to_seat == seat => {
            if prompt.seat != seat {
                return Err(Refusal::Pick(PickRefusal::NotYourPrompt {
                    seat: prompt.seat,
                }));
            }
            if prompt.is_picked(mv.card) {
                return Err(Refusal::AlreadyPicked);
            }
            Ok(Some(Intent::Discard { card: mv.card }))
        }
        _ => Ok(None),
    }
}

fn from_hand(ctx: &Ctx, seat: u8, mv: &EntryMove, to: u16) -> Result<Intent, Refusal> {
    let zones = &ctx.zones;
    let card = ctx
        .card(mv.card)
        .ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
    if zones.is_deck(to) {
        return illegal(Reason::DrawsAreAutomatic);
    }
    if zones.trash == Some(to) {
        return illegal(Reason::KillsAreAutomatic);
    }
    if zones.hand == Some(to) {
        return illegal(Reason::SameLocation);
    }
    if zones.sideboard == Some(to) {
        return illegal(Reason::SideboardStays);
    }
    if zones.rune_pool == Some(to) {
        return illegal(Reason::RunesArePaid);
    }
    if card.is_hidden() || mv.hidden {
        if zones.is_battlefield(to) {
            hide::legal(ctx, seat, mv.card, to)?;
            return Ok(Intent::Hide {
                card: mv.card,
                zone: to,
            });
        }
        return illegal(Reason::Unrevealed);
    }
    play_to(ctx, seat, mv, to, Origin::Hand)
}

fn from_champion(ctx: &Ctx, seat: u8, mv: &EntryMove, to: u16) -> Result<Intent, Refusal> {
    let zones = &ctx.zones;
    let card = ctx
        .card(mv.card)
        .ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
    if card.is_hidden() || mv.hidden {
        if zones.is_battlefield(to) {
            hide::legal(ctx, seat, mv.card, to)?;
            return Ok(Intent::Hide {
                card: mv.card,
                zone: to,
            });
        }
        return illegal(Reason::Unrevealed);
    }
    if !(zones.chain == Some(to) || zones.base == Some(to) || zones.is_battlefield(to)) {
        return illegal(Reason::ChampionToBoard);
    }
    play_to(ctx, seat, mv, to, Origin::Champion)
}

fn from_trash(ctx: &Ctx, seat: u8, mv: &EntryMove, to: u16) -> Result<Intent, Refusal> {
    let zones = &ctx.zones;
    let Some(_flow) = flow_cost(ctx, mv.card) else {
        return illegal(Reason::TrashIsFinal);
    };
    if zones.chain != Some(to) {
        return illegal(Reason::SpellsToChain);
    }
    timing_at(ctx, seat, mv.card, None)?;
    unlocked(ctx, seat, KIND_SPELL)?;
    if spells_locked(ctx, seat, mv.card) {
        return illegal(Reason::NoSpells);
    }
    play::affordable(
        ctx,
        &cost::play_item(
            ctx,
            seat,
            mv.card,
            Origin::Trash {
                leave: Leave::Banish,
            },
        ),
    )?;
    Ok(Intent::Play {
        card: mv.card,
        origin: Origin::Trash {
            leave: Leave::Banish,
        },
        location: None,
        on_chain: true,
    })
}

pub fn ambush_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    let into_enemies = ctx
        .script(card)
        .is_some_and(|script| script.has_static(crate::cards::Static::AmbushIntoEnemies));
    ctx.ambush_locations(seat, card)
        .into_iter()
        .filter(|location| {
            ctx.units_at(*location).into_iter().any(|unit| {
                unit != card
                    && (ctx.controller(unit) == seat
                        || (into_enemies && ctx.controller(unit) != seat))
            })
        })
        .collect()
}

fn ambushes_among_units(ctx: &Ctx, seat: u8, card: u32, location: Location) -> bool {
    let into_enemies = ctx
        .script(card)
        .is_some_and(|script| script.has_static(crate::cards::Static::AmbushIntoEnemies));
    ctx.units_at(location).into_iter().any(|unit| {
        unit != card
            && (ctx.controller(unit) == seat || (into_enemies && ctx.controller(unit) != seat))
    })
}

fn reaches_battlefield(ctx: &Ctx, seat: u8, card: u32, zone: u16) -> bool {
    let there = Location::Battlefield(zone);
    (ctx.ambush_battlefields(seat, card).contains(&zone)
        && ambushes_among_units(ctx, seat, card, there))
        || statics::granted_play_locations(ctx, seat, card).contains(&there)
}

pub fn spells_locked(ctx: &Ctx, seat: u8, card: u32) -> bool {
    ctx.blob.seat(seat).play_lock.contains(PlayLock::SPELLS)
        || crate::cards::fallen_feline::cannot_play(ctx, seat, card)
}

pub fn flow_cost(ctx: &Ctx, card: u32) -> Option<cost::Cost> {
    let flow = ctx.flow_of(card)?;
    Some(flow)
}

fn play_to(
    ctx: &Ctx,
    seat: u8,
    mv: &EntryMove,
    to: u16,
    origin: Origin,
) -> Result<Intent, Refusal> {
    let zones = &ctx.zones;
    let card = ctx
        .card(mv.card)
        .ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
    let kind = card.kind.as_deref().unwrap_or(KIND_UNIT);
    if !matches!(kind, KIND_UNIT | KIND_GEAR | KIND_SPELL) {
        return illegal(Reason::Unplayable);
    }
    let location = if zones.chain == Some(to) {
        None
    } else if zones.base == Some(to) {
        if mv.to_seat != seat {
            return illegal(Reason::NotYourBase);
        }
        if kind == KIND_SPELL {
            return illegal(Reason::SpellsToChain);
        }
        let here = Location::Base(seat);
        if kind == KIND_UNIT
            && ctx
                .only_play_locations(seat, mv.card)
                .is_some_and(|only| !only.contains(&here))
        {
            return illegal(Reason::NotHeld);
        }
        Some(here)
    } else if zones.is_battlefield(to) {
        match kind {
            KIND_SPELL => return illegal(Reason::SpellsToChain),
            KIND_GEAR => return illegal(Reason::GearToBase),
            _ => {}
        }
        let there = Location::Battlefield(to);
        if ctx.units_only_to_base(seat) {
            return illegal(Reason::UnitsOnlyToBase);
        }
        if ctx
            .only_play_locations(seat, mv.card)
            .is_some_and(|only| !only.contains(&there))
        {
            return illegal(if ctx.units_played_here(to) {
                Reason::NotHeld
            } else {
                Reason::NoUnitsPlayedHere
            });
        }
        let admitted = ctx.holds(seat, to)
            || ambush_locations(ctx, seat, mv.card).contains(&there)
            || ctx.granted_play_locations(seat, mv.card).contains(&there);
        if !admitted && (ctx.units_played_here(to) || !reaches_battlefield(ctx, seat, mv.card, to))
        {
            return illegal(Reason::NotHeld);
        }
        if !ctx.units_played_here(to) {
            return illegal(Reason::NoUnitsPlayedHere);
        }
        Some(Location::Battlefield(to))
    } else {
        return illegal(Reason::Unplayable);
    };
    timing_at(ctx, seat, mv.card, location)?;
    unlocked(ctx, seat, kind)?;
    if kind == KIND_SPELL && spells_locked(ctx, seat, mv.card) {
        return illegal(Reason::NoSpells);
    }
    play::affordable(ctx, &cost::play_item(ctx, seat, mv.card, origin))?;
    Ok(Intent::Play {
        card: mv.card,
        origin,
        location,
        on_chain: location.is_none(),
    })
}

pub fn timing(ctx: &Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    timing_as(ctx, seat, card, hide::reacts(ctx, card))
}

pub fn timing_at(
    ctx: &Ctx,
    seat: u8,
    card: u32,
    location: Option<Location>,
) -> Result<(), Refusal> {
    let own = timing(ctx, seat, card);
    if own.is_ok() {
        return own;
    }
    let ambush = ambush_locations(ctx, seat, card);
    let ambushing = match location {
        Some(location) => ambush.contains(&location),
        None => !ambush.is_empty(),
    };
    if ambushing {
        return timing_as(ctx, seat, card, true);
    }
    own
}

pub fn locations_for(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    let mut locations = ctx.play_locations_for(seat, card);
    for location in ambush_locations(ctx, seat, card) {
        if !locations.contains(&location) {
            locations.push(location);
        }
    }
    locations
        .into_iter()
        .filter(|location| timing_at(ctx, seat, card, Some(*location)).is_ok())
        .collect()
}

fn timing_as(ctx: &Ctx, seat: u8, card: u32, reaction: bool) -> Result<(), Refusal> {
    let blob = &*ctx.blob;
    if blob.priority.is_some() || !blob.chain.is_empty() {
        return closed_timing(ctx, seat, reaction);
    }
    if blob.phase() != Some(Phase::Action) {
        return illegal(Reason::NotActionPhase);
    }
    if let Some(showdown) = &blob.showdown {
        if !showdown.window.has_focus(seat) {
            return Err(Refusal::NotYourFocus);
        }
        if ctx.has_keyword(card, Keyword::Action) || reaction {
            return Ok(());
        }
        return illegal(Reason::ShowdownTiming);
    }
    if !blob.is_turn_player(seat) {
        return Err(Refusal::NotYourTurn);
    }
    Ok(())
}

fn closed_timing(ctx: &Ctx, seat: u8, reaction: bool) -> Result<(), Refusal> {
    let holder = ctx.blob.priority.map(|priority| priority.active);
    if holder != Some(seat) {
        return illegal(Reason::ChainClosed);
    }
    if reaction {
        return Ok(());
    }
    illegal(Reason::ClosedTiming)
}

fn from_board(ctx: &Ctx, seat: u8, mv: &EntryMove, to: u16) -> Result<Intent, Refusal> {
    let zones = &ctx.zones;
    let card = ctx
        .card(mv.card)
        .ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
    if hide::is_facedown(ctx, mv.card) {
        if zones.chain == Some(to) {
            hide::play_legal(ctx, seat, mv.card)?;
            return Ok(Intent::PlayFromFacedown { card: mv.card });
        }
        return illegal(Reason::Unrevealed);
    }
    if ctx.is_rune(mv.card) {
        return illegal(Reason::RunesArePaid);
    }
    if ctx.is_battlefield_card(mv.card) {
        return illegal(Reason::BattlefieldStays);
    }
    if ctx.is_legend(mv.card) {
        return illegal(Reason::LegendStays);
    }
    if ctx.is_gear(mv.card) {
        return illegal(Reason::GearStays);
    }
    if !ctx.is_unit(mv.card) || card.is_hidden() {
        return illegal(Reason::OnlyUnitsMove);
    }
    if zones.hand == Some(to) {
        return illegal(Reason::ToHand);
    }
    if zones.trash == Some(to) {
        return illegal(Reason::KillsAreAutomatic);
    }
    if zones.is_deck(to) {
        return illegal(Reason::DrawsAreAutomatic);
    }
    if zones.chain == Some(to) {
        return illegal(Reason::Unplayable);
    }
    let from_zone = mv
        .from
        .ok_or(Refusal::Illegal(Reason::UnknownDestination))?;
    let from = Location::of_zone(from_zone, mv.from_seat, zones)
        .ok_or(Refusal::Illegal(Reason::UnknownDestination))?;
    let destination = if zones.base == Some(to) {
        if mv.to_seat != seat {
            return illegal(Reason::NotYourBase);
        }
        Location::Base(seat)
    } else if zones.is_battlefield(to) {
        Location::Battlefield(to)
    } else {
        return illegal(Reason::UnknownDestination);
    };
    if !ctx.blob.is_turn_player(seat) {
        return Err(Refusal::NotYourTurn);
    }
    if ctx.blob.phase() != Some(Phase::Action) {
        return illegal(Reason::NotActionPhase);
    }
    if ctx.blob.showdown.is_some() {
        return Err(Refusal::ShowdownOpen);
    }
    if ctx.blob.priority.is_some() || !ctx.blob.chain.is_empty() {
        return illegal(Reason::ChainClosed);
    }
    if card.exhausted {
        return Err(Refusal::Exhausted);
    }
    if ctx.has_flag(mv.card, FLAG_NO_MOVE_BY_OWNER) {
        return illegal(Reason::Locked);
    }
    march::legal_destination(ctx, mv.card, from, destination)?;
    Ok(Intent::StandardMove {
        unit: mv.card,
        from,
        to: destination,
    })
}

pub fn supported(intent: Intent) -> bool {
    matches!(
        intent,
        Intent::Play { .. }
            | Intent::StandardMove { .. }
            | Intent::Mulligan { .. }
            | Intent::Hide { .. }
            | Intent::PlayFromFacedown { .. }
    )
}

pub fn ask(scratch: &Ctx, seat: u8, card: u32, to: u16, to_seat: u8) -> Result<Intent, Refusal> {
    ask_flagged(scratch, seat, card, to, to_seat, false)
}

pub fn ask_hidden(
    scratch: &Ctx,
    seat: u8,
    card: u32,
    to: u16,
    to_seat: u8,
) -> Result<Intent, Refusal> {
    ask_flagged(scratch, seat, card, to, to_seat, true)
}

fn ask_flagged(
    scratch: &Ctx,
    seat: u8,
    card: u32,
    to: u16,
    to_seat: u8,
    hidden: bool,
) -> Result<Intent, Refusal> {
    if let Some(winner) = scratch.winner() {
        return Err(Refusal::GameOver { winner });
    }
    let entry = match scratch.card(card) {
        Some(held) => EntryMove {
            card,
            from: held.zone,
            from_seat: held.seat,
            to: Some(to),
            to_seat,
            index: TOP,
            hidden,
        },
        None => return illegal(Reason::NoSuchCard),
    };
    classify(scratch, seat, &entry)
}

pub fn probe(ctx: &Ctx, seat: u8, card: u32, to: u16, to_seat: u8) -> Result<Intent, Refusal> {
    let mut scratch = ctx.blob.clone();
    let asked = Ctx::fresh(&ctx.table, &mut scratch, ctx.scripts, seat);
    ask(&asked, seat, card, to, to_seat)
}

pub fn actable(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let zones = &ctx.zones;
    let mut out: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| card.owner == seat)
        .filter(|card| {
            card.zone.is_some_and(|zone| {
                zones.hand == Some(zone)
                    || zones.champion == Some(zone)
                    || zones.base == Some(zone)
                    || zones.is_battlefield(zone)
                    || (zones.trash == Some(zone) && flow_cost(ctx, card.id).is_some())
            })
        })
        .map(|card| card.id)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn play_kind(ctx: &Ctx, seat: u8, card: u32, origin: Origin) -> LegalKind {
    let blob = &*ctx.blob;
    let closed = blob.priority.is_some() || !blob.chain.is_empty();
    let reacts =
        ctx.has_keyword(card, Keyword::Reaction) || !ambush_locations(ctx, seat, card).is_empty();
    if closed || (blob.showdown.is_some() && reacts) {
        return LegalKind::React;
    }
    let mut accelerated = ChainItem::new(0, ItemKind::Permanent { card }, seat, origin);
    accelerated.set_slot(SLOT_ACCELERATE, 1);
    LegalKind::Play {
        accelerate: cost::can_accelerate_item(ctx, &accelerated)
            && pay::affordable_for(
                ctx,
                seat,
                &cost::of_item(ctx, &accelerated, None),
                Paying::Item(&accelerated),
            ),
    }
}

fn note(rows: &mut Vec<Legal>, card: u32, kind: LegalKind, zone: Option<u16>) {
    if !rows.iter().any(|row| row.card == card) {
        rows.push(Legal {
            card,
            kinds: Vec::new(),
            zones: Vec::new(),
            hidden: Vec::new(),
        });
    }
    let Some(row) = rows.iter_mut().find(|row| row.card == card) else {
        return;
    };
    if !row.kinds.contains(&kind) {
        row.kinds.push(kind);
    }
    let Some(zone) = zone else {
        return;
    };
    let list = if kind == LegalKind::Hide {
        &mut row.hidden
    } else {
        &mut row.zones
    };
    if !list.contains(&zone) {
        list.push(zone);
    }
}

pub fn hideable(ctx: &Ctx, seat: u8, card: u32) -> Vec<u16> {
    if !ctx.has_keyword(card, Keyword::Hidden) {
        return Vec::new();
    }
    let from = ctx.card(card).and_then(|held| held.zone);
    if from != ctx.zones.hand && from != ctx.zones.champion {
        return Vec::new();
    }
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| hide::legal(ctx, seat, card, *zone).is_ok())
        .collect()
}

pub fn reachable(ctx: &Ctx, card: u32) -> Vec<u16> {
    let zones = &ctx.zones;
    let Some(from) = ctx.card(card).and_then(|held| held.zone) else {
        return Vec::new();
    };
    let mut out: Vec<u16> = Vec::new();
    let in_hand = zones.hand == Some(from);
    if in_hand || zones.champion == Some(from) {
        out.extend(zones.chain);
        out.extend(zones.base);
        out.extend(zones.battlefields.iter().copied());
        if in_hand && matches!(ctx.blob.why, Some(PromptWhy::Mulligan)) {
            out.extend(zones.main_deck);
        }
    } else if zones.base == Some(from) || zones.is_battlefield(from) {
        out.extend(zones.base);
        out.extend(zones.battlefields.iter().copied());
        if hide::is_facedown(ctx, card) {
            out.extend(zones.chain);
        }
    } else if zones.trash == Some(from) && flow_cost(ctx, card).is_some() {
        out.extend(zones.chain);
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn highlights(ctx: &Ctx, seat: u8) -> Vec<Legal> {
    if ctx.winner().is_some() {
        return Vec::new();
    }
    let mut rows: Vec<Legal> = Vec::new();
    let mut scratch = ctx.blob.clone();
    let asking = Ctx::fresh(&ctx.table, &mut scratch, ctx.scripts, seat);
    for card in actable(&asking, seat) {
        for to in reachable(&asking, card) {
            let Ok(intent) = ask(&asking, seat, card, to, seat) else {
                continue;
            };
            if !supported(intent) {
                continue;
            }
            let kind = match intent {
                Intent::Play { origin, .. } => play_kind(&asking, seat, card, origin),
                Intent::Hide { .. } => LegalKind::Hide,
                Intent::PlayFromFacedown { .. } => LegalKind::React,
                Intent::StandardMove { .. } => LegalKind::March,
                _ => LegalKind::Answer,
            };
            note(&mut rows, card, kind, Some(to));
        }
        for zone in hideable(&asking, seat, card) {
            note(&mut rows, card, LegalKind::Hide, Some(zone));
        }
    }
    drop(asking);
    if ctx
        .blob
        .prompt
        .as_ref()
        .is_some_and(|prompt| prompt.seat == seat)
    {
        for option in prompts::offered(ctx) {
            if let Some(card) = option.card {
                note(&mut rows, card, LegalKind::Answer, None);
            }
        }
    }
    for offer in activate::offers(ctx, seat) {
        if offer.enabled {
            note(
                &mut rows,
                offer.source,
                LegalKind::Activate {
                    ability: offer.index,
                },
                None,
            );
        }
    }
    for row in &mut rows {
        row.kinds.sort_unstable();
        row.zones.sort_unstable();
        row.hidden.sort_unstable();
    }
    rows.sort_by_key(|row| row.card);
    rows
}

fn facedown_to(ctx: &Ctx, viewer: u8, card: u32) -> bool {
    if !hide::is_facedown(ctx, card) {
        return false;
    }
    let Some(held) = ctx.card(card) else {
        return false;
    };
    let owner = held.owner;
    if owner == viewer {
        return false;
    }
    let bit = 1u8.checked_shl(u32::from(owner)).unwrap_or(0);
    ctx.blob.seat(viewer).looks_facedown_of & bit == 0
}

fn aim(ctx: &Ctx, viewer: u8, target: TargetRef) -> Option<Aim> {
    match target {
        TargetRef::Card(card) if facedown_to(ctx, viewer, card) => None,
        TargetRef::Card(card) => Some(Aim::Card(card)),
        TargetRef::Seat(seat) => Some(Aim::Seat(seat)),
        TargetRef::Zone(zone) => Some(Aim::Zone(zone)),
        TargetRef::Item(item) => Some(Aim::Item(item)),
    }
}

fn draw(arrows: &mut Vec<Arrow>, arrow: Arrow) {
    if !arrows.contains(&arrow) {
        arrows.push(arrow);
    }
}

fn attacking(ctx: &Ctx, zone: u16, attacker: u8) -> Vec<u32> {
    let designated = combat::attackers(ctx, zone);
    if !designated.is_empty() {
        return designated;
    }
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == attacker)
        .collect()
}

pub fn chain_rows(ctx: &Ctx, viewer: u8) -> Vec<ChainRow> {
    let items = ctx
        .blob
        .queue
        .iter()
        .map(|pending| &pending.item)
        .chain(ctx.blob.chain.iter());
    items
        .map(|item| {
            let source = item.kind.source();
            ChainRow {
                item: item.id,
                card: (!facedown_to(ctx, viewer, source)).then_some(source),
                seat: item.controller,
            }
        })
        .collect()
}

pub fn arrows(ctx: &Ctx, viewer: u8) -> Vec<Arrow> {
    let mut out: Vec<Arrow> = Vec::new();
    let items = ctx
        .blob
        .queue
        .iter()
        .map(|pending| &pending.item)
        .chain(ctx.blob.chain.iter());
    for item in items {
        let source = item.kind.source();
        if facedown_to(ctx, viewer, source) {
            continue;
        }
        let from = match ctx.card(source) {
            Some(_) => ArrowFrom::Card(source),
            None => ArrowFrom::Item(item.id),
        };
        for target in &item.targets {
            let Some(to) = aim(ctx, viewer, *target) else {
                continue;
            };
            let kind = match (item.kind, target) {
                (_, TargetRef::Item(_)) => ArrowKind::Counter,
                (
                    ItemKind::Ability { .. }
                    | ItemKind::Trigger { .. }
                    | ItemKind::Granted { .. }
                    | ItemKind::Lent { .. },
                    _,
                ) => ArrowKind::Ability,
                _ => ArrowKind::Spell,
            };
            draw(&mut out, Arrow { from, to, kind });
        }
    }
    for staged in &ctx.blob.staged {
        for unit in attacking(ctx, staged.zone, staged.contester) {
            if facedown_to(ctx, viewer, unit) {
                continue;
            }
            draw(
                &mut out,
                Arrow {
                    from: ArrowFrom::Card(unit),
                    to: Aim::Zone(staged.zone),
                    kind: ArrowKind::Attack,
                },
            );
        }
    }
    if let Some(showdown) = &ctx.blob.showdown {
        for unit in attacking(ctx, showdown.zone, showdown.attacker) {
            if facedown_to(ctx, viewer, unit) {
                continue;
            }
            draw(
                &mut out,
                Arrow {
                    from: ArrowFrom::Card(unit),
                    to: Aim::Zone(showdown.zone),
                    kind: ArrowKind::Attack,
                },
            );
        }
        if showdown.combat {
            let attackers = combat::attackers(ctx, showdown.zone);
            let defenders = combat::defenders(ctx, showdown.zone);
            for attacker in &attackers {
                if facedown_to(ctx, viewer, *attacker) {
                    continue;
                }
                for defender in &defenders {
                    if facedown_to(ctx, viewer, *defender) {
                        continue;
                    }
                    draw(
                        &mut out,
                        Arrow {
                            from: ArrowFrom::Card(*attacker),
                            to: Aim::Card(*defender),
                            kind: ArrowKind::Combat,
                        },
                    );
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, Expiry, ItemKind, Priority, Showdown};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::Prompt;
    use agni_plugin_sdk::view::{LegalKind, Request as ViewRequest};

    fn classify_move(
        fixture: &mut Fixture,
        seat: u8,
        card: u32,
        to: u16,
        to_seat: u8,
    ) -> Result<Intent, Refusal> {
        let action = fixtures::move_action(card, to, to_seat);
        let ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.unwrap();
        classify(&ctx, seat, &entry)
    }

    fn refused(fixture: &mut Fixture, seat: u8, card: u32, to: u16, to_seat: u8, reason: Reason) {
        assert_eq!(
            classify_move(fixture, seat, card, to, to_seat),
            Err(Refusal::Illegal(reason)),
            "{card} -> {to}"
        );
    }

    fn decide_move(
        fixture: &Fixture,
        seat: u8,
        card: u32,
        to: u16,
    ) -> Result<agni_plugin_sdk::decide::Verdict, Refusal> {
        let request = agni_plugin_sdk::decide::Request {
            plugin_state: Vec::new(),
            players: fixture.blob.players(),
            seat,
            action: fixtures::move_action(card, to, seat),
            table: fixture.table.clone(),
        };
        crate::engine::decide(&request, fixture.blob.clone())
    }

    fn plugin_view(fixture: &Fixture, seat: u8) -> agni_plugin_sdk::view::PluginView {
        crate::present::present(&ViewRequest {
            plugin_state: fixture.blob.encode(),
            players: fixture.blob.players(),
            seat,
            zones: fixture.table.zones.clone(),
            table: fixture.table.clone(),
        })
    }

    #[test]
    fn hand_to_chain_base_and_a_held_battlefield_are_plays() {
        let mut fixture = Fixture::enforced();
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Ok(Intent::Play {
                card: fixtures::HAND_SPELL,
                origin: Origin::Hand,
                location: None,
                on_chain: true
            })
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::CHAIN, 0),
            Ok(Intent::Play {
                card: fixtures::HAND_UNIT,
                origin: Origin::Hand,
                location: None,
                on_chain: true
            })
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::BASE, 0),
            Ok(Intent::Play {
                card: fixtures::HAND_UNIT,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false
            })
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_GEAR, fixtures::BASE, 0),
            Ok(Intent::Play {
                card: fixtures::HAND_GEAR,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false
            })
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::BF1,
            0,
            Reason::NotHeld,
        );
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::BF1, 0),
            Ok(Intent::Play {
                card: fixtures::HAND_UNIT,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false
            })
        );
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::BF2,
            0,
            Reason::NoUnitsPlayedHere,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_GEAR,
            fixtures::BF1,
            0,
            Reason::GearToBase,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_SPELL,
            fixtures::BF1,
            0,
            Reason::SpellsToChain,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_SPELL,
            fixtures::BASE,
            0,
            Reason::SpellsToChain,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::BASE,
            1,
            Reason::NotYourBase,
        );
    }

    #[test]
    fn decide_projects_incoming_units_for_open_and_enemy_grants() {
        let mut open = Fixture::enforced();
        open.table
            .cards
            .push(fixtures::unit(90, fixtures::HAND, 0, "Sneaky Deckhand", 2));
        open.table.cards.push(fixtures::card(
            91,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        open.resolve();
        assert!(decide_move(&open, 0, 90, fixtures::BF3).is_ok());
        let view = plugin_view(&open, 0);
        let row = view
            .legal
            .iter()
            .find(|row| row.card == 90)
            .expect("open grant");
        assert_eq!(row.kinds, [LegalKind::Play { accelerate: false }]);
        assert_eq!(
            row.zones,
            [
                fixtures::BASE,
                fixtures::BF1,
                fixtures::BF3,
                fixtures::CHAIN
            ]
        );
        assert!(decide_move(&open, 0, 90, fixtures::BF2).is_err());

        let mut enemy = Fixture::enforced();
        enemy.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        enemy.table.cards.push(fixtures::card(
            91,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        enemy.table.cards.push(fixtures::unit(
            90,
            fixtures::HAND,
            0,
            "Dauntless Vanguard",
            4,
        ));
        enemy.resolve();
        assert!(decide_move(&enemy, 0, 90, fixtures::BF2).is_ok());
        let view = plugin_view(&enemy, 0);
        let row = view
            .legal
            .iter()
            .find(|row| row.card == 90)
            .expect("enemy grant");
        assert_eq!(row.zones, [fixtures::BASE, fixtures::BF2, fixtures::CHAIN]);
        assert!(decide_move(&enemy, 0, 90, fixtures::BF1).is_err());
    }

    #[test]
    fn an_external_open_grant_is_applied_by_a_real_move() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::HAND, 0, "Plain Unit", 2));
        fixture.table.cards.push(fixtures::unit(
            91,
            fixtures::BASE,
            0,
            "Miss Fortune - Buccaneer",
            4,
        ));
        fixture.table.cards.push(fixtures::card(
            92,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.resolve();
        let verdict = decide_move(&fixture, 0, 90, fixtures::BF3).unwrap();
        assert!(verdict.accept);
        assert!(verdict.effects.iter().any(|effect| matches!(
            effect,
            agni_plugin_sdk::decide::Effect::Annotate {
                card: 90,
                key,
                value: Some(value),
            } if key == "exhausted" && value == &[1]
        )));
    }

    #[test]
    fn the_champion_plays_from_its_zone_to_the_board_only() {
        let mut fixture = Fixture::enforced();
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::CHAMPION_CARD, fixtures::BASE, 0),
            Ok(Intent::Play {
                card: fixtures::CHAMPION_CARD,
                origin: Origin::Champion,
                location: Some(Location::Base(0)),
                on_chain: false
            })
        );
        assert!(matches!(
            classify_move(&mut fixture, 0, fixtures::CHAMPION_CARD, fixtures::CHAIN, 0),
            Ok(Intent::Play {
                origin: Origin::Champion,
                location: None,
                ..
            })
        ));
        refused(
            &mut fixture,
            0,
            fixtures::CHAMPION_CARD,
            fixtures::HAND,
            0,
            Reason::ChampionToBoard,
        );
        refused(
            &mut fixture,
            0,
            fixtures::CHAMPION_CARD,
            fixtures::TRASH,
            0,
            Reason::ChampionToBoard,
        );
        refused(
            &mut fixture,
            0,
            fixtures::CHAMPION_CARD,
            fixtures::BF1,
            0,
            Reason::NotHeld,
        );
    }

    #[test]
    fn a_stolen_unit_marches_for_its_controller_and_no_longer_for_its_owner() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = false;
        {
            let mut ctx = fixture.ctx();
            assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        }
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::THEIR_UNIT, fixtures::BF1, 0),
            Ok(Intent::StandardMove {
                unit: fixtures::THEIR_UNIT,
                from: Location::Base(1),
                to: Location::Battlefield(fixtures::BF1)
            }),
            "the controller moves it"
        );
        let mut theirs = Fixture::enforced();
        theirs
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = false;
        theirs.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        theirs.blob.set_phase(Phase::Action);
        theirs.blob.seats = vec![Default::default(); 2];
        {
            let mut ctx = theirs.ctx();
            assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        }
        refused(
            &mut theirs,
            1,
            fixtures::THEIR_UNIT,
            fixtures::BF1,
            0,
            Reason::NotYourCard,
        );
        let mut ctx = fixture.ctx();
        assert!(
            crate::cards::prelude::friendly_units(&ctx, 0).contains(&fixtures::THEIR_UNIT),
            "friendly follows control"
        );
        assert!(!crate::cards::prelude::friendly_units(&ctx, 1).contains(&fixtures::THEIR_UNIT));
        assert!(ctx.set_controller(fixtures::VI, 1, fixtures::THEIR_UNIT));
        assert!(
            !march::companions(
                &ctx,
                0,
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1)
            )
            .contains(&fixtures::VI),
            "a unit the seat no longer controls is no group-move companion"
        );
    }

    #[test]
    fn standard_moves_walk_between_base_and_battlefields_under_the_rules() {
        let mut fixture = Fixture::enforced();
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::VI, fixtures::BF1, 0),
            Ok(Intent::StandardMove {
                unit: fixtures::VI,
                from: Location::Base(0),
                to: Location::Battlefield(fixtures::BF1)
            })
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::BASE,
            0,
            Reason::SameLocation,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::BASE,
            1,
            Reason::NotYourBase,
        );
        assert_eq!(
            classify_move(&mut fixture, 1, fixtures::THEIR_UNIT, fixtures::BF1, 0),
            Err(Refusal::NotYourTurn)
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::THEIR_UNIT, fixtures::BF1, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        let mut exhausted = Fixture::enforced();
        exhausted.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        assert_eq!(
            classify_move(&mut exhausted, 0, fixtures::VI, fixtures::BF1, 0),
            Err(Refusal::Exhausted)
        );
        let mut locked = Fixture::enforced();
        locked
            .blob
            .set_flag(fixtures::VI, FLAG_NO_MOVE_BY_OWNER, true);
        refused(
            &mut locked,
            0,
            fixtures::VI,
            fixtures::BF1,
            0,
            Reason::Locked,
        );
        let mut afield = Fixture::enforced();
        afield.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        refused(
            &mut afield,
            0,
            fixtures::VI,
            fixtures::BF2,
            0,
            Reason::NeedsGanking,
        );
        assert!(matches!(
            classify_move(&mut afield, 0, fixtures::VI, fixtures::BASE, 0),
            Ok(Intent::StandardMove {
                to: Location::Base(0),
                ..
            })
        ));
        let mut showdown = Fixture::enforced();
        showdown.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        assert_eq!(
            classify_move(&mut showdown, 0, fixtures::VI, fixtures::BF1, 0),
            Err(Refusal::ShowdownOpen)
        );
        let mut closed = Fixture::enforced();
        closed.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        refused(
            &mut closed,
            0,
            fixtures::VI,
            fixtures::BF1,
            0,
            Reason::ChainClosed,
        );
        let mut early = Fixture::enforced();
        early.blob.set_phase(Phase::Beginning);
        refused(
            &mut early,
            0,
            fixtures::VI,
            fixtures::BF1,
            0,
            Reason::NotActionPhase,
        );
        let mut crowded = Fixture::enforced();
        crowded.table.players = 4;
        crowded.blob = crate::GameBlob::start(4, 0, crate::state::Mode::Enforced);
        crowded
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 2, "Jinx", 2));
        crowded
            .table
            .cards
            .push(fixtures::unit(91, fixtures::BF1, 3, "Vex", 2));
        refused(
            &mut crowded,
            0,
            fixtures::VI,
            fixtures::BF1,
            0,
            Reason::TwoOtherSeats,
        );
    }

    #[test]
    fn everything_else_is_refused_with_a_reason_that_names_it() {
        let mut fixture = Fixture::enforced();
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::TRASH,
            0,
            Reason::KillsAreAutomatic,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::MAIN_DECK,
            0,
            Reason::DrawsAreAutomatic,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::SIDEBOARD,
            0,
            Reason::SideboardStays,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::RUNE_POOL,
            0,
            Reason::RunesArePaid,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::HAND,
            0,
            Reason::SameLocation,
        );
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::LEGEND,
            0,
            Reason::Unplayable,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::HAND,
            0,
            Reason::ToHand,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::TRASH,
            0,
            Reason::KillsAreAutomatic,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::MAIN_DECK,
            0,
            Reason::DrawsAreAutomatic,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::CHAIN,
            0,
            Reason::Unplayable,
        );
        refused(
            &mut fixture,
            0,
            fixtures::RUNE_A,
            fixtures::RUNE_DECK,
            0,
            Reason::RunesArePaid,
        );
        refused(
            &mut fixture,
            0,
            fixtures::RUNE_A,
            fixtures::BASE,
            0,
            Reason::RunesArePaid,
        );
        refused(
            &mut fixture,
            0,
            fixtures::GROUNDS,
            fixtures::BF3,
            0,
            Reason::BattlefieldStays,
        );
        refused(
            &mut fixture,
            0,
            fixtures::LEGEND_CARD,
            fixtures::BASE,
            0,
            Reason::LegendStays,
        );
        refused(
            &mut fixture,
            0,
            20,
            fixtures::HAND,
            0,
            Reason::DrawsAreAutomatic,
        );
        refused(
            &mut fixture,
            0,
            30,
            fixtures::RUNE_POOL,
            0,
            Reason::RunesArePaid,
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1, 0),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "a hide needs the battlefield held"
        );
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1, 0),
            Ok(Intent::Hide {
                card: fixtures::HAND_HIDDEN,
                zone: fixtures::BF1
            })
        );
        fixture.blob.set_holder(fixtures::BF1, None);
        refused(
            &mut fixture,
            0,
            fixtures::HAND_HIDDEN,
            fixtures::BASE,
            0,
            Reason::Unrevealed,
        );
        assert_eq!(
            classify_move(&mut fixture, 0, 999, fixtures::BASE, 0),
            Err(Refusal::Illegal(Reason::NoSuchCard))
        );
        let mut on_chain = Fixture::enforced();
        on_chain.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::CHAIN);
        refused(
            &mut on_chain,
            0,
            fixtures::HAND_SPELL,
            fixtures::TRASH,
            0,
            Reason::ChainResolvesItself,
        );
        let mut trashed = Fixture::enforced();
        trashed.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::TRASH);
        refused(
            &mut trashed,
            0,
            fixtures::HAND_SPELL,
            fixtures::HAND,
            0,
            Reason::TrashIsFinal,
        );
        let mut sided = Fixture::enforced();
        sided.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::SIDEBOARD);
        refused(
            &mut sided,
            0,
            fixtures::HAND_SPELL,
            fixtures::HAND,
            0,
            Reason::SideboardStays,
        );
        let mut geared = Fixture::enforced();
        geared.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        refused(
            &mut geared,
            0,
            fixtures::HAND_GEAR,
            fixtures::BF1,
            0,
            Reason::GearStays,
        );
        let mut facedown = Fixture::enforced();
        {
            let face = facedown.table.card_mut(fixtures::HAND_SPELL).unwrap();
            face.zone = Some(fixtures::BF1);
            face.name = "Consult the Past".into();
            face.energy = Some(4);
            face.domain = vec!["Mind".into()];
        }
        facedown.resolve();
        facedown.blob.card_state_mut(fixtures::HAND_SPELL).hidden_at = Some(fixtures::BF1);
        assert_eq!(
            classify_move(&mut facedown, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Ok(Intent::PlayFromFacedown {
                card: fixtures::HAND_SPELL
            })
        );
        refused(
            &mut facedown,
            0,
            fixtures::HAND_SPELL,
            fixtures::BASE,
            0,
            Reason::Unrevealed,
        );
        let nowhere = Action::Move {
            card: fixtures::HAND_UNIT,
            to: None,
            seat: 0,
            index: 0,
            hidden: false,
        };
        let ctx = fixture.ctx_for(0, &nowhere);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::UnknownDestination))
        );
        for reason in [Reason::NoSuchCard, Reason::EngineFault, Reason::DealIsOver] {
            assert!(reason.label().len() > 8);
        }
    }

    #[test]
    fn plays_need_the_right_timing_the_spell_lock_off_and_an_affordable_cost() {
        let mut fixture = Fixture::enforced();
        fixture.blob.core_mut().unwrap().player = 1;
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::BASE, 0),
            Err(Refusal::NotYourTurn)
        );
        let mut showdown = Fixture::enforced();
        showdown.blob.showdown = Some(Showdown::open(fixtures::BF1, 1, 0));
        assert_eq!(
            classify_move(&mut showdown, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Err(Refusal::NotYourFocus)
        );
        showdown.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        refused(
            &mut showdown,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::ShowdownTiming,
        );
        let mut closed = Fixture::enforced();
        closed.blob.priority = Some(Priority {
            active: 1,
            passes: 0,
        });
        refused(
            &mut closed,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::ChainClosed,
        );
        closed.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        refused(
            &mut closed,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::ClosedTiming,
        );
        let mut early = Fixture::enforced();
        early.blob.set_phase(Phase::Draw);
        refused(
            &mut early,
            0,
            fixtures::HAND_UNIT,
            fixtures::BASE,
            0,
            Reason::NotActionPhase,
        );
        let mut locked = Fixture::enforced();
        locked.blob.seat_mut(0).play_lock = PlayLock::SPELLS;
        refused(
            &mut locked,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::NoSpells,
        );
        assert!(classify_move(&mut locked, 0, fixtures::HAND_UNIT, fixtures::BASE, 0).is_ok());
        assert!(classify_move(&mut locked, 0, fixtures::HAND_GEAR, fixtures::BASE, 0).is_ok());
        locked.blob.seat_mut(0).play_lock = PlayLock::CARDS;
        refused(
            &mut locked,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::NoSpells,
        );
        refused(
            &mut locked,
            0,
            fixtures::HAND_UNIT,
            fixtures::BASE,
            0,
            Reason::NoUnits,
        );
        refused(
            &mut locked,
            0,
            fixtures::HAND_GEAR,
            fixtures::BASE,
            0,
            Reason::NoGear,
        );
        let mut broke = Fixture::enforced();
        for rune in &mut broke.table.cards {
            if rune.is_kind("Rune") {
                rune.exhausted = true;
            }
        }
        assert_eq!(
            classify_move(&mut broke, 0, fixtures::HAND_UNIT, fixtures::BASE, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
        let mut wrong = Fixture::enforced();
        for rune in &mut wrong.table.cards {
            if rune.is_kind("Rune") {
                rune.domain = vec!["Calm".into()];
                rune.name = "Calm Rune".into();
            }
        }
        assert_eq!(
            classify_move(&mut wrong, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Err(Refusal::NoPowerOf)
        );
    }

    #[test]
    fn a_reaction_is_refused_in_the_other_seats_neutral_open_state_but_plays_into_any_chain() {
        let mut reaction = Fixture::enforced();
        reaction.blob.core_mut().unwrap().player = 1;
        reaction
            .blob
            .card_state_mut(fixtures::HAND_SPELL)
            .granted
            .push((Keyword::Reaction, Expiry::Permanent));
        assert_eq!(
            classify_move(&mut reaction, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Err(Refusal::NotYourTurn)
        );
        reaction.blob.chain.push(ChainItem::new(
            1,
            ItemKind::Spell { card: 99 },
            1,
            Origin::Hand,
        ));
        reaction.blob.priority = Some(Priority {
            active: 0,
            passes: 1,
        });
        assert!(matches!(
            classify_move(&mut reaction, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0),
            Ok(Intent::Play { on_chain: true, .. })
        ));
        refused(
            &mut reaction,
            0,
            fixtures::HAND_UNIT,
            fixtures::BASE,
            0,
            Reason::ClosedTiming,
        );
        reaction.blob.set_phase(Phase::Ending);
        assert!(
            classify_move(&mut reaction, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0).is_ok(),
            "a chain at the ending step still takes reactions"
        );
        reaction.blob.showdown = Some(Showdown::open(fixtures::BF1, 1, 0));
        reaction.blob.set_phase(Phase::Action);
        assert!(
            classify_move(&mut reaction, 0, fixtures::HAND_SPELL, fixtures::CHAIN, 0).is_ok(),
            "a showdown closed state follows priority, not focus"
        );
        reaction.blob.priority = Some(Priority {
            active: 1,
            passes: 0,
        });
        refused(
            &mut reaction,
            0,
            fixtures::HAND_SPELL,
            fixtures::CHAIN,
            0,
            Reason::ChainClosed,
        );
    }

    #[test]
    fn prompts_and_setup_gate_every_drag_except_the_answering_gesture() {
        let mut fixture = Fixture::enforced();
        fixture.blob.prompt = Some(Prompt::new(1, 0, 0, 2));
        fixture.blob.why = Some(PromptWhy::Mulligan);
        fixture.blob.set_phase(Phase::Setup);
        let bottom = fixtures::move_to_bottom(fixtures::HAND_UNIT, fixtures::MAIN_DECK, 0);
        let ctx = fixture.ctx_for(0, &bottom);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            classify(&ctx, 0, &entry),
            Ok(Intent::Mulligan {
                card: fixtures::HAND_UNIT
            })
        );
        drop(ctx);
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::MAIN_DECK, 0),
            Ok(Intent::Mulligan {
                card: fixtures::HAND_UNIT
            }),
            "any index on the own deck answers the mulligan"
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::MAIN_DECK, 1),
            Err(Refusal::PromptOpen),
            "the other seat's deck is not a set-aside"
        );
        assert_eq!(
            classify_move(&mut fixture, 0, fixtures::HAND_UNIT, fixtures::BASE, 0),
            Err(Refusal::PromptOpen)
        );
        fixture.blob.close_prompt();
        refused(
            &mut fixture,
            0,
            fixtures::HAND_UNIT,
            fixtures::BASE,
            0,
            Reason::InSetup,
        );
        refused(
            &mut fixture,
            0,
            fixtures::VI,
            fixtures::BF1,
            0,
            Reason::InSetup,
        );
        let mut theirs = Fixture::enforced();
        theirs.blob.prompt = Some(Prompt::new(1, 1, 0, 2));
        theirs.blob.why = Some(PromptWhy::Mulligan);
        theirs.blob.set_phase(Phase::Setup);
        let ctx = theirs.ctx_for(0, &bottom);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            classify(&ctx, 0, &entry),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        drop(ctx);
        let mut full = Fixture::enforced();
        full.blob.prompt = Some(Prompt::new(1, 0, 0, 2));
        full.blob.prompt.as_mut().unwrap().picked = vec![1, 2];
        full.blob.why = Some(PromptWhy::Mulligan);
        full.blob.set_phase(Phase::Setup);
        let ctx = full.ctx_for(0, &bottom);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::TooManySetAside))
        );
        drop(ctx);
        let mut playing = Fixture::enforced();
        playing.blob.prompt = Some(Prompt::new(1, 0, 1, 1));
        playing.blob.why = Some(PromptWhy::PlayLocation { item: 1 });
        assert_eq!(
            classify_move(&mut playing, 0, fixtures::VI, fixtures::BF1, 0),
            Err(Refusal::PromptOpen)
        );
        let mut discarding = Fixture::enforced();
        discarding.blob.prompt = Some(Prompt::new(1, 0, 1, 1));
        discarding.blob.why = Some(PromptWhy::Discard { item: 1, stage: 0 });
        assert_eq!(
            classify_move(&mut discarding, 0, fixtures::HAND_UNIT, fixtures::TRASH, 0),
            Ok(Intent::Discard {
                card: fixtures::HAND_UNIT
            })
        );
        assert_eq!(
            classify_move(&mut discarding, 0, fixtures::HAND_UNIT, fixtures::TRASH, 1),
            Err(Refusal::PromptOpen)
        );
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct Destinations {
        plain: Vec<(u32, u16)>,
        hides: Vec<(u32, u16)>,
    }

    impl Destinations {
        fn note(&mut self, card: u32, zone: u16, hide: bool) {
            let list = if hide {
                &mut self.hides
            } else {
                &mut self.plain
            };
            if !list.contains(&(card, zone)) {
                list.push((card, zone));
            }
        }

        fn sorted(mut self) -> Self {
            self.plain.sort_unstable();
            self.hides.sort_unstable();
            self
        }
    }

    fn pairs(rows: &[Legal]) -> Destinations {
        let mut out = Destinations::default();
        for row in rows {
            for zone in &row.zones {
                out.note(row.card, *zone, false);
            }
            for zone in &row.hidden {
                out.note(row.card, *zone, true);
            }
        }
        out.sorted()
    }

    fn every_accepted_move(ctx: &Ctx, seat: u8) -> Destinations {
        let zones: Vec<u16> = ctx.table.zones.iter().map(|zone| zone.id).collect();
        let cards: Vec<u32> = ctx.table.cards.iter().map(|card| card.id).collect();
        let mut out = Destinations::default();
        for card in cards {
            for to in &zones {
                for to_seat in 0..ctx.players() {
                    if let Ok(intent) = probe(ctx, seat, card, *to, to_seat) {
                        if supported(intent) {
                            out.note(card, *to, matches!(intent, Intent::Hide { .. }));
                        }
                    }
                    match probe_hidden(ctx, seat, card, *to, to_seat) {
                        Ok(Intent::Hide { .. }) if offers_hide(ctx, card) => {
                            out.note(card, *to, true);
                        }
                        Ok(Intent::Hide { .. }) => {}
                        Ok(intent) => assert_eq!(
                            probe(ctx, seat, card, *to, to_seat),
                            Ok(intent),
                            "the hidden flag is inert on a gesture and refused everywhere else"
                        ),
                        Err(_) => {}
                    }
                }
            }
        }
        out.sorted()
    }

    fn offers_hide(ctx: &Ctx, card: u32) -> bool {
        ctx.card(card)
            .is_some_and(|held| held.is_hidden() || ctx.has_keyword(card, Keyword::Hidden))
    }

    fn probe_hidden(
        ctx: &Ctx,
        seat: u8,
        card: u32,
        to: u16,
        to_seat: u8,
    ) -> Result<Intent, Refusal> {
        let mut scratch = ctx.blob.clone();
        let asked = Ctx::fresh(&ctx.table, &mut scratch, ctx.scripts, seat);
        ask_hidden(&asked, seat, card, to, to_seat)
    }

    fn every_move_the_engine_accepts(fixture: &Fixture, seat: u8) -> Destinations {
        let zones: Vec<u16> = fixture.table.zones.iter().map(|zone| zone.id).collect();
        let cards: Vec<u32> = fixture.table.cards.iter().map(|card| card.id).collect();
        let players = fixture.blob.players().max(fixture.table.players).max(1);
        let mut out = Destinations::default();
        let accepts = |action: Action| {
            let request = agni_plugin_sdk::decide::Request {
                plugin_state: Vec::new(),
                players,
                seat,
                action,
                table: fixture.table.clone(),
            };
            crate::engine::decide(&request, fixture.blob.clone()).is_ok()
        };
        for card in cards {
            for to in &zones {
                for to_seat in 0..players {
                    if accepts(fixtures::move_action(card, *to, to_seat)) {
                        let mut scratch = fixture.blob.clone();
                        let asked =
                            Ctx::fresh(&fixture.table, &mut scratch, &fixture.scripts, seat);
                        let hide = matches!(
                            ask(&asked, seat, card, *to, to_seat),
                            Ok(Intent::Hide { .. })
                        );
                        out.note(card, *to, hide);
                    }
                }
                let hidden = Action::Move {
                    card,
                    to: Some(*to),
                    seat,
                    index: agni_plugin_sdk::decide::TOP,
                    hidden: true,
                };
                let mut scratch = fixture.blob.clone();
                let asked = Ctx::fresh(&fixture.table, &mut scratch, &fixture.scripts, seat);
                let offered = offers_hide(&asked, card)
                    && matches!(
                        probe_hidden(&asked, seat, card, *to, seat),
                        Ok(Intent::Hide { .. })
                    );
                drop(asked);
                if offered && accepts(hidden) {
                    out.note(card, *to, true);
                }
            }
        }
        out.sorted()
    }

    fn no_drift(fixture: &mut Fixture, what: &str) {
        for seat in 0..2u8 {
            let engine = every_move_the_engine_accepts(fixture, seat);
            let ctx = fixture.ctx();
            let shown = pairs(&highlights(&ctx, seat));
            assert_eq!(
                shown,
                every_accepted_move(&ctx, seat),
                "{what} · seat {seat} · classify"
            );
            assert_eq!(shown, engine, "{what} · seat {seat} · decide");
        }
    }

    fn game_request(
        fixture: &Fixture,
        seat: u8,
        event: crate::TurnEvent,
    ) -> agni_plugin_sdk::decide::Request {
        agni_plugin_sdk::decide::Request {
            plugin_state: Vec::new(),
            players: 2,
            seat,
            action: Action::Game(event.encode()),
            table: fixture.table.clone(),
        }
    }

    fn accepts(fixture: &Fixture, seat: u8, event: crate::TurnEvent) -> bool {
        let request = game_request(fixture, seat, event);
        crate::engine::decide(&request, fixture.blob.clone()).is_ok()
    }

    fn every_activation_the_engine_accepts(fixture: &Fixture, seat: u8) -> Vec<(u32, u8)> {
        let cards: Vec<u32> = fixture.table.cards.iter().map(|card| card.id).collect();
        let mut out: Vec<(u32, u8)> = Vec::new();
        for source in cards {
            for ability in 0..4u8 {
                if accepts(
                    fixture,
                    seat,
                    crate::TurnEvent::Activate { source, ability },
                ) {
                    out.push((source, ability));
                }
            }
        }
        out.sort_unstable();
        out
    }

    fn shown_activations(rows: &[Legal]) -> Vec<(u32, u8)> {
        let mut out: Vec<(u32, u8)> = rows
            .iter()
            .flat_map(|row| {
                row.kinds.iter().filter_map(move |kind| match kind {
                    LegalKind::Activate { ability } => Some((row.card, *ability)),
                    _ => None,
                })
            })
            .collect();
        out.sort_unstable();
        out
    }

    fn every_answer_the_engine_accepts(fixture: &mut Fixture, seat: u8) -> Option<Vec<u32>> {
        let (offered, prompt) = {
            let ctx = fixture.ctx();
            let id = ctx.blob.prompt.as_ref().map(|prompt| prompt.id);
            (prompts::offered(&ctx), id)
        };
        let prompt = prompt?;
        let mut out: Vec<u32> = Vec::new();
        for (index, option) in offered.iter().enumerate() {
            let pick = agni_plugin_sdk::prompt::Pick {
                prompt,
                option: index as u16,
            };
            if accepts(fixture, seat, crate::TurnEvent::Pick(pick)) {
                if let Some(card) = option.card {
                    out.push(card);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }

    fn shown_answers(rows: &[Legal]) -> Vec<u32> {
        let mut out: Vec<u32> = rows
            .iter()
            .filter(|row| row.kinds.contains(&LegalKind::Answer))
            .map(|row| row.card)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn no_kind_drift(fixture: &mut Fixture, what: &str) {
        for seat in 0..2u8 {
            let activations = every_activation_the_engine_accepts(fixture, seat);
            let answers = every_answer_the_engine_accepts(fixture, seat);
            let ctx = fixture.ctx();
            let rows = highlights(&ctx, seat);
            drop(ctx);
            assert_eq!(
                shown_activations(&rows),
                activations,
                "{what} · seat {seat} · activate"
            );
            if let Some(answers) = answers {
                assert_eq!(
                    shown_answers(&rows),
                    answers,
                    "{what} · seat {seat} · answer"
                );
            }
        }
    }

    fn with_spare_runes(fixture: &mut Fixture) {
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture.resolve();
    }

    fn mulliganing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_phase(Phase::Setup);
        fixture.blob.prompt = Some(Prompt::new(1, 0, 0, 2));
        fixture.blob.why = Some(PromptWhy::Mulligan);
        fixture
    }

    fn won() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.set_points(1, crate::rules::DEFAULT_VICTORY_SCORE);
        fixture
    }

    fn en_garde(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "En Garde", 1, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn reacting() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(en_garde(90, 1));
        fixture.resolve();
        fixture.blob.chain.push(ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
        fixture.blob.priority = Some(Priority {
            active: 1,
            passes: 0,
        });
        fixture
    }

    fn contesting() -> Fixture {
        let mut fixture = Fixture::enforced();
        for card in &mut fixture.table.cards {
            if card.id == fixtures::VI {
                card.zone = Some(fixtures::BF2);
            }
        }
        fixture.table.cards.push(fixtures::unit(
            55,
            fixtures::BF2,
            0,
            "Shadow Order Initiate",
            2,
        ));
        fixture.table.cards.push(en_garde(90, 1));
        fixture.table.cards.push(en_garde(91, 0));
        fixture.resolve();
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture
    }

    fn in_combat() -> Fixture {
        let mut fixture = contesting();
        {
            let mut ctx = fixture.ctx();
            ctx.mark_attacker(fixtures::VI);
            ctx.mark_attacker(55);
            ctx.mark_defender(fixtures::SPRITE);
        }
        fixture.blob.showdown = Some(Showdown {
            combat: true,
            ..Showdown::open(fixtures::BF2, 0, 1)
        });
        fixture
    }

    fn ambushing() -> Fixture {
        let mut fixture = reacting();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            54,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        let mut horror = fixtures::unit(92, fixtures::HAND, 1, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        fixture.table.cards.push(horror);
        fixture.resolve();
        fixture
    }

    fn flowing() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut onslaught = fixtures::spell(93, fixtures::TRASH, 0, "Onslaught", 4, 0);
        onslaught.domain = vec!["Body".into()];
        fixture.table.cards.push(onslaught);
        fixture
            .table
            .cards
            .push(fixtures::spell(94, fixtures::TRASH, 0, "Spark", 1, 0));
        with_spare_runes(&mut fixture);
        fixture.resolve();
        fixture
    }

    #[test]
    fn timing_at_reads_the_ambush_battlefields_as_reaction_timing_and_the_rest_as_the_cards_own() {
        let mut fixture = ambushing();
        let ctx = fixture.ctx();
        let horror = 92;
        assert_eq!(
            timing(&ctx, 1, horror),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "a Sorcery unit on a closed chain"
        );
        assert_eq!(timing_at(&ctx, 1, horror, None), Ok(()));
        assert_eq!(
            timing_at(&ctx, 1, horror, Some(Location::Battlefield(fixtures::BF2))),
            Ok(())
        );
        assert_eq!(
            timing_at(&ctx, 1, horror, Some(Location::Base(1))),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert_eq!(
            timing_at(&ctx, 1, horror, Some(Location::Battlefield(fixtures::BF1))),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert_eq!(
            locations_for(&ctx, 1, horror),
            [Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            timing_at(&ctx, 0, fixtures::HAND_UNIT, None),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "the seat without priority is refused whatever the card"
        );
        drop(ctx);
        let mut open = Fixture::enforced();
        let mut horror = fixtures::unit(92, fixtures::HAND, 0, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        open.table.cards.push(horror);
        open.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        open.blob.set_holder(fixtures::BF1, Some(0));
        open.resolve();
        let ctx = open.ctx();
        assert_eq!(
            locations_for(&ctx, 0, 92),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)],
            "on its own turn the union is the plain play list"
        );
        assert_eq!(
            locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)]
        );
        drop(ctx);
        let mut theirs = Fixture::enforced();
        theirs.blob.turn = Some(crate::state::TurnCore::start(2, 1));
        theirs.blob.set_phase(Phase::Action);
        let mut horror = fixtures::unit(92, fixtures::HAND, 0, "Kha'Zix - Mutating Horror", 4);
        horror.energy = Some(0);
        theirs.table.cards.push(horror);
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        theirs.resolve();
        let ctx = theirs.ctx();
        assert_eq!(
            timing_at(&ctx, 0, 92, None),
            Err(Refusal::NotYourTurn),
            "822.1.b: an open state on the other seat's turn is nobody's reaction window"
        );
        assert!(locations_for(&ctx, 0, 92).is_empty());
    }

    #[test]
    fn a_flow_spell_in_the_trash_is_a_play_to_the_chain_for_its_owner_alone() {
        let mut fixture = flowing();
        assert_eq!(
            classify_move(&mut fixture, 0, 93, fixtures::CHAIN, 0),
            Ok(Intent::Play {
                card: 93,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        refused(
            &mut fixture,
            0,
            94,
            fixtures::CHAIN,
            0,
            Reason::TrashIsFinal,
        );
        refused(
            &mut fixture,
            0,
            93,
            fixtures::BASE,
            0,
            Reason::SpellsToChain,
        );
        refused(
            &mut fixture,
            0,
            93,
            fixtures::HAND,
            0,
            Reason::SpellsToChain,
        );
        refused(&mut fixture, 1, 93, fixtures::CHAIN, 0, Reason::NotYourCard);
        let ctx = fixture.ctx();
        assert!(actable(&ctx, 0).contains(&93));
        assert!(!actable(&ctx, 0).contains(&94));
        assert_eq!(reachable(&ctx, 93), [fixtures::CHAIN]);
        assert!(reachable(&ctx, 94).is_empty());
        let rows = highlights(&ctx, 0);
        let row = rows.iter().find(|row| row.card == 93).expect("a Flow row");
        assert_eq!(row.zones, [fixtures::CHAIN]);
        assert!(row.kinds.contains(&LegalKind::Play { accelerate: false }));
        drop(ctx);
        let mut broke = flowing();
        broke
            .table
            .cards
            .retain(|card| !(41..=49).contains(&card.id));
        broke.resolve();
        assert!(matches!(
            classify_move(&mut broke, 0, 93, fixtures::CHAIN, 0),
            Err(Refusal::NotEnoughRunes { .. })
        ));
    }

    #[test]
    fn the_legal_list_is_exactly_the_moves_classify_and_decide_accept_on_every_fixture_table() {
        no_drift(&mut Fixture::enforced(), "neutral open");

        no_drift(&mut ambushing(), "the chain closed with an ambush in hand");

        no_drift(&mut flowing(), "a Flow spell in the trash");

        no_drift(&mut reacting(), "the chain closed with a reaction in hand");

        let mut showdown = contesting();
        showdown.blob.showdown = Some(Showdown::open(fixtures::BF2, 0, 1));
        no_drift(&mut showdown, "a showdown open");

        no_drift(&mut in_combat(), "a combat open");

        let mut asked = Fixture::enforced();
        asked.blob.prompt = Some(Prompt::new(3, 0, 1, 1));
        asked.blob.why = Some(PromptWhy::PlayLocation { item: 1 });
        no_drift(&mut asked, "a location prompt open");

        let mut mulligan = Fixture::enforced();
        mulligan.blob.set_phase(Phase::Setup);
        mulligan.blob.prompt = Some(Prompt::new(1, 0, 0, 2));
        mulligan.blob.why = Some(PromptWhy::Mulligan);
        no_drift(&mut mulligan, "a mulligan open");

        let mut over = won();
        no_drift(&mut over, "the game already won");
        let ctx = over.ctx();
        assert!(
            highlights(&ctx, 0).is_empty(),
            "a finished game offers none"
        );
        drop(ctx);

        let mut rich = Fixture::enforced();
        with_spare_runes(&mut rich);
        no_drift(&mut rich, "neutral open with four ready runes");

        let mut beginning = Fixture::enforced();
        with_spare_runes(&mut beginning);
        beginning.blob.set_phase(Phase::Beginning);
        no_drift(&mut beginning, "the beginning phase");

        let mut ending = Fixture::enforced();
        with_spare_runes(&mut ending);
        ending.blob.set_phase(Phase::Ending);
        no_drift(&mut ending, "the ending phase");

        let mut quiet = Fixture::enforced();
        with_spare_runes(&mut quiet);
        quiet.blob.seats[0].play_lock = PlayLock::SPELLS;
        no_drift(&mut quiet, "no spells this turn");

        let mut second = Fixture::enforced();
        with_spare_runes(&mut second);
        second.blob.turn = Some(crate::state::TurnCore::start(2, 1));
        second.blob.set_phase(Phase::Action);
        no_drift(&mut second, "the other seat's turn");

        let mut staged = Fixture::enforced();
        with_spare_runes(&mut staged);
        staged.blob.staged.push(crate::state::Staged {
            zone: fixtures::BF2,
            combat: false,
            contester: 0,
        });
        no_drift(&mut staged, "a staged showdown");

        let mut half = mulliganing();
        if let Some(prompt) = half.blob.prompt.as_mut() {
            prompt.picked.push(fixtures::HAND_UNIT);
        }
        no_drift(&mut half, "a mulligan with one card already set aside");

        let mut won_mulligan = mulliganing();
        won_mulligan.set_points(1, crate::rules::DEFAULT_VICTORY_SCORE);
        no_drift(&mut won_mulligan, "a mulligan open on a won game");
    }

    #[test]
    fn the_activate_and_answer_rows_are_exactly_what_decide_accepts() {
        let mut plain = Fixture::enforced();
        with_spare_runes(&mut plain);
        no_kind_drift(&mut plain, "neutral open with four ready runes");

        let mut over = won();
        with_spare_runes(&mut over);
        no_kind_drift(&mut over, "the game already won with a payable ability");

        let mut open = contesting();
        open.blob.showdown = Some(Showdown::open(fixtures::BF2, 0, 1));
        no_kind_drift(&mut open, "a showdown open");

        let mut chain = reacting();
        with_spare_runes(&mut chain);
        no_kind_drift(&mut chain, "the chain closed");

        no_kind_drift(&mut mulliganing(), "a mulligan open");

        let mut half = mulliganing();
        if let Some(prompt) = half.blob.prompt.as_mut() {
            prompt.picked.push(fixtures::HAND_UNIT);
        }
        no_kind_drift(&mut half, "a mulligan with one card already set aside");

        let mut won_mulligan = mulliganing();
        won_mulligan.set_points(1, crate::rules::DEFAULT_VICTORY_SCORE);
        no_kind_drift(&mut won_mulligan, "a mulligan open on a won game");
    }

    fn a_hidden_spell_in_hand() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut consult = fixtures::spell(96, fixtures::HAND, 0, "Consult the Past", 4, 0);
        consult.domain = vec!["Mind".into()];
        fixture.table.cards.push(consult);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn a_hidden_card_lights_up_the_battlefields_it_may_be_hidden_at() {
        let mut fixture = a_hidden_spell_in_hand();
        let ctx = fixture.ctx();
        let row = highlights(&ctx, 0)
            .into_iter()
            .find(|row| row.card == 96)
            .expect("the hidden spell is offered");
        assert!(
            row.kinds.contains(&LegalKind::Hide),
            "737.1.b · hiding is its own rim, not a play: {row:?}"
        );
        assert!(
            row.hidden.contains(&fixtures::BF1),
            "the battlefield seat 0 holds is offered"
        );
        assert!(
            !row.hidden.contains(&fixtures::BF2),
            "the battlefield the other seat holds is not"
        );
        assert!(
            !row.zones.contains(&fixtures::BF1),
            "a hide destination is not a play destination: {row:?}"
        );
        assert_eq!(
            hideable(&ctx, 0, 96),
            vec![fixtures::BF1],
            "and the same list drives kai's drop target"
        );
        assert!(
            hideable(&ctx, 1, 96).is_empty(),
            "the other seat is offered nothing"
        );
        assert!(
            hideable(&ctx, 0, fixtures::HAND_UNIT).is_empty(),
            "737 · a card without Hidden is never hideable"
        );
        assert!(
            highlights(&ctx, 1).iter().all(|row| row.card != 96),
            "legality stays seat-private"
        );
    }

    #[test]
    fn the_hide_destinations_are_exactly_the_hides_decide_accepts_and_never_a_play_zone() {
        let mut fixture = a_hidden_spell_in_hand();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        no_drift(
            &mut fixture,
            "a hidden card in hand with two held battlefields",
        );
        let ctx = fixture.ctx();
        let row = highlights(&ctx, 0)
            .into_iter()
            .find(|row| row.card == 96)
            .expect("the hidden spell is offered");
        assert_eq!(row.hidden, vec![fixtures::BF1, fixtures::BF2]);
        assert!(
            !row.zones.contains(&fixtures::BF1) && !row.zones.contains(&fixtures::BF2),
            "a spell's play destination is the chain, never a battlefield: {row:?}"
        );
    }

    #[test]
    fn a_hidden_flag_is_honoured_only_for_a_hide_from_hand_or_the_champion_zone() {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let ctx = fixture.ctx();
        assert_eq!(
            probe_hidden(&ctx, 0, fixtures::VI, fixtures::BF1, 0),
            Err(Refusal::Illegal(Reason::HideFromHand)),
            "a unit on the board marches face up: the fold would strip its face otherwise"
        );
        assert_eq!(
            probe_hidden(&ctx, 0, fixtures::CHAMPION_CARD, fixtures::BASE, 0),
            Err(Refusal::Illegal(Reason::Unrevealed)),
            "the champion is hidden only at a battlefield, never played face down to base"
        );
        assert_eq!(
            probe_hidden(&ctx, 0, fixtures::CHAMPION_CARD, fixtures::CHAIN, 0),
            Err(Refusal::Illegal(Reason::Unrevealed))
        );
        assert_eq!(
            probe_hidden(&ctx, 0, fixtures::CHAMPION_CARD, fixtures::BF1, 0),
            Ok(Intent::Hide {
                card: fixtures::CHAMPION_CARD,
                zone: fixtures::BF1
            }),
            "737.1: a champion card in the champion zone may be hidden at a held battlefield"
        );
        assert_eq!(
            probe_hidden(&ctx, 0, fixtures::HAND_UNIT, fixtures::BASE, 0),
            Err(Refusal::Illegal(Reason::Unrevealed))
        );
    }

    #[test]
    fn a_pool_card_that_prints_hidden_is_offered_a_hide_by_its_keyword_alone() {
        for (name, kind) in [
            ("Smoke and Mirrors", "Spell"),
            ("Switcheroo", "Spell"),
            ("Edge of Night", "Gear"),
        ] {
            let mut fixture = a_hidden_spell_in_hand();
            {
                let face = fixture.table.card_mut(96).unwrap();
                face.name = name.into();
                face.kind = Some(kind.into());
            }
            fixture.resolve();
            let ctx = fixture.ctx();
            assert!(
                !ctx.scripts.is_generic(96),
                "{name} has an M7 script of its own that still prints Hidden"
            );
            assert_eq!(
                hideable(&ctx, 0, 96),
                vec![fixtures::BF1],
                "737.1 · {name} prints Hidden, so the offer must find it"
            );
        }
    }

    #[test]
    fn a_facedown_card_lights_up_as_a_reaction_from_the_next_turn() {
        let mut fixture = a_hidden_spell_in_hand();
        fixture.table.card_mut(96).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(96).hidden_at = Some(fixtures::BF1);
        fixture.blob.card_state_mut(96).hidden_since = fixture.blob.turn();
        fixture.resolve();
        {
            let ctx = fixture.ctx();
            assert!(
                highlights(&ctx, 0).iter().all(|row| row.card != 96),
                "737.1.b · nothing is offered on the hiding turn"
            );
        }
        fixture.blob.core_mut().unwrap().turn += 1;
        let ctx = fixture.ctx();
        let row = highlights(&ctx, 0)
            .into_iter()
            .find(|row| row.card == 96)
            .expect("737.6 · the grant opens the next turn");
        assert_eq!(row.kinds, vec![LegalKind::React]);
        assert_eq!(row.zones, vec![fixtures::CHAIN]);
        assert!(
            highlights(&ctx, 1).iter().all(|row| row.card != 96),
            "and the other seat is told nothing about it"
        );
    }

    #[test]
    fn a_play_a_march_and_an_activation_carry_their_kinds_and_destinations() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture.resolve();
        let ctx = fixture.ctx();
        let rows = highlights(&ctx, 0);
        let row = |card: u32| {
            rows.iter()
                .find(|row| row.card == card)
                .unwrap_or_else(|| panic!("{card} is legal"))
                .clone()
        };
        assert_eq!(
            row(fixtures::HAND_UNIT),
            Legal {
                card: fixtures::HAND_UNIT,
                kinds: vec![LegalKind::Play { accelerate: false }],
                zones: vec![fixtures::BASE, fixtures::CHAIN],
                hidden: Vec::new(),
            }
        );
        assert_eq!(
            row(fixtures::HAND_SPELL),
            Legal {
                card: fixtures::HAND_SPELL,
                kinds: vec![LegalKind::Play { accelerate: false }],
                zones: vec![fixtures::CHAIN],
                hidden: Vec::new(),
            },
            "a spell is only ever played to the chain"
        );
        assert_eq!(
            row(fixtures::CHAMPION_CARD),
            Legal {
                card: fixtures::CHAMPION_CARD,
                kinds: vec![LegalKind::Play { accelerate: true }],
                zones: vec![fixtures::BASE, fixtures::CHAIN],
                hidden: Vec::new(),
            },
            "731.2: a fourth ready rune also pays Lillia's Accelerate"
        );
        assert_eq!(
            row(fixtures::VI),
            Legal {
                card: fixtures::VI,
                kinds: vec![LegalKind::March],
                zones: vec![fixtures::BF1, fixtures::BF2],
                hidden: Vec::new(),
            }
        );
        assert_eq!(
            row(fixtures::LEGEND_CARD),
            Legal {
                card: fixtures::LEGEND_CARD,
                kinds: vec![LegalKind::Activate { ability: 0 }],
                zones: Vec::new(),
                hidden: Vec::new(),
            }
        );
        assert!(
            !rows.iter().any(|row| row.card == fixtures::HAND_HIDDEN),
            "hiding is M6: classify calls it a Hide and act still refuses it"
        );
        assert!(highlights(&ctx, 1).is_empty(), "legality is seat-private");
    }

    #[test]
    fn an_activation_it_cannot_pay_for_is_not_legal_and_a_reaction_is_its_own_kind() {
        let mut fixture = Fixture::enforced();
        let ctx = fixture.ctx();
        assert!(
            !highlights(&ctx, 0)
                .iter()
                .any(|row| row.card == fixtures::LEGEND_CARD),
            "three ready runes cannot pay four"
        );
        drop(ctx);

        let mut reacting = reacting();
        let ctx = reacting.ctx();
        let rows = highlights(&ctx, 1);
        assert_eq!(
            rows,
            [Legal {
                card: 90,
                kinds: vec![LegalKind::React],
                zones: vec![fixtures::CHAIN],
                hidden: Vec::new(),
            }],
            "only a Reaction plays into a closed chain, and it plays as React"
        );
        assert!(
            highlights(&ctx, 0).is_empty(),
            "the seat that lost priority may not answer its own chain"
        );
    }

    #[test]
    fn a_mulligan_gesture_and_the_prompt_candidates_are_both_answers() {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_phase(Phase::Setup);
        fixture.blob.prompt = Some(Prompt::new(1, 0, 0, 2));
        fixture.blob.why = Some(PromptWhy::Mulligan);
        let ctx = fixture.ctx();
        let rows = highlights(&ctx, 0);
        for card in [
            fixtures::HAND_UNIT,
            fixtures::HAND_SPELL,
            fixtures::HAND_GEAR,
            fixtures::HAND_HIDDEN,
        ] {
            let row = rows
                .iter()
                .find(|row| row.card == card)
                .unwrap_or_else(|| panic!("{card} answers the mulligan"));
            assert_eq!(row.kinds, [LegalKind::Answer]);
            assert_eq!(row.zones, [fixtures::MAIN_DECK]);
        }
        assert!(highlights(&ctx, 1).is_empty());
    }

    #[test]
    fn act_refuses_every_intent_that_supported_leaves_out() {
        let mut fixture = Fixture::enforced();
        let refused = [Intent::Discard {
            card: fixtures::HAND_UNIT,
        }];
        for intent in refused {
            assert!(!supported(intent), "{intent:?}");
            let mut ctx = fixture.ctx();
            assert!(
                crate::engine::act(&mut ctx, 0, intent).is_err(),
                "{intent:?}"
            );
        }
        for intent in [
            Intent::Play {
                card: fixtures::HAND_SPELL,
                origin: Origin::Hand,
                location: None,
                on_chain: true,
            },
            Intent::StandardMove {
                unit: fixtures::VI,
                from: Location::Base(0),
                to: Location::Battlefield(fixtures::BF1),
            },
            Intent::Mulligan {
                card: fixtures::HAND_UNIT,
            },
            Intent::Hide {
                card: fixtures::HAND_HIDDEN,
                zone: fixtures::BF1,
            },
            Intent::PlayFromFacedown {
                card: fixtures::HAND_HIDDEN,
            },
        ] {
            assert!(supported(intent), "{intent:?}");
        }
    }

    #[test]
    fn a_pending_spell_draws_one_arrow_per_target_for_both_seats() {
        let mut fixture = Fixture::enforced();
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.targets = vec![
            TargetRef::Card(fixtures::THEIR_UNIT),
            TargetRef::Card(fixtures::SPRITE),
        ];
        fixture.blob.queue.push(crate::state::Pending {
            item,
            needs: crate::state::Needs::Choices,
        });
        let ctx = fixture.ctx();
        let drawn = vec![
            Arrow {
                from: ArrowFrom::Card(fixtures::HAND_SPELL),
                to: Aim::Card(fixtures::THEIR_UNIT),
                kind: ArrowKind::Spell,
            },
            Arrow {
                from: ArrowFrom::Card(fixtures::HAND_SPELL),
                to: Aim::Card(fixtures::SPRITE),
                kind: ArrowKind::Spell,
            },
        ];
        assert_eq!(arrows(&ctx, 0), drawn);
        assert_eq!(
            arrows(&ctx, 1),
            drawn,
            "the defender sees what the attacker chose"
        );
    }

    #[test]
    fn a_counter_points_at_the_chain_item_and_an_ability_at_the_seat() {
        let mut fixture = Fixture::enforced();
        fixture.blob.chain.push(ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
        let mut counter = ChainItem::new(
            3,
            ItemKind::Ability {
                source: fixtures::LEGEND_CARD,
                index: 0,
            },
            0,
            Origin::Board,
        );
        counter.targets = vec![TargetRef::Item(2), TargetRef::Seat(1)];
        fixture.blob.chain.push(counter);
        let ctx = fixture.ctx();
        assert_eq!(
            arrows(&ctx, 1),
            [
                Arrow {
                    from: ArrowFrom::Card(fixtures::LEGEND_CARD),
                    to: Aim::Item(2),
                    kind: ArrowKind::Counter,
                },
                Arrow {
                    from: ArrowFrom::Card(fixtures::LEGEND_CARD),
                    to: Aim::Seat(1),
                    kind: ArrowKind::Ability,
                },
            ]
        );
    }

    #[test]
    fn a_trigger_whose_source_has_left_the_table_is_drawn_from_its_chain_row() {
        let mut fixture = Fixture::enforced();
        let mut orphan = ChainItem::new(
            4,
            ItemKind::Trigger {
                source: 999,
                index: 0,
            },
            1,
            Origin::Board,
        );
        orphan.targets = vec![TargetRef::Zone(fixtures::BF1)];
        fixture.blob.chain.push(orphan);
        let ctx = fixture.ctx();
        assert_eq!(
            arrows(&ctx, 0),
            [Arrow {
                from: ArrowFrom::Item(4),
                to: Aim::Zone(fixtures::BF1),
                kind: ArrowKind::Ability,
            }]
        );
        assert_eq!(
            chain_rows(&ctx, 0),
            [ChainRow {
                item: 4,
                card: Some(999),
                seat: 1,
            }],
            "the row still names the source card so the arrow keeps one identity \
             across the moment the card leaves the table"
        );
    }

    #[test]
    fn every_chain_item_carries_the_card_that_stands_for_it_and_its_controller() {
        let mut fixture = Fixture::enforced();
        fixture.blob.chain.push(ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
        fixture.blob.chain.push(ChainItem::new(
            3,
            ItemKind::Ability {
                source: fixtures::LEGEND_CARD,
                index: 0,
            },
            1,
            Origin::Board,
        ));
        fixture.blob.queue.push(crate::state::Pending {
            item: ChainItem::new(
                7,
                ItemKind::Trigger {
                    source: fixtures::VI,
                    index: 0,
                },
                0,
                Origin::Board,
            ),
            needs: crate::state::Needs::Choices,
        });
        let ctx = fixture.ctx();
        assert_eq!(
            chain_rows(&ctx, 0),
            [
                ChainRow {
                    item: 7,
                    card: Some(fixtures::VI),
                    seat: 0,
                },
                ChainRow {
                    item: 2,
                    card: Some(fixtures::HAND_SPELL),
                    seat: 0,
                },
                ChainRow {
                    item: 3,
                    card: Some(fixtures::LEGEND_CARD),
                    seat: 1,
                },
            ],
            "the queue comes first so the chain's own rows are the last ones, \
             counted down from the top of the panel; a spell stands for itself, \
             an ability or trigger for its source"
        );
    }

    #[test]
    fn a_staged_showdown_points_its_attackers_at_the_battlefield() {
        let mut fixture = contesting();
        fixture.blob.staged.push(crate::state::Staged {
            zone: fixtures::BF2,
            combat: true,
            contester: 0,
        });
        let ctx = fixture.ctx();
        assert_eq!(
            arrows(&ctx, 1),
            [
                Arrow {
                    from: ArrowFrom::Card(fixtures::VI),
                    to: Aim::Zone(fixtures::BF2),
                    kind: ArrowKind::Attack,
                },
                Arrow {
                    from: ArrowFrom::Card(55),
                    to: Aim::Zone(fixtures::BF2),
                    kind: ArrowKind::Attack,
                },
            ],
            "a staged combat is aimed before it opens"
        );
    }

    #[test]
    fn a_combat_with_two_attackers_and_one_defender_draws_both_designations() {
        let mut fixture = in_combat();
        let ctx = fixture.ctx();
        let drawn = vec![
            Arrow {
                from: ArrowFrom::Card(fixtures::VI),
                to: Aim::Zone(fixtures::BF2),
                kind: ArrowKind::Attack,
            },
            Arrow {
                from: ArrowFrom::Card(55),
                to: Aim::Zone(fixtures::BF2),
                kind: ArrowKind::Attack,
            },
            Arrow {
                from: ArrowFrom::Card(fixtures::VI),
                to: Aim::Card(fixtures::SPRITE),
                kind: ArrowKind::Combat,
            },
            Arrow {
                from: ArrowFrom::Card(55),
                to: Aim::Card(fixtures::SPRITE),
                kind: ArrowKind::Combat,
            },
        ];
        assert_eq!(arrows(&ctx, 0), drawn);
        assert_eq!(arrows(&ctx, 1), drawn);
    }

    #[test]
    fn an_arrow_never_names_a_facedown_card_the_viewer_may_not_see() {
        let mut fixture = Fixture::enforced();
        fixture.blob.card_state_mut(fixtures::HAND_HIDDEN).hidden_at = Some(fixtures::BF1);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        item.targets = vec![
            TargetRef::Card(fixtures::HAND_HIDDEN),
            TargetRef::Card(fixtures::THEIR_UNIT),
        ];
        fixture.blob.chain.push(item);
        let open = Arrow {
            from: ArrowFrom::Card(fixtures::HAND_SPELL),
            to: Aim::Card(fixtures::THEIR_UNIT),
            kind: ArrowKind::Spell,
        };
        let secret = Arrow {
            from: ArrowFrom::Card(fixtures::HAND_SPELL),
            to: Aim::Card(fixtures::HAND_HIDDEN),
            kind: ArrowKind::Spell,
        };
        {
            let ctx = fixture.ctx();
            assert_eq!(
                arrows(&ctx, 0),
                [secret, open],
                "its own controller sees its own facedown card"
            );
            assert_eq!(
                arrows(&ctx, 1),
                [open],
                "the other seat is told nothing about the facedown card"
            );
        }
        fixture.blob.seat_mut(1).looks_facedown_of = 1;
        let ctx = fixture.ctx();
        assert_eq!(
            arrows(&ctx, 1),
            [secret, open],
            "Scuttle Crab's Deathknell lifts it for that seat"
        );
    }

    #[test]
    fn an_item_played_from_a_facedown_card_draws_no_arrow_for_the_other_seat() {
        let mut fixture = Fixture::enforced();
        fixture.blob.card_state_mut(fixtures::HAND_HIDDEN).hidden_at = Some(fixtures::BF1);
        let mut item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_HIDDEN,
            },
            0,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
        );
        item.targets = vec![TargetRef::Card(fixtures::THEIR_UNIT)];
        fixture.blob.chain.push(item);
        let ctx = fixture.ctx();
        assert_eq!(
            arrows(&ctx, 0),
            [Arrow {
                from: ArrowFrom::Card(fixtures::HAND_HIDDEN),
                to: Aim::Card(fixtures::THEIR_UNIT),
                kind: ArrowKind::Spell,
            }]
        );
        assert!(arrows(&ctx, 1).is_empty());
    }
}

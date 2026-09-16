use crate::cards::{
    is_granted, Ability, Grant, Once, Paying, SelfCost, Source, Static, Timing, GRANTED,
    IMPLICIT_FLOW, KIND_SPELL,
};
use crate::engine::cost;
use crate::engine::ctx::{Cause, Ctx, Killed};
use crate::engine::legal::{self, Reason};
use crate::engine::{attach, pay, play, statics};
use crate::state::{
    ChainItem, ItemKind, Leave, Needs, Origin, Pending, Phase, TargetRef, FLAG_ONCE_USED,
};
use crate::Refusal;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub source: u32,
    pub index: u8,
    pub label: String,
    pub enabled: bool,
}

pub fn ability_at(ctx: &Ctx, source: u32, index: u8) -> Option<&'static Ability> {
    let ability = crate::engine::targets::ability_at(ctx, source, index)?;
    ability.timing().map(|_| ability)
}

fn printed_at(ctx: &Ctx, card: u32, index: u8) -> Option<&'static Ability> {
    ctx.script(card)?.abilities.get(usize::from(index))
}

#[derive(Debug, Clone, Copy)]
pub struct Lent {
    pub lender: u32,
    pub index: u8,
    pub ability: &'static Ability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borrowing {
    Followed,
    Skipped,
}

fn texts_of(ctx: &Ctx, lender: u32, grant: Grant) -> Vec<&'static Ability> {
    match grant {
        Grant::Ability(held) => held.iter().collect(),
        Grant::Copied(text) => text(ctx, lender).iter().collect(),
        Grant::Keyword(_)
        | Grant::Static(_)
        | Grant::Might(_)
        | Grant::MightIf(..)
        | Grant::Mirror(_)
        | Grant::Borrowed(_) => Vec::new(),
    }
}

pub fn granted_texts(ctx: &Ctx, lender: u32) -> Vec<&'static Ability> {
    let Some(script) = ctx.script(lender) else {
        return Vec::new();
    };
    let projected = script.statics.iter().filter_map(|held| match held {
        Static::Level(_, grants) | Static::While(_, grants) | Static::Aura { grants, .. } => {
            Some(*grants)
        }
        _ => None,
    });
    std::iter::once(script.attached_grants())
        .chain(projected)
        .flat_map(|grants| grants.iter())
        .flat_map(|grant| texts_of(ctx, lender, *grant))
        .take(usize::from(IMPLICIT_FLOW - GRANTED))
        .collect()
}

pub fn lent_text(ctx: &Ctx, lender: u32, index: u8) -> Option<&'static Ability> {
    match index.checked_sub(GRANTED) {
        None => printed_at(ctx, lender, index),
        Some(offset) => granted_texts(ctx, lender).get(usize::from(offset)).copied(),
    }
}

fn locate(ctx: &Ctx, lender: u32, ability: &'static Ability) -> Option<Lent> {
    granted_texts(ctx, lender)
        .iter()
        .position(|held| std::ptr::eq(*held, ability))
        .map(|offset| Lent {
            lender,
            index: GRANTED + offset as u8,
            ability,
        })
}

pub fn lent_on_by(ctx: &Ctx, holder: u32, borrowing: Borrowing) -> Vec<Lent> {
    let attached = attach::granted_on(ctx, holder)
        .into_iter()
        .map(|(lender, index, ability)| Lent {
            lender,
            index,
            ability,
        });
    let projected = statics::sourced_grants_on(ctx, holder)
        .into_iter()
        .flat_map(|(source, grant)| -> Vec<Lent> {
            match grant {
                Grant::Borrowed(borrow) if borrowing == Borrowing::Followed => borrow(ctx, holder)
                    .into_iter()
                    .filter_map(|(lender, index)| {
                        lent_text(ctx, lender, index).map(|ability| Lent {
                            lender,
                            index,
                            ability,
                        })
                    })
                    .collect(),
                Grant::Borrowed(_) => Vec::new(),
                held => texts_of(ctx, source, held)
                    .into_iter()
                    .filter_map(|ability| locate(ctx, source, ability))
                    .collect(),
            }
        });
    let mut lent: Vec<Lent> = Vec::new();
    for held in attached.chain(projected) {
        if lent
            .iter()
            .any(|seen| (seen.lender, seen.index) == (held.lender, held.index))
        {
            continue;
        }
        lent.push(held);
    }
    lent.truncate(usize::from(IMPLICIT_FLOW - GRANTED));
    lent
}

pub fn lent_on(ctx: &Ctx, holder: u32) -> Vec<Lent> {
    lent_on_by(ctx, holder, Borrowing::Followed)
}

pub fn lent_at(ctx: &Ctx, holder: u32, index: u8) -> Option<Lent> {
    let offset = index.checked_sub(GRANTED)?;
    lent_on(ctx, holder).get(usize::from(offset)).copied()
}

pub fn item_kind(ctx: &Ctx, source: u32, index: u8) -> Option<ItemKind> {
    if !is_granted(index) {
        return Some(ItemKind::Ability { source, index });
    }
    lent_at(ctx, source, index).map(|lent| ItemKind::Lent {
        holder: source,
        lender: lent.lender,
        index: lent.index,
    })
}

pub fn lent_index_of(ctx: &Ctx, holder: u32, lender: u32, index: u8) -> Option<u8> {
    lent_on(ctx, holder)
        .iter()
        .position(|held| (held.lender, held.index) == (lender, index))
        .map(|offset| GRANTED + offset as u8)
}

pub fn in_play(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card).is_some_and(|held| ctx.face_in_play(held))
}

pub fn self_cost(ctx: &Ctx, source: u32, ability: &Ability) -> SelfCost {
    match ability.self_cost {
        SelfCost::Auto if ctx.is_unit(source) || ctx.is_legend(source) => SelfCost::Exhaust,
        SelfCost::Auto => SelfCost::Free,
        held => held,
    }
}

pub fn timing(ctx: &Ctx, seat: u8, timing: Timing) -> Result<(), Refusal> {
    let blob = &*ctx.blob;
    if blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    if blob.priority.is_some() || !blob.chain.is_empty() {
        if timing != Timing::Reaction {
            return Err(Refusal::Illegal(Reason::ClosedTiming));
        }
        if blob.priority.map(|priority| priority.active) != Some(seat) {
            return Err(Refusal::Illegal(Reason::ChainClosed));
        }
        return Ok(());
    }
    if blob.phase() != Some(Phase::Action) {
        return Err(Refusal::Illegal(Reason::NotActionPhase));
    }
    if let Some(showdown) = &blob.showdown {
        if !showdown.window.has_focus(seat) {
            return Err(Refusal::NotYourFocus);
        }
        if timing == Timing::Sorcery {
            return Err(Refusal::Illegal(Reason::ShowdownTiming));
        }
        return Ok(());
    }
    if !blob.is_turn_player(seat) {
        return Err(Refusal::NotYourTurn);
    }
    Ok(())
}

pub fn legal(ctx: &Ctx, seat: u8, source: u32, index: u8) -> Result<&'static Ability, Refusal> {
    let ability = playable(ctx, seat, source, index)?;
    let item = cost::activation_item(ctx, source, index);
    pay::plan_for(
        ctx,
        seat,
        &cost::of_activation(ctx, source, index),
        Paying::Item(&item),
    )?;
    Ok(ability)
}

fn playable(ctx: &Ctx, seat: u8, source: u32, index: u8) -> Result<&'static Ability, Refusal> {
    let Some(ability) = ability_at(ctx, source, index) else {
        return Err(Refusal::Illegal(Reason::NoSuchAbility));
    };
    if ctx.card(source).is_none() {
        return Err(Refusal::Illegal(Reason::NoSuchCard));
    }
    if ctx.controller(source) != seat {
        return Err(Refusal::Illegal(Reason::NotYourCard));
    }
    if ctx.is_facedown(source) {
        return Err(Refusal::Illegal(Reason::Facedown));
    }
    if !in_play(ctx, source) {
        return Err(Refusal::Illegal(Reason::NotInPlay));
    }
    if attach::is_attached(ctx, source) {
        return Err(Refusal::Illegal(Reason::Attached));
    }
    if ability.once == Once::PerTurn && ctx.has_flag(source, FLAG_ONCE_USED) {
        return Err(Refusal::Illegal(Reason::AlreadyActivated));
    }
    let timing_of = ability.timing().unwrap_or(Timing::Sorcery);
    timing(ctx, seat, timing_of)?;
    let self_cost = self_cost(ctx, source, ability);
    if matches!(self_cost, SelfCost::Exhaust | SelfCost::Disempower)
        && ctx.card(source).is_some_and(|held| held.exhausted)
    {
        return Err(Refusal::Exhausted);
    }
    if self_cost == SelfCost::Disempower && !ctx.is_empowered(source) {
        return Err(Refusal::Illegal(Reason::NotEmpowered));
    }
    if let Some(usable) = ability.usable {
        let source = Source {
            card: source,
            ability: index,
        };
        if !usable(ctx, source) {
            return Err(Refusal::Illegal(Reason::AlreadyEmpowered));
        }
    }
    let Some(kind) = item_kind(ctx, source, index) else {
        return Err(Refusal::Illegal(Reason::NoSuchAbility));
    };
    let probe = ChainItem::new(0, kind, seat, Origin::Board);
    if !crate::engine::targets::first_spec_fillable(ctx, &probe) {
        return Err(Refusal::Illegal(Reason::NoLegalTargets));
    }
    Ok(ability)
}

pub fn flow_item(seat: u8, card: u32) -> ChainItem {
    ChainItem::new(
        0,
        ItemKind::Spell { card },
        seat,
        Origin::Trash {
            leave: Leave::Banish,
        },
    )
}

pub fn flow_cost(ctx: &Ctx, seat: u8, card: u32) -> cost::Cost {
    cost::of_item(ctx, &flow_item(seat, card), None)
}

pub fn flow_playable(ctx: &Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    if ctx.card(card).is_none() {
        return Err(Refusal::Illegal(Reason::NoSuchCard));
    }
    if ctx.controller(card) != seat {
        return Err(Refusal::Illegal(Reason::NotYourCard));
    }
    if ctx.flow_of(card).is_none() {
        return Err(Refusal::Illegal(Reason::NoSuchAbility));
    }
    if !ctx.in_trash(card) {
        return Err(Refusal::Illegal(Reason::TrashIsFinal));
    }
    if ctx.blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    legal::timing(ctx, seat, card)?;
    legal::unlocked(ctx, seat, KIND_SPELL)?;
    if legal::spells_locked(ctx, seat, card) {
        return Err(Refusal::Illegal(Reason::NoSpells));
    }
    Ok(())
}

pub fn flow_legal(ctx: &Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    flow_playable(ctx, seat, card)?;
    let item = flow_item(seat, card);
    pay::plan_for(ctx, seat, &flow_cost(ctx, seat, card), Paying::Item(&item))?;
    Ok(())
}

pub fn flow_offers(ctx: &Ctx, seat: u8) -> Vec<Offer> {
    let mut cards = ctx.trash_of(seat);
    cards.sort_unstable();
    cards
        .into_iter()
        .filter(|card| flow_playable(ctx, seat, *card).is_ok())
        .map(|card| {
            let cost = flow_cost(ctx, seat, card);
            let item = flow_item(seat, card);
            Offer {
                source: card,
                index: IMPLICIT_FLOW,
                label: flow_label(card, &cost),
                enabled: pay::affordable_for(ctx, seat, &cost, Paying::Item(&item)),
            }
        })
        .collect()
}

pub fn flow_label(card: u32, cost: &cost::Cost) -> String {
    let price = cost.label();
    if price == "nothing" {
        return format!("{{card {card}}}: play from your trash");
    }
    format!("{{card {card}}}: play from your trash ({price})")
}

pub fn activate(ctx: &mut Ctx, seat: u8, source: u32, index: u8) -> Result<(), Refusal> {
    if index == IMPLICIT_FLOW {
        flow_legal(ctx, seat, source)?;
        return play::begin(
            ctx,
            seat,
            source,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        );
    }
    legal(ctx, seat, source, index)?;
    let Some(kind) = item_kind(ctx, source, index) else {
        return Err(Refusal::Illegal(Reason::NoSuchAbility));
    };
    let id = ctx.blob.next_item_id();
    let mut item = ChainItem::new(id, kind, seat, Origin::Board);
    item.stage = play::STAGE_TARGET;
    ctx.blob.queue.push(Pending {
        item,
        needs: Needs::Choices,
    });
    play::advance(ctx, id)
}

pub fn self_cost_of_item(ctx: &Ctx, item: &ChainItem) -> Option<(u32, SelfCost)> {
    let ability = crate::engine::targets::ability_of(ctx, item)?;
    match item.kind {
        ItemKind::Ability { .. } | ItemKind::Lent { .. } => Some((
            item.kind.source(),
            self_cost(ctx, item.kind.source(), ability),
        )),
        ItemKind::Trigger { .. } | ItemKind::Granted { .. } => Some((
            item.kind.source(),
            match ability.self_cost {
                SelfCost::Auto => SelfCost::Free,
                held => held,
            },
        )),
        _ => None,
    }
}

pub fn banish_target_of(item: &ChainItem) -> Option<u32> {
    item.targets.iter().find_map(|target| match target {
        TargetRef::Card(card) => Some(*card),
        _ => None,
    })
}

pub fn self_cost_payable(ctx: &Ctx, item: &ChainItem) -> bool {
    match self_cost_of_item(ctx, item) {
        Some((source, SelfCost::Exhaust)) => ctx.card(source).is_some_and(|held| !held.exhausted),
        Some((source, SelfCost::KillSelf)) => ctx.on_board(source),
        Some((source, SelfCost::Disempower)) => {
            ctx.is_empowered(source) && ctx.card(source).is_some_and(|held| !held.exhausted)
        }
        Some((_, SelfCost::BanishTarget)) => {
            banish_target_of(item).is_some_and(|card| ctx.in_trash(card))
        }
        _ => true,
    }
}

pub fn pay_self(ctx: &mut Ctx, item: &ChainItem) -> bool {
    let Some((source, self_cost)) = self_cost_of_item(ctx, item) else {
        return true;
    };
    let Some(ability) = crate::engine::targets::ability_of(ctx, item) else {
        return true;
    };
    match self_cost {
        SelfCost::Exhaust => {
            if !ctx.exhaust(source) {
                return false;
            }
        }
        SelfCost::KillSelf => {
            ctx.exhaust(source);
            if ctx.kill(source, Cause::Cost) != Killed::Yes {
                return false;
            }
        }
        SelfCost::Disempower => {
            if !ctx.is_empowered(source)
                || !ctx.in_play(source)
                || ctx.card(source).is_none_or(|held| held.exhausted)
                || !ctx.exhaust(source)
            {
                return false;
            }
            if !ctx.disempower(source) {
                return false;
            }
            ctx.narrate(format!("{{card {source}}} is disempowered for its ability"));
        }
        SelfCost::BanishTarget => {
            let Some(card) = banish_target_of(item) else {
                return false;
            };
            if !ctx.in_trash(card) || !ctx.banish_by(card, item.controller) {
                return false;
            }
            ctx.narrate(format!(
                "{{card {card}}} is banished for the {{card {source}}} ability"
            ));
        }
        SelfCost::Auto | SelfCost::Free => {}
    }
    if ability.once == Once::PerTurn {
        ctx.set_flag(source, FLAG_ONCE_USED, true);
    }
    true
}

pub fn label(ctx: &Ctx, source: u32, index: u8, ability: &Ability) -> String {
    let what = ability.label.unwrap_or("activate");
    let cost = cost::of_activation(ctx, source, index);
    let mut parts = vec![cost.label()];
    if matches!(
        self_cost(ctx, source, ability),
        SelfCost::Exhaust | SelfCost::Disempower
    ) {
        parts.push("exhaust".to_string());
    }
    parts.retain(|part| part != "nothing");
    if parts.is_empty() {
        return format!("{{card {source}}}: {what}");
    }
    format!("{{card {source}}}: {what} ({})", parts.join(", "))
}

pub fn offers(ctx: &Ctx, seat: u8) -> Vec<Offer> {
    let mut sources: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| ctx.controller(card.id) == seat && ctx.face_in_play(card))
        .map(|card| card.id)
        .collect();
    sources.sort_unstable();
    let mut offers = Vec::new();
    for source in sources {
        let printed = ctx
            .script(source)
            .map(|script| script.abilities.len())
            .unwrap_or(0) as u8;
        let lent = lent_on(ctx, source).len() as u8;
        for index in (0..printed).chain((0..lent).map(|offset| GRANTED + offset)) {
            let Ok(ability) = playable(ctx, seat, source, index) else {
                continue;
            };
            offers.push(Offer {
                source,
                index,
                label: label(ctx, source, index, ability),
                enabled: pay::affordable_for(
                    ctx,
                    seat,
                    &cost::of_activation(ctx, source, index),
                    Paying::Item(&cost::activation_item(ctx, source, index)),
                ),
            });
        }
    }
    offers.extend(flow_offers(ctx, seat));
    offers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        self, a_play_location, activated, costing, named, once_each_turn, paying_with,
    };
    use crate::cards::{Card, Cost, Domain, Flow, Keyword, Power, Static, Timing};
    use crate::engine::ctx::{Location, Token};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, priority, prompts, settle};
    use crate::state::{Priority, PromptWhy};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const LEGEND: u32 = fixtures::LEGEND_CARD;
    const FOUNTAIN: u32 = 90;

    static BLOOM: Card = prelude::legend(
        "Bloom",
        &[],
        &[named(
            costing(
                activated(
                    Timing::Sorcery,
                    Cost {
                        energy: 4,
                        power: &[],
                    },
                    &[a_play_location("where the Sprite is played")],
                    |ctx, item, _| {
                        let zone = prelude::zone_target(item, 0).unwrap_or(0);
                        let at = Location::of_zone(zone, item.controller, &ctx.zones)
                            .unwrap_or(Location::Base(item.controller));
                        prelude::spawn(ctx, item.controller, Token::Sprite, at, true);
                        Flow::Done
                    },
                ),
                |ctx, source| {
                    let seat = ctx.controller(source.card);
                    let discount = prelude::temporary_units(ctx, seat) as u8;
                    Cost {
                        energy: 4u8.saturating_sub(discount),
                        power: &[],
                    }
                },
            ),
            "play a Sprite",
        )],
    );

    static PUMP: Card = prelude::gear(
        "Pump",
        &[],
        &[once_each_turn(paying_with(
            activated(
                Timing::Action,
                Cost {
                    energy: 0,
                    power: &[Power::Domain(Domain::Fury)],
                },
                &[],
                |ctx, item, _| {
                    prelude::draw(ctx, item.controller, 1);
                    Flow::Done
                },
            ),
            crate::cards::SelfCost::Free,
        ))],
    );

    const MARTYR: u32 = 91;
    const GLASS: u32 = 92;

    static MARTYR_CARD: Card = prelude::unit(
        "Martyr",
        &[],
        &[paying_with(
            activated(
                Timing::Action,
                Cost {
                    energy: 0,
                    power: &[],
                },
                &[],
                |ctx, item, _| {
                    prelude::draw(ctx, item.controller, 1);
                    Flow::Done
                },
            ),
            SelfCost::KillSelf,
        )],
    );

    static GLASS_CARD: Card = prelude::with_replacement(
        prelude::gear("Glass", &[], &[]),
        prelude::replaces(
            |ctx, would, source| {
                ctx.is_unit(would.unit) && ctx.controller(would.unit) == ctx.controller(source.card)
            },
            |ctx, would, source| {
                ctx.kill(source.card, Cause::Replacement);
                ctx.recall(would.unit, true);
            },
        ),
    );

    #[test]
    fn an_activation_whose_self_kill_is_replaced_is_taken_back_unpaid() {
        let mut fixture = table();
        fixture
            .table
            .cards
            .push(fixtures::unit(MARTYR, fixtures::BASE, 0, "Martyr", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(GLASS, fixtures::BASE, 0, "Glass", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(MARTYR, &MARTYR_CARD)
            .with_script(GLASS, &GLASS_CARD);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(offers(&ctx, 0).iter().any(|offer| offer.source == MARTYR));
        activate(&mut ctx, 0, MARTYR, 0).unwrap();
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "403.3 · a cost that cannot be paid buys nothing"
        );
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
            "the activation never reached the chain"
        );
        assert!(
            ctx.on_board(MARTYR),
            "the replacement recalled it instead of killing it"
        );
        assert_eq!(ctx.card(GLASS).unwrap().zone, Some(fixtures::TRASH));
    }

    fn table() -> Fixture {
        let mut fixture = Fixture::enforced();
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.scripts = fixture.scripts.clone().with_script(LEGEND, &BLOOM);
        fixture
    }

    fn runes(fixture: &mut Fixture, count: usize) {
        let ids: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, id) in ids.into_iter().enumerate() {
            let card = fixture.table.card_mut(id).unwrap();
            card.exhausted = index >= count;
        }
    }

    #[test]
    fn a_dynamic_cost_falls_with_every_temporary_unit_and_is_refused_when_it_cannot_be_paid() {
        let mut fixture = table();
        runes(&mut fixture, 3);
        let mut ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, LEGEND, 0).energy, 4);
        assert_eq!(
            activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            }),
            "three ready runes cannot pay four energy"
        );
        drop(ctx);
        let mut with_sprites = table();
        runes(&mut with_sprites, 3);
        with_sprites
            .table
            .cards
            .push(sprite(fixtures::SPRITE + 1, 0));
        with_sprites.resolve();
        with_sprites.scripts = with_sprites.scripts.clone().with_script(LEGEND, &BLOOM);
        let mut ctx = with_sprites.ctx();
        assert_eq!(
            cost::of_activation(&ctx, LEGEND, 0).energy,
            3,
            "one friendly Temporary unit, one less energy"
        );
        let offers = offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            "{card 75}: play a Sprite (3 energy, exhaust)"
        );
        activate(&mut ctx, 0, LEGEND, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let options = prompts::offered(&ctx);
        assert_eq!(options.len(), 2, "the base and the held battlefield");
        drop(ctx);
    }

    fn sprite(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            might: Some(3),
            ..fixtures::card(id, fixtures::BASE, seat, "Sprite", "Unit")
        }
    }

    #[test]
    fn an_activation_pays_its_runes_exhausts_its_source_and_goes_on_the_chain() {
        let mut fixture = table();
        runes(&mut fixture, 4);
        let mut ctx = fixture.ctx();
        activate(&mut ctx, 0, LEGEND, 0).unwrap();
        let prompt = ctx.blob.prompt.clone().unwrap();
        let base = ctx.zones.base.unwrap();
        prompts::answer(
            &mut ctx,
            0,
            agni_plugin_sdk::prompt::Pick {
                prompt: prompt.id,
                option: 0,
            },
        )
        .unwrap()
        .map(|answered| crate::engine::resume(&mut ctx, &answered))
        .transpose()
        .unwrap();
        assert!(ctx.card(LEGEND).unwrap().exhausted, "the legend exhausts");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy from four ready runes"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "an activation sits on the chain");
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            })
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let sprites: Vec<u32> = ctx
            .units_at(Location::Base(0))
            .into_iter()
            .filter(|unit| ctx.card(*unit).is_some_and(|held| held.name == "Sprite"))
            .collect();
        assert_eq!(sprites.len(), 1, "the Sprite arrives");
        assert!(!ctx.card(sprites[0]).unwrap().exhausted, "it enters ready");
        assert!(ctx.is_temporary(sprites[0]));
        assert_eq!(ctx.controller(sprites[0]), 0);
        assert_eq!(base, fixtures::BASE);
        assert!(matches!(
            activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::Exhausted)
        ));
    }

    #[test]
    fn an_activation_is_refused_out_of_turn_out_of_timing_and_after_its_once_each_turn_use() {
        let mut fixture = table();
        runes(&mut fixture, 4);
        fixture
            .table
            .cards
            .push(fixtures::gear(FOUNTAIN, fixtures::BASE, 0, "Pump", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(FOUNTAIN, &PUMP);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate(&mut ctx, 1, LEGEND, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate(&mut ctx, 0, LEGEND, 3),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert_eq!(
            activate(&mut ctx, 0, fixtures::VI, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        let hand = ctx.hand_of(0).len();
        activate(&mut ctx, 0, FOUNTAIN, 0).unwrap();
        assert!(
            !ctx.card(FOUNTAIN).unwrap().exhausted,
            "the gear says otherwise, so it does not exhaust"
        );
        assert!(ctx.has_flag(FOUNTAIN, FLAG_ONCE_USED));
        assert_eq!(
            activate(&mut ctx, 0, FOUNTAIN, 0),
            Err(Refusal::Illegal(Reason::AlreadyActivated))
        );
        assert_eq!(
            activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "a Sorcery activation waits for the chain to empty"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.blob.chain.is_empty());
        chain::proceed(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(offers(&ctx, 1).is_empty(), "not seat 1's turn");
        assert!(ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Move { .. })));
    }

    static SURGE: Card = prelude::spell(
        "Surge",
        &[Keyword::Flow(Cost {
            energy: 4,
            power: &[],
        })],
        &[prelude::play(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    const SURGE_CARD: u32 = 93;

    fn trashed_surge() -> Fixture {
        let mut fixture = table();
        runes(&mut fixture, 4);
        let mut surge = fixtures::spell(SURGE_CARD, fixtures::TRASH, 0, "Surge", 6, 1);
        surge.domain = vec!["Calm".into()];
        fixture.table.cards.push(surge);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(SURGE_CARD, &SURGE);
        fixture
    }

    fn flow_offer(ctx: &Ctx, seat: u8) -> Option<Offer> {
        offers(ctx, seat)
            .into_iter()
            .find(|offer| offer.index == IMPLICIT_FLOW)
    }

    #[test]
    fn a_flow_spell_in_the_trash_is_offered_to_its_owner_in_neutral_open_at_its_flow_cost() {
        let mut fixture = trashed_surge();
        let mut ctx = fixture.ctx();
        let offer = flow_offer(&ctx, 0).expect("the trash offers the Flow play");
        assert_eq!(offer.source, SURGE_CARD);
        assert_eq!(
            offer.label,
            format!("{{card {SURGE_CARD}}}: play from your trash (4 energy)"),
            "829.1.c.1 · the Flow cost, not the printed six and a Calm"
        );
        assert!(offer.enabled);
        assert!(flow_offer(&ctx, 1).is_none(), "not for the other seat");
        assert_eq!(
            activate(&mut ctx, 1, SURGE_CARD, IMPLICIT_FLOW),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate(&mut ctx, 0, fixtures::HAND_SPELL, IMPLICIT_FLOW),
            Err(Refusal::Illegal(Reason::NoSuchAbility)),
            "a spell without Flow has no such play"
        );
        activate(&mut ctx, 0, SURGE_CARD, IMPLICIT_FLOW).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Flow play is a spell on the chain"
        );
        let item = &ctx.blob.chain[0];
        assert!(matches!(item.kind, crate::state::ItemKind::Spell { card } if card == SURGE_CARD));
        assert_eq!(
            item.origin,
            crate::state::Origin::Trash {
                leave: crate::state::Leave::Banish
            }
        );
        assert_eq!(ctx.card(SURGE_CARD).unwrap().zone, ctx.zones.chain);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy, no Calm recycled"
        );
        assert!(
            flow_offer(&ctx, 0).is_none(),
            "the card left the trash and the chain is closed"
        );
    }

    #[test]
    fn the_flow_offer_is_greyed_short_of_runes_and_absent_on_the_opponents_turn() {
        let mut fixture = trashed_surge();
        runes(&mut fixture, 3);
        let mut ctx = fixture.ctx();
        let offer = flow_offer(&ctx, 0).unwrap();
        assert!(!offer.enabled, "greyed, not hidden, while short");
        assert_eq!(
            activate(&mut ctx, 0, SURGE_CARD, IMPLICIT_FLOW),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(ctx.card(SURGE_CARD).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);
        let mut theirs = trashed_surge();
        theirs.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        theirs.blob.set_phase(crate::state::Phase::Action);
        theirs.blob.seats = vec![Default::default(); 2];
        let mut ctx = theirs.ctx();
        assert!(
            flow_offer(&ctx, 0).is_none(),
            "829.1.b.2 · Sorcery timing from the trash"
        );
        assert_eq!(
            activate(&mut ctx, 0, SURGE_CARD, IMPLICIT_FLOW),
            Err(Refusal::NotYourTurn)
        );
    }

    static NASUS_LIKE: Card = prelude::unit(
        "Ascendant",
        &[Keyword::Empower(Cost {
            energy: 2,
            power: &[],
        })],
        &[prelude::empower(Cost {
            energy: 2,
            power: &[],
        })],
    );

    const ASCENDANT: u32 = 94;

    #[test]
    fn an_empower_offer_exists_while_not_empowered_and_is_gone_and_refused_after() {
        let mut fixture = table();
        runes(&mut fixture, 4);
        fixture
            .table
            .cards
            .push(fixtures::unit(ASCENDANT, fixtures::BASE, 0, "Ascendant", 3));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(ASCENDANT, &NASUS_LIKE);
        let mut ctx = fixture.ctx();
        let empower = offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == ASCENDANT)
            .expect("offered while not empowered");
        assert_eq!(
            empower.label,
            format!("{{card {ASCENDANT}}}: empower (2 energy)")
        );
        assert!(empower.enabled);
        activate(&mut ctx, 0, ASCENDANT, 0).unwrap();
        assert!(!ctx.card(ASCENDANT).unwrap().exhausted, "SelfCost::Free");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(ASCENDANT));
        assert!(
            !offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == ASCENDANT),
            "377.2.b · gone, not greyed"
        );
        assert_eq!(
            activate(&mut ctx, 0, ASCENDANT, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        ctx.disempower(ASCENDANT);
        assert!(offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ASCENDANT));
    }

    static LANTERN: Card = prelude::gear(
        "Lantern",
        &[],
        &[named(
            prelude::disempowering_self(activated(
                Timing::Sorcery,
                Cost {
                    energy: 0,
                    power: &[],
                },
                &[],
                |ctx, item, _| {
                    prelude::draw(ctx, item.controller, 1);
                    Flow::Done
                },
            )),
            "disempower this: draw 1",
        )],
    );

    static LANTERN_READY_SUPPRESSED: Card = prelude::with_statics(
        LANTERN,
        &[Static::ReadySuppressed {
            by_effects: prelude::suppresses_itself,
            by_awaken: prelude::suppresses_itself,
        }],
    );

    const LANTERN_CARD: u32 = 97;

    fn lit() -> Fixture {
        let mut fixture = table();
        fixture.table.cards.push(fixtures::gear(
            LANTERN_CARD,
            fixtures::BASE,
            0,
            "Lantern",
            2,
        ));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(LANTERN_CARD, &LANTERN);
        fixture
    }

    #[test]
    fn a_disempower_cost_is_offered_only_while_empowered_and_is_paid_with_the_exhaust_before_the_chain(
    ) {
        let mut fixture = lit();
        let mut ctx = fixture.ctx();
        assert!(
            !offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == LANTERN_CARD),
            "442.1.a · nothing to disempower"
        );
        assert_eq!(
            activate(&mut ctx, 0, LANTERN_CARD, 0),
            Err(Refusal::Illegal(Reason::NotEmpowered))
        );
        assert!(ctx.empower(LANTERN_CARD));
        let offer = offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == LANTERN_CARD)
            .expect("offered while empowered");
        assert_eq!(
            offer.label,
            format!("{{card {LANTERN_CARD}}}: disempower this: draw 1 (exhaust)")
        );
        assert!(offer.enabled);
        let item = ChainItem::new(
            9,
            crate::state::ItemKind::Ability {
                source: LANTERN_CARD,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(
            self_cost_of_item(&ctx, &item),
            Some((LANTERN_CARD, SelfCost::Disempower))
        );
        assert!(self_cost_payable(&ctx, &item));
        let hand = ctx.hand_of(0).len();
        activate(&mut ctx, 0, LANTERN_CARD, 0).unwrap();
        assert!(
            !ctx.is_empowered(LANTERN_CARD),
            "355.10.c · paid as the ability is activated"
        );
        assert!(ctx.card(LANTERN_CARD).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Disempowered { card } if *card == LANTERN_CARD
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing until it resolves");
        assert!(!self_cost_payable(&ctx, &item));
        assert!(!pay_self(&mut ctx, &item));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.ready(LANTERN_CARD));
        assert_eq!(
            activate(&mut ctx, 0, LANTERN_CARD, 0),
            Err(Refusal::Illegal(Reason::NotEmpowered))
        );
        assert!(ctx.empower(LANTERN_CARD));
        assert!(ctx.exhaust(LANTERN_CARD));
        assert_eq!(
            activate(&mut ctx, 0, LANTERN_CARD, 0),
            Err(Refusal::Exhausted)
        );
        assert!(
            ctx.is_empowered(LANTERN_CARD),
            "a refused activation pays nothing"
        );
    }

    #[test]
    fn a_ready_suppressed_source_can_still_exhaust_to_pay_disempower() {
        let mut fixture = lit();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LANTERN_CARD, &LANTERN_READY_SUPPRESSED);
        let mut ctx = fixture.ctx();
        assert!(ctx.ready_suppressed(LANTERN_CARD));
        assert!(ctx.empower(LANTERN_CARD));
        activate(&mut ctx, 0, LANTERN_CARD, 0).unwrap();
        assert!(!ctx.is_empowered(LANTERN_CARD));
        assert!(ctx.card(LANTERN_CARD).unwrap().exhausted);
    }

    static CLONE_LIKE: Card = prelude::unit(
        "Shade",
        &[],
        &[prelude::optional(prelude::paying_with(
            prelude::on_attack(
                &[prelude::target(
                    crate::cards::Filter::And(&[
                        crate::cards::Filter::Kind(crate::cards::KIND_UNIT),
                        crate::cards::Filter::InTrash,
                        crate::cards::Filter::Friendly,
                    ]),
                    1,
                    1,
                    crate::cards::TargetKind::Card,
                    "a unit in your trash to banish",
                )],
                |ctx, item, _| {
                    prelude::grant_this_turn(ctx, item.kind.source(), Keyword::Assault(4));
                    Flow::Done
                },
            ),
            SelfCost::BanishTarget,
        ))],
    );

    const SHADE: u32 = 95;
    const FALLEN: u32 = 96;

    fn shaded() -> Fixture {
        let mut fixture = table();
        fixture
            .table
            .cards
            .push(fixtures::unit(SHADE, fixtures::BF1, 0, "Shade", 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(FALLEN, fixtures::TRASH, 0, "Fallen", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(SHADE, &CLONE_LIKE);
        fixture
    }

    fn attack_trigger() -> ChainItem {
        let mut item = ChainItem::new(
            7,
            crate::state::ItemKind::Trigger {
                source: SHADE,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets.push(crate::state::TargetRef::Card(FALLEN));
        item.spec_counts.push(1);
        item
    }

    #[test]
    fn a_banish_target_cost_is_payable_while_the_card_is_in_the_trash_and_banishes_it() {
        let mut fixture = shaded();
        let mut ctx = fixture.ctx();
        let item = attack_trigger();
        assert!(ctx.in_trash(FALLEN));
        assert_eq!(
            self_cost_of_item(&ctx, &item),
            Some((SHADE, SelfCost::BanishTarget))
        );
        assert!(self_cost_payable(&ctx, &item));
        assert!(pay_self(&mut ctx, &item));
        assert!(ctx.in_banishment(FALLEN), "427.2 · the cost is the banish");
        assert!(
            !ctx.card(SHADE).unwrap().exhausted,
            "the Shade itself is untouched"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Banished {
                card,
                owner: 0,
                by: 0,
                ..
            } if *card == FALLEN
        )));
        assert!(
            !self_cost_payable(&ctx, &item),
            "gone from the trash, the cost can no longer be paid"
        );
        assert!(!pay_self(&mut ctx, &item));
        let mut empty = attack_trigger();
        empty.targets.clear();
        assert!(!self_cost_payable(&ctx, &empty));
    }

    #[test]
    fn offers_follow_the_controller_not_the_owner() {
        let mut fixture = table();
        runes(&mut fixture, 4);
        fixture
            .table
            .cards
            .push(fixtures::gear(FOUNTAIN, fixtures::BASE, 1, "Pump", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LEGEND, &BLOOM)
            .with_script(FOUNTAIN, &PUMP);
        let mut ctx = fixture.ctx();
        assert!(!offers(&ctx, 0).iter().any(|offer| offer.source == FOUNTAIN));
        ctx.set_controller(FOUNTAIN, 0, fixtures::VI);
        assert!(
            offers(&ctx, 0).iter().any(|offer| offer.source == FOUNTAIN),
            "the thief is offered the stolen gear's ability"
        );
        assert_eq!(
            activate(&mut ctx, 1, FOUNTAIN, 0),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "the owner is not"
        );
        activate(&mut ctx, 0, FOUNTAIN, 0).unwrap();
        assert!(ctx.has_flag(FOUNTAIN, FLAG_ONCE_USED));
    }
}

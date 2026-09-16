use crate::cards::{Adds, Domain, Paying, Power, SelfCost, Spend, TOKEN_GOLD};
use crate::engine::activate;
use crate::engine::cost::{self, Cost, Need};
use crate::engine::ctx::{rune_domain, Cause, Ctx};
use crate::engine::legal::Reason;
use crate::state::{Pool, Pooled, FLAG_PAYING};
use crate::Refusal;
use agni_plugin_sdk::decide::{Effect, BOTTOM};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spent {
    pub card: u32,
    pub spend: Spend,
    pub adds: Cost,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub exhaust: Vec<u32>,
    pub recycle: Vec<u32>,
    pub with: Vec<Spent>,
    pub xp: u8,
    pub burn: u8,
    pub promises: Vec<u8>,
    pub pooled: Pool,
}

impl Plan {
    pub fn sources(&self) -> Vec<u32> {
        self.with.iter().map(|spent| spent.card).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub card: u32,
    pub adds: Adds,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rune {
    id: u32,
    domain: Option<Domain>,
    spent: bool,
}

type Offer = Option<Vec<Domain>>;

pub fn deck_size(ctx: &Ctx, seat: u8) -> usize {
    match ctx.zones.main_deck {
        Some(deck) => ctx.table.held(deck, seat).count(),
        None => 0,
    }
}

pub fn deck_too_thin(needed: u8, held: usize) -> Refusal {
    Refusal::NotEnoughRunes {
        needed,
        ready: held.min(usize::from(u8::MAX)) as u8,
    }
}

fn beyond_runes(ctx: &Ctx, seat: u8, cost: &Cost) -> Result<Plan, Refusal> {
    if cost.xp > 0 && ctx.xp(seat) < i32::from(cost.xp) {
        return Err(Refusal::Illegal(Reason::NotEnoughXp));
    }
    let deck = deck_size(ctx, seat);
    if cost.burn > 0 && deck < usize::from(cost.burn) {
        return Err(deck_too_thin(cost.burn, deck));
    }
    Ok(Plan {
        xp: cost.xp,
        burn: cost.burn,
        promises: cost.promises.clone(),
        pooled: cost.pooled.clone(),
        ..Plan::default()
    })
}

pub fn ready_golds(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut golds: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| card.name == TOKEN_GOLD && !card.exhausted)
        .filter(|card| card.owner == seat && ctx.on_board(card.id))
        .map(|card| card.id)
        .collect();
    golds.sort_by_key(|gold| (!ctx.has_flag(*gold, FLAG_PAYING), *gold));
    golds
}

pub fn pinned(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut cards: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| ctx.has_flag(card.id, FLAG_PAYING) && ctx.controller(card.id) == seat)
        .map(|card| card.id)
        .collect();
    cards.sort_unstable();
    cards
}

fn own_source_spent(ctx: &Ctx, paying: Paying) -> Option<u32> {
    let item = paying.item()?;
    match activate::self_cost_of_item(ctx, item)? {
        (source, SelfCost::Exhaust | SelfCost::KillSelf) => Some(source),
        _ => None,
    }
}

pub fn sources(ctx: &Ctx, seat: u8, paying: Paying) -> Vec<Source> {
    let spent = own_source_spent(ctx, paying);
    let mut found: Vec<Source> = ctx
        .table
        .cards
        .iter()
        .filter(|held| !held.exhausted && ctx.face_in_play(held) && !ctx.is_facedown(held.id))
        .filter(|held| ctx.controller(held.id) == seat && spent != Some(held.id))
        .filter_map(|held| {
            let adder = ctx.script(held.id)?.adds?;
            let adds = adder(ctx, seat, held.id, paying)?;
            let gives = adds.adds.energy > 0 || !adds.adds.power.is_empty();
            (adds.most > 0 && gives).then_some(Source {
                card: held.id,
                adds,
                pinned: ctx.has_flag(held.id, FLAG_PAYING),
            })
        })
        .collect();
    found.sort_by_key(|source| {
        (
            !source.pinned,
            source.adds.spend == Spend::Kill,
            source.adds.pays.energy > 0 || !source.adds.pays.power.is_empty(),
            source.card,
        )
    });
    found
}

fn runes_of(ctx: &Ctx, seat: u8) -> Vec<Rune> {
    ctx.runes_of(seat)
        .into_iter()
        .map(|rune| Rune {
            id: rune.id,
            domain: rune_domain(rune),
            spent: rune.exhausted,
        })
        .collect()
}

fn offers(ctx: &Ctx, source: &Source) -> Vec<Offer> {
    source
        .adds
        .adds
        .power
        .iter()
        .map(|power| offer_of(ctx, source.card, *power))
        .collect()
}

fn offer_of(ctx: &Ctx, card: u32, power: Power) -> Offer {
    match power {
        Power::Domain(domain) => Some(vec![domain]),
        Power::Rainbow => None,
        Power::Own => {
            let domains = ctx.card(card).map(cost::domains_of).unwrap_or_default();
            (!domains.is_empty()).then_some(domains)
        }
    }
}

fn need_of(ctx: &Ctx, card: u32, power: Power) -> Need {
    match offer_of(ctx, card, power) {
        None => Need::Rainbow,
        Some(domains) if domains.len() == 1 => Need::Domain(domains[0]),
        Some(domains) => Need::AnyOf(domains),
    }
}

fn accepts(offer: &Offer, need: &Need) -> bool {
    match (offer, need) {
        (None, _) | (Some(_), Need::Rainbow) => true,
        (Some(domains), Need::Domain(domain)) => domains.contains(domain),
        (Some(domains), Need::AnyOf(wanted)) => domains.iter().any(|held| wanted.contains(held)),
    }
}

fn strike(needs: &mut Vec<Need>, offer: &Offer, runes: &[Rune]) -> bool {
    let rank = |need: &Need| {
        let matched = runes.iter().any(|rune| need.accepts(rune.domain));
        let shape = match need {
            Need::Domain(_) => 0,
            Need::AnyOf(_) => 1,
            Need::Rainbow => 2,
        };
        (matched, shape)
    };
    let index = needs
        .iter()
        .enumerate()
        .filter(|(_, need)| accepts(offer, need))
        .min_by_key(|(index, need)| (rank(need), *index))
        .map(|(index, _)| index);
    match index {
        Some(index) => {
            needs.remove(index);
            true
        }
        None => false,
    }
}

fn contributes(ctx: &Ctx, source: &Source, cost: &Cost, runes: &[Rune]) -> bool {
    if source.adds.adds.energy > 0 && cost.energy > 0 {
        return true;
    }
    let mut needs = cost.power.clone();
    offers(ctx, source)
        .iter()
        .any(|offer| strike(&mut needs, offer, runes))
}

fn helps(ctx: &Ctx, source: &Source, cost: &Cost, runes: &[Rune]) -> bool {
    let ready = runes.iter().filter(|rune| !rune.spent).count();
    if usize::from(cost.energy) > ready {
        return source.adds.adds.energy > source.adds.pays.energy;
    }
    let mut needs = cost.power.clone();
    offers(ctx, source)
        .iter()
        .any(|offer| strike(&mut needs, offer, runes))
}

fn apply(ctx: &Ctx, cost: &Cost, source: &Source, times: u8, runes: &[Rune]) -> Cost {
    let mut next = cost.clone();
    let offers = offers(ctx, source);
    for _ in 0..times {
        next.energy = next.energy.saturating_sub(source.adds.adds.energy);
        for offer in &offers {
            strike(&mut next.power, offer, runes);
        }
        next.energy = next.energy.saturating_add(source.adds.pays.energy);
        next.power.extend(
            source
                .adds
                .pays
                .power
                .iter()
                .map(|power| need_of(ctx, source.card, *power)),
        );
    }
    next
}

fn added(ctx: &Ctx, source: &Source, times: u8) -> Cost {
    let mut total = Cost::default();
    for _ in 0..times {
        total.energy = total.energy.saturating_add(source.adds.adds.energy);
        total.power.extend(
            source
                .adds
                .adds
                .power
                .iter()
                .map(|power| need_of(ctx, source.card, *power)),
        );
    }
    total
}

struct Taken {
    left: Cost,
    with: Vec<Spent>,
    runes: Result<(Vec<u32>, Vec<u32>), Refusal>,
}

fn take(ctx: &Ctx, cost: &Cost, sources: &[Source], runes: &[Rune]) -> Taken {
    let mut left = Cost {
        energy: cost.energy,
        power: cost.power.clone(),
        ..Cost::default()
    };
    let base = runes_plan(runes, &left);
    let mut outcome = base.clone();
    let mut remaining: Vec<&Source> = sources.iter().collect();
    let mut with: Vec<Spent> = Vec::new();
    loop {
        let forced = remaining.iter().any(|source| source.pinned);
        if outcome.is_ok() && !forced {
            return Taken {
                left,
                with,
                runes: outcome,
            };
        }
        let next = remaining
            .iter()
            .position(|source| source.pinned || helps(ctx, source, &left, runes));
        let Some(index) = next else {
            return Taken {
                left,
                with,
                runes: Err(base
                    .err()
                    .or_else(|| outcome.err())
                    .unwrap_or(Refusal::NoPowerOf)),
            };
        };
        let source = remaining.remove(index);
        let mut taken = (
            apply(ctx, &left, source, source.adds.most, runes),
            source.adds.most,
        );
        for times in 1..=source.adds.most {
            let candidate = apply(ctx, &left, source, times, runes);
            if runes_plan(runes, &candidate).is_ok() {
                taken = (candidate, times);
                break;
            }
        }
        left = taken.0;
        outcome = runes_plan(runes, &left);
        with.push(Spent {
            card: source.card,
            spend: source.adds.spend,
            adds: added(ctx, source, taken.1),
        });
    }
}

fn pool_offer(held: Pooled) -> Offer {
    held.domain().map(|domain| vec![domain])
}

pub fn from_pool(ctx: &Ctx, seat: u8, cost: &Cost) -> Cost {
    let pool = &ctx.blob.seat(seat).pool;
    if !cost.pooled.is_empty() || pool.is_empty() {
        return cost.clone();
    }
    let runes = runes_of(ctx, seat);
    let mut next = cost.clone();
    let energy = pool.energy.min(cost.energy);
    next.energy -= energy;
    let mut ordered: Vec<Pooled> = pool
        .power
        .iter()
        .copied()
        .filter(|held| held.domain().is_some())
        .collect();
    ordered.extend(
        pool.power
            .iter()
            .copied()
            .filter(|held| held.domain().is_none()),
    );
    let mut power = Vec::new();
    for held in ordered {
        if strike(&mut next.power, &pool_offer(held), &runes) {
            power.push(held);
        }
    }
    next.pooled = Pool { energy, power };
    next
}

fn split(ctx: &Ctx, seat: u8, paying: Paying) -> (Vec<Source>, Vec<Source>) {
    sources(ctx, seat, paying)
        .into_iter()
        .partition(|source| source.pinned)
}

pub fn left(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> Cost {
    let (pinned, _) = split(ctx, seat, paying);
    take(
        ctx,
        &from_pool(ctx, seat, cost),
        &pinned,
        &runes_of(ctx, seat),
    )
    .left
}

pub fn choices(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> Vec<Source> {
    let runes = runes_of(ctx, seat);
    let (pinned, open) = split(ctx, seat, paying);
    let left = take(ctx, &from_pool(ctx, seat, cost), &pinned, &runes).left;
    open.into_iter()
        .filter(|source| contributes(ctx, source, &left, &runes))
        .collect()
}

pub fn runes_can_pay(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> bool {
    let (pinned, _) = split(ctx, seat, paying);
    build(ctx, seat, cost, &pinned).is_ok()
}

pub fn source_is_a_choice(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> bool {
    cost.needs_runes() && !choices(ctx, seat, cost, paying).is_empty()
}

pub fn plan(ctx: &Ctx, seat: u8, cost: &Cost) -> Result<Plan, Refusal> {
    plan_for(ctx, seat, cost, Paying::Applied)
}

pub fn plan_for(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> Result<Plan, Refusal> {
    build(ctx, seat, cost, &sources(ctx, seat, paying))
}

fn build(ctx: &Ctx, seat: u8, cost: &Cost, sources: &[Source]) -> Result<Plan, Refusal> {
    let cost = from_pool(ctx, seat, cost);
    let beyond = beyond_runes(ctx, seat, &cost)?;
    if !cost.needs_runes() {
        return Ok(beyond);
    }
    let taken = take(ctx, &cost, sources, &runes_of(ctx, seat));
    let (exhaust, recycle) = taken.runes?;
    Ok(Plan {
        exhaust,
        recycle,
        with: taken.with,
        ..beyond
    })
}

fn runes_plan(runes: &[Rune], cost: &Cost) -> Result<(Vec<u32>, Vec<u32>), Refusal> {
    let ready: Vec<Rune> = runes.iter().copied().filter(|rune| !rune.spent).collect();
    let energy = usize::from(cost.energy);
    if ready.len() < energy || runes.len() < cost.power.len() {
        return Err(Refusal::NotEnoughRunes {
            needed: energy.max(cost.power.len()) as u8,
            ready: ready.len() as u8,
        });
    }
    let wanted: Vec<Domain> = cost
        .power
        .iter()
        .flat_map(|need| match need {
            Need::Domain(domain) => vec![*domain],
            Need::AnyOf(domains) => domains.clone(),
            Need::Rainbow => Vec::new(),
        })
        .collect();
    let unwanted = |domain: Option<Domain>| !domain.is_some_and(|held| wanted.contains(&held));
    let mut ordered: Vec<&Need> = cost
        .power
        .iter()
        .filter(|need| matches!(need, Need::Domain(_)))
        .collect();
    ordered.extend(
        cost.power
            .iter()
            .filter(|need| matches!(need, Need::AnyOf(_))),
    );
    ordered.extend(
        cost.power
            .iter()
            .filter(|need| matches!(need, Need::Rainbow)),
    );
    let mut pool: Vec<Rune> = runes.to_vec();
    let mut recycle = Vec::new();
    for need in ordered {
        let rank = |rune: &Rune| -> Option<u8> {
            if !need.accepts(rune.domain) {
                return None;
            }
            let preferred = match need {
                Need::Rainbow => unwanted(rune.domain),
                _ => true,
            };
            Some(match (rune.spent, preferred) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            })
        };
        let index = pool
            .iter()
            .enumerate()
            .filter_map(|(index, rune)| rank(rune).map(|rank| (rank, index)))
            .min()
            .map(|(_, index)| index);
        match index {
            Some(index) => recycle.push(pool.remove(index).id),
            None => return Err(Refusal::NoPowerOf),
        }
    }
    let mut exhaust: Vec<u32> = ready
        .iter()
        .map(|rune| rune.id)
        .filter(|id| recycle.contains(id))
        .take(energy)
        .collect();
    let spare = |exhaust: &[u32], keep: &dyn Fn(Option<Domain>) -> bool| -> Vec<u32> {
        ready
            .iter()
            .filter(|rune| !exhaust.contains(&rune.id) && keep(rune.domain))
            .map(|rune| rune.id)
            .collect()
    };
    if exhaust.len() < energy {
        let more = spare(&exhaust, &unwanted);
        exhaust.extend(more.into_iter().take(energy - exhaust.len()));
    }
    if exhaust.len() < energy {
        let more = spare(&exhaust, &|_| true);
        exhaust.extend(more.into_iter().take(energy - exhaust.len()));
    }
    Ok((exhaust, recycle))
}

pub fn affordable(ctx: &Ctx, seat: u8, cost: &Cost) -> bool {
    plan(ctx, seat, cost).is_ok()
}

pub fn affordable_for(ctx: &Ctx, seat: u8, cost: &Cost, paying: Paying) -> bool {
    plan_for(ctx, seat, cost, paying).is_ok()
}

fn paid_label(cost: &Cost) -> String {
    let mut parts = Vec::new();
    if cost.energy > 0 {
        parts.push(format!("{} energy", cost.energy));
    }
    let rainbow = cost
        .power
        .iter()
        .filter(|need| matches!(need, Need::Rainbow))
        .count();
    if rainbow > 0 {
        parts.push(format!("{rainbow} power"));
    }
    let mut grouped: Vec<(String, usize)> = Vec::new();
    for need in &cost.power {
        if matches!(need, Need::Rainbow) {
            continue;
        }
        let label = need.label();
        match grouped.iter_mut().find(|(held, _)| *held == label) {
            Some((_, count)) => *count += 1,
            None => grouped.push((label, 1)),
        }
    }
    for (label, count) in grouped {
        parts.push(format!("{count} {label} power"));
    }
    if parts.is_empty() {
        "nothing".to_string()
    } else {
        parts.join(" and ")
    }
}

pub fn pay(ctx: &mut Ctx, seat: u8, plan: &Plan) {
    for spent in &plan.with {
        ctx.set_flag(spent.card, FLAG_PAYING, false);
        ctx.exhaust(spent.card);
        if spent.spend == Spend::Kill {
            ctx.kill(spent.card, Cause::Cost);
        }
        ctx.narrate(format!(
            "{{card {}}} pays {}",
            spent.card,
            paid_label(&spent.adds)
        ));
    }
    if let Some(deck) = ctx.zones.rune_deck {
        for rune in &plan.exhaust {
            ctx.exhaust(*rune);
        }
        for rune in &plan.recycle {
            ctx.emit(Effect::Move {
                card: *rune,
                zone: deck,
                seat,
                index: BOTTOM,
            });
        }
    }
    if plan.xp > 0 {
        ctx.spend_xp(seat, plan.xp);
    }
    if plan.burn > 0 {
        ctx.burn(seat, usize::from(plan.burn));
    }
    for index in used_promises(plan) {
        ctx.keep_promise(seat, index);
    }
    if !plan.pooled.is_empty() {
        ctx.spend_pool(seat, &plan.pooled);
    }
}

fn used_promises(plan: &Plan) -> Vec<u8> {
    let mut used = plan.promises.clone();
    used.sort_unstable_by(|a, b| b.cmp(a));
    used.dedup();
    used
}

pub fn clear_pins(ctx: &mut Ctx, seat: u8) {
    for card in pinned(ctx, seat) {
        ctx.set_flag(card, FLAG_PAYING, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Expiry, Promise, PromiseEffect, PromiseKind};

    fn gold_pays(gold: u32) -> Spent {
        Spent {
            card: gold,
            spend: Spend::Kill,
            adds: Cost {
                power: vec![Need::Rainbow],
                ..Cost::default()
            },
        }
    }

    #[test]
    fn a_plan_exhausts_for_energy_and_recycles_a_spent_rune_for_power_by_domain() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        let unit = cost::printed(ctx.card(fixtures::HAND_UNIT).unwrap());
        assert_eq!(
            plan(&ctx, 0, &unit).unwrap(),
            Plan {
                exhaust: vec![41, 42],
                recycle: Vec::new(),
                with: Vec::new(),
                ..Plan::default()
            }
        );
        let spell = cost::printed(ctx.card(fixtures::HAND_SPELL).unwrap());
        let planned = plan(&ctx, 0, &spell).unwrap();
        assert_eq!(
            planned,
            Plan {
                exhaust: vec![42, 41],
                recycle: vec![fixtures::RUNE_A],
                with: Vec::new(),
                ..Plan::default()
            },
            "the exhausted Fury rune pays the power, the Calm rune goes first for energy"
        );
        assert!(affordable(&ctx, 0, &spell));
        assert!(plan(&ctx, 0, &Cost::free()).unwrap().exhaust.is_empty());
        pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(42),
                Effect::exhaust(41),
                Effect::Move {
                    card: fixtures::RUNE_A,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(
            plan(&ctx, 0, &unit),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        let power_only = Cost {
            energy: 0,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &power_only).unwrap(),
            Plan {
                exhaust: Vec::new(),
                recycle: vec![41],
                with: Vec::new(),
                ..Plan::default()
            },
            "an exhausted rune still recycles for power"
        );
    }

    #[test]
    fn one_rune_can_be_exhausted_for_energy_and_recycled_for_power_in_the_same_plan() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        fixture
            .table
            .cards
            .push(fixtures::rune(84, 0, "Fury", false));
        let mut ctx = fixture.ctx();
        let vanguard = Cost {
            energy: 1,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        let planned = plan(&ctx, 0, &vanguard).unwrap();
        assert_eq!(
            planned,
            Plan {
                exhaust: vec![84],
                recycle: vec![84],
                with: Vec::new(),
                ..Plan::default()
            }
        );
        pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(84),
                Effect::Move {
                    card: 84,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        assert!(ctx.runes_of(0).is_empty());
        let mut three = Fixture::enforced();
        three.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let ctx = three.ctx();
        let tidecaller = Cost {
            energy: 2,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        let planned = plan(&ctx, 0, &tidecaller).unwrap();
        assert_eq!(planned.recycle, [fixtures::RUNE_A]);
        assert_eq!(planned.exhaust, [fixtures::RUNE_A, 42]);
        let left: Vec<u32> = ctx
            .ready_runes_of(0)
            .into_iter()
            .map(|rune| rune.id)
            .filter(|rune| !planned.exhaust.contains(rune) && !planned.recycle.contains(rune))
            .collect();
        assert_eq!(left, [41, 43], "two of four ready runes stay ready");
    }

    #[test]
    fn a_ready_gold_pays_a_power_the_rune_pool_is_short_of_and_dies_for_it() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        fixture
            .table
            .cards
            .push(fixtures::rune(84, 0, "Calm", false));
        fixture.table.cards.push(fixtures::gold(85, 0, false));
        fixture.table.tokens.push(85);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(ready_golds(&ctx, 0), [85]);
        let fury = Cost {
            energy: 1,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &fury).unwrap(),
            Plan {
                exhaust: vec![84],
                recycle: Vec::new(),
                with: vec![gold_pays(85)],
                ..Plan::default()
            },
            "no Fury rune: the Gold stands in as a rainbow source"
        );
        assert!(
            source_is_a_choice(&ctx, 0, &fury, Paying::Applied),
            "with no rune that fits, the Gold is still the seat's to spend · asked"
        );
        let calm = Cost {
            energy: 0,
            power: vec![Need::Domain(Domain::Calm)],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &calm).unwrap().recycle,
            [84],
            "a rune that fits is spent before a Gold"
        );
        assert!(
            source_is_a_choice(&ctx, 0, &calm, Paying::Applied),
            "rune or Gold: the seat is asked"
        );
        assert!(runes_can_pay(&ctx, 0, &calm, Paying::Applied));
        assert!(
            !runes_can_pay(&ctx, 0, &fury, Paying::Applied),
            "the runes alone cannot pay a Fury"
        );
        assert!(choices(&ctx, 0, &fury, Paying::Applied)
            .iter()
            .any(|source| source.card == 85));
        drop(ctx);
        let mut ctx = fixture.ctx();
        let planned = plan(&ctx, 0, &fury).unwrap();
        pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(85),
                Effect::Despawn { card: 85 },
                Effect::exhaust(84),
            ],
            "kill it and exhaust it, then the energy is paid"
        );
        assert!(ctx.card(85).is_none());
        assert!(ready_golds(&ctx, 0).is_empty());
        assert_eq!(
            plan(&ctx, 0, &fury),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            }),
            "the Gold is spent and the Calm rune with it"
        );
    }

    #[test]
    fn a_pinned_gold_pays_before_any_rune_and_the_pin_is_cleared_when_it_is_spent() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::gold(85, 0, false));
        fixture.table.tokens.push(85);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let fury = Cost {
            energy: 0,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        assert_eq!(plan(&ctx, 0, &fury).unwrap().recycle, [fixtures::RUNE_A]);
        ctx.set_flag(85, FLAG_PAYING, true);
        let planned = plan(&ctx, 0, &fury).unwrap();
        assert_eq!(
            planned,
            Plan {
                exhaust: Vec::new(),
                recycle: Vec::new(),
                with: vec![gold_pays(85)],
                ..Plan::default()
            },
            "the seat asked for the Gold"
        );
        pay(&mut ctx, 0, &planned);
        assert!(!ctx.has_flag(85, FLAG_PAYING));
        let mut cancelled = Fixture::enforced();
        cancelled.table.cards.push(fixtures::gold(86, 0, false));
        cancelled.table.tokens.push(86);
        cancelled.table.tokens.sort_unstable();
        cancelled.resolve();
        let mut ctx = cancelled.ctx();
        ctx.set_flag(86, FLAG_PAYING, true);
        clear_pins(&mut ctx, 0);
        assert!(!ctx.has_flag(86, FLAG_PAYING));
    }

    #[test]
    fn power_needs_are_matched_domain_first_then_either_then_rainbow_and_refused_when_missing() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        fixture
            .table
            .cards
            .push(fixtures::rune(80, 0, "Calm", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(81, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(82, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(83, 0, "Mind", false));
        let ctx = fixture.ctx();
        let cost = Cost {
            energy: 1,
            power: vec![
                Need::Rainbow,
                Need::AnyOf(vec![Domain::Calm, Domain::Chaos]),
                Need::Domain(Domain::Fury),
            ],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &cost).unwrap(),
            Plan {
                exhaust: vec![80],
                recycle: vec![82, 80, 83],
                with: Vec::new(),
                ..Plan::default()
            }
        );
        let missing = Cost {
            energy: 0,
            power: vec![Need::Domain(Domain::Body)],
            ..Cost::default()
        };
        assert_eq!(plan(&ctx, 0, &missing), Err(Refusal::NoPowerOf));
        let neither = Cost {
            energy: 0,
            power: vec![Need::AnyOf(vec![Domain::Body, Domain::Order])],
            ..Cost::default()
        };
        assert_eq!(plan(&ctx, 0, &neither), Err(Refusal::NoPowerOf));
        let all_energy = Cost {
            energy: 4,
            power: Vec::new(),
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &all_energy).unwrap().exhaust,
            [80, 81, 82, 83]
        );
        let too_much = Cost {
            energy: 5,
            power: Vec::new(),
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &too_much),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            })
        );
        let too_many_powers = Cost {
            energy: 0,
            power: vec![Need::Rainbow; 5],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &too_many_powers),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            })
        );
        let theirs = Cost {
            energy: 1,
            power: vec![Need::Domain(Domain::Mind)],
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 1, &theirs).unwrap(),
            Plan {
                exhaust: vec![44],
                recycle: vec![44],
                with: Vec::new(),
                ..Plan::default()
            }
        );
    }

    #[test]
    fn an_xp_spend_is_refused_short_and_lands_after_the_rune_exhausts_when_paid() {
        let mut fixture = Fixture::enforced();
        fixture.set_xp(0, 1);
        let mut ctx = fixture.ctx();
        let two = Cost {
            energy: 1,
            xp: 2,
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &two),
            Err(Refusal::Illegal(Reason::NotEnoughXp)),
            "202 · an XP spend with a linked effect is a cost, legal only while affordable"
        );
        assert!(!affordable(&ctx, 0, &two));
        let one = Cost {
            energy: 1,
            xp: 1,
            ..Cost::default()
        };
        let planned = plan(&ctx, 0, &one).unwrap();
        assert_eq!(planned.xp, 1);
        assert_eq!(planned.exhaust, [41]);
        pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(41),
                Effect::score(0, crate::rules::COUNTER_XP, -1),
            ],
            "357.2 · the spend lands with the rest of the cost, after the runes"
        );
        assert_eq!(ctx.xp(0), 0);
        let only_xp = Cost {
            xp: 1,
            ..Cost::default()
        };
        assert_eq!(
            plan(&ctx, 0, &only_xp),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        drop(ctx);
        let mut rich = Fixture::enforced();
        rich.set_xp(0, 2);
        let mut ctx = rich.ctx();
        let planned = plan(&ctx, 0, &two).unwrap();
        pay(&mut ctx, 0, &planned);
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} spends 2 XP".to_string()));
    }

    #[test]
    fn a_burn_cost_is_payable_only_while_the_deck_holds_enough_and_burns_at_payment() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        assert_eq!(deck_size(&ctx, 0), 4);
        let burn = Cost {
            burn: 1,
            ..Cost::default()
        };
        let planned = plan(&ctx, 0, &burn).unwrap();
        assert_eq!(planned.burn, 1);
        assert!(planned.exhaust.is_empty() && planned.recycle.is_empty());
        pay(&mut ctx, 0, &planned);
        assert_eq!(deck_size(&ctx, 0), 3);
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Burned { seat: 0, card: 23 }
        )));
        let five = Cost {
            burn: 5,
            ..Cost::default()
        };
        assert_eq!(plan(&ctx, 0, &five), Err(deck_too_thin(5, 3)));
        assert!(!affordable(&ctx, 0, &five));
        assert!(affordable(&ctx, 1, &burn));
        assert!(!affordable(
            &ctx,
            1,
            &Cost {
                burn: 3,
                ..Cost::default()
            }
        ));
    }

    #[test]
    fn a_promised_plan_keeps_the_seats_promises_when_it_is_paid() {
        let mut fixture = Fixture::enforced();
        let discount = Promise {
            kind: PromiseKind::Any,
            effect: PromiseEffect::Discount(Pool {
                energy: 2,
                power: vec![Pooled::Rainbow, Pooled::Rainbow],
            }),
            until: Expiry::Permanent,
        };
        let repeat = Promise {
            kind: PromiseKind::Spell,
            effect: PromiseEffect::RepeatForCost,
            until: Expiry::EndOfTurn(1),
        };
        fixture.blob.seat_mut(0).promises = vec![discount.clone(), repeat.clone()];
        let mut ctx = fixture.ctx();
        let plain = plan(&ctx, 0, &Cost::free()).unwrap();
        assert!(plain.promises.is_empty());
        pay(&mut ctx, 0, &plain);
        assert_eq!(
            ctx.blob.seat(0).promises,
            [discount.clone(), repeat.clone()],
            "a trigger's payment leaves them"
        );
        let promised = plan(
            &ctx,
            0,
            &Cost {
                promises: vec![0, 0],
                ..Cost::default()
            },
        )
        .unwrap();
        assert_eq!(promised.promises, [0, 0]);
        pay(&mut ctx, 0, &promised);
        assert_eq!(
            ctx.blob.seat(0).promises,
            std::slice::from_ref(&repeat),
            "one promise is kept once, whatever the plan repeats"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s discount is used".to_string()));
        let stale = Plan {
            promises: vec![4],
            ..Plan::default()
        };
        pay(&mut ctx, 0, &stale);
        assert_eq!(ctx.blob.seat(0).promises, [repeat]);
    }

    #[test]
    fn the_rune_pool_pays_first_across_any_number_of_costs_and_a_pooled_domain_pays_its_kind_only()
    {
        let mut fixture = Fixture::enforced();
        fixture.blob.seat_mut(0).pool = Pool {
            energy: 2,
            power: vec![Pooled::Domain(Domain::Calm), Pooled::Rainbow],
        };
        let mut ctx = fixture.ctx();
        let spell = Cost {
            energy: 1,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        let priced = from_pool(&ctx, 0, &spell);
        assert_eq!((priced.energy, priced.power.len()), (0, 0));
        assert_eq!(
            from_pool(&ctx, 0, &priced),
            priced,
            "a cost already priced against the pool is not priced twice"
        );
        let planned = plan(&ctx, 0, &spell).unwrap();
        assert!(
            planned.exhaust.is_empty() && planned.recycle.is_empty(),
            "the pool pays the whole spell before any rune"
        );
        assert_eq!(
            planned.pooled,
            Pool {
                energy: 1,
                power: vec![Pooled::Rainbow]
            },
            "the Calm power cannot pay a Fury need"
        );
        pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.blob.seat(0).pool,
            Pool {
                energy: 1,
                power: vec![Pooled::Domain(Domain::Calm)]
            }
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} spends 1 energy and 1 rainbow from their rune pool".to_string()));
        let ability = Cost {
            energy: 3,
            power: vec![Need::Domain(Domain::Calm)],
            ..Cost::default()
        };
        let planned = plan(&ctx, 0, &ability).unwrap();
        assert_eq!(
            planned.pooled,
            Pool {
                energy: 1,
                power: vec![Pooled::Domain(Domain::Calm)]
            },
            "what is left of the pool pays a second cost of any kind"
        );
        assert_eq!(planned.exhaust.len(), 2);
        assert!(planned.recycle.is_empty());
        pay(&mut ctx, 0, &planned);
        assert!(ctx.blob.seat(0).pool.is_empty());
        assert!(
            plan(&ctx, 0, &ability).is_err(),
            "the runes alone cannot pay it again"
        );
        ctx.blob.seat_mut(0).pool = Pool {
            energy: 0,
            power: vec![Pooled::Rainbow],
        };
        let mixed = Cost {
            energy: 0,
            power: vec![Need::Rainbow, Need::Domain(Domain::Mind)],
            ..Cost::default()
        };
        let priced = from_pool(&ctx, 0, &mixed);
        assert_eq!(
            priced.power,
            [Need::Rainbow],
            "a pooled rainbow strikes the need no rune of the seat can pay"
        );
    }
}

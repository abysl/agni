use crate::cards::{self, Domain, Keyword, Power, Source, Static, KIND_GEAR, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::engine::pay;
use crate::engine::{activate, statics, targets};
use crate::state::{
    ChainItem, CostedGrant, ItemKind, Leave, Origin, Pool, Pooled, Price, Promise, PromiseEffect,
    PromiseKind, TargetRef, SLOT_ACCELERATE, SLOT_PROMISED_REPEAT, SLOT_REPEAT,
};
use agni_plugin_sdk::table::CardInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    Domain(Domain),
    AnyOf(Vec<Domain>),
    Rainbow,
}

impl Need {
    pub fn accepts(&self, domain: Option<Domain>) -> bool {
        match self {
            Need::Domain(wanted) => domain == Some(*wanted),
            Need::AnyOf(wanted) => domain.is_some_and(|held| wanted.contains(&held)),
            Need::Rainbow => true,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Need::Domain(domain) => domain.label().to_string(),
            Need::AnyOf(domains) => domains
                .iter()
                .map(|domain| domain.label())
                .collect::<Vec<_>>()
                .join("/"),
            Need::Rainbow => "any".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cost {
    pub energy: u8,
    pub power: Vec<Need>,
    pub xp: u8,
    pub burn: u8,
    pub promises: Vec<u8>,
    pub pooled: Pool,
}

impl Cost {
    pub fn free() -> Self {
        Self::default()
    }

    pub fn is_free(&self) -> bool {
        self.energy == 0 && self.power.is_empty() && self.xp == 0 && self.burn == 0
    }

    pub fn needs_runes(&self) -> bool {
        self.energy > 0 || !self.power.is_empty()
    }

    pub fn promised(&self) -> bool {
        !self.promises.is_empty()
    }

    pub fn plus(mut self, other: Cost) -> Cost {
        self.energy = self.energy.saturating_add(other.energy);
        self.power.extend(other.power);
        self.xp = self.xp.saturating_add(other.xp);
        self.burn = self.burn.saturating_add(other.burn);
        self.promises.extend(other.promises);
        self.pooled.add(&other.pooled);
        self
    }

    pub fn less(mut self, discount: &Cost) -> Cost {
        self.energy = self.energy.saturating_sub(discount.energy);
        for need in &discount.power {
            strike(&mut self.power, need);
        }
        self
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.energy > 0 {
            parts.push(format!("{} energy", self.energy));
        }
        let mut grouped: Vec<(String, u8)> = Vec::new();
        for need in &self.power {
            let label = need.label();
            match grouped.iter_mut().find(|(held, _)| *held == label) {
                Some((_, count)) => *count = count.saturating_add(1),
                None => grouped.push((label, 1)),
            }
        }
        for (label, count) in grouped {
            parts.push(format!("{count} {label} power"));
        }
        if self.xp > 0 {
            parts.push(format!("{} XP", self.xp));
        }
        if self.burn > 0 {
            parts.push(format!("burn {}", self.burn));
        }
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(" and ")
        }
    }
}

fn strike(power: &mut Vec<Need>, need: &Need) {
    let exact = match need {
        Need::Domain(domain) => power.iter().position(|held| held == &Need::Domain(*domain)),
        _ => None,
    };
    let index = exact
        .or_else(|| power.iter().position(|held| matches!(held, Need::Rainbow)))
        .or_else(|| power.iter().position(|held| matches!(held, Need::AnyOf(_))))
        .or_else(|| {
            power
                .iter()
                .position(|held| matches!(held, Need::Domain(_)))
        });
    if let Some(index) = index {
        power.remove(index);
    }
}

pub fn domains_of(card: &CardInfo) -> Vec<Domain> {
    card.domain
        .iter()
        .filter_map(|domain| Domain::parse(domain))
        .collect()
}

fn power_needs(domains: &[Domain], power: usize) -> Vec<Need> {
    match domains.len() {
        0 => vec![Need::Rainbow; power],
        1 => vec![Need::Domain(domains[0]); power],
        n if n == power => domains.iter().copied().map(Need::Domain).collect(),
        _ => vec![Need::AnyOf(domains.to_vec()); power],
    }
}

pub fn printed(card: &CardInfo) -> Cost {
    let domains = domains_of(card);
    Cost {
        energy: card.energy.unwrap_or(0),
        power: power_needs(&domains, usize::from(card.power.unwrap_or(0))),
        ..Cost::default()
    }
}

pub fn accelerate(card: &CardInfo) -> Cost {
    let domains = domains_of(card);
    let power = match domains.as_slice() {
        [only] => Need::Domain(*only),
        _ => Need::Rainbow,
    };
    Cost {
        energy: 1,
        power: vec![power],
        ..Cost::default()
    }
}

pub fn of_parts(energy: u8, power: &[Power], domains: &[Domain]) -> Cost {
    let power = power
        .iter()
        .map(|need| match need {
            Power::Domain(domain) => Need::Domain(*domain),
            Power::Rainbow => Need::Rainbow,
            Power::Own => match domains {
                [] => Need::Rainbow,
                [only] => Need::Domain(*only),
                many => Need::AnyOf(many.to_vec()),
            },
        })
        .collect();
    Cost {
        energy,
        power,
        ..Cost::default()
    }
}

pub fn of_script(cost: &cards::Cost, domains: &[Domain]) -> Cost {
    of_parts(cost.energy, cost.power, domains)
}

pub fn of_grant(grant: &CostedGrant, domains: &[Domain]) -> Cost {
    of_parts(grant.energy, &grant.power, domains)
}

pub fn printed_of(ctx: &Ctx, card: u32, accelerated: bool) -> Cost {
    let Some(face) = ctx.card(card) else {
        return Cost::free();
    };
    let mut cost = printed(face);
    if accelerated && ctx.has_keyword(card, Keyword::Accelerate) {
        cost = cost.plus(accelerate(face));
    }
    cost
}

pub fn priced(ctx: &Ctx, card: u32, price: Price, accelerated: bool) -> Cost {
    let accelerate = || {
        if accelerated && can_accelerate(ctx, card) {
            ctx.card(card).map(accelerate).unwrap_or_default()
        } else {
            Cost::free()
        }
    };
    match price {
        Price::Printed => printed_of(ctx, card, accelerated),
        Price::PowerOnly => {
            let mut cost = printed_of(ctx, card, false);
            cost.energy = 0;
            cost.plus(accelerate())
        }
        Price::LessEnergy(amount) => {
            let mut cost = printed_of(ctx, card, accelerated);
            cost.energy = cost.energy.saturating_sub(amount);
            cost
        }
        Price::Free | Price::Ignored => accelerate(),
    }
}

pub fn self_discount(ctx: &Ctx, card: u32, seat: u8) -> Cost {
    let Some(script) = ctx.script(card) else {
        return Cost::free();
    };
    let domains = ctx.domains_of(card);
    script
        .statics
        .iter()
        .filter_map(|held| match held {
            Static::SelfDiscount(discount) => Some(discount(ctx, card, seat)),
            _ => None,
        })
        .fold(Cost::free(), |sum, discount| {
            sum.plus(of_script(&discount, &domains))
        })
}

fn sources_in_play(ctx: &Ctx) -> Vec<u32> {
    let mut sources: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| ctx.face_in_play(held) && !held.is_hidden())
        .map(|held| held.id)
        .collect();
    sources.sort_unstable();
    sources
}

fn from_other_cards(
    ctx: &Ctx,
    item: &ChainItem,
    pick: fn(&Static) -> Option<cards::ItemDiscount>,
) -> Cost {
    let domains = ctx.domains_of(item.kind.source());
    let mut sum = Cost::free();
    for source in sources_in_play(ctx) {
        let Some(script) = ctx.script(source) else {
            continue;
        };
        for priced in script.statics.iter().filter_map(pick) {
            sum = sum.plus(of_script(&priced(ctx, item, source), &domains));
        }
    }
    sum
}

pub fn play_discounts(ctx: &Ctx, item: &ChainItem) -> Cost {
    if !matches!(
        item.kind,
        ItemKind::Spell { .. } | ItemKind::Permanent { .. }
    ) {
        return Cost::free();
    }
    from_other_cards(ctx, item, |held| match held {
        Static::PlayDiscount(discount) => Some(*discount),
        _ => None,
    })
}

pub fn ability_discounts(ctx: &Ctx, item: &ChainItem) -> Cost {
    if !matches!(item.kind, ItemKind::Ability { .. } | ItemKind::Lent { .. }) {
        return Cost::free();
    }
    from_other_cards(ctx, item, |held| match held {
        Static::AbilityDiscount(discount) => Some(*discount),
        _ => None,
    })
}

pub fn surcharges(ctx: &Ctx, item: &ChainItem) -> Cost {
    if !matches!(
        item.kind,
        ItemKind::Spell { .. } | ItemKind::Permanent { .. }
    ) {
        return Cost::free();
    }
    from_other_cards(ctx, item, |held| match held {
        Static::Surcharge(surcharge) => Some(*surcharge),
        _ => None,
    })
}

fn is_play_of(ctx: &Ctx, kind: PromiseKind, item: &ChainItem) -> bool {
    match (kind, item.kind) {
        (PromiseKind::Any, ItemKind::Spell { .. } | ItemKind::Permanent { .. }) => true,
        (PromiseKind::Spell, ItemKind::Spell { .. }) => true,
        (PromiseKind::Gear, ItemKind::Permanent { card }) => ctx.kind_of(card) == Some(KIND_GEAR),
        (PromiseKind::Unit, ItemKind::Permanent { card }) => ctx.is_unit(card),
        _ => false,
    }
}

pub fn promise_covers(ctx: &Ctx, promise: &Promise, item: &ChainItem) -> bool {
    if !is_play_of(ctx, promise.kind, item) {
        return false;
    }
    match promise.effect {
        PromiseEffect::Discount(_) | PromiseEffect::RepeatForCost => true,
        PromiseEffect::FreeForPower { max_energy } => {
            item.origin == Origin::Hand
                && ctx
                    .card(item.kind.source())
                    .and_then(|face| face.energy)
                    .unwrap_or(0)
                    <= max_energy
        }
    }
}

fn promises_covering<'a>(
    ctx: &'a Ctx,
    item: &'a ChainItem,
) -> impl Iterator<Item = (u8, &'a Promise)> + 'a {
    ctx.blob
        .seat(item.controller)
        .promises
        .iter()
        .enumerate()
        .filter(move |(_, promise)| promise_covers(ctx, promise, item))
        .map(|(index, promise)| (index.min(usize::from(u8::MAX)) as u8, promise))
}

fn free_for_power(ctx: &Ctx, item: &ChainItem) -> bool {
    promises_covering(ctx, item)
        .any(|(_, promise)| matches!(promise.effect, PromiseEffect::FreeForPower { .. }))
}

pub fn repeat_of(ctx: &Ctx, item: &ChainItem) -> Option<Cost> {
    repeat_of_item(ctx, item)
}

pub fn promised_repeat_of(ctx: &Ctx, item: &ChainItem) -> Option<Cost> {
    let card = item.kind.card()?;
    promises_covering(ctx, item)
        .any(|(_, promise)| promise.effect == PromiseEffect::RepeatForCost)
        .then(|| printed_of(ctx, card, false))
}

pub fn promised(ctx: &Ctx, item: &ChainItem) -> Cost {
    let mut sum = Cost::free();
    let mut repeated = false;
    let mut freed = false;
    for (index, promise) in promises_covering(ctx, item) {
        match &promise.effect {
            PromiseEffect::Discount(pool) => {
                sum = sum.plus(Cost {
                    energy: pool.energy,
                    power: pool
                        .power
                        .iter()
                        .map(|held| match held {
                            Pooled::Rainbow => Need::Rainbow,
                            Pooled::Domain(domain) => Need::Domain(*domain),
                        })
                        .collect(),
                    ..Cost::default()
                });
            }
            PromiseEffect::RepeatForCost => {
                if repeated {
                    continue;
                }
                repeated = true;
            }
            PromiseEffect::FreeForPower { .. } => {
                if freed {
                    continue;
                }
                freed = true;
            }
        }
        sum.promises.push(index);
    }
    sum
}

pub fn total(ctx: &Ctx, card: u32, accelerated: bool) -> Cost {
    let mut item = play_item(ctx, ctx.controller(card), card, Origin::Hand);
    if accelerated {
        item.set_slot(SLOT_ACCELERATE, 1);
    }
    of_item(ctx, &item, None)
}

pub fn can_accelerate(ctx: &Ctx, card: u32) -> bool {
    ctx.is_unit(card) && ctx.has_keyword(card, Keyword::Accelerate)
}

fn granting_sources(ctx: &Ctx) -> Vec<u32> {
    let mut sources: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| statics::in_play(ctx, held.id))
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| !script.statics.is_empty())
        })
        .map(|held| held.id)
        .collect();
    sources.sort_unstable();
    sources
}

pub fn granted_accelerate(ctx: &Ctx, item: &ChainItem) -> bool {
    granting_sources(ctx).into_iter().any(|source| {
        ctx.script(source).is_some_and(|script| {
            script.statics.iter().any(|held| {
                matches!(held, Static::GrantsAccelerate(grants) if grants(ctx, item, source))
            })
        })
    })
}

pub fn can_accelerate_item(ctx: &Ctx, item: &ChainItem) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    ctx.is_unit(card)
        && (ctx.has_keyword(card, Keyword::Accelerate) || granted_accelerate(ctx, item))
}

pub fn granted_repeat(ctx: &Ctx, item: &ChainItem) -> Option<Cost> {
    let ItemKind::Spell { card } = item.kind else {
        return None;
    };
    let domains = ctx.domains_of(card);
    granting_sources(ctx)
        .into_iter()
        .find_map(|source| {
            ctx.script(source).and_then(|script| {
                script.statics.iter().find_map(|held| match held {
                    Static::GrantsRepeat(grants) => grants(ctx, item, source),
                    _ => None,
                })
            })
        })
        .map(|granted| of_script(&granted, &domains))
}

pub fn repeat_of_item(ctx: &Ctx, item: &ChainItem) -> Option<Cost> {
    let ItemKind::Spell { card } = item.kind else {
        return None;
    };
    let domains = ctx.domains_of(card);
    ctx.script(card)
        .and_then(|script| {
            script.keywords.iter().find_map(|held| match held {
                Keyword::Repeat(cost) => Some(*cost),
                _ => None,
            })
        })
        .map(|repeat| of_script(&repeat, &domains))
        .or_else(|| ctx.granted_cost(card, Keyword::Repeat(cards::Cost::FREE)))
        .or_else(|| granted_repeat(ctx, item))
}

pub fn ignores_deflect(ctx: &Ctx, item: &ChainItem) -> bool {
    ctx.ignores_deflect(item.kind.source())
}

pub fn deflect(ctx: &Ctx, item: &ChainItem, extra: Option<TargetRef>) -> usize {
    if ignores_deflect(ctx, item) || ctx.deflect_ignored_here(item, extra) {
        return 0;
    }
    item.targets
        .iter()
        .copied()
        .chain(extra)
        .filter_map(|target| match target {
            TargetRef::Card(card) => Some(card),
            _ => None,
        })
        .filter(|card| ctx.card(*card).is_some() && ctx.controller(*card) != item.controller)
        .map(|card| usize::from(ctx.deflect_of(card)))
        .sum()
}

fn origin_cost(ctx: &Ctx, item: &ChainItem, card: u32) -> Cost {
    if let Some(limited) = &item.limited {
        let accelerated = item.accelerated() && can_accelerate_item(ctx, item);
        if matches!(limited.price, Price::LessEnergy(_)) {
            let mut cost = printed_of(ctx, card, false);
            if accelerated {
                if let Some(face) = ctx.card(card) {
                    cost = cost.plus(accelerate(face));
                }
            }
            return cost;
        }
        let mut cost = priced(ctx, card, limited.price, false);
        if accelerated {
            if let Some(face) = ctx.card(card) {
                cost = cost.plus(accelerate(face));
            }
        }
        return cost;
    }
    let base = match item.origin {
        Origin::Facedown { .. } | Origin::Banishment | Origin::Revealed { .. } => Cost::free(),
        Origin::Trash {
            leave: Leave::Banish,
        } => match ctx.flow_of(card) {
            Some(flow) => flow,
            None => printed_of(ctx, card, false),
        },
        Origin::Trash {
            leave: Leave::Recycle,
        } if ctx.kind_of(card) == Some(KIND_SPELL) => Cost {
            energy: 0,
            ..ctx.card(card).map(printed).unwrap_or_default()
        },
        Origin::Trash {
            leave: Leave::Recycle,
        } => Cost::free(),
        Origin::Hand | Origin::Champion | Origin::Board => printed_of(ctx, card, false),
    };
    if matches!(item.origin, Origin::Facedown { .. }) || !item.accelerated() {
        return base;
    }
    match ctx.card(card) {
        Some(face) if can_accelerate_item(ctx, item) => base.plus(accelerate(face)),
        _ => base,
    }
}

pub fn base_of_item(ctx: &Ctx, item: &ChainItem) -> Cost {
    match item.kind {
        ItemKind::Spell { card } | ItemKind::Permanent { card } => {
            let mut cost = origin_cost(ctx, item, card);
            if free_for_power(ctx, item) {
                cost.energy = 0;
            }
            let domains = ctx.domains_of(card);
            if item.slot(SLOT_REPEAT) == Some(1) {
                if let Some(repeat) = repeat_of_item(ctx, item) {
                    cost = cost.plus(repeat);
                }
            }
            if let Some(script) = ctx.script(card) {
                if item.paid_additional() {
                    if let Some(additional) = script.additional {
                        cost = cost.plus(of_script(&additional, &domains));
                    }
                }
            }
            if item.slot(SLOT_PROMISED_REPEAT) == Some(1) {
                if let Some(repeat) = promised_repeat_of(ctx, item) {
                    cost = cost.plus(repeat);
                }
            }
            let mut cost = cost.plus(surcharges(ctx, item));
            if let Some(Price::LessEnergy(amount)) =
                item.limited.as_ref().map(|limited| limited.price)
            {
                cost.energy = cost.energy.saturating_sub(amount);
            }
            cost
        }
        ItemKind::Ability { source, index }
        | ItemKind::Trigger { source, index }
        | ItemKind::Granted {
            holder: source,
            index,
            ..
        }
        | ItemKind::Lent {
            holder: source,
            index,
            ..
        } => {
            let Some(ability) = targets::ability_of(ctx, item) else {
                return Cost::free();
            };
            let domains = ctx.domains_of(source);
            let script_cost = match ability.extra {
                Some(extra) => extra(
                    ctx,
                    Source {
                        card: source,
                        ability: index,
                    },
                ),
                None => ability.cost.unwrap_or(cards::Cost::FREE),
            };
            Cost {
                xp: ability.xp,
                burn: ability.burn,
                ..of_script(&script_cost, &domains)
            }
        }
    }
}

pub fn discounts_of(ctx: &Ctx, item: &ChainItem) -> Cost {
    let seat = item.controller;
    match item.kind {
        ItemKind::Spell { card } | ItemKind::Permanent { card } => self_discount(ctx, card, seat)
            .plus(play_discounts(ctx, item))
            .plus(promised(ctx, item)),
        ItemKind::Ability { .. } | ItemKind::Lent { .. } => ability_discounts(ctx, item),
        ItemKind::Trigger { .. } | ItemKind::Granted { .. } => Cost::free(),
    }
}

pub fn of_activation(ctx: &Ctx, source: u32, index: u8) -> Cost {
    of_item(ctx, &activation_item(ctx, source, index), None)
}

pub fn base_of_activation(ctx: &Ctx, source: u32, index: u8) -> Cost {
    base_of_item(ctx, &activation_item(ctx, source, index))
}

pub fn activation_item(ctx: &Ctx, source: u32, index: u8) -> ChainItem {
    let kind =
        activate::item_kind(ctx, source, index).unwrap_or(ItemKind::Ability { source, index });
    ChainItem::new(0, kind, ctx.controller(source), Origin::Board)
}

pub fn play_item(ctx: &Ctx, seat: u8, card: u32, origin: Origin) -> ChainItem {
    let kind = if ctx.kind_of(card) == Some(KIND_SPELL) {
        ItemKind::Spell { card }
    } else {
        ItemKind::Permanent { card }
    };
    ChainItem::new(0, kind, seat, origin)
}

pub fn priced_item(ctx: &Ctx, seat: u8, card: u32, price: Price) -> ChainItem {
    let mut item = play_item(ctx, seat, card, Origin::Hand);
    item.limited = Some(crate::state::Limited {
        zones: Vec::new(),
        price,
    });
    item
}

pub fn of_item(ctx: &Ctx, item: &ChainItem, extra: Option<TargetRef>) -> Cost {
    let ignored = item
        .limited
        .as_ref()
        .is_some_and(|limited| limited.price == Price::Ignored);
    let mut cost = base_of_item(ctx, item);
    let deflect = deflect(ctx, item, extra);
    cost.power.extend(vec![Need::Rainbow; deflect]);
    let discount = discounts_of(ctx, item);
    let mut cost = cost.less(&discount);
    cost.promises = discount.promises;
    if ignored {
        cost.energy = 0;
        cost.power.clear();
        cost.xp = 0;
        cost.burn = 0;
        cost.pooled = Pool::default();
        return cost;
    }
    pay::from_pool(ctx, item.controller, &cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{Delayed, Expiry, GameBlob, RevealedFrom, When};

    static IGNORED_REPEAT: crate::cards::Card = prelude::spell(
        "Ignored Repeat",
        &[Keyword::Repeat(crate::cards::Cost {
            energy: 1,
            power: &[],
        })],
        &[prelude::play(&[], |ctx, _, _| {
            prelude::draw(ctx, 0, 1);
            crate::cards::Flow::Done
        })],
    );

    #[test]
    fn printed_costs_read_energy_and_power_by_domain_shape() {
        let single = fixtures::spell(1, 0, 0, "Rebuke", 2, 2);
        assert_eq!(
            printed(&single),
            Cost {
                energy: 2,
                power: vec![Need::Domain(Domain::Fury), Need::Domain(Domain::Fury)],
                ..Cost::default()
            }
        );
        let mut paired = single.clone();
        paired.domain = vec!["Calm".into(), "Mind".into()];
        assert_eq!(
            printed(&paired).power,
            [Need::Domain(Domain::Calm), Need::Domain(Domain::Mind)]
        );
        let mut either = paired.clone();
        either.power = Some(1);
        assert_eq!(
            printed(&either).power,
            [Need::AnyOf(vec![Domain::Calm, Domain::Mind])]
        );
        let mut colorless = single.clone();
        colorless.domain.clear();
        assert_eq!(printed(&colorless).power, [Need::Rainbow, Need::Rainbow]);
        let free = fixtures::card(2, 0, 0, "Gold", "Gear");
        assert!(printed(&free).is_free());
        assert_eq!(printed(&free).label(), "nothing");
        assert_eq!(printed(&either).label(), "2 energy and 1 Calm/Mind power");
        assert_eq!(
            printed(&colorless).plus(Cost::free()).label(),
            "2 energy and 2 any power",
            "identical power needs read as one count"
        );
        let mixed = Cost {
            energy: 0,
            power: vec![
                Need::Domain(Domain::Calm),
                Need::Rainbow,
                Need::Domain(Domain::Calm),
            ],
            ..Cost::free()
        };
        assert_eq!(mixed.label(), "2 Calm power and 1 any power");
        assert!(Need::Rainbow.accepts(None));
        assert!(Need::Domain(Domain::Calm).accepts(Some(Domain::Calm)));
        assert!(!Need::Domain(Domain::Calm).accepts(Some(Domain::Mind)));
        assert!(Need::AnyOf(vec![Domain::Calm]).accepts(Some(Domain::Calm)));
        assert!(!Need::AnyOf(vec![Domain::Calm]).accepts(None));
    }

    #[test]
    fn accelerate_adds_one_energy_and_one_power_of_the_units_domain() {
        let unit = fixtures::unit(1, 0, 0, "Vi", 3);
        assert_eq!(
            accelerate(&unit),
            Cost {
                energy: 1,
                power: vec![Need::Domain(Domain::Fury)],
                ..Cost::default()
            }
        );
        let mut dual = unit.clone();
        dual.domain = vec!["Calm".into(), "Mind".into()];
        assert_eq!(accelerate(&dual).power, [Need::Rainbow]);
        let script = cards::Cost {
            energy: 4,
            power: &[Power::Own, Power::Rainbow, Power::Domain(Domain::Body)],
        };
        assert_eq!(
            of_script(&script, &[Domain::Calm]).power,
            [
                Need::Domain(Domain::Calm),
                Need::Rainbow,
                Need::Domain(Domain::Body)
            ]
        );
        assert_eq!(of_script(&script, &[]).power[0], Need::Rainbow);
        assert_eq!(
            of_script(&script, &[Domain::Calm, Domain::Mind]).power[0],
            Need::AnyOf(vec![Domain::Calm, Domain::Mind])
        );
        let mut fixture = Fixture::enforced();
        let ctx = fixture.ctx();
        assert_eq!(total(&ctx, fixtures::HAND_UNIT, false).energy, 2);
        assert_eq!(total(&ctx, fixtures::HAND_UNIT, true).energy, 2);
        assert!(!can_accelerate(&ctx, fixtures::HAND_UNIT));
        assert!(total(&ctx, 999, true).is_free());
    }

    #[test]
    fn xp_and_burn_ride_the_cost_and_print_beside_energy_and_power() {
        let mut cost = Cost {
            energy: 1,
            xp: 2,
            burn: 1,
            ..Cost::default()
        };
        assert!(!cost.is_free());
        assert!(cost.needs_runes());
        assert_eq!(cost.label(), "1 energy and 2 XP and burn 1");
        cost.energy = 0;
        assert!(!cost.needs_runes(), "XP and Burn are not rune needs");
        assert!(!cost.is_free());
        let summed = cost.clone().plus(Cost {
            xp: 1,
            ..Cost::default()
        });
        assert_eq!(summed.xp, 3);
        assert_eq!(
            Cost {
                xp: 1,
                ..Cost::default()
            }
            .label(),
            "1 XP"
        );
    }

    #[test]
    fn a_power_discount_strikes_a_rainbow_first_then_an_either_then_a_printed_domain() {
        let full = Cost {
            energy: 3,
            power: vec![
                Need::Domain(Domain::Calm),
                Need::AnyOf(vec![Domain::Calm, Domain::Body]),
                Need::Rainbow,
            ],
            ..Cost::default()
        };
        let one = Cost {
            power: vec![Need::Rainbow],
            ..Cost::default()
        };
        let struck = full.clone().less(&one);
        assert_eq!(
            struck.power,
            [
                Need::Domain(Domain::Calm),
                Need::AnyOf(vec![Domain::Calm, Domain::Body])
            ],
            "the rainbow goes first"
        );
        let struck = struck.less(&one);
        assert_eq!(
            struck.power,
            [Need::Domain(Domain::Calm)],
            "then the either"
        );
        let struck = struck.less(&one);
        assert!(
            struck.power.is_empty(),
            "356.4.f.1 · then the printed domain need"
        );
        assert!(
            struck.clone().less(&one).power.is_empty(),
            "never below zero"
        );
        let two_energy = Cost {
            energy: 2,
            ..Cost::default()
        };
        assert_eq!(full.clone().less(&two_energy).energy, 1);
        assert_eq!(two_energy.clone().less(&full).energy, 0, "356.6 · floored");
        let by_domain = Cost {
            power: vec![Need::Domain(Domain::Calm)],
            ..Cost::default()
        };
        assert_eq!(
            full.less(&by_domain).power,
            [Need::AnyOf(vec![Domain::Calm, Domain::Body]), Need::Rainbow],
            "a domain discount takes its own need first"
        );
    }

    fn ready_runes(ctx: &Ctx, seat: u8) -> Vec<(u32, Option<Domain>)> {
        ctx.runes_of(seat)
            .into_iter()
            .filter(|rune| !rune.exhausted)
            .map(|rune| (rune.id, crate::engine::ctx::rune_domain(rune)))
            .collect()
    }

    #[test]
    fn limited_and_revealed_units_charge_item_qualified_accelerate() {
        let mut limited = Fixture::enforced();
        let rek_sai = fixtures::unit(90, fixtures::BASE, 0, "Rek'Sai - Breacher", 3);
        let mut champion = fixtures::unit(91, fixtures::CHAMPION, 0, "Tunneler", 2);
        champion.energy = Some(1);
        champion.power = Some(1);
        limited.table.cards.extend([rek_sai, champion]);
        limited.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        limited.resolve();
        let mut ctx = limited.ctx();
        let before = ready_runes(&ctx, 0);
        let mut quoted = play_item(&ctx, 0, 91, Origin::Champion);
        quoted.limited = Some(crate::state::Limited {
            zones: vec![fixtures::BASE],
            price: Price::Printed,
        });
        quoted.set_slot(SLOT_ACCELERATE, 1);
        let quote = of_item(&ctx, &quoted, None);
        assert_eq!(quote.energy, 2);
        assert_eq!(quote.power.len(), 2);
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 91,
                by: 0,
                origin: Origin::Champion,
                locations: vec![crate::engine::ctx::Location::Base(0)],
                price: Price::Printed,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(crate::state::PromptWhy::OptionalCost { cost, .. }) if cost == SLOT_ACCELERATE as u8)
        );
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        let mut restored_blob = GameBlob::decode(&saved_blob).unwrap();
        let mut ctx = Ctx::fresh(&saved_table, &mut restored_blob, &limited.scripts, 0);
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { cost, .. })
                if cost == SLOT_ACCELERATE as u8
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.location(91),
            Some(crate::engine::ctx::Location::Base(0))
        );
        let after = ready_runes(&ctx, 0);
        let spent: Vec<_> = before
            .iter()
            .filter(|rune| !after.iter().any(|remaining| remaining.0 == rune.0))
            .collect();
        assert_eq!(spent.len(), 2);
        assert!(spent
            .iter()
            .all(|(_, domain)| *domain == Some(Domain::Fury)));

        let mut revealed = Fixture::enforced();
        let mut card = fixtures::unit(92, fixtures::CHAIN, 0, "Rek'Sai - Breacher", 3);
        card.energy = Some(1);
        card.power = Some(1);
        revealed.table.cards.push(card);
        revealed.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        revealed.resolve();
        let mut ctx = revealed.ctx();
        let before = ready_runes(&ctx, 0);
        play_engine::begin(
            &mut ctx,
            0,
            92,
            Origin::Revealed {
                from: RevealedFrom::Deck,
            },
            Some(crate::engine::ctx::Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(crate::state::PromptWhy::OptionalCost { cost, .. }) if cost == SLOT_ACCELERATE as u8)
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.location(92),
            Some(crate::engine::ctx::Location::Base(0))
        );
        let after = ready_runes(&ctx, 0);
        let spent: Vec<_> = before
            .iter()
            .filter(|rune| !after.iter().any(|remaining| remaining.0 == rune.0))
            .collect();
        assert_eq!(spent.len(), 1);
        assert_eq!(spent[0].1, Some(Domain::Fury));
    }

    #[test]
    fn limited_less_energy_discount_applies_after_accelerate() {
        let mut fixture = Fixture::enforced();
        let mut card = fixtures::unit(90, fixtures::HAND, 0, "Lillia - Fae Fawn", 2);
        card.energy = Some(2);
        card.power = Some(0);
        card.domain = vec!["Calm".into()];
        fixture.table.cards.push(card);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: vec![crate::engine::ctx::Location::Base(0)],
                price: Price::LessEnergy(3),
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { cost, .. })
                if cost == SLOT_ACCELERATE as u8
        ));
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(saved_table);
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        let mut ctx = fixture.ctx();
        let before = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.location(90),
            Some(crate::engine::ctx::Location::Base(0))
        );
        assert_eq!(ctx.ready_runes_of(0).len(), before - 1);
        assert_eq!(ctx.card(42).unwrap().zone, Some(fixtures::RUNE_DECK));
        assert!(!ctx.card(90).unwrap().exhausted);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    fn ignored_repeat_fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            90,
            fixtures::HAND,
            0,
            "Ignored Repeat",
            1,
            0,
        ));
        for card in &mut fixture.table.cards {
            if card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0 {
                card.exhausted = true;
            }
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(90, &IGNORED_REPEAT);
        fixture
    }

    #[test]
    fn ignored_repeat_accepts_after_reload_and_runs_twice_without_resources() {
        let mut fixture = ignored_repeat_fixture();
        let mut ctx = fixture.ctx();
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: Vec::new(),
                price: Price::Ignored,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { cost, .. })
                if cost == SLOT_REPEAT as u8
        ));
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(90, &IGNORED_REPEAT);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, crate::engine::ctx::Event::Drew { seat: 0, .. }))
                .count(),
            2,
            "{:?}",
            ctx.events
        );
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn ignored_repeat_decline_after_reload_runs_once_without_resources() {
        let mut fixture = ignored_repeat_fixture();
        let mut ctx = fixture.ctx();
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: Vec::new(),
                price: Price::Ignored,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(90, &IGNORED_REPEAT);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, crate::engine::ctx::Event::Drew { seat: 0, .. }))
                .count(),
            1,
            "{:?}",
            ctx.events
        );
        assert!(!ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    fn ignored_keeper_fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut keeper = fixtures::unit(90, fixtures::HAND, 0, "Clockwork Keeper", 2);
        keeper.energy = Some(2);
        keeper.domain = vec!["Calm".into()];
        fixture.table.cards.push(keeper);
        for card in &mut fixture.table.cards {
            if card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0 {
                card.exhausted = true;
            }
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn ignored_clockwork_keeper_accepts_additional_after_reload_and_draws_without_resources() {
        let mut fixture = ignored_keeper_fixture();
        let mut ctx = fixture.ctx();
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: vec![crate::engine::ctx::Location::Base(0)],
                price: Price::Ignored,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { cost, .. })
                if cost == crate::state::SLOT_ADDITIONAL as u8
        ));
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn ignored_clockwork_keeper_decline_after_reload_draws_nothing() {
        let mut fixture = ignored_keeper_fixture();
        let mut ctx = fixture.ctx();
        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: vec![crate::engine::ctx::Location::Base(0)],
                price: Price::Ignored,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn ignored_limited_play_zeroes_surcharges_and_consumes_matching_metadata() {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Vaults of Helia".into();
        let mut unit = fixtures::unit(90, fixtures::HAND, 0, "Jinx", 2);
        unit.energy = Some(3);
        unit.power = Some(1);
        fixture.table.cards.push(unit);
        fixture.blob.delayed.push(Delayed {
            when: When::EndOfTurn(1),
            source: fixtures::GROUNDS,
            seat: 0,
            ability: 1,
            args: Vec::new(),
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.script(fixtures::GROUNDS).is_some());
        let mut free = play_item(&ctx, 0, 90, Origin::Hand);
        free.limited = Some(crate::state::Limited {
            zones: vec![fixtures::BASE],
            price: Price::Free,
        });
        assert_eq!(surcharges(&ctx, &free).energy, 1);
        ctx.blob.seat_mut(0).promises = vec![any_card_discount(0, 1, Expiry::Permanent)];
        assert_eq!(of_item(&ctx, &free, None).energy, 1);
        let mut ignored = free.clone();
        ignored.limited.as_mut().unwrap().price = Price::Ignored;
        let ignored_cost = of_item(&ctx, &ignored, None);
        assert_eq!(ignored_cost.energy, 0);
        assert!(ignored_cost.power.is_empty());
        assert_eq!(ignored_cost.promises, [0]);

        play_engine::begin_limited(
            &mut ctx,
            crate::engine::play::LimitedPlay {
                card: 90,
                by: 0,
                origin: Origin::Hand,
                locations: vec![crate::engine::ctx::Location::Base(0)],
                price: Price::Ignored,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(90),
            Some(crate::engine::ctx::Location::Base(0))
        );
        assert!(ctx.blob.seat(0).promises.is_empty());
        assert_eq!(ready_runes(&ctx, 0).len(), 3);
    }

    fn priced(id: u32, zone: u16, seat: u8, energy: u8, power: u8, domain: &str) -> CardInfo {
        let name = if domain == "Calm" {
            "Nasus, Ascended"
        } else {
            "Brawler"
        };
        let mut card = fixtures::unit(id, zone, seat, name, 8);
        card.energy = Some(energy);
        card.power = Some(power);
        card.domain = vec![domain.into()];
        card
    }

    fn any_card_discount(energy: u8, rainbow: u8, until: Expiry) -> Promise {
        Promise {
            kind: PromiseKind::Any,
            effect: PromiseEffect::Discount(Pool {
                energy,
                power: vec![Pooled::Rainbow; usize::from(rainbow)],
            }),
            until,
        }
    }

    #[test]
    fn a_next_card_promise_turns_an_eight_calm_into_six_and_is_gone_once_paid() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(priced(90, fixtures::HAND, 0, 8, 1, "Calm"));
        for id in 46..52 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Calm", false));
        }
        fixture.resolve();
        fixture.blob.seat_mut(0).promises = vec![any_card_discount(2, 2, Expiry::Permanent)];
        let mut ctx = fixture.ctx();
        let item = ChainItem::new(1, ItemKind::Permanent { card: 90 }, 0, Origin::Hand);
        let discounted = of_item(&ctx, &item, None);
        assert_eq!(discounted.energy, 6);
        assert!(
            discounted.power.is_empty(),
            "the second [A] finds nothing to strike"
        );
        assert_eq!(discounted.promises, [0]);
        assert_eq!(total(&ctx, 90, false).energy, 6, "the pre-play gate agrees");
        let mut theirs = item.clone();
        theirs.controller = 1;
        assert_eq!(
            of_item(&ctx, &theirs, None).energy,
            8,
            "the discount is the seat's, not the card's"
        );
        let trigger = ChainItem::new(
            2,
            ItemKind::Trigger {
                source: fixtures::LEGEND_CARD,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert!(
            !of_item(&ctx, &trigger, None).promised(),
            "a trigger is neither a play nor an activation"
        );
        let planned = crate::engine::pay::plan(&ctx, 0, &discounted).unwrap();
        assert_eq!(planned.promises, [0]);
        crate::engine::pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.blob.seat(0).promises.is_empty(), "consumed by the play");
        assert_eq!(of_item(&ctx, &item, None).energy, 8, "and gone after");
        assert!(!of_item(&ctx, &item, None).promised());
    }

    #[test]
    fn promises_apply_by_kind_stack_when_they_match_and_expire_with_their_turn() {
        let mut fixture = Fixture::enforced();
        fixture.blob.seat_mut(0).promises = vec![
            Promise {
                kind: PromiseKind::Spell,
                effect: PromiseEffect::Discount(Pool {
                    energy: 1,
                    power: Vec::new(),
                }),
                until: Expiry::EndOfTurn(1),
            },
            any_card_discount(1, 1, Expiry::Permanent),
            Promise {
                kind: PromiseKind::Unit,
                effect: PromiseEffect::Discount(Pool {
                    energy: 2,
                    power: Vec::new(),
                }),
                until: Expiry::EndOfTurn(2),
            },
            Promise {
                kind: PromiseKind::Gear,
                effect: PromiseEffect::FreeForPower { max_energy: 2 },
                until: Expiry::EndOfTurn(1),
            },
        ];
        let mut ctx = fixture.ctx();
        let spell = play_item(&ctx, 0, fixtures::HAND_SPELL, Origin::Hand);
        let priced = of_item(&ctx, &spell, None);
        assert_eq!(
            (priced.energy, priced.power.len()),
            (0, 0),
            "the spell promise and the any-card promise both price Spark"
        );
        assert_eq!(priced.promises, [0, 1]);
        let unit = play_item(&ctx, 0, fixtures::HAND_UNIT, Origin::Hand);
        let priced = of_item(&ctx, &unit, None);
        assert_eq!(priced.energy, 0, "two less one less two, not below zero");
        assert_eq!(priced.promises, [1, 2]);
        let gear = play_item(&ctx, 0, fixtures::HAND_GEAR, Origin::Hand);
        let priced = of_item(&ctx, &gear, None);
        assert_eq!(
            priced.energy, 0,
            "356.1.b · the energy is ignored before the discount"
        );
        assert_eq!(priced.promises, [1, 3]);
        let mut elsewhere = gear.clone();
        elsewhere.origin = Origin::Board;
        assert!(!promise_covers(
            &ctx,
            &ctx.blob.seat(0).promises[3],
            &elsewhere
        ));
        let priced = of_item(&ctx, &elsewhere, None);
        assert_eq!(priced.energy, 1, "a play from elsewhere pays its energy");
        assert_eq!(priced.promises, [1]);
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(
            ctx.blob.seat(0).promises,
            [
                any_card_discount(1, 1, Expiry::Permanent),
                Promise {
                    kind: PromiseKind::Unit,
                    effect: PromiseEffect::Discount(Pool {
                        energy: 2,
                        power: Vec::new(),
                    }),
                    until: Expiry::EndOfTurn(2),
                }
            ],
            "the turn's promises lapse, the others wait"
        );
        assert_eq!(of_item(&ctx, &spell, None).energy, 1);
    }

    #[test]
    fn a_body_face_plans_a_body_rune_and_is_refused_on_a_seat_without_one() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(priced(90, fixtures::HAND, 0, 5, 1, "Body"));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        let rengar = total(&ctx, 90, false);
        assert_eq!(rengar.energy, 5);
        assert_eq!(rengar.power, [Need::Domain(Domain::Body)]);
        assert_eq!(
            crate::engine::pay::plan(&ctx, 0, &rengar).unwrap().recycle,
            [46]
        );
        drop(ctx);
        let mut calm_mind = Fixture::enforced();
        calm_mind
            .table
            .cards
            .push(priced(90, fixtures::HAND, 0, 1, 1, "Body"));
        calm_mind.resolve();
        let ctx = calm_mind.ctx();
        assert_eq!(
            crate::engine::pay::plan(&ctx, 0, &total(&ctx, 90, false)),
            Err(crate::Refusal::NoPowerOf),
            "no Body rune on a Fury/Calm seat"
        );
        let either = Cost {
            power: vec![Need::AnyOf(vec![Domain::Calm, Domain::Body])],
            ..Cost::default()
        };
        assert_eq!(
            crate::engine::pay::plan(&ctx, 0, &either).unwrap().recycle,
            [42],
            "Alpha Strike's [C] over Calm/Body accepts the Calm rune"
        );
    }

    #[test]
    fn the_origin_fixes_the_base_cost_and_the_paid_slots_add_their_costs() {
        static FLOWING: cards::Card = prelude::spell(
            "Onslaught",
            &[
                Keyword::Flow(cards::Cost {
                    energy: 4,
                    power: &[],
                }),
                Keyword::Repeat(cards::Cost {
                    energy: 1,
                    power: &[Power::Domain(Domain::Mind)],
                }),
            ],
            &[],
        );
        let mut fixture = Fixture::enforced();
        let mut spell = fixtures::spell(90, fixtures::TRASH, 0, "Onslaught", 6, 1);
        spell.domain = vec!["Body".into()];
        fixture.table.cards.push(spell);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(90, &FLOWING);
        let ctx = fixture.ctx();
        let from = |origin: Origin| ChainItem::new(1, ItemKind::Spell { card: 90 }, 0, origin);
        let flow = base_of_item(
            &ctx,
            &from(Origin::Trash {
                leave: Leave::Banish,
            }),
        );
        assert_eq!(
            (flow.energy, flow.power.len()),
            (4, 0),
            "829.1.c.1 · the Flow cost"
        );
        let fizz = base_of_item(
            &ctx,
            &from(Origin::Trash {
                leave: Leave::Recycle,
            }),
        );
        assert_eq!(
            (fizz.energy, fizz.power),
            (0, vec![Need::Domain(Domain::Body)]),
            "356.1.b.2 · the printed power with the energy ignored"
        );
        assert!(base_of_item(&ctx, &from(Origin::Banishment)).is_free());
        assert!(base_of_item(&ctx, &from(Origin::Facedown { zone: 9 })).is_free());
        assert_eq!(base_of_item(&ctx, &from(Origin::Hand)).energy, 6);
        let mut repeated = from(Origin::Hand);
        repeated.set_slot(crate::state::SLOT_REPEAT, 1);
        let doubled = base_of_item(&ctx, &repeated);
        assert_eq!(doubled.energy, 7);
        assert_eq!(
            doubled.power,
            [Need::Domain(Domain::Body), Need::Domain(Domain::Mind)]
        );
        let mut declined = from(Origin::Hand);
        declined.set_slot(crate::state::SLOT_REPEAT, 0);
        assert_eq!(base_of_item(&ctx, &declined).energy, 6);
        drop(ctx);
        let mut trash_unit = Fixture::enforced();
        trash_unit
            .table
            .cards
            .push(priced(91, fixtures::TRASH, 0, 3, 1, "Body"));
        trash_unit.resolve();
        let ctx = trash_unit.ctx();
        let mut unit = ChainItem::new(
            1,
            ItemKind::Permanent { card: 91 },
            0,
            Origin::Trash {
                leave: Leave::Recycle,
            },
        );
        unit.set_slot(crate::state::SLOT_ADDITIONAL, 1);
        assert!(
            base_of_item(&ctx, &unit).is_free(),
            "a unit played ignoring its cost from the trash is free, and prints no additional"
        );
        unit.set_slot(crate::state::SLOT_ACCELERATE, 1);
        assert!(
            base_of_item(&ctx, &unit).is_free(),
            "a unit without Accelerate pays nothing for a stray accelerate answer"
        );
    }
}

use super::prelude::{adding, legend};
use super::{Adds, Card, Cost, Item, Paying, Power, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::ItemKind;

pub const PAYS: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow, Power::Rainbow],
};
pub const ADDS: Cost = Cost {
    energy: 2,
    power: &[],
};

pub static CARD: Card = adding(legend("Renekton - Butcher of the Sands", &[], &[]), adds);

pub fn for_units(ctx: &Ctx, paying: &Item) -> bool {
    match paying.kind {
        ItemKind::Permanent { card } => ctx.kind_of(card) == Some(KIND_UNIT),
        ItemKind::Ability { source, .. } | ItemKind::Lent { holder: source, .. } => {
            ctx.is_unit(source)
        }
        ItemKind::Spell { .. } | ItemKind::Trigger { .. } | ItemKind::Granted { .. } => false,
    }
}

pub fn can_pay_for(ctx: &Ctx, seat: u8, legend: u32) -> bool {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    ready && mine && ctx.ready_runes_of(seat).len() >= PAYS.power.len()
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, legend: u32, paying: &Item) -> Option<Cost> {
    let units = paying.controller == seat && for_units(ctx, paying);
    (can_pay_for(ctx, seat, legend) && units).then_some(ADDS)
}

fn adds(ctx: &Ctx, seat: u8, legend: u32, paying: Paying) -> Option<Adds> {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    let units = match paying {
        Paying::Applied => false,
        Paying::Item(item) => item.controller == seat && for_units(ctx, item),
    };
    (ready && mine && units).then_some(Adds::exhausting(ADDS).paying(PAYS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Cost as Total;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::state::{ChainItem, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};

    const RENEKTON: u32 = fixtures::LEGEND_CARD;
    const ANVIL: u32 = 90;

    fn pit() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(RENEKTON).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::gear(ANVIL, fixtures::BASE, 0, "Anvil", 2));
        fixture.resolve();
        fixture
    }

    fn with_runes(mut fixture: Fixture, ready: u32) -> Fixture {
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        for id in 46..46 + ready {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        fixture.resolve();
        fixture
    }

    fn unit_play(seat: u8) -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            seat,
            Origin::Hand,
        )
    }

    fn gear_play(seat: u8) -> ChainItem {
        ChainItem::new(
            8,
            ItemKind::Permanent {
                card: fixtures::HAND_GEAR,
            },
            seat,
            Origin::Hand,
        )
    }

    fn spell_play(seat: u8) -> ChainItem {
        ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            seat,
            Origin::Hand,
        )
    }

    fn ability_of(source: u32, seat: u8) -> ChainItem {
        ChainItem::new(
            10,
            ItemKind::Ability { source, index: 0 },
            seat,
            Origin::Board,
        )
    }

    fn trigger_of(source: u32, seat: u8) -> ChainItem {
        ChainItem::new(
            11,
            ItemKind::Trigger { source, index: 0 },
            seat,
            Origin::Board,
        )
    }

    fn two_energy() -> Total {
        Total {
            energy: 2,
            ..Total::default()
        }
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if Some(*zone) == ctx.zones.rune_deck => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_legend_is_an_add_source_for_units_that_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(PAYS.energy, 0);
        assert_eq!(PAYS.power, [Power::Rainbow, Power::Rainbow]);
        assert_eq!(ADDS.energy, 2);
        assert!(ADDS.power.is_empty());
        let mut fixture = pit();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(RENEKTON).unwrap(), &CARD));
        assert!(ctx.is_legend(RENEKTON));
        assert_eq!(
            activate::legal(&ctx, 0, RENEKTON, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the two rainbow and the exhaust are paid with, not activated"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            adds_while_paying(&ctx, 0, RENEKTON, &unit_play(0)),
            Some(ADDS)
        );
    }

    #[test]
    fn he_adds_for_a_unit_play_or_a_units_ability_and_for_nothing_else() {
        let mut fixture = pit();
        let ctx = fixture.ctx();
        assert!(for_units(&ctx, &unit_play(0)));
        assert!(for_units(&ctx, &ability_of(fixtures::VI, 0)));
        assert!(!for_units(&ctx, &gear_play(0)), "a gear is not a unit");
        assert!(!for_units(&ctx, &spell_play(0)), "a spell is not a unit");
        assert!(
            !for_units(&ctx, &ability_of(ANVIL, 0)),
            "a gear's ability is not a unit's"
        );
        assert!(
            !for_units(&ctx, &ability_of(RENEKTON, 0)),
            "a legend's ability is not a unit's"
        );
        assert!(
            !for_units(&ctx, &trigger_of(fixtures::VI, 0)),
            "a unit's trigger is not played, it triggers"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, RENEKTON, &ability_of(fixtures::VI, 0)),
            Some(ADDS)
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, RENEKTON, &gear_play(0)),
            None,
            "spend this Energy only to play units or activated abilities of units"
        );
        assert_eq!(adds_while_paying(&ctx, 0, RENEKTON, &spell_play(0)), None);
        assert_eq!(
            adds_while_paying(&ctx, 1, RENEKTON, &unit_play(1)),
            None,
            "the legend is seat 0's"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, RENEKTON, &unit_play(1)),
            None,
            "an opponent's unit is not his to pay for"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, fixtures::VI, &unit_play(0)),
            None,
            "a unit is no legend"
        );
    }

    #[test]
    fn he_needs_to_be_ready_and_two_ready_runes_for_his_own_two_rainbow() {
        let mut spent = pit();
        spent.table.card_mut(RENEKTON).unwrap().exhausted = true;
        let ctx = spent.ctx();
        assert!(!can_pay_for(&ctx, 0, RENEKTON));
        assert_eq!(adds_while_paying(&ctx, 0, RENEKTON, &unit_play(0)), None);
        drop(ctx);
        let mut one = with_runes(pit(), 1);
        let ctx = one.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert!(
            !can_pay_for(&ctx, 0, RENEKTON),
            "one ready rune cannot make two rainbow"
        );
        assert_eq!(adds_while_paying(&ctx, 0, RENEKTON, &unit_play(0)), None);
        drop(ctx);
        let mut two = with_runes(pit(), 2);
        let ctx = two.ctx();
        assert!(can_pay_for(&ctx, 0, RENEKTON));
        assert_eq!(
            adds_while_paying(&ctx, 0, RENEKTON, &unit_play(0)),
            Some(ADDS)
        );
        assert!(
            pay::plan(&ctx, 0, &two_energy()).is_ok(),
            "two runes make two energy on their own"
        );
    }

    #[test]
    fn a_ready_renekton_pays_two_energy_of_a_unit_by_two_rainbow_and_the_exhaust_and_never_for_a_gear(
    ) {
        let mut fixture = with_runes(pit(), 2);
        let mut ctx = fixture.ctx();
        let four = Total {
            energy: 4,
            ..Total::default()
        };
        assert!(
            pay::plan(&ctx, 0, &four).is_err(),
            "an applied cost is no unit · he does not add for it"
        );
        assert!(
            pay::plan_for(&ctx, 0, &four, Paying::Item(&gear_play(0))).is_err(),
            "a gear is not a unit · he does not add for it"
        );
        assert!(
            pay::plan_for(&ctx, 0, &four, Paying::Item(&spell_play(0))).is_err(),
            "a spell is not a unit · he does not add for it"
        );
        let unit = unit_play(0);
        let planned = pay::plan_for(&ctx, 0, &four, Paying::Item(&unit))
            .expect("Renekton adds two of the four");
        pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.card(RENEKTON).unwrap().exhausted, "he exhausts to add");
        assert_eq!(recycled(&ctx).len(), 2, "two runes recycle for his rainbow");
        assert!(
            pay::plan_for(&ctx, 0, &two_energy(), Paying::Item(&unit)).is_err(),
            "an exhausted Renekton adds nothing more this turn"
        );
        drop(ctx);
        let mut gear = with_runes(pit(), 2);
        let mut ctx = gear.ctx();
        ctx.table.card_mut(fixtures::HAND_GEAR).unwrap().energy = Some(4);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 2
            }),
            "a gear is not a unit · he does not add for it"
        );
    }
}

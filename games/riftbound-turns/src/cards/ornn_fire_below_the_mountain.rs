use super::prelude::{adding, legend, RAINBOW};
use super::{Adds, Card, Cost, Item, Paying, KIND_GEAR};
use crate::engine::ctx::Ctx;
use crate::state::ItemKind;

pub const ADDS: Cost = RAINBOW;

pub static CARD: Card = adding(legend("Ornn - Fire Below the Mountain", &[], &[]), adds);

pub fn for_gear(ctx: &Ctx, paying: &Item) -> bool {
    match paying.kind {
        ItemKind::Permanent { card } => ctx.kind_of(card) == Some(KIND_GEAR),
        ItemKind::Ability { source, .. } | ItemKind::Lent { holder: source, .. } => {
            ctx.is_gear(source)
        }
        ItemKind::Spell { .. } | ItemKind::Trigger { .. } | ItemKind::Granted { .. } => false,
    }
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, legend: u32, paying: &Item) -> Option<Cost> {
    adds(ctx, seat, legend, Paying::Item(paying)).map(|held| held.adds)
}

fn adds(ctx: &Ctx, seat: u8, legend: u32, paying: Paying) -> Option<Adds> {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    let gear = match paying {
        Paying::Applied => false,
        Paying::Item(item) => item.controller == seat && for_gear(ctx, item),
    };
    (ready && mine && gear).then_some(Adds::exhausting(ADDS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Power};
    use crate::engine::cost::{Cost as Total, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::state::{ChainItem, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};

    const ORNN: u32 = fixtures::LEGEND_CARD;
    const ANVIL: u32 = 90;

    fn forge() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ORNN).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::gear(ANVIL, fixtures::BASE, 0, "Anvil", 2));
        fixture.resolve();
        fixture
    }

    fn without_runes(mut fixture: Fixture) -> Fixture {
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture.resolve();
        fixture
    }

    fn gear_play(seat: u8) -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Permanent {
                card: fixtures::HAND_GEAR,
            },
            seat,
            Origin::Hand,
        )
    }

    fn unit_play(seat: u8) -> ChainItem {
        ChainItem::new(
            8,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
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

    fn rainbow() -> Total {
        Total {
            energy: 0,
            power: vec![Need::Rainbow],
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
    fn the_legend_is_an_add_source_for_gear_that_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 0);
        assert_eq!(ADDS.power, [Power::Rainbow]);
        let mut fixture = forge();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(ORNN).unwrap(), &CARD));
        assert!(ctx.is_legend(ORNN));
        assert_eq!(
            activate::legal(&ctx, 0, ORNN, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(adds_while_paying(&ctx, 0, ORNN, &gear_play(0)), Some(ADDS));
    }

    #[test]
    fn he_adds_for_a_gear_play_or_a_gear_ability_and_for_nothing_else() {
        let mut fixture = forge();
        let ctx = fixture.ctx();
        assert!(for_gear(&ctx, &gear_play(0)));
        assert!(for_gear(&ctx, &ability_of(ANVIL, 0)));
        assert!(!for_gear(&ctx, &unit_play(0)), "a unit is not gear");
        assert!(!for_gear(&ctx, &spell_play(0)), "a spell is not gear");
        assert!(
            !for_gear(&ctx, &ability_of(fixtures::VI, 0)),
            "a unit's ability is not a gear's"
        );
        assert!(
            !for_gear(&ctx, &trigger_of(ANVIL, 0)),
            "a gear's trigger is not used, it triggers"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, ORNN, &ability_of(ANVIL, 0)),
            Some(ADDS)
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, ORNN, &unit_play(0)),
            None,
            "use only to play gear or use gear abilities"
        );
        assert_eq!(adds_while_paying(&ctx, 0, ORNN, &spell_play(0)), None);
        assert_eq!(
            adds_while_paying(&ctx, 1, ORNN, &gear_play(1)),
            None,
            "the legend is seat 0's"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, ORNN, &gear_play(1)),
            None,
            "an opponent's gear is not his to pay for"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, fixtures::VI, &gear_play(0)),
            None,
            "a unit is no legend"
        );
        drop(ctx);
        let mut spent = forge();
        spent.table.card_mut(ORNN).unwrap().exhausted = true;
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, ORNN, &gear_play(0)), None);
    }

    #[test]
    fn a_ready_ornn_pays_one_rainbow_of_a_gear_by_exhausting_and_never_for_a_spell() {
        let mut fixture = without_runes(forge());
        let mut ctx = fixture.ctx();
        assert!(
            pay::plan(&ctx, 0, &rainbow()).is_err(),
            "an applied cost is no gear · he does not add for it"
        );
        let gear = gear_play(0);
        let planned =
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&gear)).expect("Ornn adds the rainbow");
        pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.card(ORNN).unwrap().exhausted, "he exhausts to add");
        assert!(recycled(&ctx).is_empty(), "no rune recycles");
        assert!(
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&gear)).is_err(),
            "an exhausted Ornn adds nothing more this turn"
        );
        drop(ctx);
        let mut spell = without_runes(forge());
        let ctx = spell.ctx();
        assert!(
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&spell_play(0))).is_err(),
            "a spell is not gear · he does not add for it"
        );
        assert!(
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&unit_play(0))).is_err(),
            "a unit is not gear · he does not add for it"
        );
        drop(ctx);
        let mut spell = without_runes(forge());
        let mut ctx = spell.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            }),
            "a spell is not gear · he does not add for it"
        );
    }
}

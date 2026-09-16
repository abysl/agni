use super::prelude::{adding, legend, RAINBOW};
use super::{Adds, Card, Cost, Item, Paying};
use crate::engine::ctx::Ctx;
use crate::state::ItemKind;

pub const ADDS: Cost = RAINBOW;

pub static CARD: Card = adding(legend("Kai'Sa - Daughter of the Void", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, legend: u32, paying: &Item) -> Option<Cost> {
    adds(ctx, seat, legend, Paying::Item(paying)).map(|held| held.adds)
}

fn adds(ctx: &Ctx, seat: u8, legend: u32, paying: Paying) -> Option<Adds> {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    let spell = match paying {
        Paying::Applied => false,
        Paying::Item(item) => {
            matches!(item.kind, ItemKind::Spell { .. }) && item.controller == seat
        }
    };
    (ready && mine && spell).then_some(Adds::exhausting(ADDS))
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

    const KAISA: u32 = fixtures::LEGEND_CARD;

    fn void() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(KAISA).unwrap().name = CARD.name.into();
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

    fn spell_item(seat: u8) -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            seat,
            Origin::Hand,
        )
    }

    fn unit_item(seat: u8) -> ChainItem {
        ChainItem::new(
            8,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            seat,
            Origin::Hand,
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
    fn the_legend_is_an_add_source_that_is_paid_with_and_never_activated() {
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
        let mut fixture = void();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(KAISA).unwrap(), &CARD));
        assert!(ctx.is_legend(KAISA));
        assert_eq!(
            activate::legal(&ctx, 0, KAISA, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            adds_while_paying(&ctx, 0, KAISA, &spell_item(0)),
            Some(ADDS)
        );
    }

    #[test]
    fn she_adds_only_for_her_controllers_spells_and_only_while_ready() {
        let mut fixture = void();
        let ctx = fixture.ctx();
        assert_eq!(
            adds_while_paying(&ctx, 0, KAISA, &unit_item(0)),
            None,
            "use only to play spells"
        );
        assert_eq!(
            adds_while_paying(&ctx, 1, KAISA, &spell_item(1)),
            None,
            "the legend is seat 0's"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, KAISA, &spell_item(1)),
            None,
            "an opponent's spell is not hers to pay for"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, fixtures::VI, &spell_item(0)),
            None,
            "a unit is no legend"
        );
        drop(ctx);
        let mut spent = void();
        spent.table.card_mut(KAISA).unwrap().exhausted = true;
        let ctx = spent.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, KAISA, &spell_item(0)), None);
    }

    #[test]
    fn a_ready_kaisa_pays_one_rainbow_of_a_spell_by_exhausting_and_never_for_a_unit() {
        let mut fixture = without_runes(void());
        let mut ctx = fixture.ctx();
        assert!(
            pay::plan(&ctx, 0, &rainbow()).is_err(),
            "an applied cost is no spell · she does not add for it"
        );
        let spell = spell_item(0);
        let planned = pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&spell))
            .expect("Kai'Sa adds the rainbow");
        pay::pay(&mut ctx, 0, &planned);
        assert!(ctx.card(KAISA).unwrap().exhausted, "she exhausts to add");
        assert!(recycled(&ctx).is_empty(), "no rune recycles");
        assert!(
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&spell)).is_err(),
            "an exhausted Kai'Sa adds nothing more this turn"
        );
        drop(ctx);
        let mut unit = without_runes(void());
        let ctx = unit.ctx();
        assert!(
            pay::plan_for(&ctx, 0, &rainbow(), Paying::Item(&unit_item(0))).is_err(),
            "a unit is not a spell · she does not add for it"
        );
        drop(ctx);
        let mut unit = without_runes(void());
        let mut ctx = unit.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            }),
            "a unit is not a spell · she does not add for it"
        );
    }
}

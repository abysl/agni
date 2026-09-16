use super::prelude::{adding, legend, ONE_ENERGY};
use super::{Adds, Card, Cost, Item, Paying};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = ONE_ENERGY;

pub static CARD: Card = adding(legend("Diana - Scorn of the Moon", &[], &[]), adds);

pub fn during_a_showdown(ctx: &Ctx) -> bool {
    ctx.blob.showdown.is_some()
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, legend: u32, paying: &Item) -> Option<Cost> {
    adds(ctx, seat, legend, Paying::Item(paying)).map(|held| held.adds)
}

fn adds(ctx: &Ctx, seat: u8, legend: u32, paying: Paying) -> Option<Adds> {
    let ready = ctx.card(legend).is_some_and(|held| !held.exhausted);
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    let theirs = paying.item().is_some_and(|item| item.controller != seat);
    (ready && mine && !theirs && during_a_showdown(ctx)).then_some(Adds::exhausting(ADDS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Cost as Total;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, pay};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const DIANA: u32 = fixtures::LEGEND_CARD;
    const RAIDER: u32 = 90;

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(DIANA).unwrap().name = CARD.name.into();
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DIANA).unwrap(), &CARD));
        fixture
    }

    fn contested() -> Fixture {
        let mut fixture = temple();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
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

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    fn exhausted(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Annotate {
                    card,
                    key,
                    value: Some(_),
                } if key == "exhausted" => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_legend_is_a_showdown_gated_add_source_that_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain, reaction or not"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(ADDS.energy, 1);
        assert!(ADDS.power.is_empty());
        let mut fixture = temple();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(DIANA).unwrap(), &CARD));
        assert!(ctx.is_legend(DIANA));
        assert_eq!(
            activate::legal(&ctx, 0, DIANA, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
    }

    #[test]
    fn outside_a_showdown_she_adds_nothing_and_inside_one_she_adds_one_energy_to_her_controller() {
        let mut quiet = temple();
        let ctx = quiet.ctx();
        assert!(!during_a_showdown(&ctx));
        assert_eq!(
            adds_while_paying(&ctx, 0, DIANA, &spell_item(0)),
            None,
            "spend this energy only during showdowns"
        );
        drop(ctx);
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(during_a_showdown(&ctx));
        assert_eq!(
            adds_while_paying(&ctx, 0, DIANA, &spell_item(0)),
            Some(ADDS)
        );
        assert_eq!(
            adds_while_paying(&ctx, 1, DIANA, &spell_item(1)),
            None,
            "she is seat 0's legend"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, DIANA, &spell_item(1)),
            None,
            "nor does she pay for the opponent's item"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, fixtures::VI, &spell_item(0)),
            None,
            "a unit is no legend"
        );
        assert!(ctx.exhaust(DIANA));
        assert_eq!(
            adds_while_paying(&ctx, 0, DIANA, &spell_item(0)),
            None,
            "an exhausted Diana adds nothing more this turn"
        );
    }

    #[test]
    fn during_a_showdown_a_ready_diana_pays_an_energy_the_rune_pool_is_short_of() {
        let mut fixture = contested();
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        let planned = pay::plan(&ctx, 0, &energy(1)).expect("Diana adds the energy");
        pay::pay(&mut ctx, 0, &planned);
        assert!(
            ctx.card(DIANA).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(exhausted(&ctx), [DIANA]);
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "an exhausted Diana adds nothing more this turn"
        );
        drop(ctx);
        let mut quiet = temple();
        for rune in [41, 42, 43] {
            quiet.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = quiet.ctx();
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "outside a showdown her energy cannot be spent"
        );
    }
}

use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, friendly_units, play, spell, with_statics,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 5;

pub fn highest_friendly_might(ctx: &Ctx, seat: u8) -> u8 {
    friendly_units(ctx, seat)
        .into_iter()
        .map(|unit| ctx.current_might(unit))
        .max()
        .and_then(|might| u8::try_from(might).ok())
        .unwrap_or(0)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: highest_friendly_might(ctx, seat),
        power: &[],
    }
}

fn split(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    spell(
        "Sky Splitter",
        &[Keyword::Action],
        &[play(
            &[a_unit_at_a_battlefield("a unit at a battlefield")],
            split,
        )],
    ),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal};
    use crate::state::{ChainItem, Expiry, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const SPLITTER: u32 = 90;
    const GIANT: u32 = 91;

    fn armed(giant_might: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            SPLITTER,
            fixtures::HAND,
            0,
            "Sky Splitter",
            8,
            1,
        ));
        if let Some(might) = giant_might {
            fixture
                .table
                .cards
                .push(fixtures::unit(GIANT, fixtures::BF1, 0, "Giant", might));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card: SPLITTER }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_is_an_action_with_a_self_discount_read_from_the_highest_friendly_might() {
        assert!(std::ptr::eq(script_of("Sky Splitter").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        let mut fixture = armed(Some(6));
        let ctx = fixture.ctx();
        assert_eq!(highest_friendly_might(&ctx, 0), 6);
        assert_eq!(
            highest_friendly_might(&ctx, 1),
            3,
            "the opponent's best is the Sprite"
        );
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 2, "8 less 6");
        assert_eq!(cost::of_item(&ctx, &item(0), None).power.len(), 1);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            5,
            "the opponent's copy would read their own units"
        );
        drop(ctx);
        let mut none = armed(None);
        let ctx = none.ctx();
        assert_eq!(highest_friendly_might(&ctx, 0), 3, "Vi in the base counts");
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 5);
        drop(ctx);
        let mut nobody = armed(None);
        nobody.table.cards.retain(|card| card.id != fixtures::VI);
        nobody.resolve();
        let ctx = nobody.ctx();
        assert_eq!(highest_friendly_might(&ctx, 0), 0);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 8);
    }

    #[test]
    fn the_discount_reads_current_might_and_never_goes_below_free() {
        let mut fixture = armed(Some(6));
        let mut ctx = fixture.ctx();
        ctx.might(GIANT, 3, Expiry::EndOfTurn(1), None, 7);
        assert_eq!(highest_friendly_might(&ctx, 0), 9);
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            0,
            "nine Might on an eight-energy spell saturates at free"
        );
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, 2);
    }

    #[test]
    fn played_for_two_it_deals_five_and_is_refused_at_the_undiscounted_price() {
        let mut fixture = armed(Some(6));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SPLITTER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "two energy from the four ready runes"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert_eq!(ctx.card(SPLITTER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
        let mut small = armed(None);
        let ctx = small.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: SPLITTER,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            }),
            "with only Vi's three Might it still costs five"
        );
    }
}

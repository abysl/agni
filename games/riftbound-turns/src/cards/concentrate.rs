use super::prelude::{done, draw, play, spell, with_statics};
use super::{Card, Cost, Flow, Item, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 2;
pub const LEVEL_SIX: u8 = 6;
pub const LEVEL_ELEVEN: u8 = 11;
pub const DISCOUNT_AT_SIX: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const DISCOUNT_AT_ELEVEN: Cost = Cost {
    energy: 4,
    power: &[],
};

pub fn level_reached(ctx: &Ctx, seat: u8, level: u8) -> bool {
    ctx.xp(seat) >= i32::from(level)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if level_reached(ctx, seat, LEVEL_ELEVEN) {
        DISCOUNT_AT_ELEVEN
    } else if level_reached(ctx, seat, LEVEL_SIX) {
        DISCOUNT_AT_SIX
    } else {
        Cost::FREE
    }
}

fn concentrate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = with_statics(
    spell("Concentrate", &[], &[play(&[], concentrate)]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CONCENTRATE: u32 = 90;
    const PRINTED: u8 = 5;

    fn concentrate_card() -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(CONCENTRATE, fixtures::HAND, 0, "Concentrate", PRINTED, 0)
        }
    }

    fn at_xp(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(concentrate_card());
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card: CONCENTRATE }, seat, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> EntryMove {
        EntryMove {
            card: CONCENTRATE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_spell_with_a_level_keyed_self_discount() {
        assert!(std::ptr::eq(script_of("Concentrate").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!((LEVEL_SIX, LEVEL_ELEVEN), (6, 11));
        assert_eq!(DISCOUNT_AT_SIX.energy, 2);
        assert_eq!(DISCOUNT_AT_ELEVEN.energy, 4);
        assert_eq!(DRAWS, 2);
    }

    #[test]
    fn it_costs_five_below_six_xp_three_from_six_and_one_from_eleven() {
        for (xp, energy) in [(0, 5), (5, 5), (6, 3), (10, 3), (11, 1), (30, 1)] {
            let mut fixture = at_xp(xp);
            let ctx = fixture.ctx();
            assert_eq!(level_reached(&ctx, 0, LEVEL_SIX), xp >= 6);
            assert_eq!(level_reached(&ctx, 0, LEVEL_ELEVEN), xp >= 11);
            assert_eq!(
                cost::of_item(&ctx, &item(0), None).energy,
                energy,
                "at {xp} XP"
            );
            assert_eq!(cost::total(&ctx, CONCENTRATE, false).energy, energy);
            assert!(cost::of_item(&ctx, &item(0), None).power.is_empty());
        }
    }

    #[test]
    fn the_level_reads_the_controllers_xp_never_the_opponents() {
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(concentrate_card());
        theirs.set_xp(1, 20);
        theirs.resolve();
        let ctx = theirs.ctx();
        assert!(!level_reached(&ctx, 0, LEVEL_SIX));
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            PRINTED,
            "the opponent's twenty XP is not the controller's"
        );
    }

    #[test]
    fn at_level_six_three_ready_runes_play_it_and_it_draws_two() {
        let mut fixture = at_xp(6);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::play_from_hand(&mut ctx, 0, CONCENTRATE).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "three energy · the printed five less two"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            2
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 2);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
        assert_eq!(ctx.card(CONCENTRATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_level_eleven_one_rune_is_enough_and_below_six_three_runes_are_refused() {
        let mut leveled = at_xp(11);
        for rune in [41, 42] {
            leveled.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = leveled.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        fixtures::play_from_hand(&mut ctx, 0, CONCENTRATE).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "one energy · five less four"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(CONCENTRATE).unwrap().zone, Some(fixtures::TRASH));
        let mut unleveled = at_xp(5);
        let ctx = unleveled.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 3
            }),
            "five XP is not Level 6 · the printed five is owed"
        );
    }
}

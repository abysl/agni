use super::prelude::{channel_exhausted, done, draw, play, spell, with_statics};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const WITHIN: i32 = 3;
const DRAWS: usize = 1;
const RUNES: usize = 1;

pub fn opponent_is_close(ctx: &Ctx, seat: u8) -> bool {
    let victory = ctx.victory_score();
    (0..ctx.players())
        .filter(|other| *other != seat)
        .any(|other| ctx.points(other) + WITHIN >= victory)
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if opponent_is_close(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

fn center(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    channel_exhausted(ctx, item.controller, RUNES);
    done()
}

pub static CARD: Card = with_statics(
    spell("Find Your Center", &[Keyword::Action], &[play(&[], center)]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, priority};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CENTER: u32 = 90;
    const TOP_RUNE: u32 = 32;

    fn center_card() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::spell(CENTER, fixtures::HAND, 0, "Find Your Center", 3, 0)
        }
    }

    fn table(their_points: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(center_card());
        if their_points > 0 {
            fixture.set_points(1, their_points);
        }
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card: CENTER }, seat, Origin::Hand)
    }

    #[test]
    fn the_spell_is_an_action_with_a_conditional_self_discount() {
        let fixture = table(0);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CENTER).unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!(WITHIN, 3);
    }

    #[test]
    fn it_reads_one_energy_at_five_to_seven_opponent_points_and_three_below() {
        for (points, energy) in [(0, 3), (4, 3), (5, 1), (6, 1), (7, 1)] {
            let mut fixture = table(points);
            let ctx = fixture.ctx();
            assert_eq!(ctx.options.victory_score, 8);
            assert_eq!(
                cost::of_item(&ctx, &item(0), None).energy,
                energy,
                "at {points} opponent points"
            );
            assert_eq!(cost::total(&ctx, CENTER, false).energy, energy);
            assert_eq!(
                cost::of_item(&ctx, &item(1), None).energy,
                3,
                "the opponent's own score never discounts it for them"
            );
        }
        let mut own = table(0);
        own.set_points(0, 7);
        let ctx = own.ctx();
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            3,
            "the controller's own points are not an opponent's"
        );
    }

    #[test]
    fn played_for_one_it_draws_and_channels_an_exhausted_rune_and_is_refused_at_full_price() {
        let mut fixture = table(6);
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        fixtures::play_from_hand(&mut ctx, 0, CENTER).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "one energy from the one ready rune"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "one drawn, the spell itself gone"
        );
        assert_eq!(
            ctx.table.held(fixtures::RUNE_POOL, 0).count(),
            pool + 1,
            "one rune channelled"
        );
        assert!(
            ctx.card(TOP_RUNE).unwrap().exhausted,
            "it arrives exhausted"
        );
        assert_eq!(ctx.card(TOP_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(ctx.card(CENTER).unwrap().zone, Some(fixtures::TRASH));
        let mut far = table(0);
        for id in [41, 42, 43] {
            far.table.card_mut(id).unwrap().exhausted = true;
        }
        far.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let ctx = far.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: CENTER,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 1
            }),
            "far from the victory score it costs the printed three"
        );
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Cost, Keyword, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;

pub const ADDITIONAL_XP: u8 = 3;
pub const DISCOUNT: Cost = Cost {
    energy: 3,
    power: &[],
};

pub fn xp_as_the_additional_cost_until_card_additional_carries_xp() -> cost::Cost {
    cost::Cost {
        xp: ADDITIONAL_XP,
        ..cost::Cost::free()
    }
}

pub fn paid_the_xp_on_the_way_in(ctx: &Ctx, card: u32) -> bool {
    ctx.blob
        .queue
        .iter()
        .any(|pending| pending.item.kind.card() == Some(card) && pending.item.paid_additional())
}

fn hammer_discount(ctx: &Ctx, card: u32, _: u8) -> Cost {
    if paid_the_xp_on_the_way_in(ctx, card) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Poppy - Defender of the Meek",
        &[Keyword::Ambush, Keyword::Tank],
        &[],
    ),
    &[Static::SelfDiscount(hammer_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::pay;
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const POPPY: u32 = 90;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];
    const PRINTED: u8 = 6;

    fn poppy() -> CardInfo {
        let mut card = fixtures::unit(POPPY, fixtures::HAND, 0, "Poppy - Defender of the Meek", 5);
        card.energy = Some(PRINTED);
        card.power = Some(1);
        card.domain = vec!["Order".into()];
        card
    }

    fn keep(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poppy());
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(POPPY).unwrap(), &CARD));
        fixture
    }

    fn item(card: u32) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card }, 0, Origin::Hand)
    }

    fn additional_confirm(ctx: &Ctx) -> bool {
        matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        )
    }

    #[test]
    fn the_script_prints_ambush_and_tank_with_one_self_discount_and_the_xp_cost_is_the_named_seam()
    {
        assert!(std::ptr::eq(
            script_of("Poppy - Defender of the Meek").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Ambush, Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is energy and power · XP is not one"
        );
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(hammer_discount)));
        assert_eq!(ADDITIONAL_XP, 3);
        assert_eq!(DISCOUNT.energy, 3);
        assert!(DISCOUNT.power.is_empty());
        let seam = xp_as_the_additional_cost_until_card_additional_carries_xp();
        assert_eq!(seam.label(), "3 XP");
        let mut rich = keep(3);
        let ctx = rich.ctx();
        assert!(pay::affordable(&ctx, 0, &seam));
        drop(ctx);
        let mut poor = keep(2);
        let ctx = poor.ctx();
        assert!(!pay::affordable(&ctx, 0, &seam));
    }

    #[test]
    fn the_discount_reads_the_pending_plays_additional_slot_and_takes_three_energy_off() {
        let mut fixture = keep(3);
        let ctx = fixture.ctx();
        let plain = cost::of_item(&ctx, &item(POPPY), None);
        assert_eq!(plain.energy, PRINTED, "no pending play, no discount");
        assert_eq!(plain.power.len(), 1);
        assert!(!paid_the_xp_on_the_way_in(&ctx, POPPY));
        let mut unpaid = item(POPPY);
        unpaid.set_slot(SLOT_ADDITIONAL, 0);
        ctx.blob.queue.push(Pending {
            item: unpaid,
            needs: Needs::Choices,
        });
        assert!(!paid_the_xp_on_the_way_in(&ctx, POPPY));
        assert_eq!(
            cost::of_item(&ctx, &ctx.blob.queue[0].item.clone(), None).energy,
            PRINTED,
            "declined, the play costs the printed six"
        );
        ctx.blob.queue[0].item.set_slot(SLOT_ADDITIONAL, 1);
        assert!(paid_the_xp_on_the_way_in(&ctx, POPPY));
        let paid = cost::of_item(&ctx, &ctx.blob.queue[0].item.clone(), None);
        assert_eq!(paid.energy, PRINTED - DISCOUNT.energy);
        assert_eq!(paid.power.len(), 1, "the Order power stays");
        assert!(
            !paid_the_xp_on_the_way_in(&ctx, fixtures::HAND_UNIT),
            "another card's play is not hers"
        );
        assert_eq!(
            cost::of_item(&ctx, &item(fixtures::HAND_UNIT), None).energy,
            2
        );
    }

    #[test]
    fn without_the_confirm_she_plays_for_the_printed_six_and_ambushes_where_you_have_units() {
        let mut fixture = keep(3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 6);
        assert!(
            ctx.ambush_locations(0, POPPY).is_empty(),
            "no battlefield holds a friendly unit"
        );
        drop(ctx);
        let mut forward = keep(3);
        forward.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        forward.blob.set_holder(fixtures::BF1, Some(0));
        forward.resolve();
        let ctx = forward.ctx();
        assert_eq!(
            ctx.ambush_locations(0, POPPY),
            [Location::Battlefield(fixtures::BF1)],
            "Vi holds the battlefield she can ambush into"
        );
        drop(ctx);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POPPY).unwrap();
        assert!(!additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert!(ctx.on_board(POPPY));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "no play trigger is printed");
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "six runes for six energy");
        assert_eq!(ctx.xp(0), 3, "no XP is spent");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pool_short_of_six_refuses_the_play() {
        let mut fixture = keep(3);
        fixture.table.card_mut(SPARE_RUNES[0]).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, POPPY),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 5
            })
        );
        assert!(!ctx.on_board(POPPY));
        assert_eq!(ctx.xp(0), 3);
    }

    #[test]
    #[ignore = "engine gap · cards::Cost carries energy and power only, so Card.additional cannot ask for 3 XP; with an xp field the script is with_additional(..., ADDITIONAL), play::advance offers the confirm at STAGE_ADDITIONAL when the seat holds 3 XP, and the paid slot prices her at three"]
    fn with_three_xp_the_play_offers_the_additional_cost_and_paying_it_makes_her_cost_three() {
        let mut fixture = keep(3);
        for rune in SPARE_RUNES {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::play_from_hand(&mut ctx, 0, POPPY).unwrap();
        assert!(additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.on_board(POPPY));
        assert_eq!(ctx.xp(0), 0, "the XP is spent with the rest of the cost");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "three runes for three energy"
        );
        assert!(ctx.fault.is_none());
    }
}

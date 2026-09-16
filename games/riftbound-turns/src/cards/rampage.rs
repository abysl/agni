use super::prelude::{
    a_friendly_unit, an_enemy_unit, card_target, deal, done, might_this_turn, paid_additional,
    play, spell, with_additional,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};
pub const MIGHT: i16 = 2;

fn clash(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let mine = card_target(ctx, item, 0).filter(|unit| ctx.on_board(*unit));
    let theirs = card_target(ctx, item, 1).filter(|unit| ctx.on_board(*unit));
    if let Some(unit) = mine {
        if paid_additional(item) {
            might_this_turn(ctx, item, unit, MIGHT, None);
            ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
        }
    }
    let (Some(mine), Some(theirs)) = (mine, theirs) else {
        return done();
    };
    let to_theirs = u8::try_from(ctx.current_might(mine).max(0)).unwrap_or(u8::MAX);
    let to_mine = u8::try_from(ctx.current_might(theirs).max(0)).unwrap_or(u8::MAX);
    deal(ctx, item, theirs, to_theirs);
    deal(ctx, item, mine, to_mine);
    done()
}

pub static CARD: Card = with_additional(
    spell(
        "Rampage",
        &[],
        &[play(
            &[
                a_friendly_unit("a friendly unit"),
                an_enemy_unit("an enemy unit"),
            ],
            clash,
        )],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, targets};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const RAMPAGE: u32 = 90;
    const BRUTE: u32 = 91;
    const BODY_RUNE: u32 = 46;

    fn rampage() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Body".into()],
            ..fixtures::spell(RAMPAGE, fixtures::HAND, 0, "Rampage", 3, 0)
        }
    }

    fn arena(body_rune: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rampage());
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 4));
        if body_rune {
            fixture
                .table
                .cards
                .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        }
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    fn pending_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        }
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card, n, .. } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn rampage_prints_an_additional_body_cost_and_chooses_two_units() {
        let fixture = arena(true);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RAMPAGE).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 2);
    }

    #[test]
    fn the_additional_cost_adds_a_body_need_only_when_the_slot_says_paid() {
        let mut fixture = arena(true);
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Spell { card: RAMPAGE }, 0, Origin::Hand);
        let plain = cost::of_item(&ctx, &item, None);
        assert_eq!(plain.energy, 3);
        assert!(plain.power.is_empty());
        item.set_slot(SLOT_ADDITIONAL, 0);
        assert!(cost::of_item(&ctx, &item, None).power.is_empty());
        item.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &item, None);
        assert_eq!(paid.energy, 3);
        assert_eq!(paid.power, [Need::Domain(Domain::Body)]);
        assert_eq!(paid.label(), "3 energy and 1 Body power");
        assert!(item.paid_additional());
    }

    fn additional_confirm(ctx: &Ctx) -> Option<PromptWhy> {
        match ctx.blob.why {
            Some(PromptWhy::OptionalCost { item, cost })
                if usize::from(cost) == SLOT_ADDITIONAL =>
            {
                Some(PromptWhy::OptionalCost { item, cost })
            }
            _ => None,
        }
    }

    #[test]
    fn paid_it_gives_the_friendly_unit_two_might_and_both_deal_their_might_to_each_other() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAMPAGE).unwrap();
        assert!(
            additional_confirm(&ctx).is_some(),
            "355.1.a · the additional cost is asked before any target: {:?}",
            ctx.blob.why
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert!(ctx
            .blob
            .pending(pending_item(&ctx))
            .unwrap()
            .item
            .paid_additional());
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Body rune is recycled for the additional cost"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three energy from the four ready runes"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 + 2 this turn");
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, 5), (fixtures::VI, 4)],
            "each takes the other's Might, the buff first"
        );
        assert!(!ctx.on_board(BRUTE), "5 damage kills the 4-Might Brute");
        assert!(
            ctx.on_board(fixtures::VI),
            "4 damage does not kill a 5-Might Vi"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn unpaid_it_buffs_nothing_and_the_two_trade_their_printed_might() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAMPAGE).unwrap();
        assert!(additional_confirm(&ctx).is_some());
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!ctx
            .blob
            .pending(pending_item(&ctx))
            .unwrap()
            .item
            .paid_additional());
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "no additional cost, no Body rune spent"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(damage_events(&ctx), [(BRUTE, 3), (fixtures::VI, 4)]);
        assert!(!ctx.on_board(fixtures::VI), "4 damage kills a 3-Might Vi");
        assert!(ctx.on_board(BRUTE));
    }

    #[test]
    fn with_no_body_rune_the_confirm_is_skipped_and_the_unpaid_play_goes_through() {
        let mut fixture = arena(false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAMPAGE).unwrap();
        assert!(
            additional_confirm(&ctx).is_none(),
            "no Body rune, so the confirm is not offered: {:?}",
            ctx.blob.why
        );
        let item = pending_item(&ctx);
        let mut paid = ctx.blob.pending(item).unwrap().item.clone();
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!crate::engine::pay::affordable(
            &ctx,
            0,
            &cost::of_item(&ctx, &paid, None)
        ));
        let spec = targets::specs_of(&ctx, &paid)[0];
        assert_eq!(spec.label, "a friendly unit");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the unpaid play goes through");
        assert!(!ctx.blob.chain[0].paid_additional());
    }
}

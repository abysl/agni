use super::possession::ENEMY_UNIT_AT_A_BATTLEFIELD;
use super::prelude::{
    a_card, card_target, done, exhaust, paid_additional, play, recall, spell, xp_of,
};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const XP_COST: u8 = 5;
pub const MIGHT_LIMIT: u8 = 3;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_MIGHT_OR_LESS: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::AtBattlefield,
    Filter::MightAtMost(MIGHT_LIMIT),
]);

pub const ANY_ENEMY_UNIT_AT_A_BATTLEFIELD_WHEN_PAID: Filter = ENEMY_UNIT_AT_A_BATTLEFIELD;

pub fn spend_xp_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    xp_of(ctx, seat) >= i32::from(XP_COST)
}

pub fn pay_spend_xp_cost(ctx: &mut Ctx, seat: u8) -> bool {
    if !ctx.spend_xp(seat, XP_COST) {
        return false;
    }
    ctx.narrate(format!(
        "{{seat {seat}}} spends {XP_COST} XP as an additional cost"
    ));
    true
}

pub fn conscriptable(ctx: &Ctx, item: &Item, unit: u32) -> bool {
    ctx.is_unit(unit)
        && ctx.at_battlefield(unit)
        && ctx.controller(unit) != item.controller
        && (paid_additional(item) || ctx.current_might(unit) <= i32::from(MIGHT_LIMIT))
}

pub fn conscript(ctx: &mut Ctx, item: &Item, unit: u32) -> bool {
    if !ctx.set_controller(unit, item.controller, unit) {
        return false;
    }
    exhaust(ctx, unit);
    recall(ctx, unit, false);
    ctx.narrate(format!("{{card {unit}}} is exhausted and recalled"));
    true
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !conscriptable(ctx, item, unit) {
        ctx.narrate(format!(
            "{{card {unit}}} has more than {MIGHT_LIMIT} Might now · it is not conscripted"
        ));
        return done();
    }
    conscript(ctx, item, unit);
    done()
}

pub static CARD: Card = spell(
    "Conscription",
    &[],
    &[play(
        &[a_card(
            ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_MIGHT_OR_LESS,
            "an enemy unit at a battlefield with 3 Might or less",
        )],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const CONSCRIPTION: u32 = 90;
    const THEIR_CONSCRIPTION: u32 = 91;
    const BIG: u32 = 92;
    const CHAOS: [u32; 4] = [100, 101, 102, 103];

    fn conscription(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Conscription", 5, 2);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn draft(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(conscription(CONSCRIPTION, 0));
        fixture
            .table
            .cards
            .push(conscription(THEIR_CONSCRIPTION, 1));
        for rune in CHAOS {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BIG, fixtures::BF1, 1, "Colossus", 6));
        fixture.set_xp(0, xp);
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn paid_item() -> ChainItem {
        let mut item = ChainItem::new(9, ItemKind::Spell { card: CONSCRIPTION }, 0, Origin::Hand);
        item.set_slot(SLOT_ADDITIONAL, 1);
        assert!(item.paid_additional());
        item
    }

    #[test]
    fn the_script_is_a_plain_spell_over_a_small_enemy_unit_at_a_battlefield_with_an_xp_seam() {
        assert!(std::ptr::eq(script_of("Conscription").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is a rune cost; 5 XP is the seam"
        );
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(
            ability.targets[0].filter,
            ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_MIGHT_OR_LESS
        );
        assert_eq!(
            ANY_ENEMY_UNIT_AT_A_BATTLEFIELD_WHEN_PAID,
            Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]),
            "the Possession spec a paid play widens to"
        );
        assert_eq!(XP_COST, 5);
        assert_eq!(MIGHT_LIMIT, 3);
        let mut fixture = draft(4);
        let mut ctx = fixture.ctx();
        assert!(!spend_xp_cost_payable(&ctx, 0));
        assert!(!pay_spend_xp_cost(&mut ctx, 0));
        assert_eq!(ctx.xp(0), 4);
        ctx.score_xp(0, 1);
        assert!(spend_xp_cost_payable(&ctx, 0));
        assert!(pay_spend_xp_cost(&mut ctx, 0));
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} spends 5 XP as an additional cost".to_string()));
        let unpaid = ChainItem::new(9, ItemKind::Spell { card: CONSCRIPTION }, 0, Origin::Hand);
        assert!(conscriptable(&ctx, &unpaid, fixtures::SPRITE));
        assert!(
            !conscriptable(&ctx, &unpaid, BIG),
            "6 Might is too much unpaid"
        );
        assert!(
            !conscriptable(&ctx, &unpaid, fixtures::THEIR_UNIT),
            "in their base"
        );
        assert!(!conscriptable(&ctx, &unpaid, fixtures::VI), "yours");
        let paid = paid_item();
        assert!(
            conscriptable(&ctx, &paid, BIG),
            "paid: any enemy unit at a battlefield"
        );
        assert!(!conscriptable(&ctx, &paid, fixtures::THEIR_UNIT));
    }

    #[test]
    fn a_small_enemy_at_a_battlefield_is_taken_exhausted_and_recalled_to_your_base() {
        let mut fixture = draft(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONSCRIPTION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "cancel"],
            "the 3-Might Sprite; the 6-Might Colossus and Jinx in their base are out"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(
            ctx.controller(fixtures::SPRITE),
            1,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.controller(fixtures::SPRITE), 0);
        assert_eq!(ctx.owner(fixtures::SPRITE), 1, "ownership never changes");
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::SPRITE).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::SPRITE,
            zone: fixtures::BASE,
            seat: 0,
            index: TOP
        }));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Moved { .. })),
            "456 · a recall is not a move"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} takes control of {card 60}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} is exhausted and recalled".to_string()));
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert_eq!(ctx.card(CONSCRIPTION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_conscript_readies_with_your_permanents_on_your_next_awaken() {
        let mut fixture = draft(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONSCRIPTION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        phases::start_turn(&mut ctx);
        assert!(!ctx.card(fixtures::SPRITE).unwrap().exhausted);
        assert_eq!(ctx.controller(fixtures::SPRITE), 0);
    }

    #[test]
    fn a_pump_in_response_over_three_might_leaves_the_unpaid_conscription_empty_handed() {
        let mut fixture = draft(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONSCRIPTION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::SPRITE, 1, None);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.controller(fixtures::SPRITE),
            1,
            "356.3.e · a 4-Might unit is no longer a legal target for the unpaid text"
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.card(CONSCRIPTION).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn paid_the_seam_conscripts_any_enemy_unit_at_a_battlefield() {
        let mut fixture = draft(5);
        let mut ctx = fixture.ctx();
        assert!(pay_spend_xp_cost(&mut ctx, 0));
        let item = paid_item();
        assert!(conscript(&mut ctx, &item, BIG));
        assert_eq!(ctx.controller(BIG), 0);
        assert_eq!(ctx.location(BIG), Some(Location::Base(0)));
        assert!(ctx.card(BIG).unwrap().exhausted);
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert!(!conscript(&mut ctx, &item, 999), "no such card");
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_big_units_units_in_bases_and_your_own() {
        let mut fixture = draft(5);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CONSCRIPTION)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CONSCRIPTION).unwrap();
        for wrong in [BIG, fixtures::THEIR_UNIT, fixtures::VI, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit of 3 Might or less at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CONSCRIPTION).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 5, "nothing was spent");
    }

    #[test]
    #[ignore = "engine gap · spend-XP optional additional cost at the pay stage (the Bard - Mercurial row): play::advance offers the rune cost in Card.additional only, so the 5 XP is never asked, never recorded as paid_additional, and the target spec cannot widen to any enemy unit at a battlefield for a paid play"]
    fn with_five_xp_the_play_offers_the_additional_cost_and_a_paid_play_may_take_the_colossus() {
        let mut fixture = draft(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONSCRIPTION).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "355.1.a · the additional cost is asked before any target: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.xp(0),
            0,
            "357.2 · the XP is spent as the spell is played"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "paid: any enemy unit at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BIG}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.controller(BIG), 0);
    }
}

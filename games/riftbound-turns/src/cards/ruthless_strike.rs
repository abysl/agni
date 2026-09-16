use super::brazen_buccaneer::a_discard_can_be_offered_as_the_additional_cost;
use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, paid_additional, play, spell,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;
pub const PAID_DAMAGE: u8 = 5;
pub const DISCARDS: u8 = 1;

pub fn damage_for(paid_the_discard: bool) -> u8 {
    if paid_the_discard {
        PAID_DAMAGE
    } else {
        DAMAGE
    }
}

pub fn discard_can_be_offered(ctx: &Ctx, seat: u8) -> bool {
    a_discard_can_be_offered_as_the_additional_cost(ctx, seat)
}

fn strike(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let amount = damage_for(paid_additional(item));
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {unit}}} takes {amount}"));
    }
    done()
}

pub static CARD: Card = spell(
    "Ruthless Strike",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        strike,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const BRUTE: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(STRIKE, fixtures::HAND, 0, "Ruthless Strike", 3, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(STRIKE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
    }

    fn dealt(ctx: &Ctx, unit: u32) -> Vec<u8> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card, n, .. } if *card == unit => Some(*n),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_an_action_over_a_unit_at_a_battlefield_with_no_rune_additional_cost() {
        assert!(std::ptr::eq(script_of("Ruthless Strike").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert!(
            CARD.additional.is_none(),
            "Card.additional is energy and power · a discard is not one"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(damage_for(false), DAMAGE);
        assert_eq!(damage_for(true), PAID_DAMAGE);
        assert_eq!((DAMAGE, PAID_DAMAGE, DISCARDS), (3, 5, 1));
    }

    #[test]
    fn the_offer_needs_a_card_in_hand_to_discard() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert!(discard_can_be_offered(&ctx, 0));
        assert!(discard_can_be_offered(&ctx, 1), "one hidden card is a card");
        drop(ctx);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!discard_can_be_offered(&ctx, 1));
    }

    #[test]
    fn unpaid_it_deals_three_to_a_unit_at_a_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "units at battlefields only"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(!ctx.blob.chain[0].paid_additional());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(dealt(&ctx, BRUTE), [DAMAGE]);
        assert!(ctx.on_board(BRUTE), "three on four Might survives");
        assert!(ctx.blob.log.contains(&"{card 92} takes 3".to_string()));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the spell left the hand and nothing was discarded"
        );
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_the_additional_cost_recorded_on_the_item_it_deals_five_instead() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        ctx.blob.chain[0].set_slot(SLOT_ADDITIONAL, 1);
        assert!(ctx.blob.chain[0].paid_additional());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: PAID_DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "five kills the 4-Might Brute");
        assert!(ctx.blob.log.contains(&"{card 92} takes 5".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_takes_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.recall(BRUTE, false);
        fixtures::pass_until_open(&mut ctx);
        assert!(dealt(&ctx, BRUTE).is_empty());
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · a non-resource optional additional cost (discard 1) at the pay stage, the Brazen Buccaneer row: play::advance's additional stage offers energy and power only, so the play never asks for the discard, never records it as paid_additional, and the spell deals three from a real play; with a discard-N additional-cost kind the play asks at STAGE_ADDITIONAL, the discard lands in the trash before the item finalizes, and the resolution reads paid_additional as five"]
    fn from_a_real_play_the_discard_is_offered_at_the_additional_stage_and_paid_it_deals_five() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })),
            "you may discard 1 as an additional cost"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.chain[0].paid_additional());
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 2);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(dealt(&ctx, BRUTE), [PAID_DAMAGE]);
        assert!(!ctx.on_board(BRUTE));
    }
}

use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    a_play_location, done, play, spawn, unit, when, zone_target, Location, Token,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

const CLONE_ARRIVES_READY: bool = false;
pub const WHERE: TargetSpec = a_play_location("where the Shadow Clone is played");

pub fn conjure(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(clone) = spawn(ctx, seat, Token::ShadowClone, at, CLONE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {clone}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Zed, From the Shadows",
    &[],
    &[when(play(&[WHERE], conjure), paid_additional_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::brazen_buccaneer::{
        a_discard_can_be_offered_as_the_additional_cost, DISCARDS,
    };
    use crate::cards::{script_of, Trigger, TOKEN_SHADOW_CLONE};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const ZED: u32 = 90;

    fn zed(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Fury".into()],
            ..fixtures::unit(ZED, zone, 0, "Zed, From the Shadows", 4)
        }
    }

    fn shadows() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(zed(fixtures::HAND));
        fixture.resolve();
        fixture
    }

    fn clones(ctx: &Ctx) -> Vec<CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SHADOW_CLONE)
            .cloned()
            .collect()
    }

    #[test]
    fn the_script_gates_its_clone_on_the_additional_cost_which_is_not_a_resource() {
        assert!(std::ptr::eq(
            script_of("Zed, From the Shadows").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Zed, From the Shadows");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is energy and power · a discard is not one"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the clone");
        assert!(ability.condition.is_some());
        assert_eq!(
            ability.targets,
            [WHERE],
            "no printed location · the clone is played wherever he may play a unit"
        );
        assert_eq!(DISCARDS, 1);
    }

    #[test]
    fn the_offer_needs_a_card_in_hand_to_discard() {
        let mut fixture = shadows();
        let ctx = fixture.ctx();
        assert!(a_discard_can_be_offered_as_the_additional_cost(&ctx, 0));
        assert!(
            a_discard_can_be_offered_as_the_additional_cost(&ctx, 1),
            "one hidden card is a card"
        );
        drop(ctx);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!a_discard_can_be_offered_as_the_additional_cost(&ctx, 1));
    }

    #[test]
    fn today_he_is_played_without_an_offer_and_no_clone_follows() {
        let mut fixture = shadows();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ZED).unwrap();
        assert!(
            !matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL),
            "no resource additional cost, no confirm: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(ZED), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "nothing discarded");
        assert!(clones(&ctx).is_empty());
    }

    #[test]
    fn the_paid_trigger_plays_an_exhausted_zero_might_shadow_clone_where_it_was_aimed() {
        let mut fixture = shadows();
        fixture.table.card_mut(ZED).unwrap().zone = Some(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: ZED,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.set_slot(SLOT_ADDITIONAL, 1);
        item.targets = vec![TargetRef::Zone(fixtures::BF1)];
        assert_eq!(conjure(&mut ctx, &item, Stage(0)), Flow::Done);
        let clone = clones(&ctx).pop().expect("a Shadow Clone");
        assert_eq!(clone.zone, Some(fixtures::BF1), "a held battlefield");
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {}}} to {{zone {}}}",
            clone.id,
            fixtures::BF1
        )));
        drop(ctx);

        let mut fixture = shadows();
        fixture.table.card_mut(ZED).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: ZED,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.set_slot(SLOT_ADDITIONAL, 1);
        item.targets = vec![TargetRef::Zone(fixtures::BASE)];
        assert_eq!(conjure(&mut ctx, &item, Stage(0)), Flow::Done);
        let clone = clones(&ctx).pop().expect("a Shadow Clone");
        assert_eq!(clone.zone, Some(fixtures::BASE));
        assert_eq!(clone.owner, 0);
        assert!(clone.exhausted, "a token unit enters exhausted");
        assert_eq!(clone.might, Some(0));
        assert!(ctx.is_token(clone.id));
        assert!(std::ptr::eq(
            ctx.script(clone.id).unwrap(),
            &crate::cards::shadow_clone::CARD
        ));
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, controller: 0, kind, .. } if *card == clone.id && kind == crate::cards::KIND_UNIT
            )),
            "the clone is played, so play-a-unit triggers see it"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {}}} to their base",
            clone.id
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · play::advance's additional stage offers energy and power only; with a discard-N additional-cost kind the play asks for the discard, records it on the item as paid, and the paid play trigger conjures the Shadow Clone"]
    fn playing_him_offers_a_discard_and_the_paid_play_conjures_a_clone() {
        let mut fixture = shadows();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ZED).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert!(ctx.in_trash(fixtures::HAND_GEAR));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ZED
        ));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let clone = clones(&ctx).pop().expect("a Shadow Clone");
        assert_eq!(clone.zone, Some(fixtures::BASE));
        assert!(clone.exhausted);
    }
}

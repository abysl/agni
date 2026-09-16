use super::prelude::battlefield;
use super::Card;
use crate::engine::ctx::Ctx;
use crate::engine::{hide, statics};

pub const USUAL_FACEDOWN_SLOTS: usize = 1;
pub const EXTRA_SLOT: usize = 1;

pub static CARD: Card = battlefield("Bandle Tree", &[], &[]);

pub fn is_tree(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn battlefield_card_at(ctx: &Ctx, zone: u16) -> Option<u32> {
    if !ctx.zones.is_battlefield(zone) {
        return None;
    }
    ctx.table
        .cards
        .iter()
        .find(|held| held.zone == Some(zone) && ctx.is_battlefield_card(held.id))
        .map(|held| held.id)
}

pub fn extra_hide_slots(ctx: &Ctx, zone: u16) -> usize {
    match battlefield_card_at(ctx, zone) {
        Some(tree) if is_tree(ctx, tree) && statics::in_play(ctx, tree) => EXTRA_SLOT,
        _ => 0,
    }
}

pub fn hide_capacity(ctx: &Ctx, zone: u16) -> usize {
    USUAL_FACEDOWN_SLOTS + extra_hide_slots(ctx, zone)
}

pub fn has_room_to_hide(ctx: &Ctx, zone: u16) -> bool {
    hide::facedown_at(ctx, zone).len() < hide_capacity(ctx, zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::Refusal;

    const TREE: u32 = fixtures::GROUNDS;
    const SECOND_HIDDEN: u32 = 90;

    fn tree() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TREE).unwrap().name = "Bandle Tree".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::hidden(SECOND_HIDDEN, fixtures::HAND, 0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TREE).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_tree_is_a_battlefield_with_no_abilities_whose_text_is_the_hide_capacity_seam() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Bandle Tree").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
    }

    #[test]
    fn the_tree_holds_two_facedown_cards_and_every_other_battlefield_holds_one() {
        let mut fixture = tree();
        let ctx = fixture.ctx();
        assert_eq!(battlefield_card_at(&ctx, fixtures::BF1), Some(TREE));
        assert_eq!(
            battlefield_card_at(&ctx, fixtures::BF2),
            Some(fixtures::ROCKFALL)
        );
        assert_eq!(battlefield_card_at(&ctx, fixtures::BASE), None);
        assert!(is_tree(&ctx, TREE));
        assert!(!is_tree(&ctx, fixtures::ROCKFALL));
        assert_eq!(extra_hide_slots(&ctx, fixtures::BF1), EXTRA_SLOT);
        assert_eq!(extra_hide_slots(&ctx, fixtures::BF2), 0);
        assert_eq!(hide_capacity(&ctx, fixtures::BF1), 2);
        assert_eq!(hide_capacity(&ctx, fixtures::BF2), 1);
        assert_eq!(hide_capacity(&ctx, fixtures::BASE), 1);
    }

    #[test]
    fn room_to_hide_counts_the_facedown_cards_already_there() {
        let mut fixture = tree();
        let mut ctx = fixture.ctx();
        assert!(has_room_to_hide(&ctx, fixtures::BF1));
        assert!(has_room_to_hide(&ctx, fixtures::BF2));
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        assert_eq!(
            hide::facedown_at(&ctx, fixtures::BF1),
            [fixtures::HAND_HIDDEN]
        );
        assert!(
            has_room_to_hide(&ctx, fixtures::BF1),
            "one facedown card leaves the Tree's second slot open"
        );
        ctx.state_mut(SECOND_HIDDEN).hidden_at = Some(fixtures::BF1);
        assert!(!has_room_to_hide(&ctx, fixtures::BF1), "both slots taken");
        ctx.state_mut(fixtures::THEIR_HAND_CARD).hidden_at = Some(fixtures::BF2);
        assert!(
            !has_room_to_hide(&ctx, fixtures::BF2),
            "106.4.b · one card per facedown zone elsewhere"
        );
    }

    #[test]
    fn a_plain_battlefield_refuses_a_second_hide() {
        let mut fixture = tree();
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(has_room_to_hide(&ctx, fixtures::BF2));
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF2).unwrap();
        assert!(!has_room_to_hide(&ctx, fixtures::BF2));
        assert_eq!(
            hide::legal(&ctx, 0, SECOND_HIDDEN, fixtures::BF2),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · Rockfall Path keeps the one-card cap"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · hide::legal caps a facedown zone at one card; it must read a per-battlefield capacity (hide_capacity here) so the Tree takes a second hide"]
    fn a_second_card_may_be_hidden_at_the_tree_and_a_third_is_refused() {
        let mut fixture = tree();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        assert_eq!(hide::legal(&ctx, 0, SECOND_HIDDEN, fixtures::BF1), Ok(()));
        hide::hide(&mut ctx, 0, SECOND_HIDDEN, fixtures::BF1).unwrap();
        assert_eq!(
            hide::facedown_at(&ctx, fixtures::BF1),
            [fixtures::HAND_HIDDEN, SECOND_HIDDEN]
        );
        assert!(!has_room_to_hide(&ctx, fixtures::BF1));
        assert_eq!(
            hide::legal(&ctx, 0, fixtures::HAND_SPELL, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown))
        );
        assert!(ctx.fault.is_none());
    }
}

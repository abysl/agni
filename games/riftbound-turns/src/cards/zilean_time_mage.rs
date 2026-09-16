use super::prelude::{at_battlefield, unit};
use super::Card;
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::FLAG_ONCE_USED;

pub fn is_zilean(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn can_double_a_token_play(ctx: &Ctx, zilean: u32) -> bool {
    is_zilean(ctx, zilean)
        && statics::in_play(ctx, zilean)
        && at_battlefield(ctx, zilean)
        && !ctx.has_flag(zilean, FLAG_ONCE_USED)
}

pub fn would_double_a_token_play(ctx: &Ctx, seat: u8) -> Option<u32> {
    let mut zileans: Vec<u32> = ctx
        .faces_on_board()
        .map(|held| held.id)
        .filter(|card| ctx.controller(*card) == seat && can_double_a_token_play(ctx, *card))
        .collect();
    zileans.sort_unstable();
    zileans.into_iter().next()
}

pub fn spend_the_doubling(ctx: &mut Ctx, zilean: u32) -> bool {
    if !can_double_a_token_play(ctx, zilean) {
        return false;
    }
    ctx.set_flag(zilean, FLAG_ONCE_USED, true);
    ctx.narrate(format!(
        "{{card {zilean}}} plays an additional copy of the token"
    ));
    true
}

pub static CARD: Card = unit("Zilean - Time Mage", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{expiry, settle};
    use agni_plugin_sdk::table::CardInfo;

    const ZILEAN: u32 = 90;
    const THEIR_ZILEAN: u32 = 91;

    fn zilean(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Zilean - Time Mage", 5);
        card.domain = vec!["Mind".into()];
        card.energy = Some(5);
        card.power = Some(1);
        card
    }

    fn clockwork(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(zilean(ZILEAN, zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ZILEAN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn sprites_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == crate::cards::TOKEN_SPRITE && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_replacement_waits_on_the_engine() {
        assert!(std::ptr::eq(
            script_of("Zilean - Time Mage").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn the_seam_names_a_ready_zilean_at_a_battlefield_of_the_token_players_seat() {
        let mut fixture = clockwork(fixtures::BF1);
        fixture
            .table
            .cards
            .push(zilean(THEIR_ZILEAN, fixtures::BF2, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_zilean(&ctx, ZILEAN));
        assert!(!is_zilean(&ctx, fixtures::VI));
        assert!(can_double_a_token_play(&ctx, ZILEAN));
        assert_eq!(would_double_a_token_play(&ctx, 0), Some(ZILEAN));
        assert_eq!(
            would_double_a_token_play(&ctx, 1),
            Some(THEIR_ZILEAN),
            "371 · each controller's own replacement"
        );
    }

    #[test]
    fn in_the_base_stunned_or_in_hand_he_doubles_nothing() {
        let mut fixture = clockwork(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            !can_double_a_token_play(&ctx, ZILEAN),
            "while I'm at a battlefield"
        );
        assert_eq!(would_double_a_token_play(&ctx, 0), None);
        drop(ctx);
        let mut fixture = clockwork(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(!can_double_a_token_play(&ctx, ZILEAN));
        assert_eq!(would_double_a_token_play(&ctx, 0), None);
        drop(ctx);
        let mut fixture = clockwork(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.stun(ZILEAN);
        assert!(
            can_double_a_token_play(&ctx, ZILEAN),
            "a stunned unit still has its static text"
        );
        assert!(!can_double_a_token_play(&ctx, fixtures::VI));
    }

    #[test]
    fn spending_the_doubling_marks_him_for_the_turn_and_expiration_clears_it() {
        let mut fixture = clockwork(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(spend_the_doubling(&mut ctx, ZILEAN));
        assert!(ctx.has_flag(ZILEAN, FLAG_ONCE_USED));
        assert!(!can_double_a_token_play(&ctx, ZILEAN), "once each turn");
        assert_eq!(would_double_a_token_play(&ctx, 0), None);
        assert!(!spend_the_doubling(&mut ctx, ZILEAN));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ZILEAN}}} plays an additional copy of the token"
        )));
        expiry::at_expiration(&mut ctx);
        assert!(!ctx.has_flag(ZILEAN, FLAG_ONCE_USED));
        assert!(
            can_double_a_token_play(&ctx, ZILEAN),
            "371.2.b · the next turn's play may be doubled"
        );
    }

    #[test]
    #[ignore = "engine gap · a replacement on token plays: Card.replacement covers kills only and Ctx::spawn (and the by-hand spawn seams beside faithful_manufactor::spawn_recruit) consult nothing, so a token play is never doubled. With Ctx::spawn asking would_double_a_token_play(ctx, owner) and, on a yes to the optional (371.2), spawning one more of the same face at the same location with the same ready state (375) and calling spend_the_doubling, a Sprite played for his controller arrives as two"]
    fn a_token_played_for_his_controller_arrives_as_two_and_the_choice_is_once_each_turn() {
        let mut fixture = clockwork(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).is_some());
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(sprites_of(&ctx, 0).len(), 2);
        assert!(ctx.has_flag(ZILEAN, FLAG_ONCE_USED));
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).is_some());
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(sprites_of(&ctx, 0).len(), 3);
    }
}

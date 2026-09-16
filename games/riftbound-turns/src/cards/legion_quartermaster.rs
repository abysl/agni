use super::prelude::{bounce, friendly_gear, unit};
use super::Card;
use crate::engine::ctx::Ctx;

pub const RETURNS: usize = 1;

pub fn return_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut gear = friendly_gear(ctx, seat);
    gear.sort_unstable();
    gear
}

pub fn return_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    return_candidates(ctx, seat).len() >= RETURNS
}

pub fn pay_return_cost(ctx: &mut Ctx, gear: u32) -> bool {
    if !ctx.is_gear(gear) || !bounce(ctx, gear) {
        return false;
    }
    ctx.narrate(format!(
        "{{card {gear}}} is returned to its owner's hand as an additional cost"
    ));
    true
}

pub static CARD: Card = unit("Legion Quartermaster", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const QUARTERMASTER: u32 = 90;
    const MY_GEAR: u32 = 91;
    const THEIR_GEAR: u32 = 92;
    const GOLD: u32 = 93;

    fn quartermaster() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(QUARTERMASTER, fixtures::HAND, 0, "Legion Quartermaster", 4)
        }
    }

    fn depot(with_gear: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(quartermaster());
        if with_gear {
            fixture
                .table
                .cards
                .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Rations", 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Loot", 2));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_quartermaster_is_a_plain_unit_whose_whole_text_is_the_return_cost() {
        assert!(std::ptr::eq(
            script_of("Legion Quartermaster").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; his is mandatory and returns a gear"
        );
        assert_eq!(RETURNS, 1);
    }

    #[test]
    fn the_candidates_are_the_controllers_gear_on_the_board_and_the_cost_returns_one_to_hand() {
        let mut fixture = depot(true);
        fixture.table.cards.push(fixtures::gold(GOLD, 0, false));
        fixture.table.tokens.push(GOLD);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(return_candidates(&ctx, 0), [MY_GEAR, GOLD]);
        assert_eq!(
            return_candidates(&ctx, 1),
            [THEIR_GEAR],
            "each seat reads its own gear"
        );
        assert!(return_cost_payable(&ctx, 0));
        let hand = ctx.hand_of(0).len();
        assert!(pay_return_cost(&mut ctx, MY_GEAR));
        assert!(ctx.in_hand(MY_GEAR));
        assert_eq!(ctx.card(MY_GEAR).unwrap().seat, 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MY_GEAR}}} is returned to its owner's hand as an additional cost"
        )));
        assert!(
            !pay_return_cost(&mut ctx, MY_GEAR),
            "a gear already in hand cannot pay twice"
        );
        assert!(
            !pay_return_cost(&mut ctx, fixtures::VI),
            "a unit is not a gear"
        );
        assert_eq!(return_candidates(&ctx, 0), [GOLD]);
        assert!(pay_return_cost(&mut ctx, GOLD));
        assert!(ctx.card(GOLD).is_none(), "a returned token ceases to exist");
        assert!(return_candidates(&ctx, 0).is_empty());
        assert!(
            !return_cost_payable(&ctx, 0),
            "356.2.a.1 · without a friendly gear the mandatory cost cannot be paid"
        );
        let mut alone = depot(false);
        alone.resolve();
        let ctx = alone.ctx();
        assert!(return_candidates(&ctx, 0).is_empty());
        assert!(!return_cost_payable(&ctx, 0));
    }

    #[test]
    #[ignore = "engine gap · a mandatory non-resource additional cost (return a friendly gear to its owner's hand) at the pay stage; play::advance knows optional rune costs only, so the play neither asks which gear returns nor refuses without one"]
    fn playing_him_asks_which_friendly_gear_returns_and_is_refused_without_one() {
        let mut fixture = depot(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, QUARTERMASTER).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "which friendly gear pays: {:?}",
            ctx.blob.why
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_GEAR}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_GEAR}}}")).unwrap();
        assert!(
            ctx.in_hand(MY_GEAR),
            "357.2 · the return is paid before he enters"
        );
        assert_eq!(ctx.location(QUARTERMASTER), Some(Location::Base(0)));
        assert!(ctx.on_board(THEIR_GEAR));
        let mut alone = depot(false);
        let mut ctx = alone.ctx();
        assert!(
            fixtures::play_from_hand(&mut ctx, 0, QUARTERMASTER).is_err(),
            "no friendly gear, no play"
        );
        assert_eq!(ctx.card(QUARTERMASTER).unwrap().zone, Some(fixtures::HAND));
    }
}

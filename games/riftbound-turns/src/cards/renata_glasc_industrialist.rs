use super::prelude::{unit, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn enters_ready(ctx: &Ctx, source: u32, card: u32) -> bool {
    statics::in_play(ctx, source)
        && ctx.is_token(card)
        && ctx.on_board(card)
        && ctx.controller(card) == ctx.controller(source)
}

pub fn your_tokens_enter_ready(ctx: &Ctx, seat: u8) -> bool {
    ctx.faces_on_board()
        .filter(|card| card.name == CARD.name && ctx.controller(card.id) == seat)
        .any(|card| statics::in_play(ctx, card.id))
}

pub static CARD: Card = with_statics(
    unit("Renata Glasc - Industrialist", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyTokens,
        when: enters_ready,
        grants: &[Grant::Static(Static::EntersReady(|_, _| true))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn_gold, Location, Token};
    use crate::cards::script_of;
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};

    const RENATA: u32 = 90;

    fn foundry(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            RENATA,
            zone,
            0,
            "Renata Glasc - Industrialist",
            4,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENATA).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_an_aura_that_grants_enters_ready_to_your_tokens() {
        assert!(std::ptr::eq(
            script_of("Renata Glasc - Industrialist").unwrap(),
            &CARD
        ));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyTokens,
                grants: [Grant::Static(Static::EntersReady(_))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
    }

    #[test]
    fn the_seam_names_your_tokens_on_the_board_while_renata_is_in_play() {
        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(your_tokens_enter_ready(&ctx, 0));
        assert!(!your_tokens_enter_ready(&ctx, 1));
        let gold = spawn_gold(&mut ctx, 0, false).unwrap();
        let soldier = ctx
            .spawn(0, Token::SandSoldier, Location::Base(0), false)
            .unwrap();
        let theirs = ctx
            .spawn(1, Token::SandSoldier, Location::Base(1), false)
            .unwrap();
        assert!(enters_ready(&ctx, RENATA, gold), "a Gold is a token");
        assert!(enters_ready(&ctx, RENATA, soldier), "so is a unit token");
        assert!(!enters_ready(&ctx, RENATA, theirs), "yours");
        assert!(!enters_ready(&ctx, RENATA, fixtures::VI), "tokens");
        assert!(!enters_ready(&ctx, RENATA, RENATA), "she is no token");
        assert!(
            !enters_ready(&ctx, RENATA, fixtures::SPRITE),
            "their Sprite"
        );
        ctx.kill(RENATA, Cause::Cleanup { last_item: None });
        assert!(
            !enters_ready(&ctx, RENATA, gold),
            "365.1 · a dead Renata has no passive"
        );
        assert!(!your_tokens_enter_ready(&ctx, 0));
        drop(ctx);
        let mut fixture = foundry(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let gold = spawn_gold(&mut ctx, 0, false).unwrap();
        assert!(!enters_ready(&ctx, RENATA, gold), "nor one in hand");
        assert!(!your_tokens_enter_ready(&ctx, 0));
    }

    #[test]
    fn a_token_played_beside_her_enters_ready() {
        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let gold = spawn_gold(&mut ctx, 0, false).unwrap();
        assert!(
            !ctx.card(gold).unwrap().exhausted,
            "your tokens enter ready"
        );
        let soldier = ctx
            .spawn(0, Token::SandSoldier, Location::Base(0), false)
            .unwrap();
        assert!(!ctx.card(soldier).unwrap().exhausted);
        let theirs = ctx
            .spawn(1, Token::SandSoldier, Location::Base(1), false)
            .unwrap();
        assert!(ctx.card(theirs).unwrap().exhausted, "not theirs");
    }
}

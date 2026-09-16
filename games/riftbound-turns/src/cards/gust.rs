use super::prelude::{a_card, bounce, card_target, done, play, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT_LIMIT: u8 = 3;
pub const SMALL_UNIT_AT_BATTLEFIELD: Filter = Filter::And(&[
    Filter::Unit,
    Filter::AtBattlefield,
    Filter::MightAtMost(MIGHT_LIMIT),
]);

fn gust(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        bounce(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Gust",
    &[Keyword::Reaction],
    &[play(
        &[a_card(
            SMALL_UNIT_AT_BATTLEFIELD,
            "a unit at a battlefield with 3 Might or less",
        )],
        gust,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Expiry, PromptWhy};

    const GUST: u32 = 90;
    const BIG: u32 = 91;

    #[test]
    fn gust_returns_a_small_unit_at_a_battlefield_reading_current_might() {
        assert!(CARD.has_keyword(Keyword::Reaction));
        let mut fixture = Fixture::enforced();
        let mut gust = fixtures::spell(GUST, fixtures::HAND, 0, "Gust", 1, 0);
        gust.domain = vec!["Fury".into()];
        fixture.table.cards.push(gust);
        fixture
            .table
            .cards
            .push(fixtures::unit(BIG, fixtures::BF1, 1, "Brute", 4));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture
            .blob
            .card_state_mut(BIG)
            .might
            .push(crate::state::MightMod {
                delta: -1,
                until: Expiry::EndOfTurn(1),
                src: 0,
            });
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUST).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 91}", "cancel"],
            "a 4-Might unit under a -1 is a candidate; the unit in base is not"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(BIG).unwrap().zone, Some(fixtures::HAND));
    }
}

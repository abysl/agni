use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 5;

fn punch(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Punch First",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], punch)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;

    const PUNCH: u32 = 90;

    #[test]
    fn punch_first_is_an_action_that_gives_a_unit_five_might_this_turn() {
        assert!(CARD.has_keyword(Keyword::Action));
        let mut fixture = Fixture::enforced();
        let mut punch = fixtures::spell(PUNCH, fixtures::HAND, 0, "Punch First", 1, 2);
        punch.domain = vec!["Fury".into()];
        fixture.table.cards.push(punch);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PUNCH).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 8);
        ctx.expire(crate::state::Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.card(PUNCH).unwrap().zone, Some(fixtures::TRASH));
    }
}

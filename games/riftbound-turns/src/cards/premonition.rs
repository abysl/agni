use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const CARDS: usize = 3;

fn foresee(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, CARDS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = spell("Premonition", &[Keyword::Reaction], &[play(&[], foresee)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};

    const PREMONITION: u32 = 90;

    #[test]
    fn premonition_is_a_reaction_that_draws_three() {
        assert!(CARD.has_keyword(Keyword::Reaction));
        let mut fixture = Fixture::enforced();
        let mut spell = fixtures::spell(PREMONITION, fixtures::HAND, 0, "Premonition", 2, 3);
        spell.domain = vec!["Fury".into()];
        fixture.table.cards.push(spell);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, PREMONITION).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + CARDS);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            CARDS
        );
    }
}

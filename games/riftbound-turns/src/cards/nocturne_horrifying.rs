use super::prelude::{unit, RAINBOW};
use super::{Card, Cost, Keyword};
use crate::engine::ctx::Ctx;

pub static CARD: Card = unit("Nocturne - Horrifying", &[Keyword::Ganking], &[]);

pub const SEEN_COST: Cost = RAINBOW;

pub fn seen_among(ctx: &Ctx, seat: u8, looked: &[u32]) -> Vec<u32> {
    looked
        .iter()
        .copied()
        .filter(|card| {
            ctx.card(*card).is_some_and(|held| held.owner == seat)
                && ctx
                    .script(*card)
                    .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Power;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::march;
    use crate::state::PromptWhy;
    use agni_plugin_sdk::decide::Effect;

    const NOCTURNE: u32 = 90;
    const THEIR_NOCTURNE: u32 = 91;
    const MY_TOP: u32 = 23;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut nocturne = fixtures::unit(NOCTURNE, zone, 0, "Nocturne - Horrifying", 4);
        nocturne.domain = vec!["Chaos".into()];
        fixture.table.cards.push(nocturne);
        let mut theirs = fixtures::unit(
            THEIR_NOCTURNE,
            fixtures::MAIN_DECK,
            1,
            "Nocturne - Horrifying",
            4,
        );
        theirs.domain = vec!["Chaos".into()];
        fixture.table.cards.push(theirs);
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_ganking_and_the_seen_cost_is_one_rainbow() {
        assert!(std::ptr::eq(
            script_of("Nocturne - Horrifying").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(SEEN_COST.energy, 0);
        assert_eq!(SEEN_COST.power, [Power::Rainbow]);
        let mut fixture = armed(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(NOCTURNE, Keyword::Ganking));
        let from = Location::Battlefield(fixtures::BF1);
        let to = Location::Battlefield(fixtures::BF2);
        assert_eq!(
            march::legal_destination(&ctx, NOCTURNE, from, to),
            Ok(()),
            "Ganking: from a battlefield he may move to another battlefield"
        );
        assert_eq!(
            march::legal_destination(&ctx, fixtures::VI, from, to),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::NeedsGanking
            ))
        );
    }

    #[test]
    fn seen_among_picks_only_your_own_nocturnes_out_of_the_cards_you_looked_at() {
        let mut fixture = armed(fixtures::MAIN_DECK);
        let ctx = fixture.ctx();
        assert_eq!(
            seen_among(&ctx, 0, &[MY_TOP, NOCTURNE, THEIR_NOCTURNE, fixtures::VI]),
            [NOCTURNE]
        );
        assert_eq!(
            seen_among(&ctx, 1, &[NOCTURNE, THEIR_NOCTURNE]),
            [THEIR_NOCTURNE]
        );
        assert!(seen_among(&ctx, 0, &[MY_TOP, fixtures::HAND_UNIT]).is_empty());
        assert!(seen_among(&ctx, 0, &[]).is_empty());
    }

    #[test]
    fn today_looking_at_him_on_top_of_the_deck_offers_nothing() {
        let mut fixture = armed(fixtures::MAIN_DECK);
        let deck = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == NOCTURNE)
            .unwrap();
        let top = fixture.table.cards.remove(deck);
        fixture.table.cards.push(top);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.peek_top(0), Some(NOCTURNE));
        crate::engine::triggers::collect(&mut ctx);
        assert!(ctx.blob.queue.is_empty() && ctx.blob.prompt.is_none());
        assert!(ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Peek { card, seat: 0 } if *card == NOCTURNE)));
    }

    #[test]
    #[ignore = "engine gap · triggers off the board: triggers::sources lists in-play cards only and no Looked event carries the cards a player looked at without drawing; a hand-to-board play for SEEN_COST from that trigger (seen_among is the seam) is owed"]
    fn when_you_look_at_him_on_top_of_your_deck_you_may_play_him_for_a_rainbow() {
        let mut fixture = armed(fixtures::MAIN_DECK);
        let deck = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == NOCTURNE)
            .unwrap();
        let top = fixture.table.cards.remove(deck);
        fixture.table.cards.push(top);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.peek_top(0), Some(NOCTURNE));
        crate::engine::triggers::collect(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(NOCTURNE), Some(Location::Base(0)));
    }
}

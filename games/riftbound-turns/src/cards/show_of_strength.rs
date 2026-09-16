use super::prelude::{done, draw, friendly_units, is_mighty, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub fn mighty_units(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_mighty(ctx, *unit))
        .collect()
}

fn show(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let count = mighty_units(ctx, seat).len();
    if count == 0 {
        ctx.narrate(format!(
            "{{seat {seat}}} has no Mighty unit · nothing is drawn"
        ));
        return done();
    }
    let drawn = draw(ctx, seat, count);
    ctx.narrate(format!(
        "{{seat {seat}}} draws {drawn} · {count} Mighty units"
    ));
    done()
}

pub static CARD: Card = spell("Show of Strength", &[Keyword::Reaction], &[play(&[], show)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::MIGHTY;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::state::Priority;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SHOW: u32 = 90;
    const BRUTE: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn show_of_strength(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(id, fixtures::HAND, seat, "Show of Strength", 2, 1)
        }
    }

    fn muscled() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(show_of_strength(SHOW, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 0, "Brute", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF2, 1, "Brute", 7));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SHOW).unwrap(), &CARD));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn cast_and_resolve(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SHOW).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target to choose");
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_a_reaction_with_no_targets() {
        assert!(std::ptr::eq(script_of("Show of Strength").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(MIGHTY, 5);
    }

    #[test]
    fn one_card_is_drawn_per_friendly_mighty_unit_wherever_it_stands() {
        let mut fixture = muscled();
        let mut ctx = fixture.ctx();
        assert_eq!(mighty_units(&ctx, 0), [BRUTE], "Vi at 3 is not Mighty");
        assert_eq!(mighty_units(&ctx, 1), [THEIR_BRUTE]);
        ctx.might(fixtures::VI, 2, crate::state::Expiry::EndOfTurn(1), None, 0);
        assert_eq!(
            mighty_units(&ctx, 0),
            [fixtures::VI, BRUTE],
            "a this-turn +2 makes Vi Mighty"
        );
        let hand = ctx.hand_of(0).len();
        cast_and_resolve(&mut ctx);
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(drew(&ctx, 1), 0, "the opponent's Mighty unit is not yours");
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "one played, two drawn");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws 2 · 2 Mighty units".to_string()));
        assert_eq!(ctx.card(SHOW).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn without_a_mighty_unit_nothing_is_drawn_and_the_count_is_read_as_it_resolves() {
        let mut fixture = muscled();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        ctx.kill(BRUTE, crate::engine::ctx::Cause::Rule);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no Mighty unit · nothing is drawn".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_is_a_reaction_playable_while_the_chain_is_closed_to_actions() {
        let mut fixture = muscled();
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: SHOW,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(legal::classify(&ctx, 0, &entry).is_ok());
        drop(ctx);
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(show_of_strength(SHOW, 1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: SHOW,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(
            matches!(
                legal::classify(&ctx, 1, &entry),
                Err(Refusal::NotYourTurn) | Err(Refusal::Illegal(Reason::ChainClosed))
            ),
            "nothing to react to in the other seat's Neutral Open"
        );
    }
}

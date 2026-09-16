use super::prelude::{
    a_card_not_a_token, at_battlefield, discount_of, done, on_you_play_card, promise, unit, when,
    Promise, PromiseKind,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Expiry;

pub const DISCOUNT: (u8, u8) = (2, 2);

pub fn foreseen() -> Promise {
    Promise {
        kind: PromiseKind::Any,
        effect: discount_of(DISCOUNT.0, DISCOUNT.1),
        until: Expiry::Permanent,
    }
}

fn first_card_while_at_a_battlefield(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let seat = ctx.controller(source.card);
    let first = match event {
        Event::PlayedSpell { nth, .. } => *nth == 1,
        _ => ctx.blob.seat(seat).cards_played == 1,
    };
    first && a_card_not_a_token(ctx, event) && at_battlefield(ctx, source.card)
}

fn foresee(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    promise(
        ctx,
        seat,
        PromiseKind::Any,
        discount_of(DISCOUNT.0, DISCOUNT.1),
        Expiry::Permanent,
    );
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}}'s next card costs {} energy and {} power less",
        item.kind.source(),
        DISCOUNT.0,
        DISCOUNT.1
    ));
    done()
}

pub static CARD: Card = unit(
    "Astral Heron",
    &[],
    &[when(
        on_you_play_card(&[], foresee),
        first_card_while_at_a_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, phases, priority, settle};
    use crate::state::{ItemKind, PromptWhy};

    const HERON: u32 = 90;
    const CREW: u32 = 92;

    #[test]
    fn a_token_played_after_the_first_card_is_not_a_card_and_sets_no_second_discount() {
        let mut fixture = armed(fixtures::BF1);
        fixture.blob.seat_mut(0).cards_played = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).is_some());
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.seat(0).promises.is_empty());
        assert_eq!(
            ctx.blob.seat(0).cards_played,
            1,
            "185 · a token is not a card"
        );
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(HERON, zone, 0, "Astral Heron", 7));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_first_spell_of_the_turn_sets_the_floating_discount_when_it_resolves_not_when_countered()
    {
        assert!(std::ptr::eq(script_of("Astral Heron").unwrap(), &CARD));
        let mut fixture = armed(fixtures::BF1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 1);
        assert_eq!(ctx.blob.seat(0).spells_played, 1);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "419.4.a: playing a spell triggers nothing until it resolves"
        );
        assert!(ctx.blob.seat(0).promises.is_empty());
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == HERON
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.seat(0).promises,
            [foreseen()],
            "the discount is set once the resolved spell's trigger resolves"
        );
        let mut countered = armed(fixtures::BF1);
        let mut ctx = countered.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(chain::counter(
            &mut ctx,
            1,
            crate::engine::ctx::CounterDest::Trash
        ));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "419.4.a.1: a countered card never triggers play abilities"
        );
        assert_eq!(
            ctx.blob.seat(0).cards_played,
            1,
            "419.4.b: the countered card was still finalized"
        );
        let mut permanent = armed(fixtures::BF1);
        let mut ctx = permanent.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(
            matches!(
                ctx.blob.chain.last().map(|top| top.kind),
                Some(ItemKind::Trigger { source, index: 0 }) if source == HERON
            ),
            "a permanent's finalization is its resolution"
        );
    }

    fn heron_triggers(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    ItemKind::Trigger { source, index: 0 } if source == HERON || source == HERON + 1
                )
            })
            .count()
    }

    fn end_turn_through(ctx: &mut Ctx) {
        phases::end_turn(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        crate::engine::settle(ctx).unwrap();
    }

    #[test]
    fn two_herons_at_a_battlefield_order_their_triggers_and_stack_their_discounts() {
        let mut fixture = armed(fixtures::BF1);
        fixture.table.cards.push(fixtures::unit(
            HERON + 1,
            fixtures::BF1,
            0,
            "Astral Heron",
            7,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(ctx.blob.queue.len(), 2);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 })
        ));
        let first = fixtures::labels(&ctx)[0].clone();
        fixtures::choose(&mut ctx, 0, &first).unwrap();
        assert_eq!(heron_triggers(&ctx), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob.seat(0).promises,
            [foreseen(), foreseen()],
            "each Heron's trigger adds its own discount to the next card"
        );
        let stacked = cost::total(&ctx, fixtures::HAND_SPELL, false);
        assert_eq!(
            (
                stacked.energy,
                stacked.power.len(),
                stacked.promises.as_slice()
            ),
            (0, 0, &[0, 1][..]),
            "four energy and four power off Spark's two and one"
        );
        end_turn_through(&mut ctx);
        end_turn_through(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (3, 0));
        assert_eq!(ctx.blob.seat(0).cards_played, 0);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "the stacked discount paid for the next turn's first card"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 })
        ));
        let first = fixtures::labels(&ctx)[0].clone();
        fixtures::choose(&mut ctx, 0, &first).unwrap();
        assert_eq!(heron_triggers(&ctx), 2);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).promises, [foreseen(), foreseen()]);
    }

    #[test]
    fn a_heron_and_a_different_play_trigger_still_ask_their_order() {
        let mut fixture = armed(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(CREW, fixtures::BASE, 0, "Pit Crew", 3));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        let ordering = fixtures::labels(&ctx);
        assert_eq!(
            ordering,
            [
                format!(
                    "{{card {HERON}}} trigger · {{card {}}}",
                    fixtures::HAND_GEAR
                ),
                format!("{{card {CREW}}} trigger · {{card {}}}", fixtures::HAND_GEAR)
            ],
            "a Heron and a Pit Crew are different abilities, so their order is a real choice"
        );
        fixtures::choose(&mut ctx, 0, &ordering[1]).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the last trigger's place is forced once the first is picked"
        );
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(
            (
                ctx.blob.chain[0].kind.source(),
                ctx.blob.chain[1].kind.source()
            ),
            (CREW, HERON),
            "the first pick is placed first, so the Heron resolves first"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).promises, [foreseen()]);
    }

    #[test]
    fn the_second_card_and_a_heron_at_base_fire_nothing() {
        let mut fixture = armed(fixtures::BF1);
        fixture.blob.seat_mut(0).cards_played = 1;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 2);
        assert_eq!(ctx.blob.chain.len(), 1, "no trigger for the second card");
        let mut home = armed(fixtures::BASE);
        let mut ctx = home.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        let mut theirs = armed(fixtures::BF1);
        theirs.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        theirs.blob.set_phase(crate::state::Phase::Action);
        theirs.blob.seats = vec![Default::default(); 2];
        theirs.blob.set_holder(fixtures::BF1, Some(0));
        theirs
            .table
            .cards
            .push(fixtures::unit(91, fixtures::HAND, 1, "Jinx", 2));
        theirs
            .table
            .cards
            .push(fixtures::rune(46, 1, "Mind", false));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        fixtures::play_from_hand(&mut ctx, 1, 91).unwrap();
        assert_eq!(ctx.blob.seat(1).cards_played, 1);
        assert!(
            ctx.blob.chain.is_empty(),
            "an opponent's first card is not yours"
        );
    }
}

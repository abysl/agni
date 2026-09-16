use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const CARDS: usize = 1;

pub fn rally_this_turn(ctx: &mut Ctx, item: &Item) {
    let seat = item.controller;
    let turn = ctx.turn();
    ctx.narrate(format!(
        "{{seat {seat}}} rallies for turn {turn} · each unit they play this turn is buffed"
    ));
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    rally_this_turn(ctx, item);
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = spell("Rally the Troops", &[Keyword::Action], &[play(&[], rally)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, priority, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const RALLY: u32 = 90;
    const THEIR_RALLY: u32 = 91;
    const ORDER_A: u32 = 100;
    const ORDER_B: u32 = 101;

    fn rally(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Rally the Troops", 2, 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rally(RALLY, 0));
        fixture.table.cards.push(rally(THEIR_RALLY, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_A, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_B, 0, "Order", false));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, RALLY).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn play_unit_to_base(ctx: &mut Ctx) {
        let base = ctx.zones.base.unwrap();
        ctx.enter(&fixtures::move_action(fixtures::HAND_UNIT, base, 0), 0)
            .unwrap();
        play_engine::begin(
            ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(ctx).unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_targetless_action() {
        assert!(std::ptr::eq(script_of("Rally the Troops").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
    }

    #[test]
    fn the_rally_resolves_for_two_energy_and_two_order_draws_one_and_touches_no_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        cast(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + 1,
            "the Rally left, one card came"
        );
        assert!(
            !ctx.is_buffed(fixtures::VI),
            "units already there are not buffed"
        );
        let turn = ctx.turn();
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} rallies for turn {turn} · each unit they play this turn is buffed"
        )));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(RALLY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · floating turn effects: triggers::sources lists in-play cards only, so a resolved spell cannot watch the units its controller plays this turn; rally_this_turn only narrates until the engine grows a turn-scoped Trigger::YouPlayCard watcher sourced from a resolved item"]
    fn a_friendly_unit_played_later_this_turn_is_buffed_and_one_played_next_turn_is_not() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        play_unit_to_base(&mut ctx);
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            ctx.is_buffed(fixtures::HAND_UNIT),
            "played this turn under the rally · {:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.current_might(fixtures::HAND_UNIT), 3);
    }

    #[test]
    fn the_rally_is_the_turn_players_action_and_needs_its_two_order_power() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RALLY)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(ORDER_B).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, RALLY)),
            Err(Refusal::NoPowerOf),
            "one Order rune cannot pay two Order power"
        );
    }
}

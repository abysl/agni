use super::prelude::{banish_by, done, play, queue_turn, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn take_a_turn_after_this_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    queue_turn(ctx, seat);
    ctx.narrate(format!("{{seat {seat}}} will take a turn after this one"));
    banish_by(ctx, item.kind.source(), item.controller);
    done()
}

pub static CARD: Card = spell("Time Warp", &[], &[play(&[], take_a_turn_after_this_one)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{CounterDest, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, priority, settle};
    use crate::state::{GameBlob, Phase};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const WARP: u32 = 90;
    const THEIR_WARP: u32 = 91;
    const ENERGY: u8 = 10;
    const POWER: u8 = 4;
    const FIRST_EXTRA_RUNE: u32 = 100;

    fn warp(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Time Warp", ENERGY, POWER);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(warp(WARP, 0));
        fixture.table.cards.push(warp(THEIR_WARP, 1));
        for rune in FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 14 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(WARP).unwrap(), &CARD));
        fixture
    }

    fn pass_both(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn end_turn(ctx: &mut Ctx) {
        phases::end_turn(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_ten_cost_spell_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Time Warp").unwrap(),
            &CARD
        ));
        assert!(
            CARD.keywords.is_empty(),
            "no Action, no Reaction: Sorcery timing"
        );
        assert_eq!(CARD.abilities.len(), 1);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.abilities[0].cost.is_none());
    }

    #[test]
    fn time_warp_queues_its_controllers_extra_turn_and_banishes_itself() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARP).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(effect, Effect::Move { zone, .. } if Some(*zone) == ctx.zones.rune_deck))
                .count(),
            4,
            "four Mind runes recycled for the power"
        );
        assert!(ctx.blob.extra_turns.is_empty(), "nothing until it resolves");
        pass_both(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.extra_turns, [0]);
        assert!(ctx.events.contains(&Event::TurnQueued { seat: 0 }));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Banished {
                card,
                owner: 0,
                by: 0,
                ..
            } if *card == WARP
        )));
        assert!(
            ctx.effects.contains(&Effect::Move {
                card: WARP,
                zone: fixtures::BANISHMENT,
                seat: 0,
                index: TOP
            }),
            "banished by its own execution: {:?}",
            ctx.effects
        );
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { card, zone, .. } if *card == WARP && *zone == fixtures::TRASH
            )),
            "chain::finish leaves a card its execution already moved"
        );
        assert!(ctx.in_banishment(WARP));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} will take a turn after this one".to_string()));
        end_turn(&mut ctx);
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (2, 0),
            "the extra turn is the controller's, right after this one"
        );
        assert!(ctx.blob.extra_turns.is_empty());
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} takes an extra turn".to_string()));
        end_turn(&mut ctx);
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (3, 1),
            "the rotation is untouched afterwards (737)"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_saved_item_self_banishes_by_its_captured_controller_after_control_changes() {
        let mut fixture = armed();
        let (table, blob) = {
            let mut ctx = fixture.ctx();
            fixtures::play_from_hand(&mut ctx, 0, WARP).unwrap();
            assert_eq!(ctx.blob.chain.len(), 1);
            assert_eq!(ctx.blob.chain[0].controller, 0);
            assert!(ctx.set_controller(WARP, 1, WARP));
            ctx.table.card_mut(WARP).unwrap().owner = 1;
            (
                ctx.table.clone(),
                GameBlob::decode(&ctx.blob.encode()).unwrap(),
            )
        };
        fixture.table = table;
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(WARP), 1);
        assert_eq!(ctx.card(WARP).unwrap().owner, 1);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        pass_both(&mut ctx);
        assert_eq!(ctx.blob.extra_turns, [0]);
        assert!(ctx.events.contains(&Event::Banished {
            card: WARP,
            owner: 1,
            by: 0,
            token: false,
        }));
    }

    #[test]
    fn a_countered_time_warp_goes_to_the_trash_and_queues_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARP).unwrap();
        let item = ctx.blob.chain[0].id;
        assert!(ctx.counter_item(item, CounterDest::Trash));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.extra_turns.is_empty());
        assert!(ctx.in_trash(WARP));
        assert!(!ctx.in_banishment(WARP));
        end_turn(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
    }

    #[test]
    fn the_other_seat_cannot_play_it_on_this_turn_and_short_runes_refuse_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(matches!(
            fixtures::play_from_hand(&mut ctx, 1, THEIR_WARP),
            Err(Refusal::NotYourTurn) | Err(Refusal::Illegal(Reason::ClosedTiming))
        ));
        assert!(ctx.blob.chain.is_empty());
        let mut poor = Fixture::enforced();
        poor.table.cards.push(warp(WARP, 0));
        poor.resolve();
        let mut ctx = poor.ctx();
        assert!(fixtures::play_from_hand(&mut ctx, 0, WARP).is_err());
        assert!(ctx.blob.extra_turns.is_empty());
    }
}

use super::prelude::{done, play, spawn_gold, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const GOLDS: usize = 4;
pub const GOLD_ARRIVES_READY: bool = false;

pub fn play_golds(ctx: &mut Ctx, seat: u8, count: usize) -> Vec<u32> {
    (0..count)
        .filter_map(|_| spawn_gold(ctx, seat, GOLD_ARRIVES_READY))
        .collect()
}

fn hoard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_golds(ctx, item.controller, GOLDS);
    done()
}

pub static CARD: Card = unit("Trove Golem", &[], &[play(&[], hoard)]);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, KIND_GEAR, TOKEN_GOLD};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const GOLEM: u32 = 90;

    pub fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn golem(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(GOLEM, zone, 0, "Trove Golem", 9);
        card.domain = vec!["Order".into()];
        card.energy = Some(1);
        card
    }

    fn vault() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(golem(fixtures::HAND));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GOLEM).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(script_of("Trove Golem").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert_eq!(GOLDS, 4);
    }

    #[test]
    fn playing_the_golem_plays_four_exhausted_golds_into_your_base_when_the_trigger_resolves() {
        let mut fixture = vault();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GOLEM).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the golem asks nothing");
        assert!(ctx.on_board(GOLEM));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == GOLEM
        ));
        assert!(golds_of(&ctx, 0).is_empty(), "the Golds wait for the chain");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds, [next, next + 1, next + 2, next + 3]);
        for gold in golds {
            assert!(ctx.is_gear(gold));
            assert!(ctx.is_token(gold));
            assert_eq!(ctx.card(gold).unwrap().kind.as_deref(), Some(KIND_GEAR));
            assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
            assert!(ctx.effects.contains(&Effect::exhaust(gold)));
            assert_eq!(ctx.location(gold), Some(Location::Base(0)));
            assert!(ctx.events.contains(&Event::Played {
                card: gold,
                controller: 0,
                kind: KIND_GEAR.into(),
                origin: Origin::Board,
                paid_additional: false,
            }));
        }
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{seat 0} gains a Gold")
                .count(),
            GOLDS
        );
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_golem_killed_in_response_still_pays_out_and_the_other_seats_golem_pays_them() {
        let mut fixture = vault();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GOLEM).unwrap();
        ctx.kill(GOLEM, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(GOLEM));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert_eq!(
            golds_of(&ctx, 0).len(),
            GOLDS,
            "the Golds go to the base, not here"
        );
        drop(ctx);
        let mut fixture = vault();
        fixture.table.card_mut(GOLEM).unwrap().seat = 1;
        fixture.table.card_mut(GOLEM).unwrap().owner = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 1, GOLEM, Origin::Hand, Some(Location::Base(1))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 1).len(), GOLDS);
        assert!(golds_of(&ctx, 0).is_empty());
        for gold in golds_of(&ctx, 1) {
            assert_eq!(ctx.location(gold), Some(Location::Base(1)));
        }
    }

    #[test]
    fn a_golem_marching_or_holding_plays_nothing() {
        let mut fixture = vault();
        fixture.table.card_mut(GOLEM).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            GOLEM,
            Location::Battlefield(fixtures::BF1),
            crate::engine::ctx::MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(golds_of(&ctx, 0).is_empty(), "a move is not a play");
    }
}

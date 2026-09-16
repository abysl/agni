use super::prelude::{done, on_move, spawn_gold, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

const GOLD_ARRIVES_READY: bool = false;

fn run(ctx: &mut Ctx, item: &Item, _stage: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = unit("Treasure Hunter", &[], &[on_move(&[], run)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Where, Who, KIND_GEAR, TOKEN_GOLD};
    use crate::engine::cost::{Cost, Need};
    use crate::engine::ctx::{EntryMove, Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, legal, pay, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const HUNTER: u32 = 90;
    const HAND_HUNTER: u32 = 91;

    fn hunter(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Treasure Hunter", 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn hired() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hunter(HUNTER, fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(hunter(HAND_HUNTER, fixtures::HAND, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, unit: u32, to: u16, to_seat: u8) -> EntryMove {
        let before = ctx.card(unit);
        EntryMove {
            card: unit,
            from: before.and_then(|held| held.zone),
            from_seat: before.map(|held| held.seat).unwrap_or(0),
            to: Some(to),
            to_seat,
            index: TOP,
            hidden: false,
        }
    }

    fn golds(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn moves(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Moved { card, .. } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_hunter_is_a_plain_unit_whose_only_ability_watches_its_own_moves() {
        let fixture = hired();
        let script = fixture.scripts.of_card(HUNTER).unwrap();
        assert!(std::ptr::eq(script, &CARD));
        assert_eq!(CARD.name, "Treasure Hunter");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert!(ability.cost.is_none());
    }

    #[test]
    fn a_march_plays_an_exhausted_gold_when_the_trigger_resolves() {
        let mut fixture = hired();
        let action = fixtures::move_action(HUNTER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(moves(&ctx), [HUNTER]);
        assert!(golds(&ctx, 0).is_empty(), "the Gold waits for the chain");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HUNTER
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(ctx.blob.prompt.is_none(), "the hunter asks nothing");
        let next = ctx.table.next_id;
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let gold = *golds(&ctx, 0).first().expect("one Gold in seat 0's base");
        assert_eq!(gold, next);
        assert_eq!(golds(&ctx, 1), Vec::<u32>::new());
        assert!(ctx.is_gear(gold));
        assert!(ctx.is_token(gold));
        assert_eq!(ctx.card(gold).unwrap().kind.as_deref(), Some(KIND_GEAR));
        assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert_eq!(ctx.card(gold).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(gold).unwrap().seat, 0);
        assert!(ctx.effects.contains(&Effect::exhaust(gold)));
        assert!(ctx.events.contains(&Event::Played {
            card: gold,
            controller: 0,
            kind: KIND_GEAR.into(),
            origin: crate::state::Origin::Board,
            paid_additional: false,
        }));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} ability resolves".to_string()));
    }

    #[test]
    fn an_effect_move_pays_the_hunter_too_and_a_recall_never_does() {
        let mut fixture = hired();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                HUNTER,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(golds(&ctx, 0).len(), 1, "Charm's move is still a move");
        ctx.recall(HUNTER, true);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(golds(&ctx, 0).len(), 1, "a recall is not a move");
        assert_eq!(moves(&ctx), [HUNTER], "the hunter in hand never moved");
        assert_eq!(ctx.card(HAND_HUNTER).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn the_minted_gold_arrives_exhausted_and_an_exhausted_gold_pays_for_nothing() {
        let mut fixture = hired();
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        let steep = Cost {
            energy: 0,
            power: vec![Need::Rainbow; runes + 1],
            ..Cost::default()
        };
        let before = pay::plan(&ctx, 0, &steep);
        ctx.move_unit(
            HUNTER,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let gold = *golds(&ctx, 0).first().unwrap();
        assert!(!ctx.is_rune(gold));
        assert_eq!(ctx.runes_of(0).len(), runes, "no rune was minted");
        assert_eq!(
            pay::plan(&ctx, 0, &steep),
            before,
            "the Gold arrives exhausted: it cannot exhaust itself to pay"
        );
        assert!(matches!(before, Err(Refusal::NotEnoughRunes { .. })));
    }

    #[test]
    fn a_ready_gold_pays_the_rainbow_power_a_rune_pool_is_short_of() {
        let mut fixture = hired();
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        let steep = Cost {
            energy: 0,
            power: vec![Need::Rainbow; runes + 1],
            ..Cost::default()
        };
        assert!(matches!(
            pay::plan(&ctx, 0, &steep),
            Err(Refusal::NotEnoughRunes { .. })
        ));
        ctx.move_unit(
            HUNTER,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let gold = *golds(&ctx, 0).first().unwrap();
        ctx.ready(gold);
        assert!(
            pay::plan(&ctx, 0, &steep).is_ok(),
            "the ready Gold covers the rainbow the runes cannot"
        );
    }

    #[test]
    fn a_hunter_that_may_not_march_mints_nothing() {
        let mut fixture = hired();
        let action = fixtures::move_action(HUNTER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(1, &action);
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, HUNTER, fixtures::BF1, 0)),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "the other seat cannot walk my hunter"
        );
        settle(&mut ctx).unwrap();
        assert!(golds(&ctx, 0).is_empty());
        assert!(golds(&ctx, 1).is_empty());
        assert!(ctx.effects.is_empty());
        let mut fixture = hired();
        fixture.table.card_mut(HUNTER).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, HUNTER, fixtures::BF1, 0)),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, HAND_HUNTER, fixtures::BF1, 0))
                .err()
                .unwrap(),
            Refusal::Illegal(Reason::NotHeld),
            "a hunter in hand is played, not marched"
        );
        settle(&mut ctx).unwrap();
        assert!(golds(&ctx, 0).is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }
}

use super::prelude::{done, spawn_gold, triggered, unit};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

const GOLD_ARRIVES_READY: bool = false;

fn plunder(ctx: &mut Ctx, item: &Item, _stage: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = unit(
    "Plundering Poro",
    &[],
    &[triggered(Trigger::Conquer(Who::Me), &[], plunder)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{KIND_GEAR, TOKEN_GOLD};
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, legal, priority, settle};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;

    fn poro(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Plundering Poro", 2);
        card.domain = vec!["Mind".into()];
        card
    }

    fn raiding() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(PORO, fixtures::BF1, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn golds(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn entry(ctx: &Ctx, unit: u32, to: u16) -> EntryMove {
        let before = ctx.card(unit);
        EntryMove {
            card: unit,
            from: before.and_then(|held| held.zone),
            from_seat: before.map(|held| held.seat).unwrap_or(0),
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_poro_is_a_plain_unit_whose_only_ability_watches_its_own_conquests() {
        let fixture = raiding();
        let script = fixture.scripts.of_card(PORO).unwrap();
        assert!(std::ptr::eq(script, &CARD));
        assert_eq!(CARD.name, "Plundering Poro");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::Me));
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert!(ability.cost.is_none());
        assert!(ability.timing().is_none());
    }

    #[test]
    fn conquering_with_the_poro_plays_an_exhausted_gold_when_the_trigger_resolves() {
        let mut fixture = raiding();
        let mut ctx = fixture.ctx();
        let next = ctx.table.next_id;
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![PORO]
        )));
        assert!(golds(&ctx, 0).is_empty(), "the Gold waits for the chain");
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PORO
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(ctx.blob.prompt.is_none(), "the poro asks nothing");
        resolve_chain(&mut ctx);
        let gold = *golds(&ctx, 0).first().expect("one Gold in seat 0's base");
        assert_eq!(gold, next);
        assert_eq!(golds(&ctx, 1), Vec::<u32>::new());
        assert!(ctx.is_gear(gold));
        assert!(ctx.is_token(gold));
        assert_eq!(ctx.card(gold).unwrap().kind.as_deref(), Some(KIND_GEAR));
        assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert_eq!(ctx.card(gold).unwrap().seat, 0);
        assert!(ctx.effects.contains(&Effect::exhaust(gold)));
        assert!(ctx.events.contains(&Event::Played {
            card: gold,
            controller: 0,
            kind: KIND_GEAR.into(),
            origin: Origin::Board,
            paid_additional: false,
        }));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} ability resolves".to_string()));
    }

    #[test]
    fn a_hold_is_not_a_conquest_and_a_second_conquest_the_same_turn_is_only_kept() {
        let mut fixture = raiding();
        let mut ctx = fixture.ctx();
        cleanup::hold(&mut ctx, fixtures::BF1, 0);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "holding is not conquering");
        assert!(golds(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = raiding();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(golds(&ctx, 0).len(), 1);
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Kept(0),
            "once per battlefield per turn"
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds(&ctx, 0).len(), 1, "no second Gold");
    }

    #[test]
    fn a_conquest_the_poro_is_not_standing_on_mints_nothing_for_either_seat() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(PORO, fixtures::BASE, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi conquered, not the poro");
        assert!(golds(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = raiding();
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_contested(fixtures::BF2, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF2),
            cleanup::Established::Conquered(1),
            "seat 1's Sprite conquers the battlefield it stands on"
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "an enemy conquest is not mine");
        assert!(golds(&ctx, 0).is_empty());
        assert!(golds(&ctx, 1).is_empty());
    }

    #[test]
    fn a_poro_the_other_seat_cannot_march_never_reaches_a_battlefield_to_plunder() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(PORO, fixtures::BASE, 0));
        fixture.resolve();
        let action = fixtures::move_action(PORO, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(1, &action);
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, PORO, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "the other seat cannot walk my poro"
        );
        settle(&mut ctx).unwrap();
        assert!(golds(&ctx, 0).is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.is_empty());
    }
}

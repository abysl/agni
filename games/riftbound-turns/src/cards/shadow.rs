use super::prelude::{
    a_card, activated, card_target, done, exhausting_self, named, stun, unit, with_statics,
};
use super::{Card, Cost, Filter, Flow, Item, Power, Stage, Static, TargetSpec, Timing};
use crate::engine::ctx::{Ctx, Location};

pub const CLOAK: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};

pub const ENEMY_ATTACKER_HERE: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Attacker, Filter::Here]);
pub const TARGET: TargetSpec = a_card(ENEMY_ATTACKER_HERE, "an enemy unit attacking here to stun");

pub fn played_to_a_battlefield(ctx: &Ctx, me: u32) -> bool {
    let pending = ctx
        .blob
        .queue
        .iter()
        .find(|pending| pending.item.kind.card() == Some(me))
        .map(|pending| {
            pending
                .item
                .zone_target()
                .is_some_and(|zone| ctx.zones.is_battlefield(zone))
        });
    match pending {
        Some(to_battlefield) => to_battlefield,
        None => matches!(ctx.location(me), Some(Location::Battlefield(_))),
    }
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    played_to_a_battlefield(ctx, me)
}

fn smother(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Shadow",
        &[],
        &[named(
            exhausting_self(activated(Timing::Action, CLOAK, &[TARGET], smother)),
            "stun an enemy unit attacking here",
        )],
    ),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SHADOW: u32 = 90;
    const RAIDER: u32 = 91;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn shadow(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Calm".into(), "Chaos".into()],
            ..fixtures::unit(SHADOW, zone, seat, "Shadow", MIGHT)
        }
    }

    fn gloom(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shadow(zone, 0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SHADOW).unwrap(),
            &CARD
        ));
        fixture
    }

    fn raided() -> Fixture {
        let mut fixture = gloom(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_one_exhausting_action_over_an_enemy_attacker_here_and_the_seam() {
        assert!(std::ptr::eq(script_of("Shadow").unwrap(), &CARD));
        assert_eq!(CARD.name, "Shadow");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let cloak = &CARD.abilities[0];
        assert_eq!(cloak.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(cloak.cost, Some(CLOAK));
        assert_eq!(cloak.self_cost, SelfCost::Exhaust);
        assert_eq!(cloak.targets, &[TARGET]);
        assert_eq!(cloak.targets[0].filter, ENEMY_ATTACKER_HERE);
    }

    #[test]
    fn the_seam_reads_the_pending_plays_location_and_then_where_it_stands() {
        let mut fixture = gloom(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, SHADOW), "in hand, nowhere yet");
        fixtures::play_from_hand(&mut ctx, 0, SHADOW).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "your base".to_string(),
                format!("{{zone {}}}", fixtures::BF1),
                "cancel".to_string()
            ]
        );
        assert!(!enters_ready(&ctx, SHADOW), "no location chosen yet");
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(SHADOW),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(enters_ready(&ctx, SHADOW), "played to a battlefield");
        assert!(
            !ctx.card(SHADOW).unwrap().exhausted,
            "369.3 · played to a battlefield, it enters ready"
        );
        drop(ctx);
        let mut fixture = gloom(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHADOW).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(SHADOW), Some(Location::Base(0)));
        assert!(!enters_ready(&ctx, SHADOW), "played to the base");
    }

    #[test]
    fn the_action_exhausts_it_pays_one_and_a_rainbow_and_stuns_an_enemy_attacking_here() {
        let mut fixture = raided();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::THEIR_UNIT));
        assert!(ctx.mark_attacker(RAIDER));
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == SHADOW && offer.index == 0)
            .expect("the cloak is offered");
        assert!(offer.enabled);
        assert!(offer.label.contains("stun an enemy unit attacking here"));
        activate::activate(&mut ctx, 0, SHADOW, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {RAIDER}}}"),
                "cancel".to_string()
            ],
            "the attackers here and nobody else"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.card(SHADOW).unwrap().exhausted, "exhausted as the cost");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "one rune exhausts for the energy and recycles for the rainbow"
        );
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(RAIDER)]);
        assert!(!ctx.is_stunned(RAIDER), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(RAIDER));
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RAIDER}}} is stunned")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_attacking_here_the_action_is_refused_and_an_exhausted_shadow_cannot_pay() {
        let mut fixture = raided();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SHADOW, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "enemies at rest here are not attacking"
        );
        assert!(ctx.mark_attacker(fixtures::VI));
        assert_eq!(
            activate::activate(&mut ctx, 0, SHADOW, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "a friendly attacker is no target"
        );
        drop(ctx);
        let mut away = gloom(fixtures::BASE);
        away.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        away.resolve();
        let mut ctx = away.ctx();
        assert!(ctx.mark_attacker(fixtures::THEIR_UNIT));
        assert_eq!(
            activate::activate(&mut ctx, 0, SHADOW, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "an attacker elsewhere is not here"
        );
        drop(ctx);
        let mut spent = raided();
        spent.table.card_mut(SHADOW).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert!(ctx.mark_attacker(RAIDER));
        assert_eq!(
            activate::activate(&mut ctx, 0, SHADOW, 0),
            Err(Refusal::Exhausted)
        );
        assert!(!ctx.is_stunned(RAIDER));
    }

    #[test]
    fn played_to_a_battlefield_it_enters_ready() {
        let mut fixture = gloom(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHADOW).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SHADOW));
        assert!(
            !ctx.card(SHADOW).unwrap().exhausted,
            "369.3 · played to a battlefield, I enter ready"
        );
    }
}

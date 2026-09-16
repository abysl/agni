use super::prelude::{
    a_unit_at_a_battlefield, activated, card_target, deal, done, exhausting_self, gear, named,
    with_statics,
};
use super::{Card, Cost, Flow, Item, Stage, Static, Timing};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const FIRE: u8 = 0;

fn fire(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!(
                "{{card {}}} deals {DAMAGE} to {{card {unit}}}",
                item.kind.source()
            ));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Iron Ballista",
        &[],
        &[named(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit_at_a_battlefield("a unit at a battlefield to deal 2")],
                fire,
            )),
            "deal 2 to a unit at a battlefield",
        )],
    ),
    &[Static::EntersExhausted],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::Target;

    const BALLISTA: u32 = 90;
    const SCOUT: u32 = 91;

    fn ballista(zone: u16) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::gear(BALLISTA, zone, 0, "Iron Ballista", 3);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ballista(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_enters_exhausted_as_a_static_and_fires_by_exhausting() {
        let fixture = armed(fixtures::BASE);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BALLISTA).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Iron Ballista");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert!(CARD.has_static(Static::EntersExhausted));
        let fire = &CARD.abilities[usize::from(FIRE)];
        assert_eq!(fire.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(fire.self_cost, SelfCost::Exhaust);
        assert_eq!(fire.cost, Some(Cost::FREE));
        assert_eq!(fire.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(fire.label, Some("deal 2 to a unit at a battlefield"));
    }

    #[test]
    fn playing_it_from_hand_lands_it_exhausted_before_anyone_can_use_it() {
        let mut fixture = armed(fixtures::HAND);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BALLISTA).unwrap();
        assert_eq!(ctx.card(BALLISTA).unwrap().zone, Some(fixtures::BASE));
        assert!(
            ctx.card(BALLISTA).unwrap().exhausted,
            "369.3 · the entry itself is replaced, nothing goes on the chain"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "no trigger to respond to or counter"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, .. } if *card == BALLISTA
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BALLISTA}}} enters exhausted")));
        assert_eq!(
            activate::activate(&mut ctx, 0, BALLISTA, FIRE),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_ballista_deals_two_to_a_unit_at_a_battlefield_and_the_lethal_kill_lands() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == BALLISTA)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!("{{card {BALLISTA}}}: deal 2 to a unit at a battlefield (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, BALLISTA, FIRE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let offered: Vec<Option<u32>> = prompts::offered(&ctx).iter().map(|opt| opt.card).collect();
        assert_eq!(
            offered,
            [Some(fixtures::SPRITE), Some(SCOUT), None],
            "units at battlefields of either side, then cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert!(ctx.card(BALLISTA).unwrap().exhausted);
        assert_eq!(
            damage(&ctx, fixtures::SPRITE),
            0,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert_eq!(damage(&ctx, fixtures::SPRITE), 2);
        assert!(ctx.on_board(fixtures::SPRITE), "two is not lethal for a 3");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 2, .. } if *card == fixtures::SPRITE
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BALLISTA}}} deals 2 to {{card {}}}",
            fixtures::SPRITE
        )));
        drop(ctx);
        fixture.table.card_mut(BALLISTA).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BALLISTA, FIRE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(!ctx.on_board(SCOUT), "two is lethal for the friendly 2");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == SCOUT
        )));
    }

    #[test]
    fn a_unit_in_a_base_is_no_target_and_without_any_the_activation_is_refused() {
        let mut fixture = armed(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != SCOUT);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().seat = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, BALLISTA, FIRE),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, BALLISTA, FIRE),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.card(BALLISTA).unwrap().exhausted);
    }
}

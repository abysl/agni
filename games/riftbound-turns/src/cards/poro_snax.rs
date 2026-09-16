use super::prelude::{activated, done, draw, gear, named, paying_with, play, usable_if};
use super::{Card, Cost, Domain, Flow, Item, Power, SelfCost, Source, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const FEED: u8 = 0;
pub const EAT: u8 = 1;
pub const SNACK: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Calm)],
};

pub fn ready_to_exhaust(ctx: &Ctx, source: Source) -> bool {
    ctx.card(source.card).is_some_and(|held| !held.exhausted)
}

fn feed(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = gear(
    "Poro Snax",
    &[],
    &[
        play(&[], feed),
        usable_if(
            named(
                paying_with(
                    activated(Timing::Sorcery, SNACK, &[], feed),
                    SelfCost::KillSelf,
                ),
                "exhaust and kill this: draw 1",
            ),
            ready_to_exhaust,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const SNAX: u32 = 90;
    const CALM_RUNE: u32 = 42;

    fn snax(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Calm".into()],
            exhausted,
            ..fixtures::gear(SNAX, zone, seat, "Poro Snax", 1)
        }
    }

    fn pantry(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(snax(zone, seat, exhausted));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SNAX).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn snax_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == SNAX)
            .map(|offer| (offer.label, offer.enabled))
            .collect()
    }

    #[test]
    fn the_script_draws_on_play_and_has_one_paid_kill_this_activation_gated_on_being_ready() {
        assert!(std::ptr::eq(script_of("Poro Snax").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let feed = &CARD.abilities[usize::from(FEED)];
        assert_eq!(feed.trigger, Trigger::Play);
        assert!(feed.targets.is_empty() && !feed.optional);
        let eat = &CARD.abilities[usize::from(EAT)];
        assert_eq!(eat.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(eat.cost, Some(SNACK));
        assert_eq!(eat.self_cost, SelfCost::KillSelf);
        assert!(
            eat.usable.is_some(),
            "the exhaust is part of the cost: a spent Snax can't be eaten"
        );
        assert!(eat.targets.is_empty());
        assert_eq!(eat.label, Some("exhaust and kill this: draw 1"));
        assert_eq!(SNACK.energy, 1);
        assert_eq!(SNACK.power, [Power::Domain(Domain::Calm)]);
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn playing_the_snax_lands_it_ready_and_its_trigger_draws_one_when_it_resolves() {
        let mut fixture = pantry(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SNAX).unwrap();
        assert_eq!(ctx.card(SNAX).unwrap().zone, Some(fixtures::BASE));
        assert!(
            !ctx.card(SNAX).unwrap().exhausted,
            "149.1 · gear enters ready"
        );
        assert_eq!(
            ctx.card(CALM_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Calm rune is recycled for the power"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == SNAX && index == FEED
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the Snax left the hand, the draw waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn one_energy_a_calm_rune_the_exhaust_and_the_kill_buy_one_more_card() {
        let mut fixture = pantry(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            snax_offers(&ctx),
            [(
                format!(
                    "{{card {SNAX}}}: exhaust and kill this: draw 1 (1 energy and 1 Calm power)"
                ),
                true
            )]
        );
        activate::activate(&mut ctx, 0, SNAX, EAT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert!(!ctx.on_board(SNAX), "the kill is paid as the cost");
        assert_eq!(ctx.card(SNAX).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            ctx.effects.contains(&Effect::exhaust(SNAX)),
            "exhausted on the way out"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: false, .. } if *card == SNAX
        )));
        assert_eq!(
            ctx.card(CALM_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Calm rune is exhausted for the energy, then recycled for the power"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index } if source == SNAX && index == EAT
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for resolution");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spent_snax_the_other_seat_and_a_pool_without_calm_are_refused() {
        let mut fixture = pantry(fixtures::BASE, 0, true);
        let mut ctx = fixture.ctx();
        assert!(
            snax_offers(&ctx).is_empty(),
            "an exhausted Snax offers nothing"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, SNAX, EAT),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses a Snax that can't be exhausted"
        );
        assert!(ctx.on_board(SNAX));
        drop(ctx);
        let mut fixture = pantry(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SNAX, EAT),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.on_board(SNAX));
        drop(ctx);
        let mut fixture = pantry(fixtures::BASE, 0, false);
        fixture.table.cards.retain(|card| card.id != CALM_RUNE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            snax_offers(&ctx),
            [(
                format!(
                    "{{card {SNAX}}}: exhaust and kill this: draw 1 (1 energy and 1 Calm power)"
                ),
                false
            )],
            "offered but greyed: no Calm to pay with"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, SNAX, EAT),
            Err(Refusal::NoPowerOf)
        );
        assert!(ctx.on_board(SNAX));
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
    }
}

use super::prelude::{
    activated, done, exhausting_self, gear, named, promise_this_turn, PromiseEffect, PromiseKind,
    RAINBOW,
};
use super::{Card, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const OPEN: u8 = 0;

pub fn next_spell_repeats_for_its_cost(ctx: &mut Ctx, seat: u8) {
    promise_this_turn(ctx, seat, PromiseKind::Spell, PromiseEffect::RepeatForCost);
    ctx.narrate(format!(
        "the next spell {{seat {seat}}} plays this turn has Repeat equal to its cost"
    ));
}

fn open(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    next_spell_repeats_for_its_cost(ctx, item.controller);
    done()
}

pub static CARD: Card = gear(
    "Temporal Portal",
    &[],
    &[named(
        exhausting_self(activated(Timing::Sorcery, RAINBOW, &[], open)),
        "the next spell you play this turn has Repeat equal to its cost",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{play, spell, Promise};
    use crate::cards::{script_of, Cost, Keyword, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{Expiry, ItemKind, PromptWhy, SLOT_PROMISED_REPEAT, SLOT_REPEAT};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const PORTAL: u32 = 90;
    const ECHO: u32 = 91;
    const BELLOWS: u32 = 92;

    static ECHO_CARD: Card = spell("Echo", &[], &[play(&[], |_, _, _| Flow::Done)]);

    static BELLOWS_CARD: Card = spell(
        "Bellows",
        &[Keyword::Repeat(Cost {
            energy: 1,
            power: &[],
        })],
        &[play(&[], |_, _, _| Flow::Done)],
    );

    fn portal(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(PORTAL, zone, seat, "Temporal Portal", 3)
        }
    }

    fn rift(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(portal(zone, seat, exhausted));
        fixture
            .table
            .cards
            .push(fixtures::spell(ECHO, fixtures::HAND, 0, "Echo", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(BELLOWS, fixtures::HAND, 0, "Bellows", 1, 0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(ECHO, &ECHO_CARD)
            .with_script(BELLOWS, &BELLOWS_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PORTAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn recycled_runes(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if *zone == fixtures::RUNE_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_one_untargeted_rainbow_and_exhaust_activation() {
        assert!(std::ptr::eq(script_of("Temporal Portal").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[usize::from(OPEN)];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(RAINBOW));
        assert!(ability.targets.is_empty());
        assert_eq!(
            ability.label,
            Some("the next spell you play this turn has Repeat equal to its cost")
        );
    }

    #[test]
    fn the_portal_pays_a_rune_of_any_domain_exhausts_onto_the_chain_and_records_the_promise() {
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == PORTAL)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!(
                    "{{card {PORTAL}}}: the next spell you play this turn has Repeat equal to its cost (1 any power, exhaust)"
                ),
                true
            )]
        );
        activate::activate(&mut ctx, 0, PORTAL, OPEN).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert!(
            ctx.card(PORTAL).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(
            recycled_runes(&ctx).len(),
            1,
            "one rune of any domain pays the rainbow"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index } if source == PORTAL && index == OPEN
        ));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(
            &"the next spell {seat 0} plays this turn has Repeat equal to its cost".to_string()
        ));
        assert_eq!(
            ctx.blob.seat(0).promises,
            [Promise {
                kind: PromiseKind::Spell,
                effect: PromiseEffect::RepeatForCost,
                until: Expiry::EndOfTurn(1),
            }]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, PORTAL, OPEN),
            Err(Refusal::Exhausted),
            "one opening per readying"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spent_portal_the_other_seat_and_an_empty_rune_pool_are_refused() {
        let mut fixture = rift(fixtures::BASE, 0, true);
        let mut ctx = fixture.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == PORTAL));
        assert_eq!(
            activate::activate(&mut ctx, 0, PORTAL, OPEN),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, PORTAL, OPEN),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(PORTAL).unwrap().exhausted);
        drop(ctx);
        let mut fixture = rift(fixtures::BASE, 0, false);
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == PORTAL && !offer.enabled));
        assert!(matches!(
            activate::activate(&mut ctx, 0, PORTAL, OPEN),
            Err(Refusal::NotEnoughRunes { .. })
        ));
        assert!(!ctx.card(PORTAL).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_next_spell_this_turn_offers_a_repeat_equal_to_its_cost() {
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PORTAL, OPEN).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, ECHO).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_PROMISED_REPEAT as u8
            }),
            "Echo costs one energy, so its Repeat costs one energy"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "the rainbow recycled the spent rune; the spell and its repeat exhaust one each"
        );
    }

    #[test]
    fn a_spell_with_printed_repeat_is_offered_both_instances_and_resolves_three_times() {
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PORTAL, OPEN).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_REPEAT as u8
            }),
            "820.1.c.2 · the printed Repeat is its own instance"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_PROMISED_REPEAT as u8
            }),
            "820.3 · and the promised one is offered beside it"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "the spell, its printed Repeat and its promised Repeat exhaust one rune each"
        );
        assert!(
            ctx.blob.seat(0).promises.is_empty(),
            "Bellows was the next spell"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].repeats(), 2);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == &format!("{{card {BELLOWS}}} repeats"))
                .count(),
            2,
            "three executions"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_printed_repeat_still_offers_the_promised_one() {
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PORTAL, OPEN).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_PROMISED_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(ctx.blob.chain[0].repeats(), 1);
        assert!(ctx.blob.seat(0).promises.is_empty());
        drop(ctx);
        let mut fixture = rift(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "without the Portal only the printed Repeat is asked: {:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(ctx.blob.chain[0].repeats(), 1);
    }
}

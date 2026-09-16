use super::prelude::{
    asking, done, draw, exhausting_self, gear, on_friendly_unit_dies, optional, remember_card,
    remembered_cards, with_candidates,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::hide;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const DRAWS: usize = 1;
pub const PICKED: u8 = 1;
pub const PLACED: u8 = 2;
pub const QUESTION: &str =
    "a card from your hand, then pick it again for the top of your Main Deck or skip for the bottom";

pub fn hand_cards(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICKED => ctx
            .hand_of(item.controller)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        PLACED => remembered_cards(item)
            .into_iter()
            .filter(|card| ctx.in_hand(*card))
            .map(TargetRef::Card)
            .collect(),
        _ => Vec::new(),
    }
}

fn put_on_top(ctx: &mut Ctx, card: u32) -> bool {
    let Some(deck) = ctx.zones.main_deck else {
        return false;
    };
    let owner = ctx.owner(card);
    hide::reveal_before(ctx, card, deck);
    ctx.emit(Effect::Move {
        card,
        zone: deck,
        seat: owner,
        index: TOP,
    });
    true
}

pub fn remember(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICKED => {
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| ctx.hand_of(seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} keeps their hand"));
                return done();
            };
            remember_card(ctx, card);
            Flow::Ask(ctx.ask_resume(item, PLACED, 0, 1))
        }
        PLACED => {
            let Some(card) = remembered_cards(item)
                .into_iter()
                .find(|card| ctx.in_hand(*card))
            else {
                ctx.narrate(format!("{{seat {seat}}} keeps their hand"));
                return done();
            };
            if ctx.picks().contains(&card) {
                put_on_top(ctx, card);
                ctx.narrate(format!(
                    "{{seat {seat}}} puts {{card {card}}} on top of their Main Deck"
                ));
            } else {
                ctx.recycle_to_bottom(card);
                ctx.narrate(format!(
                    "{{seat {seat}}} puts {{card {card}}} on the bottom of their Main Deck"
                ));
            }
            done()
        }
        _ => {
            let drawn = draw(ctx, seat, DRAWS);
            ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
            if ctx.hand_of(seat).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card in hand"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICKED, 1, 1))
        }
    }
}

pub static CARD: Card = gear(
    "Altar of Memories",
    &[],
    &[asking(
        with_candidates(
            optional(exhausting_self(on_friendly_unit_dies(&[], remember))),
            hand_cards,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::table::CardInfo;

    const ALTAR: u32 = 90;
    const ALLY: u32 = 91;
    const TRINKET: u32 = 92;

    fn altar(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            exhausted,
            ..fixtures::gear(ALTAR, fixtures::BASE, 0, "Altar of Memories", 2)
        }
    }

    fn chapel(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(altar(exhausted));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ALTAR).unwrap(), &CARD));
        fixture
    }

    fn deck_moves(ctx: &Ctx) -> Vec<(u32, u32)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card, zone, index, ..
                } if *zone == fixtures::MAIN_DECK => Some((*card, *index)),
                _ => None,
            })
            .collect()
    }

    fn altar_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == ALTAR))
            .map(|item| item.id)
            .collect()
    }

    fn wake_by_a_friendly_death(ctx: &mut Ctx) {
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(ctx).unwrap();
        assert_eq!(altar_items(ctx).len(), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: altar_items(ctx)[0],
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(fixtures::labels(ctx), ["yes", "no"]);
    }

    #[test]
    fn the_script_is_the_pool_name_with_an_optional_exhaust_on_a_friendly_death() {
        assert!(std::ptr::eq(script_of("Altar of Memories").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(DRAWS, 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert!(ability.optional, "you may exhaust me");
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
    }

    #[test]
    fn yes_exhausts_the_altar_draws_one_and_puts_the_picked_card_on_top_of_the_deck() {
        let mut fixture = chapel(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        wake_by_a_friendly_death(&mut ctx);
        assert!(!ctx.card(ALTAR).unwrap().exhausted, "nothing until yes");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(ALTAR).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for resolution");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: altar_items(&ctx)[0],
                stage: PICKED
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {ALTAR}}}: choose {QUESTION} (0 of 1)")
        );
        let offered: Vec<String> = ctx
            .hand_of(0)
            .iter()
            .map(|card| format!("{{card {card}}}"))
            .collect();
        assert_eq!(fixtures::labels(&ctx), offered, "every card in hand");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: altar_items(&ctx)[0],
                stage: PLACED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::HAND_SPELL),
                "skip".to_string()
            ],
            "the picked card again for the top, skip for the bottom"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_moves(&ctx), [(fixtures::HAND_SPELL, TOP)]);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert_eq!(
            ctx.top_of(fixtures::MAIN_DECK, 0, 1),
            [fixtures::HAND_SPELL],
            "on top"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), deck);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} puts {{card {}}} on top of their Main Deck",
            fixtures::HAND_SPELL
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_second_pick_sends_the_card_to_the_bottom_and_declining_keeps_the_altar_ready() {
        let mut fixture = chapel(false);
        let mut ctx = fixture.ctx();
        wake_by_a_friendly_death(&mut ctx);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck_moves(&ctx), [(fixtures::HAND_UNIT, BOTTOM)]);
        assert_ne!(ctx.top_of(fixtures::MAIN_DECK, 0, 1), [fixtures::HAND_UNIT]);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} puts {{card {}}} on the bottom of their Main Deck",
            fixtures::HAND_UNIT
        )));
        drop(ctx);
        let mut fixture = chapel(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        wake_by_a_friendly_death(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.card(ALTAR).unwrap().exhausted);
        assert!(altar_items(&ctx).is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(deck_moves(&ctx).is_empty());
        drop(ctx);
        let mut fixture = chapel(true);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "a spent Altar has nothing to offer"
        );
        assert!(altar_items(&ctx).is_empty());
    }

    #[test]
    fn an_enemy_death_and_a_friendly_gear_death_queue_nothing_for_the_altar() {
        let mut fixture = chapel(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none(),
            "an enemy unit is not friendly"
        );
        assert_eq!(ctx.kill(TRINKET, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Died { card, unit: false, .. } if *card == TRINKET
        )));
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none(),
            "gear is not a unit"
        );
        assert!(!ctx.card(ALTAR).unwrap().exhausted);
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn a_friendly_unit_dying_asks_to_exhaust_the_altar_draws_one_and_places_a_card() {
        let mut fixture = chapel(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(ALTAR).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: PICKED, .. })
        ));
    }
}

use super::prelude::{asking, channel_exhausted, done, draw, on_conquer_me, unit, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DEFLECT: u8 = 1;
pub const DRAWS: usize = 1;
pub const RUNES: usize = 1;
const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "a deck · draw 1 from the main deck or channel 1 rune exhausted";

pub fn decks(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    let mut offered = Vec::new();
    if let Some(main_deck) = ctx.zones.main_deck {
        offered.push(TargetRef::Zone(main_deck));
    }
    if let Some(rune_deck) = ctx.zones.rune_deck {
        if ctx.table.held(rune_deck, seat).next().is_some() {
            offered.push(TargetRef::Zone(rune_deck));
        }
    }
    offered
}

fn take_from(ctx: &mut Ctx, seat: u8, deck: u32) {
    let rune_deck = ctx.zones.rune_deck.map(u32::from);
    let main_deck = ctx.zones.main_deck.map(u32::from);
    if Some(deck) == rune_deck {
        channel_exhausted(ctx, seat, RUNES);
    } else if Some(deck) == main_deck {
        draw(ctx, seat, DRAWS);
    }
}

fn spoils(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        STAGE_PICKED => {
            if let Some(deck) = ctx.picks().first().copied() {
                take_from(ctx, seat, deck);
            }
            done()
        }
        _ => match decks(ctx, item, stage).as_slice() {
            [] => done(),
            [TargetRef::Zone(only)] => {
                take_from(ctx, seat, u32::from(*only));
                done()
            }
            _ => Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 1, 1)),
        },
    }
}

pub static CARD: Card = unit(
    "Qiyana - Victorious",
    &[Keyword::Deflect(DEFLECT)],
    &[asking(
        with_candidates(on_conquer_me(&[], spoils), decks),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const QIYANA: u32 = 90;
    const TOP_OF_MAIN_DECK: u32 = 23;
    const TOP_OF_RUNE_DECK: u32 = 32;

    fn qiyana(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(QIYANA, zone, seat, "Qiyana - Victorious", 4)
        }
    }

    fn contested(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(qiyana(zone, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_until_asked(ctx: &mut Ctx) {
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == QIYANA
        ));
        assert!(ctx.blob.prompt.is_none(), "the choice comes at resolution");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_deflect_one_and_a_conquer_trigger_that_asks_which_deck() {
        assert!(std::ptr::eq(
            script_of("Qiyana - Victorious").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Qiyana - Victorious");
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(!conquer.optional, "draw or channel: one of them happens");
        assert!(conquer.targets.is_empty());
        assert!(conquer.candidates.is_some());
        assert_eq!(conquer.question, Some(QUESTION));
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
    }

    #[test]
    fn conquering_asks_main_deck_or_rune_deck_and_the_main_deck_draws_one() {
        let mut fixture = contested(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let runes = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        resolve_until_asked(&mut ctx);
        let item = match ctx.blob.why {
            Some(PromptWhy::Resume { item, stage }) => {
                assert_eq!(stage, STAGE_PICKED);
                item
            }
            other => panic!("the trigger asks which deck, not {other:?}"),
        };
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::MAIN_DECK),
                format!("{{zone {}}}", fixtures::RUNE_DECK)
            ]
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item,
                    stage: STAGE_PICKED
                }
            ),
            format!("{{card {QIYANA}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::MAIN_DECK)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.effects.contains(&Effect::Move {
            card: TOP_OF_MAIN_DECK,
            zone: fixtures::HAND,
            seat: 0,
            index: agni_plugin_sdk::decide::TOP,
        }));
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(
            ctx.table.held(fixtures::RUNE_POOL, 0).count(),
            runes,
            "no rune was channelled"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_rune_deck_channels_one_rune_exhausted_and_draws_nothing() {
        let mut fixture = contested(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let runes = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        conquer(&mut ctx);
        resolve_until_asked(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::RUNE_DECK)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.table.held(fixtures::RUNE_POOL, 0).count(),
            runes + RUNES
        );
        assert_eq!(
            ctx.card(TOP_OF_RUNE_DECK).unwrap().zone,
            Some(fixtures::RUNE_POOL)
        );
        assert!(
            ctx.card(TOP_OF_RUNE_DECK).unwrap().exhausted,
            "channelled exhausted"
        );
        assert!(ctx.effects.contains(&Effect::exhaust(TOP_OF_RUNE_DECK)));
        assert_eq!(ctx.hand_of(0).len(), hand, "no draw");
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
    }

    #[test]
    fn with_an_empty_rune_deck_only_the_main_deck_is_offered_and_it_is_taken_without_asking() {
        let mut fixture = contested(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::RUNE_DECK) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx);
        resolve_until_asked(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "one deck left is no choice: {:?} {:?}",
            ctx.blob.why,
            fixtures::labels(&ctx)
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }

    #[test]
    fn a_conquer_without_her_asks_nothing_and_a_hold_is_not_a_conquer() {
        let mut fixture = contested(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "Vi conquered without her");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), hand);

        let mut fixture = contested(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}

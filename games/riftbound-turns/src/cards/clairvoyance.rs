use super::prelude::{asking, done, draw, play, spell, with_candidates};
use super::scryers_bloom::{look, predicted, predicted_cards, put_on_top, recycle};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const PREDICTS: usize = 5;
pub const DRAWS: usize = 2;
pub const RECYCLE: u8 = super::scryers_bloom::RECYCLE;
pub const ORDER: u8 = super::scryers_bloom::ORDER;
pub const QUESTION: &str =
    "the predicted cards to recycle, then the kept ones top first (skip keeps their order)";

fn order(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let kept = predicted(ctx, item);
    let mut ordered: Vec<u32> = Vec::new();
    for card in ctx.picks().iter().copied() {
        if kept.contains(&card) && !ordered.contains(&card) {
            ordered.push(card);
        }
    }
    if ordered.is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} keeps the predicted cards in their order"
        ));
        return finish(ctx, item);
    }
    for card in ordered.iter().rev() {
        put_on_top(ctx, seat, *card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} puts {} back on top in their chosen order",
        ordered.len()
    ));
    finish(ctx, item)
}

fn finish(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

fn foresee(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        RECYCLE => recycle(ctx, item, u8::MAX, finish),
        ORDER => order(ctx, item),
        _ => look(ctx, item, PREDICTS, finish),
    }
}

pub static CARD: Card = spell(
    "Clairvoyance",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(play(&[], foresee), predicted_cards),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::{priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const SIGHT: u32 = 90;
    const THEIR_SIGHT: u32 = 91;
    const EXTRA: [u32; 3] = [26, 27, 28];
    const MY_DECK: [u32; 7] = [20, 21, 22, 23, 26, 27, 28];
    const TOP_FIVE: [u32; 5] = [28, 27, 26, 23, 22];

    fn sight(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Clairvoyance", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sight(SIGHT, 0));
        fixture.table.cards.push(sight(THEIR_SIGHT, 1));
        for id in EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SIGHT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the predict comes at resolution");
        fixtures::pass_until_open(ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RECYCLE
            })
        );
    }

    #[test]
    fn the_script_is_a_reaction_with_one_asking_targetless_play_ability() {
        assert!(std::ptr::eq(script_of("Clairvoyance").unwrap(), &CARD));
        assert_eq!(CARD.name, "Clairvoyance");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(CARD.flow_cost().is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((PREDICTS, DRAWS), (5, 2));
    }

    #[test]
    fn the_predict_peeks_five_recycles_the_picks_orders_the_rest_top_first_then_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), MY_DECK.to_vec());
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx);
        assert_eq!(
            peeks(&ctx),
            TOP_FIVE.map(|card| (card, 0)).to_vec(),
            "436.1.a · the look reaches its controller only"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 5));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 28}",
                "{card 27}",
                "{card 26}",
                "{card 23}",
                "{card 22}",
                "done",
                "skip"
            ]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SIGHT}}}: choose {QUESTION} (0 of 5)")
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} predicts 5 · looks at the top 5 cards of their deck".to_string()));
        fixtures::choose(&mut ctx, 0, "{card 27}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ORDER
            }),
            "three stay · their order"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 28}", "{card 26}", "{card 23}", "done", "skip"]
        );
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 27, 20, 21, 23, 26, 28],
            "each recycle goes under the last"
        );
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 28}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 26}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS, "the draw follows the predict");
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(
            ctx.hand_of(0).contains(&23) && ctx.hand_of(0).contains(&28),
            "the two put on top are the two drawn"
        );
        assert_eq!(deck_of(&ctx, 0), [22, 27, 20, 21, 26]);
        assert!(ctx.effects.contains(&Effect::Move {
            card: 27,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts 3 back on top in their chosen order".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
        assert_eq!(ctx.card(SIGHT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(deck_of(&ctx, 1), [24, 25], "the other deck is untouched");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_both_questions_keeps_the_order_and_a_partial_order_lifts_only_the_picks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ORDER
            })
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles nothing".to_string()));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23, 26]);
        assert!(ctx.hand_of(0).contains(&28) && ctx.hand_of(0).contains(&27));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps the predicted cards in their order".to_string()));
        drop(ctx);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.hand_of(0).contains(&22) && ctx.hand_of(0).contains(&28),
            "22 lifted to the top, then the untouched 28"
        );
        assert_eq!(deck_of(&ctx, 0), [20, 21, 23, 26, 27]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_all_but_one_skips_the_order_and_a_short_deck_predicts_what_there_is() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        for card in ["{card 28}", "{card 27}", "{card 26}", "{card 23}"] {
            fixtures::choose(&mut ctx, 0, card).unwrap();
        }
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none(), "one card needs no order");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.hand_of(0).contains(&22) && ctx.hand_of(0).contains(&21));
        assert_eq!(deck_of(&ctx, 0), [23, 26, 27, 28, 20]);
        drop(ctx);
        let mut thin = armed();
        thin.table.cards.retain(|card| {
            card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0 || card.id == 23
        });
        thin.resolve();
        let mut ctx = thin.ctx();
        cast(&mut ctx);
        assert_eq!(peeks(&ctx), [(23, 0)], "436.4 · as many as possible");
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} predicts 5 · looks at the top card of their deck".to_string()));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.hand_of(0).contains(&23));
        assert_eq!(drew(&ctx, 0), 1, "one card to draw of the two");
        drop(ctx);
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SIGHT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to predict".to_string()));
        assert_eq!(drew(&ctx, 0), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_and_predicts_its_own_deck() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SIGHT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "a Reaction goes over my spell");
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 25}", "{card 24}", "done", "skip"],
            "their two-card deck"
        );
        assert_eq!(peeks(&ctx), [(25, 1), (24, 1)]);
        fixtures::choose(&mut ctx, 1, "{card 25}").unwrap();
        fixtures::choose(&mut ctx, 1, "done").unwrap();
        assert!(ctx.blob.prompt.is_none(), "one kept needs no order");
        assert_eq!(ctx.blob.chain.len(), 1, "my spell still waits beneath");
        assert_eq!(drew(&ctx, 1), 2);
        assert!(ctx.hand_of(1).contains(&24) && ctx.hand_of(1).contains(&25));
        assert!(deck_of(&ctx, 1).is_empty());
        assert_eq!(deck_of(&ctx, 0), MY_DECK.to_vec());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_pick_and_a_closed_chain_without_a_reaction_window_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 9 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 9,
                count: 7
            }))
        );
        drop(ctx);
        let mut mine = armed();
        let mut ctx = mine.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SIGHT)).is_ok(),
            "a Reaction answers my spell"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SIGHT)),
            Err(Refusal::NotYourTurn),
            "no open window on my turn once the chain is empty"
        );
        let mut poor = armed();
        for rune in poor
            .table
            .cards
            .iter_mut()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
        {
            rune.exhausted = true;
        }
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SIGHT)),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
    }
}

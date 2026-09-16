use super::prelude::{asking, done, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 3;
pub const QUESTION: &str = "a card to put into your hand";
const PICK: u8 = 1;

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

fn on_top(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    looked(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == PICK {
        return keep(ctx, item);
    }
    let seat = item.controller;
    let top = looked(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, PICK, 1, 1))
}

fn keep(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let Some(kept) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card))
    else {
        return done();
    };
    if let Some(hand) = ctx.zones.hand {
        ctx.emit(Effect::Move {
            card: kept,
            zone: hand,
            seat,
            index: TOP,
        });
    }
    ctx.narrate(format!("{{seat {seat}}} puts a card into their hand"));
    let rest: Vec<u32> = top.into_iter().filter(|card| *card != kept).collect();
    for card in &rest {
        ctx.recycle_to_bottom(*card);
    }
    if !rest.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", rest.len()));
    }
    done()
}

pub static CARD: Card = spell(
    "Stacked Deck",
    &[Keyword::Action],
    &[asking(with_candidates(play(&[], run), on_top), QUESTION)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, prompts, resume, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const STACKED: u32 = 90;
    const THEIR_STACKED: u32 = 91;

    fn stacked(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Stacked Deck", 1, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stacked(STACKED, 0));
        fixture.table.cards.push(stacked(THEIR_STACKED, 1));
        fixture.resolve();
        fixture
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

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, STACKED).unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_an_action_spell_with_one_asking_play_ability() {
        assert_eq!(CARD.name, "Stacked Deck");
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn three_peeks_reach_the_controller_the_pick_is_a_put_and_the_rest_go_under_in_order() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        cast(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        for card in [23, 22, 21] {
            assert!(
                ctx.effects.contains(&Effect::Peek { card, seat: 0 }),
                "{card} is peeked by its controller"
            );
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
        }
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "{card 21}"],
            "the top three, top first, and no closer"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {STACKED}}}: choose {QUESTION} (0 of 1)")
        );
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "the spell resolved");
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Drew { .. })),
            "a put is not a draw"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: 23,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx.effects.contains(&Effect::Move {
            card: 21,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(
            deck_of(&ctx, 0),
            [21, 23, 20],
            "top to bottom the deck now reads 20, 23, 21: the rest went under in the listed order"
        );
        assert_eq!(
            ctx.card(STACKED).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell is trashed after it resolves"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thin_deck_offers_what_there_is_and_an_empty_one_does_nothing() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![20, 21].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "{card 22}"]);
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(deck_of(&ctx, 0), [22]);
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to look at");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} has no cards left to look at"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_pick_is_refused_and_an_action_has_no_window_on_their_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STACKED)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 3 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 3,
                count: 3
            }))
        );
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 })
            .unwrap()
            .unwrap();
        resume(&mut ctx, &answered).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.fault.is_none());
    }
}

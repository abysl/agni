use super::prelude::{a_spell, asking, counter_to_hand, done, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const RECYCLE: u8 = 1;

pub static CARD: Card = spell(
    "Abandon",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(play(&[a_spell("a spell to counter")], run), top_of_deck),
        "the top card of your deck to recycle",
    )],
);

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == RECYCLE {
        return recycle(ctx, item);
    }
    counter_to_hand(ctx, item, 0);
    predict(ctx, item)
}

fn predicted(ctx: &Ctx, seat: u8) -> Option<u32> {
    let deck = ctx.zones.main_deck?;
    ctx.table.held(deck, seat).last().map(|card| card.id)
}

fn top_of_deck(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    predicted(ctx, item.controller)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn predict(ctx: &mut Ctx, item: &Item) -> Flow {
    if ctx.peek_top(item.controller).is_none() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, RECYCLE, 0, 1))
}

fn recycle(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = predicted(ctx, seat);
    match ctx.picks().first().copied() {
        Some(card) if top == Some(card) => {
            ctx.recycle_to_bottom(card);
            ctx.narrate(format!(
                "{{seat {seat}}} recycles the top card of their deck"
            ));
        }
        _ => ctx.narrate(format!("{{seat {seat}}} keeps the top card of their deck")),
    }
    done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_step, priority, prompts, resume, settle};
    use crate::state::{PromptWhy, TargetRef, FLAG_NOT_PLAYED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick, PickRefusal};

    const MY_ABANDON: u32 = 77;
    const THEIR_ABANDON: u32 = 83;
    const MY_TOP: u32 = 23;
    const THEIR_TOP: u32 = 25;
    const THEIR_NEXT: u32 = 24;

    fn abandon(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Abandon", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(abandon(MY_ABANDON, 0));
        fixture.table.cards.push(abandon(THEIR_ABANDON, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Chaos", false));
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_step::begin(ctx, seat, card, crate::state::Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play_step::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play_step::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                (PromptWhy::Resume { .. }, _) => resume(ctx, &answered)?,
                (why, _) => panic!("only target and resume prompts are expected here: {why:?}"),
            }
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn cancel(ctx: &Ctx) -> u16 {
        prompts::offered(ctx).len() as u16 - 1
    }

    fn predict_open_for(ctx: &Ctx, item: u16, seat: u8) -> bool {
        ctx.blob.why
            == Some(PromptWhy::Resume {
                item,
                stage: RECYCLE,
            })
            && ctx.blob.prompt.as_ref().map(|prompt| prompt.seat) == Some(seat)
    }

    fn keep(ctx: &mut Ctx, seat: u8) {
        let skip = cancel(ctx);
        pick(ctx, seat, skip).unwrap();
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

    fn played_spells<'a>(ctx: &'a Ctx<'_>) -> Vec<&'a Event> {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::PlayedSpell { .. }))
            .collect()
    }

    fn hand_returns(ctx: &Ctx, card: u32) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move { card: moved, zone, .. }
                        if *moved == card && *zone == fixtures::HAND
                )
            })
            .count()
    }

    fn countered_by_seat_one(fixture: &mut Fixture) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, THEIR_ABANDON).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        ctx
    }

    #[test]
    fn the_script_is_a_reaction_that_counters_one_spell_then_predicts() {
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, crate::cards::Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, crate::cards::TargetKind::Item);
        assert_eq!(ability.targets[0].filter, crate::cards::Filter::Spell);
        assert!(
            ability.candidates.is_some(),
            "Predict picks the top card at resolution"
        );
        assert_eq!(
            ability.question,
            Some("the top card of your deck to recycle")
        );
        assert!(std::ptr::eq(
            crate::cards::script_of("Abandon").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn abandon_counters_a_spell_back_to_its_owners_hand_and_nothing_is_refunded() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let deck_before = deck_of(&ctx, 1);
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let paid = ctx.effects.clone();
        assert!(!paid.is_empty(), "Spark was paid");
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_ABANDON).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 71} on the chain", "cancel"]);
        let offered = prompts::offered(&ctx);
        assert_eq!(offered[0].answer, Answer::Item(1));
        assert_eq!(offered[0].card, Some(fixtures::HAND_SPELL));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 83}: choose a spell to counter (0 of 1)"
        );
        pick(&mut ctx, 1, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        assert_eq!(priority::holder(&ctx), Some(1));
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::exhaust(45)),
            "Abandon is paid with seat 1's runes: {:?}",
            ctx.effects
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(
            predict_open_for(&ctx, 2, 1),
            "the counter landed and Predict asks: {:?}",
            ctx.blob.why
        );
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND),
            "the counter is done before the question"
        );
        keep(&mut ctx, 1);
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
        assert!(ctx.hand_of(0).contains(&fixtures::HAND_SPELL));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::HAND_SPELL,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { card, zone, .. }
                if *card == fixtures::HAND_SPELL && *zone == fixtures::TRASH
        )));
        assert_eq!(
            ctx.card(THEIR_ABANDON).unwrap().zone,
            Some(fixtures::TRASH),
            "Abandon itself is trashed once it resolves"
        );
        assert!(ctx.effects.starts_with(&paid), "nothing is refunded");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 71} is countered · back to hand".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 83} resolves");
        assert_eq!(
            played_spells(&ctx),
            [&Event::PlayedSpell {
                item: 2,
                controller: 1,
                nth: 1
            }],
            "a countered Spark was never played"
        );
        assert!(!ctx.blob.seat(0).played_main);
        assert!(ctx.blob.seat(1).played_main);
        assert!(!ctx.has_flag(fixtures::HAND_SPELL, FLAG_NOT_PLAYED));
        assert_eq!(
            deck_of(&ctx, 1),
            deck_before,
            "the top card was kept where it was"
        );
    }

    #[test]
    fn abandon_with_no_spell_on_the_chain_can_only_be_taken_back() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MY_ABANDON).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert!(labels(&ctx).contains(&"cancel".to_string()));
        assert!(
            prompts::offered(&ctx)
                .iter()
                .all(|opt| opt.card.is_none() && opt.answer.closes()),
            "nothing on the chain to counter: {:?}",
            labels(&ctx)
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        let cancel = cancel(&ctx);
        pick(&mut ctx, 0, cancel).unwrap();
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: MY_ABANDON,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} takes back {card 77}"
        );
        assert!(
            peeks(&ctx).is_empty(),
            "a taken-back Abandon predicts nothing"
        );
    }

    #[test]
    fn abandon_is_refused_in_the_other_seats_open_state_and_without_priority() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ABANDON)),
            Err(Refusal::NotYourTurn),
            "a Reaction needs a chain or the turn"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ABANDON)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 still holds priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert!(legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ABANDON)).is_ok());
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn abandon_whose_spell_already_left_the_chain_counters_nothing_but_still_predicts() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, THEIR_ABANDON).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        play_from_hand(&mut ctx, 0, MY_ABANDON).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 71} on the chain", "{card 83} on the chain", "cancel"],
            "any spell on the chain is a candidate, friend or foe"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 3);
        assert_eq!(ctx.blob.chain[2].targets, [TargetRef::Item(1)]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            predict_open_for(&ctx, 3, 0),
            "seat 0's Abandon resolves first and predicts for seat 0"
        );
        assert_eq!(
            labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        keep(&mut ctx, 0);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "Spark left, seat 1's Abandon remains"
        );
        assert_eq!(ctx.blob.chain[0].kind.source(), THEIR_ABANDON);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(
            predict_open_for(&ctx, 2, 1),
            "a stale target counters nothing, Predict still happens"
        );
        assert_eq!(
            labels(&ctx),
            [format!("{{card {THEIR_TOP}}}"), "skip".to_string()]
        );
        keep(&mut ctx, 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert_eq!(hand_returns(&ctx, fixtures::HAND_SPELL), 1);
        assert_eq!(hand_returns(&ctx, THEIR_ABANDON), 0);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 83} resolves");
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(MY_ABANDON).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND),
            "the stale target is skipped, Spark stays in hand"
        );
        assert_eq!(played_spells(&ctx).len(), 2);
        assert_eq!(peeks(&ctx), [(MY_TOP, 0), (THEIR_TOP, 1)]);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_countered_abandon_returns_to_its_hand_without_predicting_and_the_spell_beneath_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, THEIR_ABANDON).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        play_from_hand(&mut ctx, 0, MY_ABANDON).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(ctx.blob.chain[2].targets, [TargetRef::Item(2)]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(predict_open_for(&ctx, 3, 0));
        keep(&mut ctx, 0);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.hand_of(1).contains(&THEIR_ABANDON));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 83} is countered · back to hand".to_string()));
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.fault.is_none());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.log.contains(&"{card 71} resolves".to_string()));
        assert!(ctx.blob.seat(0).played_main);
        assert!(
            !ctx.blob.seat(1).played_main,
            "seat 1's only spell was countered"
        );
        assert_eq!(played_spells(&ctx).len(), 2);
        assert_eq!(
            peeks(&ctx),
            [(MY_TOP, 0)],
            "a countered Abandon never looks at the deck"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn after_the_counter_abandon_asks_its_controller_whether_to_recycle_the_top_card() {
        let mut fixture = armed();
        let mut ctx = countered_by_seat_one(&mut fixture);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, .. })),
            "the counter landed and the Predict question is open for seat 1"
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.min, prompt.max), (0, 1));
        assert!(!prompt.cancel, "a resolving spell cannot be taken back");
        assert_eq!(deck_of(&ctx, 1), [THEIR_NEXT, THEIR_TOP]);
        assert_eq!(
            labels(&ctx),
            [format!("{{card {THEIR_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 83}: choose the top card of your deck to recycle (0 of 1)"
        );
        assert_eq!(
            peeks(&ctx),
            [(THEIR_TOP, 1)],
            "127.4 · the look reaches its controller only"
        );
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Reveal { .. })),
            "a look is not a reveal"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} looks at the top card of their deck".to_string()));
        assert_eq!(ctx.blob.chain.len(), 1, "Abandon waits on the chain");
        assert_eq!(ctx.blob.chain[0].kind.source(), THEIR_ABANDON);
        assert_eq!(ctx.blob.chain[0].stage, RECYCLE);
        assert_eq!(
            ctx.card(THEIR_ABANDON).unwrap().zone,
            ctx.zones.chain,
            "not yet trashed"
        );
        pick(&mut ctx, 1, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert_eq!(
            deck_of(&ctx, 1),
            [THEIR_TOP, THEIR_NEXT],
            "403.1.a · recycled to the bottom of the Main Deck"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 1,
            index: BOTTOM
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} recycles the top card of their deck".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 83} resolves");
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            deck_of(&ctx, 0),
            [20, 21, 22, MY_TOP],
            "the other deck is untouched"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn declining_the_recycle_leaves_the_looked_at_card_on_top() {
        let mut fixture = armed();
        let mut ctx = countered_by_seat_one(&mut fixture);
        assert!(predict_open_for(&ctx, 2, 1));
        keep(&mut ctx, 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert_eq!(deck_of(&ctx, 1), [THEIR_NEXT, THEIR_TOP]);
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { card, .. } if *card == THEIR_TOP
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} keeps the top card of their deck".to_string()));
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_predict_question_is_the_controllers_alone_and_holds_the_chain() {
        let mut fixture = armed();
        let mut ctx = countered_by_seat_one(&mut fixture);
        assert!(predict_open_for(&ctx, 2, 1));
        assert_eq!(
            pick(&mut ctx, 0, 0),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "seat 0 may neither recycle nor keep for seat 1"
        );
        assert_eq!(
            pick(&mut ctx, 1, 2),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        assert_eq!(priority::pass(&mut ctx, 1), Err(Refusal::PromptOpen));
        assert_eq!(priority::pass(&mut ctx, 0), Err(Refusal::PromptOpen));
        assert!(predict_open_for(&ctx, 2, 1), "the question is still open");
        assert_eq!(deck_of(&ctx, 1), [THEIR_NEXT, THEIR_TOP]);
        assert_eq!(peeks(&ctx), [(THEIR_TOP, 1)], "no second look");
    }

    #[test]
    fn an_empty_deck_has_nothing_to_look_at_so_abandon_resolves_without_asking() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.seat == 1));
        fixture.resolve();
        let ctx = countered_by_seat_one(&mut fixture);
        assert!(
            ctx.blob.prompt.is_none(),
            "418.1.c · look at as many as possible"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        assert!(peeks(&ctx).is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND),
            "the counter still lands"
        );
        assert_eq!(ctx.card(THEIR_ABANDON).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 83} resolves");
    }
}

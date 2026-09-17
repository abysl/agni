use super::prelude::{a_unit, another_unit, card_target, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

const PUMP: i16 = 2;
const SHRINK: i16 = -2;

const TARGETS: &[TargetSpec] = &[
    a_unit("a unit to give +2 might"),
    another_unit("another unit to give -2 might"),
];

fn dance(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, PUMP, None);
    }
    if let Some(unit) = card_target(ctx, item, 1) {
        might_this_turn(ctx, item, unit, SHRINK, None);
    }
    Flow::Done
}

pub static CARD: Card = spell(
    "Defiant Dance",
    &[Keyword::Reaction],
    &[play(TARGETS, dance)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::Target;

    const DANCE: u32 = 90;
    const THEIR_DANCE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            DANCE,
            fixtures::HAND,
            0,
            "Defiant Dance",
            1,
            1,
        ));
        let mut theirs = fixtures::spell(THEIR_DANCE, fixtures::HAND, 1, "Defiant Dance", 1, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
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
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                _ => panic!("Defiant Dance only opens target prompts: {answered:?}"),
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

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn counter_effects(ctx: &Ctx, card: u32) -> Vec<i32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter: COUNTER_MIGHT,
                    delta,
                } if *held == card => Some(*delta),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_two_target_reaction() {
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let specs = CARD.abilities[0].targets;
        assert_eq!(specs.len(), 2);
        assert_eq!((specs[0].min, specs[0].max), (1, 1));
        assert_eq!((specs[1].min, specs[1].max), (1, 1));
        assert!(std::ptr::eq(
            crate::cards::script_of("Defiant Dance").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn it_pumps_one_unit_and_shrinks_another_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DANCE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a unit to give +2 might (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "the first pick is not another unit"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            "{card 90}: choose another unit to give -2 might (0 of 1)"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        for card in [fixtures::VI, fixtures::THEIR_UNIT] {
            assert!(ctx.events.contains(&Event::Chosen {
                card,
                by: 0,
                item: 1
            }));
        }
        assert!(!ctx.effects.is_empty(), "the cost is paid at finalize");
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -2);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "a might below zero counts as zero"
        );
        assert_eq!(counter_effects(&ctx, fixtures::VI), [2]);
        assert_eq!(counter_effects(&ctx, fixtures::THEIR_UNIT), [-2]);
        assert_eq!(ctx.card(DANCE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.seat(0).played_main);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(counter_effects(&ctx, fixtures::VI), [-2]);
        assert_eq!(counter_effects(&ctx, fixtures::THEIR_UNIT), [2]);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|row| row.might.is_empty()));
    }

    #[test]
    fn a_target_gone_or_no_longer_a_unit_is_skipped_and_the_other_half_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DANCE).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        ctx.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::HAND);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(
            counter_effects(&ctx, fixtures::THEIR_UNIT),
            Vec::<i32>::new()
        );
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|row| row.might.is_empty()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DANCE).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        ctx.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::TRASH);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(counter_effects(&ctx, fixtures::VI), Vec::<i32>::new());
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -2);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn it_is_refused_out_of_priority_and_refuses_the_same_unit_twice_before_reacting() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            play_from_hand(&mut ctx, 1, THEIR_DANCE),
            Err(Refusal::NotYourTurn),
            "a reaction in the other seat's neutral open is refused"
        );
        assert_eq!(ctx.card(THEIR_DANCE).unwrap().zone, Some(fixtures::HAND));
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            play_from_hand(&mut ctx, 1, THEIR_DANCE),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 1 waits for priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_DANCE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        pick(&mut ctx, 1, 2).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 1, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the same unit cannot take both halves"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 1, &[DANCE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a card in hand is not a unit"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 1, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the second unit is not optional"
        );
        assert!(ctx.blob.pending(2).is_some(), "the play is still pending");
        assert_eq!(labels(&ctx), ["{card 50}", "{card 60}", "cancel"]);
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(
            ctx.blob.chain[1].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Card(fixtures::VI)
            ]
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 2);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert_eq!(might_counter(&ctx, fixtures::VI), -2);
        assert_eq!(ctx.current_might(fixtures::VI), 1);
        assert_eq!(ctx.card(THEIR_DANCE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.seat(1).played_main);
    }

    #[test]
    fn with_one_unit_on_the_board_the_second_half_only_offers_cancel() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE && card.id != fixtures::THEIR_UNIT);
        fixture.table.tokens.clear();
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DANCE).unwrap();
        assert_eq!(labels(&ctx), ["{card 50}", "cancel"]);
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.starts_with("{card")),
            "no other unit to shrink: {offered:?}"
        );
        let cancel = offered.iter().position(|label| label == "cancel").unwrap();
        assert!(ctx.effects.is_empty(), "nothing was paid");
        pick(&mut ctx, 0, cancel as u16).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: DANCE,
                zone: fixtures::HAND,
                seat: 0,
                index: agni_plugin_sdk::decide::TOP
            }]
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx.blob.is_neutral_open());
    }
}

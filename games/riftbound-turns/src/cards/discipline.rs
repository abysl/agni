use super::prelude::{a_unit, card_target, done, draw, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const MIGHT: i16 = 2;
const CARDS: usize = 1;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = spell(
    "Discipline",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_engine, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::Target;

    const DISCIPLINE: u32 = 90;
    const THEIR_DISCIPLINE: u32 = 91;

    fn discipline(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Discipline", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(discipline(DISCIPLINE, 0));
        fixture.table.cards.push(discipline(THEIR_DISCIPLINE, 1));
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
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                (why, answer) => panic!("Discipline opens only target prompts: {why:?} {answer:?}"),
            }
        }
        settle(ctx)
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

    #[test]
    fn the_script_is_a_reaction_with_one_unit_target() {
        assert_eq!(CARD.name, "Discipline");
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert!(super::super::script_of("Discipline").is_some());
    }

    #[test]
    fn discipline_asks_for_a_unit_then_gives_it_two_might_this_turn_and_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit on the board, friend or foe"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a unit (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::VI,
            by: 0,
            item: 1
        }));
        assert_eq!(
            ctx.effects,
            [Effect::exhaust(41), Effect::exhaust(42)],
            "two energy, no power"
        );
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
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx.effects.contains(&Effect::Move {
            card: 23,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.card(DISCIPLINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 might this turn".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.events.contains(&Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1
        }));
        let row = ctx.state_of(fixtures::VI).unwrap();
        assert_eq!(row.might.len(), 1);
        assert_eq!(row.might[0].until, crate::state::Expiry::EndOfTurn(1));
    }

    #[test]
    fn the_other_seat_reacts_with_discipline_on_its_own_unit_and_draws() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        let hand = ctx.hand_of(1).len();
        play_from_hand(&mut ctx, 1, THEIR_DISCIPLINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        pick(&mut ctx, 1, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(ctx.effects.contains(&Effect::exhaust(44)));
        assert!(ctx.effects.contains(&Effect::exhaust(45)));
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 2);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert_eq!(ctx.hand_of(1).len(), hand);
        assert!(ctx.effects.contains(&Effect::Move {
            card: 25,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert_eq!(
            ctx.card(THEIR_DISCIPLINE).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.seat(1).played_main);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
    }

    #[test]
    fn a_target_that_left_the_board_is_unaffected_but_the_draw_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Counter { counter, .. } if *counter == COUNTER_MIGHT
        )));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "356.3.e.5: the draw is not a target"
        );
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("gets +2 might")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(DISCIPLINE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn discipline_is_refused_from_the_other_seat_in_neutral_open_and_needs_a_unit_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DISCIPLINE)),
            Err(Refusal::NotYourTurn),
            "a reaction has no window in the other seat's neutral open state"
        );
        play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one unit is required"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DISCIPLINE)),
            Err(Refusal::PromptOpen),
            "seat 1 waits while seat 0 finishes its play"
        );
        let cancel = labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert_eq!(ctx.card(DISCIPLINE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| !card.is_kind("Unit") || card.zone == Some(fixtures::HAND));
        empty.resolve();
        let mut ctx = empty.ctx();
        play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.starts_with("{card")),
            "no unit on the board to choose: {offered:?}"
        );
        assert_eq!(offered.last().map(String::as_str), Some("cancel"));
        pick(&mut ctx, 0, offered.len() as u16 - 1).unwrap();
        assert_eq!(ctx.card(DISCIPLINE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}

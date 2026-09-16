use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Keyword};

pub const MIGHT_LOSS: i16 = -4;
pub const MIGHT_FLOOR: i32 = 1;

pub static CARD: Card = spell(
    "Smoke Screen",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], |ctx, item, _| {
        if let Some(unit) = card_target(ctx, item, 0) {
            might_this_turn(ctx, item, unit, MIGHT_LOSS, Some(MIGHT_FLOOR));
        }
        done()
    })],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play, priority, prompts, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::Target;

    const MINE: u32 = 90;
    const THEIRS: u32 = 91;
    const BRUTE: u32 = 92;
    const THEIR_RUNE: u32 = 46;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            MINE,
            fixtures::HAND,
            0,
            "Smoke Screen",
            2,
            1,
        ));
        let mut theirs = fixtures::spell(THEIRS, fixtures::HAND, 1, "Smoke Screen", 2, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_RUNE, 1, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 6));
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
        play::begin(ctx, seat, card, crate::state::Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick_card(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let offered = prompts::offered(ctx);
        let option = offered
            .iter()
            .position(|opt| opt.card == Some(card))
            .expect("the unit is offered") as u16;
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            let PromptWhy::Target { item, spec } = answered.why else {
                panic!("a target prompt was open");
            };
            play::choose_targets(ctx, item, spec, &answered.prompt.picked)?;
        }
        settle(ctx)
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_registered_as_a_reaction_with_one_unit_target() {
        assert_eq!(
            crate::cards::script_of("Smoke Screen").map(|card| card.name),
            Some("Smoke Screen")
        );
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(
            CARD.abilities[0].targets[0].filter,
            crate::cards::Filter::Unit
        );
        assert_eq!(
            (
                CARD.abilities[0].targets[0].min,
                CARD.abilities[0].targets[0].max
            ),
            (1, 1)
        );
    }

    #[test]
    fn a_big_unit_loses_four_might_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let offered = prompts::offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{card 50}", "{card 60}", "{card 81}", "{card 92}", "cancel"],
            "every unit on the board is a legal target, friend or foe"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a unit (0 of 1)"
        );
        pick_card(&mut ctx, 0, BRUTE).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: BRUTE,
            by: 0,
            item: 1
        }));
        assert_eq!(
            ctx.current_might(BRUTE),
            6,
            "nothing happens before it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BRUTE), 2);
        assert_eq!(might_counter(&ctx, BRUTE), -4);
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(BRUTE),
            counter: COUNTER_MIGHT,
            delta: -4
        }));
        assert_eq!(ctx.card(MINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(BRUTE), 2);
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(BRUTE),
            counter: COUNTER_MIGHT,
            delta: 4
        }));
        assert_eq!(ctx.current_might(BRUTE), 6);
        assert_eq!(might_counter(&ctx, BRUTE), 0);
    }

    #[test]
    fn a_small_unit_stops_at_one_might() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, fixtures::THEIR_UNIT).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 1);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -1);
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::THEIR_UNIT),
            counter: COUNTER_MIGHT,
            delta: -1
        }));
        assert!(!ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::THEIR_UNIT),
            counter: COUNTER_MIGHT,
            delta: -4
        }));
    }

    #[test]
    fn a_unit_already_at_one_might_is_left_alone() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(93, fixtures::BASE, 0, "Runt", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, 93).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(93), 1);
        assert_eq!(might_counter(&ctx, 93), 0);
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Counter {
                target: Target::Card(93),
                counter: COUNTER_MIGHT,
                ..
            }
        )));
        assert_eq!(ctx.card(MINE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_reaction_waits_for_priority_then_joins_the_chain_on_the_other_seats_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIRS)),
            Err(Refusal::NotYourTurn),
            "no chain and not their turn: nothing to react to"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIRS)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 holds priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIRS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        pick_card(&mut ctx, 1, fixtures::VI).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "Smoke Screen resolved above Spark");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(ctx.current_might(fixtures::VI), 1);
        assert_eq!(might_counter(&ctx, fixtures::VI), -2);
        assert_eq!(ctx.card(THEIRS).unwrap().zone, Some(fixtures::TRASH));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 1);
    }

    #[test]
    fn the_target_that_left_the_board_gets_nothing_and_the_spell_still_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, BRUTE).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.kill(BRUTE, crate::engine::ctx::Cause::Rule);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(MINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Counter {
                target: Target::Card(BRUTE),
                counter: COUNTER_MIGHT,
                ..
            }
        )));
    }
}

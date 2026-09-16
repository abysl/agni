use super::prelude::{done, might_this_turn, triggered, unit};
use super::{Card, Trigger};

pub static CARD: Card = unit(
    "Ravenbloom Student",
    &[],
    &[triggered(Trigger::YouPlaySpell, &[], |ctx, item, _| {
        let me = item.kind.source();
        if ctx.on_board(me) {
            might_this_turn(ctx, item, me, 1, None);
        }
        done()
    })],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::spell;
    use crate::cards::Keyword;
    use crate::engine::ctx::{Ctx, EntryMove, Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, phases, play, priority, settle};
    use crate::state::{ItemKind, Origin, Priority, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::Target;

    static REPLY: Card = spell("Reply", &[Keyword::Reaction], &[]);

    const STUDENT: u32 = 90;
    const SECOND_STUDENT: u32 = 91;
    const HAND_STUDENT: u32 = 92;
    const THEIR_REPLY: u32 = 93;

    fn student(id: u32, zone: u16, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Ravenbloom Student", 2);
        card.domain = vec!["Mind".into()];
        card
    }

    fn enrolled() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(student(STUDENT, fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(student(HAND_STUDENT, fixtures::HAND, 0));
        let mut reply = fixtures::spell(THEIR_REPLY, fixtures::HAND, 1, "Reply", 1, 0);
        reply.domain = vec!["Mind".into()];
        fixture.table.cards.push(reply);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_REPLY, &REPLY);
        fixture
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn played_spells(ctx: &Ctx) -> Vec<u8> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::PlayedSpell { controller, .. } => Some(*controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_registry_resolves_the_student_by_name() {
        let fixture = enrolled();
        let script = fixture.scripts.of_card(STUDENT).unwrap();
        assert!(std::ptr::eq(script, &CARD));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::YouPlaySpell);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert!(CARD.keywords.is_empty());
    }

    #[test]
    fn the_student_grows_by_one_after_its_controllers_spell_resolves_and_shrinks_back_at_end_of_turn(
    ) {
        let mut fixture = enrolled();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(STUDENT), 2);
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.blob.queue.is_empty(),
            "nothing triggers before the spell resolves"
        );
        assert_eq!(might_counter(&ctx, STUDENT), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(played_spells(&ctx), [0]);
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger is on the chain");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == STUDENT
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(ctx.blob.prompt.is_none(), "the student asks nothing");
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            })
        );
        assert_eq!(
            might_counter(&ctx, STUDENT),
            0,
            "the bonus waits for the trigger to resolve"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(might_counter(&ctx, STUDENT), 1);
        assert_eq!(ctx.current_might(STUDENT), 3);
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "only the student grows"
        );
        assert_eq!(
            might_counter(&ctx, HAND_STUDENT),
            0,
            "a student in hand never triggers"
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} ability resolves");
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(STUDENT), 3);
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(STUDENT),
            counter: COUNTER_MIGHT,
            delta: -1
        }));
        assert_eq!(ctx.current_might(STUDENT), 2);
        assert_eq!(might_counter(&ctx, STUDENT), 0);
    }

    #[test]
    fn an_opponents_spell_leaves_the_student_alone_even_when_it_resolves_on_the_students_turn() {
        let mut fixture = enrolled();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(
                &ctx,
                1,
                &EntryMove {
                    card: THEIR_REPLY,
                    from: ctx.zones.hand,
                    from_seat: 1,
                    to: ctx.zones.chain,
                    to_seat: 0,
                    index: TOP,
                    hidden: false,
                }
            ),
            Err(Refusal::NotYourTurn),
            "no chain, no reaction window for the other seat"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, THEIR_REPLY).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(played_spells(&ctx), [1]);
        assert_eq!(ctx.blob.chain.len(), 1, "only the first spell remains");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert!(
            ctx.blob.queue.is_empty(),
            "the opponent's spell is not 'you play a spell'"
        );
        assert_eq!(might_counter(&ctx, STUDENT), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(played_spells(&ctx), [1, 0]);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == STUDENT
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, STUDENT), 1);
        assert_eq!(ctx.current_might(STUDENT), 3);
    }

    #[test]
    fn two_students_are_ordered_and_each_grows_once() {
        let mut fixture = enrolled();
        fixture
            .table
            .cards
            .push(student(SECOND_STUDENT, fixtures::BASE, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 })
        ));
        let first = fixtures::labels(&ctx)[0].clone();
        fixtures::choose(&mut ctx, 0, &first).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, STUDENT), 0);
        assert_eq!(might_counter(&ctx, SECOND_STUDENT), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, STUDENT), 1);
        assert_eq!(might_counter(&ctx, SECOND_STUDENT), 1);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_student_that_left_the_board_before_its_trigger_resolves_gets_nothing() {
        let mut fixture = enrolled();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.bounce(STUDENT));
        assert_eq!(ctx.card(STUDENT).unwrap().zone, Some(fixtures::HAND));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, STUDENT), 0);
        assert!(ctx.state_of(STUDENT).is_none_or(|row| row.might.is_empty()));
    }
}

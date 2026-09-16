use super::prelude::{a_card, card_target, done, draw, play, spell};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 2;

pub const FRIENDLY_UNIT_WITHOUT_TEMPORARY: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Not(&Filter::Temporary),
]);

fn call(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(unit) = card_target(ctx, item, 0) {
        if ctx.mark_temporary(unit) {
            ctx.narrate(format!("{{card {unit}}} is Temporary"));
        }
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = spell(
    "Shadow's Call",
    &[],
    &[play(
        &[a_card(
            FRIENDLY_UNIT_WITHOUT_TEMPORARY,
            "a friendly unit without [Temporary]",
        )],
        call,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger, IMPLICIT_TEMPORARY};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, triggers};
    use crate::rules::COUNTER_TEMPORARY;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const CALL: u32 = 90;
    const THEIR_CALL: u32 = 91;
    const FADING: u32 = 92;
    const ORDER_RUNE: u32 = 100;

    fn call_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Shadow's Call", 2, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(call_card(CALL, 0));
        fixture.table.cards.push(call_card(THEIR_CALL, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(FADING, fixtures::BASE, 0, "Fading", 2));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(FADING),
            counter: COUNTER_TEMPORARY,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    fn temporary_matches(ctx: &Ctx, seat: u8) -> Vec<u32> {
        triggers::find(ctx, &Event::BeginningPhase { seat })
            .into_iter()
            .filter(|held| held.index == IMPLICIT_TEMPORARY)
            .map(|held| held.source)
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_spell_over_one_friendly_unit_that_is_not_yet_temporary() {
        assert!(std::ptr::eq(script_of("Shadow's Call").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT_WITHOUT_TEMPORARY);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(DRAWS, 2);
    }

    #[test]
    fn a_friendly_unit_becomes_temporary_and_its_controller_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "the Temporary friendly unit and the enemy units are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(!ctx.is_temporary(fixtures::VI));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_temporary(fixtures::VI));
        assert!(ctx.on_board(fixtures::VI), "742.1 · it dies later, not now");
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::VI), COUNTER_TEMPORARY),
            Some(1)
        );
        assert_eq!(temporary_matches(&ctx, 0), [fixtures::VI, FADING]);
        assert_eq!(draws(&ctx), 2);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 2);
        assert!(ctx.blob.log.contains(&"{card 50} is Temporary".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_the_board_is_untouched_but_the_draw_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let hand = ctx.zones.hand.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, hand, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_temporary(fixtures::VI));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("is Temporary")));
        assert_eq!(
            draws(&ctx),
            2,
            "359.3.e.5 · the draw is not about the target"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_temporary_or_enemy_unit_is_refused_and_the_opponents_copy_waits_for_their_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CALL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        for wrong in [
            FADING,
            fixtures::SPRITE,
            fixtures::THEIR_UNIT,
            fixtures::HAND_UNIT,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is Temporary, an enemy or not on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}

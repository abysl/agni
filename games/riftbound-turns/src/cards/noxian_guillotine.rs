use super::prelude::{a_unit, card_target, done, kill, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub fn legion_active(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.seat(seat).cards_played > 1
}

fn mark_for_the_guillotine(ctx: &mut Ctx, _: &Item, unit: u32) {
    ctx.narrate(format!(
        "{{card {unit}}} is marked · it dies the next time it takes damage this turn"
    ));
}

fn drop_the_blade(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if legion_active(ctx, item.controller) {
        ctx.narrate("Legion · the blade drops now");
        if kill(ctx, item, unit) == Killed::Yes {
            ctx.narrate(format!("{{card {unit}}} dies"));
        }
        return done();
    }
    mark_for_the_guillotine(ctx, item, unit);
    done()
}

pub static CARD: Card = spell(
    "Noxian Guillotine",
    &[Keyword::Action, Keyword::Legion],
    &[play(&[a_unit("a unit")], drop_the_blade)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const GUILLOTINE: u32 = 90;
    const THEIR_GUILLOTINE: u32 = 91;
    const ORDER_RUNE: u32 = 100;
    const FURY_D: u32 = 101;

    fn guillotine(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Noxian Guillotine", 4, 1);
        card.domain = vec!["Fury".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guillotine(GUILLOTINE, 0));
        fixture.table.cards.push(guillotine(THEIR_GUILLOTINE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_D, 0, "Fury", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn play_a_unit_first(ctx: &mut Ctx) {
        play_engine::begin(
            ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert_eq!(ctx.blob.seat(0).cards_played, 1);
    }

    #[test]
    fn the_script_is_a_legion_action_over_one_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Noxian Guillotine").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Noxian Guillotine");
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Legion));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, UNIT);
    }

    #[test]
    fn with_another_card_played_this_turn_the_blade_drops_at_once() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_a_unit_first(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, GUILLOTINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            ["cancel", "{card 50}", "{card 60}", "{card 70}", "{card 81}"],
            "any unit on the board, the one just played included"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.blob.seat(0).cards_played,
            2,
            "812.1.c · the Guillotine is the second card finalized this turn"
        );
        assert!(legion_active(&ctx, 0));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "nothing before it resolves"
        );
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, .. } if *card == fixtures::THEIR_UNIT)
        ));
        assert!(ctx
            .blob
            .log
            .contains(&"Legion · the blade drops now".to_string()));
        assert!(ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("is marked")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(GUILLOTINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn as_the_first_card_of_the_turn_it_only_marks_the_unit_and_kills_nothing_now() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUILLOTINE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.seat(0).cards_played, 1);
        assert!(!legion_active(&ctx, 0));
        both_pass(&mut ctx);
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx.blob.log.contains(
            &"{card 81} is marked · it dies the next time it takes damage this turn".to_string()
        ));
        assert_eq!(ctx.card(GUILLOTINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · a turn-scoped damage watcher · triggers::sources lists in-play cards only, so a resolved spell cannot watch for the next damage on its target; mark_for_the_guillotine only narrates until the engine grows floating triggers"]
    fn without_legion_the_marked_unit_dies_the_next_time_it_takes_damage_this_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUILLOTINE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "one damage on a 2-Might unit is not lethal by itself: the mark killed it"
        );
    }

    #[test]
    fn a_target_that_left_the_board_is_left_alone_even_under_legion() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_a_unit_first(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, GUILLOTINE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert_eq!(ctx.card(GUILLOTINE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn non_units_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_GUILLOTINE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, GUILLOTINE).unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, ORDER_RUNE] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(GUILLOTINE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}

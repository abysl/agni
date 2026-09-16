use super::prelude::{
    asking, choosing, chosen_mode, done, draw, mode, on_move_to_battlefield, run_mode, unit,
};
use super::{Card, Flow, Item, ModeSpec, Stage};
use crate::engine::ctx::Ctx;
use crate::state::PromptWhy;

pub const QUESTION: &str = "one: each player discards 1, or each player draws 1";
pub const STAGE_MODE: u8 = 1;
pub const STAGE_DISCARDED: u8 = 2;
pub const DRAWS: usize = 1;
pub const MODES: &[ModeSpec] = &[
    mode("each player discards 1", &[], everyone_discards),
    mode("each player draws 1", &[], everyone_draws),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Discard,
    Draw,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Discard),
        1 => Some(Mode::Draw),
        _ => None,
    }
}

pub fn seats_in_turn_order(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

fn everyone_draws(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.narrate(format!(
        "{{card {}}}: each player draws 1",
        item.kind.source()
    ));
    for seat in seats_in_turn_order(ctx) {
        let drawn = draw(ctx, seat, DRAWS);
        ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    }
    done()
}

fn everyone_discards(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.narrate(format!(
        "{{card {}}}: each player discards 1",
        item.kind.source()
    ));
    next_discard(ctx, item, 0)
}

fn next_discard(ctx: &mut Ctx, item: &Item, from: usize) -> Flow {
    let seats = seats_in_turn_order(ctx);
    let mut index = from;
    while let Some(seat) = seats.get(index).copied() {
        index += 1;
        if ctx.hand_of(seat).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no card to discard"));
            continue;
        }
        let stage = STAGE_DISCARDED.saturating_add(u8::try_from(index).unwrap_or(u8::MAX));
        return Flow::Ask(ctx.ask(
            seat,
            1,
            1,
            false,
            PromptWhy::Discard {
                item: item.id,
                stage,
            },
        ));
    }
    done()
}

fn swift(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_MODE => run_mode(ctx, item, stage),
        discarded if discarded > STAGE_DISCARDED => {
            next_discard(ctx, item, usize::from(discarded - STAGE_DISCARDED))
        }
        _ => Flow::Ask(ctx.ask_resume(item, STAGE_MODE, 1, 1)),
    }
}

pub static CARD: Card = unit(
    "Minah Swiftfoot",
    &[],
    &[asking(
        choosing(on_move_to_battlefield(&[], swift), MODES),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, prompts, settle};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const MINAH: u32 = 90;
    const THEIR_SPELL: u32 = 95;

    fn minah(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(MINAH, zone, seat, "Minah Swiftfoot", 6)
        }
    }

    fn trail(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(minah(zone, 0));
        fixture.table.cards.push(fixtures::spell(
            THEIR_SPELL,
            fixtures::HAND,
            1,
            "Their Spark",
            1,
            0,
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MINAH).unwrap(), &CARD));
        fixture
    }

    fn her_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == MINAH))
            .count()
    }

    fn walk_out(ctx: &mut Ctx) {
        march::standard_move(
            ctx,
            0,
            MINAH,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(ctx).unwrap();
        assert_eq!(her_items(ctx), 1, "the move trigger waits on the chain");
        assert!(ctx.blob.prompt.is_none(), "the mode is asked at resolution");
        pass_until_parked(ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_MODE
            })
        );
        assert_eq!(
            fixtures::labels(ctx),
            ["each player discards 1", "each player draws 1"],
            "the two modes by name; no skip on a choose-one"
        );
    }

    fn trash_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::TRASH, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_one_move_to_battlefield_trigger_over_two_named_modes() {
        assert!(std::ptr::eq(script_of("Minah Swiftfoot").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.candidates.is_none());
        assert_eq!(ability.modes.len(), 2);
        assert_eq!(ability.modes[0].label, "each player discards 1");
        assert_eq!(ability.modes[1].label, "each player draws 1");
        assert!(ability.modes.iter().all(|mode| mode.targets.is_empty()));
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = trail(fixtures::BASE);
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: MINAH,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Discard));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Draw));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
        assert_eq!(seats_in_turn_order(&ctx), [0, 1], "the turn player first");
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_draw_mode_has_each_player_draw_one_in_turn_order() {
        let mut fixture = trail(fixtures::BASE);
        let action = fixtures::move_action(MINAH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let mine = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        walk_out(&mut ctx);
        fixtures::choose(&mut ctx, 0, "each player draws 1").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), mine + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), theirs + DRAWS);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert_eq!(ctx.blob.seat(1).draws, 1);
        let mine_at = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 0} draws 1")
            .expect("the turn player draws first");
        let theirs_at = ctx
            .blob
            .log
            .iter()
            .position(|line| line == "{seat 1} draws 1")
            .unwrap();
        assert!(mine_at < theirs_at);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MINAH}}}: each player draws 1")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_discard_mode_asks_each_player_in_turn_for_a_card_and_only_they_may_answer() {
        let mut fixture = trail(fixtures::BASE);
        let action = fixtures::move_action(MINAH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let mine = ctx.hand_of(0).len();
        walk_out(&mut ctx);
        fixtures::choose(&mut ctx, 0, "each player discards 1").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DISCARDED + 1
            }),
            "the turn player discards first"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(fixtures::labels(&ctx).len(), mine, "every card in hand");
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(trash_of(&ctx, 0), [fixtures::HAND_GEAR]);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DISCARDED + 2
            }),
            "then the next player"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        assert_eq!(fixtures::labels(&ctx).len(), 2, "their two cards");
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SPELL}}}")).unwrap();
        assert_eq!(trash_of(&ctx, 1), [THEIR_SPELL]);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), mine - 1);
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MINAH}}}: each player discards 1")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} discards {{card {THEIR_SPELL}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_player_with_an_empty_hand_is_passed_over() {
        let mut fixture = trail(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.seat != 1);
        fixture.resolve();
        let action = fixtures::move_action(MINAH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk_out(&mut ctx);
        fixtures::choose(&mut ctx, 0, "each player discards 1").unwrap();
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        assert!(ctx.blob.chain.is_empty(), "nobody else has a card");
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no card to discard".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_walk_home_is_no_move_to_a_battlefield() {
        let mut fixture = trail(fixtures::BF1);
        let action = fixtures::move_action(MINAH, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            MINAH,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(MINAH), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn the_modes_are_offered_by_name() {
        let mut fixture = trail(fixtures::BASE);
        let action = fixtures::move_action(MINAH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            MINAH,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        pass_until_parked(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            ["each player discards 1", "each player draws 1"]
        );
    }
}

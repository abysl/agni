use super::prelude::{
    asking, battlefield, location_of, on_unit_played_here, once_per_seat_each_turn,
    trigger_subject, when, with_candidates, Location,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::march;
use crate::state::TargetRef;

const PICK: u8 = 1;

fn a_non_token_unit(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { card, .. } if !ctx.is_token(*card))
}

fn other_units_of_the_player_here(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let Some(at @ Location::Battlefield(_)) = location_of(ctx, item.kind.source()) else {
        return Vec::new();
    };
    let played = trigger_subject(item);
    ctx.units_at(at)
        .into_iter()
        .filter(|unit| Some(*unit) != played)
        .filter(|unit| ctx.controller(*unit) == item.controller)
        .filter(|unit| ctx.movable_to_base(*unit))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    other_units_of_the_player_here(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn may_move_another_unit_home(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = other_units_of_the_player_here(ctx, item);
        if let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| offered.contains(picked))
        {
            march::effect_move(ctx, item, unit, Location::Base(seat));
        }
        return Flow::Done;
    }
    if other_units_of_the_player_here(ctx, item).is_empty() {
        return Flow::Done;
    }
    Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
}

pub static CARD: Card = battlefield(
    "Star Spring",
    &[],
    &[asking(
        with_candidates(
            when(
                once_per_seat_each_turn(on_unit_played_here(&[], may_move_another_unit_home)),
                a_non_token_unit,
            ),
            candidates,
        ),
        "another unit you control here to move to its base",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{prelude, Once, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{once_by_seat, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const SPRING: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;
    const THEIR_ROOKIE: u32 = 91;

    fn spring() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SPRING).unwrap().name = "Star Spring".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut jinx = fixtures::unit(JINX, fixtures::HAND, 0, "Jinx", 2);
        jinx.energy = Some(1);
        fixture.table.cards.push(jinx);
        let mut rookie = fixtures::unit(THEIR_ROOKIE, fixtures::HAND, 1, "Pit Rookie", 1);
        rookie.energy = Some(1);
        fixture.table.cards.push(rookie);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPRING).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_here(ctx: &mut Ctx, seat: u8, card: u32) {
        fixtures::play_from_hand(ctx, seat, card).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(ctx, seat, "{zone 9}").unwrap();
        }
        fixtures::pass_until_open(ctx);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_spring_has_one_once_per_seat_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Star Spring").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitPlayedHere);
        assert_eq!(ability.once, Once::PerSeatPerTurn);
        assert!(ability.condition.is_some());
        assert!(ability.candidates.is_some());
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn the_first_non_token_unit_a_player_plays_here_may_send_another_of_theirs_home() {
        let mut fixture = spring();
        let mut ctx = fixture.ctx();
        play_here(&mut ctx, 0, JINX);
        assert_eq!(
            ctx.location(JINX),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert!(ctx.has_flag(SPRING, once_by_seat(0)));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "skip"],
            "the unit just played is not another unit"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SPRING}}}: choose another unit you control here to move to its base (0 of 1)")
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::VI,
            zone: fixtures::BASE,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_moves_nothing_and_the_second_unit_this_turn_asks_nothing() {
        let mut fixture = spring();
        let mut second = fixtures::unit(92, fixtures::HAND, 0, "Caitlyn", 2);
        second.energy = Some(1);
        fixture.table.cards.push(second);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_here(&mut ctx, 0, JINX);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let effects = ctx.effects.len();
        play_here(&mut ctx, 0, 92);
        assert!(
            ctx.blob.prompt.is_none(),
            "seat 0's once is spent this turn"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(92), Some(Location::Battlefield(fixtures::BF1)));
        assert!(!ctx.effects[effects..].iter().any(|effect| matches!(
            effect,
            Effect::Move { card, zone, .. } if *card == fixtures::VI && *zone == fixtures::BASE
        )));
    }

    #[test]
    fn a_token_played_here_asks_nothing_and_spends_no_once_and_a_lone_unit_asks_nothing() {
        let mut fixture = spring();
        let mut ctx = fixture.ctx();
        prelude::spawn(
            &mut ctx,
            0,
            prelude::Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            false,
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(!ctx.has_flag(SPRING, once_by_seat(0)));
        let mut alone = spring();
        alone.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        alone.resolve();
        let mut ctx = alone.ctx();
        play_here(&mut ctx, 0, JINX);
        assert!(ctx.has_flag(SPRING, once_by_seat(0)));
        assert!(
            ctx.blob.prompt.is_none(),
            "no other unit here: nothing to ask (055)"
        );
        assert!(ctx.blob.chain.is_empty());
    }
}

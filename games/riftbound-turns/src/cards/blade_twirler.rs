use super::prelude::{burn, done, on_move, once_each_turn, seat_target, target, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const BURNED: usize = 1;
pub const A_PLAYER: TargetSpec =
    target(Filter::Any, 1, 1, TargetKind::Seat, "a player who burns 1");

fn twirl(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(seat) = seat_target(item, 0) else {
        return done();
    };
    let burned = burn(ctx, seat, BURNED);
    if burned == 0 {
        ctx.narrate(format!("{{seat {seat}}} has no card left to burn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Blade Twirler",
    &[],
    &[once_each_turn(on_move(&[A_PLAYER], twirl))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Once, Trigger, Where, Who};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{expiry, legal, march, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef, FLAG_ONCE_USED};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const TWIRLER: u32 = 90;
    const THIRD_FIELD: u32 = 91;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const THEIR_DECK: [u32; 2] = [24, 25];

    fn twirler(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Fury".into()],
            ..fixtures::unit(TWIRLER, zone, seat, "Blade Twirler", 4)
        }
    }

    fn dojo(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(twirler(zone, 0));
        fixture.table.cards.push(fixtures::card(
            THIRD_FIELD,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TWIRLER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn deck(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn trash(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::TRASH, seat)
            .map(|card| card.id)
            .collect()
    }

    fn twirler_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == TWIRLER))
            .count()
    }

    fn walk(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, TWIRLER, from, to);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_one_once_a_turn_move_trigger_choosing_any_player() {
        assert!(std::ptr::eq(script_of("Blade Twirler").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert_eq!(
            ability.once,
            Once::PerTurn,
            "the first time I move each turn"
        );
        assert_eq!(ability.targets, [A_PLAYER]);
        assert_eq!(A_PLAYER.kind, TargetKind::Seat);
        assert_eq!(A_PLAYER.filter, Filter::Any, "a player · yourself included");
        assert_eq!((A_PLAYER.min, A_PLAYER.max), (1, 1));
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(BURNED, 1);
    }

    #[test]
    fn the_first_move_asks_for_a_player_and_they_burn_one_when_it_resolves() {
        let mut fixture = dojo(fixtures::BASE);
        let action = fixtures::move_action(TWIRLER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(TWIRLER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.has_flag(TWIRLER, FLAG_ONCE_USED),
            "383.3.e · spent as it triggers"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{seat 0}", "{seat 1}"]);
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[TWIRLER]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a card is not a player"
        );
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        assert_eq!(twirler_items(&ctx), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Seat(1)]);
        assert_eq!(deck(&ctx, 1), THEIR_DECK, "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck(&ctx, 1), [24], "their top card is gone");
        assert_eq!(trash(&ctx, 1), [25]);
        assert_eq!(deck(&ctx, 0), MY_DECK, "the chooser's deck is untouched");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Burned { seat: 1, card: 25 })));
        assert!(ctx.blob.log.contains(&"{seat 1} burns 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_move_the_same_turn_is_silent_and_the_next_turn_twirls_again() {
        let mut fixture = dojo(fixtures::BASE);
        let action = fixtures::move_action(TWIRLER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::choose(&mut ctx, 0, "{seat 0}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(deck(&ctx, 0), [20, 21, 22], "you may choose yourself");
        assert_eq!(trash(&ctx, 0), [23]);
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            TWIRLER,
            Location::Battlefield(fixtures::BF3),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(TWIRLER),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert_eq!(
            twirler_items(&ctx),
            0,
            "383.3.e.1 · only the first move each turn"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck(&ctx, 0), [20, 21, 22]);
        expiry::at_expiration(&mut ctx);
        assert!(
            !ctx.has_flag(TWIRLER, FLAG_ONCE_USED),
            "the turn's memory expires"
        );
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            TWIRLER,
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            twirler_items(&ctx),
            1,
            "the first move of a new turn twirls again"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_player_with_an_empty_deck_burns_nothing_and_an_opponents_move_of_him_still_twirls() {
        let mut fixture = dojo(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| !THEIR_DECK.contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(TWIRLER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(deck(&ctx, 1).is_empty());
        assert!(trash(&ctx, 1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no card left to burn".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = dojo(fixtures::BF1);
        let mut ctx = fixture.ctx();
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            TWIRLER,
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        eprintln!(
            "DEBUG log={:?} events={:?} queue={} loc={:?}",
            ctx.blob.log,
            ctx.events,
            ctx.blob.queue.len(),
            ctx.location(TWIRLER)
        );
        assert_eq!(
            twirler_items(&ctx),
            1,
            "whoever moves him, his controller chooses the player"
        );
        assert_eq!(ctx.blob.queue[0].item.controller, 0);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(fixtures::labels(&ctx), ["{seat 0}", "{seat 1}"]);
    }
}

use super::prelude::{battlefield, done, might_this_turn, trigger_subject, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger, Where, Who};
use crate::engine::ctx::{Ctx, Event, Location};

pub const MIGHT: i16 = 1;

fn here(ctx: &Ctx, source: Source) -> Option<Location> {
    ctx.card(source.card)
        .and_then(|held| held.zone)
        .map(Location::Battlefield)
}

pub fn moved_from_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Moved { from, .. } = event else {
        return false;
    };
    from.is_some() && *from == here(ctx, source)
}

pub fn any_unit_moving_from_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    moved_from_here(ctx, event, source)
}

fn give_it_one_this_turn(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if !ctx.on_board(unit) {
        return done();
    }
    might_this_turn(ctx, item, unit, MIGHT, None);
    ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} Might this turn"));
    done()
}

pub static CARD: Card = battlefield(
    "Back-Alley Bar",
    &[],
    &[when(
        triggered(
            Trigger::Move {
                of: Who::Any,
                to: Where::FromLocation,
            },
            &[],
            give_it_one_this_turn,
        ),
        moved_from_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::engine::ctx::MoveCause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, priority, settle, triggers};
    use crate::state::ItemKind;

    const BAR: u32 = fixtures::GROUNDS;
    const SECOND: u32 = 90;

    fn bar_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(BAR).unwrap().name = "Back-Alley Bar".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BAR).unwrap(), &CARD));
        fixture
    }

    fn bar_items(ctx: &Ctx) -> Vec<(u8, Option<u32>)> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == BAR => {
                    Some((item.controller, item.subject_card()))
                }
                _ => None,
            })
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_bar_is_a_move_trigger_gated_on_leaving_this_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Back-Alley Bar").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Any,
                to: Where::FromLocation
            }
        );
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn a_unit_that_moves_from_here_by_effect_gets_one_might_until_the_turn_ends() {
        let mut fixture = bar_held_by_me();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            fixtures::VI,
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(bar_items(&ctx), [(0, Some(fixtures::VI))]);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "not before it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            ctx.current_might(SECOND),
            2,
            "the one that stayed is untouched"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +{MIGHT} Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_standard_move_out_to_another_battlefield_fires_it_and_a_move_in_does_not() {
        let mut fixture = bar_held_by_me();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            SECOND,
            Location::Battlefield(fixtures::BF2),
            MoveCause::Standard,
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(SECOND),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(bar_items(&ctx), [(0, Some(SECOND))]);
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(SECOND), 3, "+1 wherever it went");
        drop(ctx);
        let mut arriving = bar_held_by_me();
        arriving.table.card_mut(SECOND).unwrap().zone = Some(fixtures::BASE);
        arriving.resolve();
        let mut ctx = arriving.ctx();
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            SECOND,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        assert!(
            bar_items(&ctx).is_empty(),
            "moving to here is not moving from here"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(SECOND), 2);
        drop(ctx);
        let mut elsewhere = bar_held_by_me();
        let mut ctx = elsewhere.ctx();
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            fixtures::SPRITE,
            Location::Base(1),
        );
        settle(&mut ctx).unwrap();
        assert!(
            bar_items(&ctx).is_empty(),
            "a move from another battlefield"
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
    }

    #[test]
    fn the_condition_reads_the_departure_and_never_a_move_without_one() {
        let mut fixture = bar_held_by_me();
        let ctx = fixture.ctx();
        let source = Source {
            card: BAR,
            ability: 0,
        };
        assert!(moved_from_here(
            &ctx,
            &Event::Moved {
                card: fixtures::VI,
                from: Some(Location::Battlefield(fixtures::BF1)),
                to: Location::Base(0),
                cause: MoveCause::Effect,
                by: Some(0),
            },
            source
        ));
        assert!(!moved_from_here(
            &ctx,
            &Event::Moved {
                card: fixtures::VI,
                from: None,
                to: Location::Base(0),
                cause: MoveCause::Effect,
                by: Some(0),
            },
            source
        ));
        assert!(!moved_from_here(
            &ctx,
            &Event::Moved {
                card: fixtures::SPRITE,
                from: Some(Location::Battlefield(fixtures::BF2)),
                to: Location::Base(1),
                cause: MoveCause::Effect,
                by: Some(0),
            },
            source
        ));
    }

    #[test]
    fn an_enemy_unit_moved_from_here_by_an_effect_gets_the_bonus_too() {
        let mut fixture = bar_held_by_me();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let source = Source {
            card: BAR,
            ability: 0,
        };
        let event = Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Battlefield(fixtures::BF1)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0),
        };
        assert!(any_unit_moving_from_here(&ctx, &event, source));
        assert!(
            triggers::find(&ctx, &event)
                .iter()
                .any(|found| found.source == BAR),
            "Who::Any admits the mover regardless of side; the condition asks whether it left here"
        );
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            fixtures::THEIR_UNIT,
            Location::Base(1),
        );
        settle(&mut ctx).unwrap();
        resolve_top(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 3);
    }
}

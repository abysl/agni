use super::prelude::{done, on_move, score_point, unit, when};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;

pub const SCORING_MOVE: u8 = 3;

pub fn moves_this_turn(_ctx: &Ctx, _unit: u32) -> Option<u8> {
    None
}

fn the_third_move_this_turn(ctx: &Ctx, _: &Event, source: Source) -> bool {
    moves_this_turn(ctx, source.card) == Some(SCORING_MOVE)
}

fn ride_the_wind(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Yasuo - Windrider",
    &[Keyword::Ganking],
    &[when(on_move(&[], ride_the_wind), the_third_move_this_turn)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, settle};
    use agni_plugin_sdk::table::CardInfo;

    const YASUO: u32 = 90;

    fn yasuo(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(YASUO, zone, seat, "Yasuo - Windrider", 4)
        }
    }

    fn windswept(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(yasuo(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(YASUO, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    fn march_again(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        ctx.ready(YASUO);
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        march(fixture, to)
    }

    #[test]
    fn the_script_prints_ganking_and_a_move_trigger_conditioned_on_the_third_move() {
        assert!(std::ptr::eq(script_of("Yasuo - Windrider").unwrap(), &CARD));
        assert_eq!(CARD.name, "Yasuo - Windrider");
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let third = &CARD.abilities[0];
        assert_eq!(
            third.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(third.condition.is_some(), "the third time");
        assert!(!third.optional);
        assert!(third.targets.is_empty());
        assert!(third.cost.is_none());
        assert_eq!(SCORING_MOVE, 3);
    }

    #[test]
    fn a_first_and_second_move_score_nothing() {
        let mut fixture = windswept(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.location(YASUO),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.chain.is_empty(), "one move is not the third");
        assert_eq!(ctx.points(0), 0);
        drop(ctx);
        let ctx = march_again(&mut fixture, fixtures::BF2);
        assert_eq!(
            ctx.location(YASUO),
            Some(Location::Battlefield(fixtures::BF2)),
            "Ganking walks battlefield to battlefield"
        );
        assert!(ctx.blob.chain.is_empty(), "two moves are not the third");
        assert_eq!(ctx.points(0), 0);
    }

    #[test]
    fn the_seam_reports_no_count_until_the_engine_keeps_one() {
        let mut fixture = windswept(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(moves_this_turn(&ctx, YASUO), None);
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: moves per card this turn in CardState, reset at expiration, read by moves_this_turn"]
    fn the_third_move_in_a_turn_scores_a_point_and_the_fourth_does_not() {
        let mut fixture = windswept(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(moves_this_turn(&ctx, YASUO), Some(1));
        drop(ctx);
        let ctx = march_again(&mut fixture, fixtures::BF2);
        assert_eq!(moves_this_turn(&ctx, YASUO), Some(2));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut ctx = march_again(&mut fixture, fixtures::BF3);
        assert_eq!(moves_this_turn(&ctx, YASUO), Some(3));
        assert_eq!(ctx.blob.chain.len(), 1, "the third move triggers");
        assert_eq!(ctx.points(0), 0, "the point waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.points(0), 1);
        drop(ctx);
        let ctx = march_again(&mut fixture, fixtures::BASE);
        assert_eq!(moves_this_turn(&ctx, YASUO), Some(4));
        assert!(
            ctx.blob.chain.is_empty(),
            "the third time, not every time after"
        );
        assert_eq!(ctx.points(0), 1);
    }
}

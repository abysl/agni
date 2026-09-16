use super::emperors_divide::send_home;
use super::prelude::{card_targets, done, on_attack, optional, target, unit, with_statics};
use super::{Card, Filter, Flow, Item, Stage, Static, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const WITHIN: i32 = 3;
pub const MIGHT_AT_MOST: u8 = 5;
pub const ANY_NUMBER: u8 = u8::MAX;

pub const SMALL_ENEMY_UNITS_HERE: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Enemy,
        Filter::Here,
        Filter::MightAtMost(MIGHT_AT_MOST),
        Filter::MovableToBase,
    ]),
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of enemy units here with 5 Might or less to move to their base",
);

pub fn far_from_victory(ctx: &Ctx, seat: u8) -> bool {
    (ctx.victory_score() - ctx.points(seat)).abs() > WITHIN
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    far_from_victory(ctx, ctx.controller(me))
}

fn corrupt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let units = card_targets(ctx, item);
    if units.is_empty() {
        ctx.narrate(format!("{{card {me}}} moves no unit"));
        return done();
    }
    for unit in units {
        send_home(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Corrupted Dragon",
        &[],
        &[optional(on_attack(&[SMALL_ENEMY_UNITS_HERE], corrupt))],
    ),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const DRAGON: u32 = 90;
    const BRUTE: u32 = 91;
    const GIANT: u32 = 92;
    const ALLY: u32 = 93;
    const EXTRA_RUNES: [u32; 6] = [100, 101, 102, 103, 104, 105];

    fn dragon(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(10),
            power: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(DRAGON, zone, 0, "Corrupted Dragon", 10)
        }
    }

    fn lair(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dragon(zone));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(GIANT, fixtures::BF1, 1, "Giant", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 1));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DRAGON).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(DRAGON));
        settle(ctx).unwrap();
    }

    fn moves(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Moved {
                    card,
                    cause: MoveCause::Effect,
                    ..
                } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_unit_with_an_any_number_attack_trigger_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Corrupted Dragon").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional, "you may");
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets, &[SMALL_ENEMY_UNITS_HERE]);
        assert_eq!(
            (SMALL_ENEMY_UNITS_HERE.min, SMALL_ENEMY_UNITS_HERE.max),
            (0, ANY_NUMBER)
        );
        assert_eq!((WITHIN, MIGHT_AT_MOST), (3, 5));
    }

    #[test]
    fn the_score_is_far_from_victory_while_more_than_three_points_short_of_it() {
        let mut fixture = lair(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(ctx.options.victory_score, DEFAULT_VICTORY_SCORE);
        assert!(far_from_victory(&ctx, 0), "0 of 8");
        assert!(enters_ready(&ctx, DRAGON));
        drop(ctx);
        for (points, far) in [(4, true), (5, false), (7, false), (8, false)] {
            let mut fixture = lair(fixtures::HAND);
            fixture.set_points(0, points);
            let ctx = fixture.ctx();
            assert_eq!(
                far_from_victory(&ctx, 0),
                far,
                "{points} of 8 · within 3 means 5, 6, 7 or 8"
            );
            assert_eq!(enters_ready(&ctx, DRAGON), far);
            assert!(
                enters_ready(&ctx, fixtures::THEIR_UNIT),
                "the opponent's own score is read for their Dragon"
            );
        }
        let mut short = lair(fixtures::HAND);
        short.table.options = vec![("victory_score".into(), 6)];
        short.set_points(0, 3);
        let ctx = short.ctx();
        assert_eq!(ctx.options.victory_score, 6);
        assert!(!far_from_victory(&ctx, 0), "3 of 6 is within 3");
    }

    #[test]
    fn attacking_offers_the_enemy_units_here_of_five_or_less_and_the_picks_go_to_their_base() {
        let mut fixture = lair(fixtures::BF1);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "Jinx at 2 and the Brute at exactly 5 · not the Giant at 6, not the Ally"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), "done".to_string()],
            "a trigger's picks cannot be cancelled"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "every candidate picked · the prompt closes itself"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAGON
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Card(BRUTE)
            ]
        );
        assert!(moves(&ctx).is_empty(), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(moves(&ctx), [fixtures::THEIR_UNIT, BRUTE]);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(ctx.location(BRUTE), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(GIANT),
            Some(Location::Battlefield(fixtures::BF1)),
            "6 Might stays"
        );
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_moves_nothing_and_a_pick_that_grew_past_five_stays() {
        let mut fixture = lair(fixtures::BF1);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, []);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(moves(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DRAGON}}} moves no unit")));
        drop(ctx);

        let mut fixture = lair(fixtures::BF1);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let pump = ctx.blob.chain[0].clone();
        crate::cards::prelude::might_this_turn(&mut ctx, &pump, BRUTE, 1, None);
        assert_eq!(ctx.current_might(BRUTE), 6);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1)),
            "355 · the target is judged again as the trigger resolves"
        );
        assert!(moves(&ctx).is_empty());
    }

    #[test]
    fn played_from_hand_he_enters_ready_only_while_far_from_the_victory_score() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DRAGON));
        fixtures::play_from_hand(&mut ctx, 0, DRAGON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
        assert!(ctx.on_board(DRAGON));
        assert!(
            !ctx.card(DRAGON).unwrap().exhausted,
            "369.3 · 0 of 8 is not within 3 points, so he enters ready"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut close = lair(fixtures::HAND);
        close.set_points(0, 5);
        let mut ctx = close.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAGON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.card(DRAGON).unwrap().exhausted,
            "5 of 8 is within 3 points · the ordinary exhausted entry"
        );
    }
}

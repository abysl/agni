use super::prelude::{
    card_target, charm_destination, done, exhausting_self, gear, move_unit, optional, play, stun,
    target, trigger_subject, triggered, when, MoveCause, MOVABLE_ENEMY_UNIT,
};
use super::{Card, Filter, Flow, Item, Source, Stage, TargetKind, TargetSpec, Trigger, Where, Who};
use crate::engine::ctx::{Ctx, Event};

pub const SHOVE: u8 = 0;
pub const BLAST: u8 = 1;
const UNIT: usize = 0;
const DESTINATION: usize = 1;

pub const AN_ENEMY_UNIT_TO_MOVE: TargetSpec = target(
    MOVABLE_ENEMY_UNIT,
    0,
    1,
    TargetKind::Card,
    "an enemy unit to move",
);
pub const WHERE_IT_GOES: TargetSpec = target(
    Filter::DifferentLocationFrom(0),
    0,
    1,
    TargetKind::Zone,
    "where it goes",
);

pub const AN_ENEMY_UNIT_MOVES: Trigger = Trigger::Move {
    of: Who::Enemy,
    to: Where::Any,
};

pub fn an_enemy_unit_moved_by_an_effect(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Moved {
        card,
        cause: MoveCause::Effect,
        by,
        ..
    } = event
    else {
        return false;
    };
    let owner = ctx.controller(source.card);
    ctx.is_unit(*card) && ctx.controller(*card) != owner && *by == Some(owner)
}

fn shove(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    done()
}

pub fn blast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if !ctx.on_board(unit) {
        ctx.narrate(format!("{{card {unit}}} has left the board · no stun"));
        return done();
    }
    stun(ctx, unit);
    done()
}

pub static CARD: Card = gear(
    "Blast Cone",
    &[],
    &[
        optional(play(&[AN_ENEMY_UNIT_TO_MOVE, WHERE_IT_GOES], shove)),
        when(
            optional(exhausting_self(triggered(AN_ENEMY_UNIT_MOVES, &[], blast))),
            an_enemy_unit_moved_by_an_effect,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::ctx::{Location, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const CONE: u32 = 90;
    const CHAOS_RUNES: [u32; 2] = [46, 47];

    fn cone(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(CONE, zone, 0, CARD.name, 4)
        }
    }

    fn brush(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cone(zone, exhausted));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: CONE,
            ability: BLAST,
        }
    }

    fn moved(card: u32, cause: MoveCause) -> Event {
        Event::Moved {
            card,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(fixtures::BF1),
            cause,
            by: Some(0),
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_optional_play_move_and_an_optional_exhaust_stun_on_your_enemy_moves() {
        assert!(std::ptr::eq(script_of("Blast Cone").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let shove = &CARD.abilities[usize::from(SHOVE)];
        assert_eq!(shove.trigger, Trigger::Play);
        assert!(shove.optional, "you may move");
        assert_eq!(shove.targets.len(), 2);
        assert_eq!(shove.targets[UNIT].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(shove.targets[UNIT].min, 0, "the may is the skip");
        assert_eq!(shove.targets[DESTINATION].kind, TargetKind::Zone);
        assert_eq!(shove.targets[DESTINATION].min, 0);
        let blast = &CARD.abilities[usize::from(BLAST)];
        assert_eq!(
            blast.trigger,
            Trigger::Move {
                of: Who::Enemy,
                to: Where::Any
            }
        );
        assert!(blast.optional, "you may exhaust this");
        assert_eq!(blast.self_cost, SelfCost::Exhaust);
        assert!(blast.condition.is_some());
        assert!(blast.targets.is_empty());
    }

    #[test]
    fn playing_it_may_move_an_enemy_unit_and_the_skip_leaves_everyone_in_place() {
        let mut fixture = brush(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONE).unwrap();
        assert!(ctx.on_board(CONE));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                "skip".to_string()
            ],
            "enemy units, then the may"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Base(1)),
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(
            !ctx.is_stunned(fixtures::THEIR_UNIT),
            "the exhaust is still an open choice, so nothing has stunned it yet"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = brush(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CONE).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_condition_reads_an_enemy_unit_moved_by_an_effect_and_nothing_else() {
        let mut fixture = brush(fixtures::BASE, false);
        let ctx = fixture.ctx();
        assert!(an_enemy_unit_moved_by_an_effect(
            &ctx,
            &moved(fixtures::THEIR_UNIT, MoveCause::Effect),
            source()
        ));
        assert!(
            !an_enemy_unit_moved_by_an_effect(
                &ctx,
                &moved(fixtures::THEIR_UNIT, MoveCause::Standard),
                source()
            ),
            "their own march is not a move you make"
        );
        assert!(
            !an_enemy_unit_moved_by_an_effect(
                &ctx,
                &moved(fixtures::VI, MoveCause::Effect),
                source()
            ),
            "a friendly mover"
        );
        assert!(!an_enemy_unit_moved_by_an_effect(
            &ctx,
            &Event::Attacks {
                card: fixtures::THEIR_UNIT
            },
            source()
        ));
    }

    #[test]
    fn the_blast_stuns_the_moved_unit_and_skips_one_that_left_the_board() {
        let mut fixture = brush(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        let mut item = Item::new(
            7,
            ItemKind::Trigger {
                source: CONE,
                index: BLAST,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(crate::state::TargetRef::Card(fixtures::THEIR_UNIT));
        assert_eq!(blast(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is stunned", fixtures::THEIR_UNIT)));
        ctx.kill(fixtures::SPRITE, crate::engine::ctx::Cause::Rule);
        item.subject = Some(crate::state::TargetRef::Card(fixtures::SPRITE));
        assert_eq!(blast(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} has left the board · no stun",
            fixtures::SPRITE
        )));
    }

    #[test]
    fn moving_a_friendly_unit_with_an_effect_queues_nothing_for_the_cone() {
        let mut fixture = brush(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert!(
            !ctx.blob.chain.iter().any(|item| item.kind.source() == CONE),
            "a friendly mover is not an enemy unit"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.card(CONE).unwrap().exhausted);
    }

    #[test]
    fn moving_an_enemy_unit_asks_to_exhaust_the_cone_and_yes_stuns_it() {
        let mut fixture = brush(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(CONE).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
    }
}

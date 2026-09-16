use super::prelude::{done, draw, triggered, unit, when};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage, Trigger, Where, Who};
use crate::engine::ctx::{Ctx, Location};

pub const SHIELD: u8 = 3;
pub const DRAWS: usize = 1;

pub const A_MOVE_TO_A_BATTLEFIELD: Trigger = Trigger::Move {
    of: Who::Enemy,
    to: Where::Battlefield,
};

pub fn an_opponent_moved_to_another_battlefield(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Moved {
        card,
        to: Location::Battlefield(zone),
        ..
    } = event
    else {
        return false;
    };
    ctx.controller(*card) != ctx.controller(source.card)
        && ctx.location(source.card) != Some(Location::Battlefield(*zone))
}

fn thunderous_smash(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Volibear - Imposing",
    &[Keyword::Shield(SHIELD), Keyword::Tank],
    &[when(
        triggered(A_MOVE_TO_A_BATTLEFIELD, &[], thunderous_smash),
        an_opponent_moved_to_another_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const VOLIBEAR: u32 = 90;
    const THEIR_RAIDER: u32 = 91;

    fn volibear(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(12),
            power: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(VOLIBEAR, zone, seat, "Volibear - Imposing", 10)
        }
    }

    fn storm(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(volibear(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_RAIDER, fixtures::BASE, 1, "Raider", 2));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn effect_move(ctx: &mut Ctx, by: u8, unit: u32, to: Location) {
        ctx.actor = by;
        assert_eq!(ctx.move_unit(unit, to, MoveCause::Effect), Moved::Moved);
        settle(ctx).unwrap();
    }

    fn march(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(VOLIBEAR, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn the_script_prints_shield_three_and_tank_and_one_conditional_move_watcher() {
        assert!(std::ptr::eq(
            script_of("Volibear - Imposing").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Volibear - Imposing");
        assert_eq!(CARD.keywords, [Keyword::Shield(3), Keyword::Tank]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let watch = &CARD.abilities[0];
        assert_eq!(
            watch.trigger,
            Trigger::Move {
                of: Who::Enemy,
                to: Where::Battlefield
            }
        );
        assert!(watch.condition.is_some());
        assert!(!watch.optional);
        assert!(watch.targets.is_empty());
        assert!(watch.cost.is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_condition_reads_an_enemy_move_to_a_battlefield_other_than_his() {
        let mut fixture = storm(fixtures::BF1);
        let ctx = fixture.ctx();
        let source = Source {
            card: VOLIBEAR,
            ability: 0,
        };
        let to = |zone: u16, card: u32| Event::Moved {
            card,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(zone),
            cause: MoveCause::Standard,
            by: None,
        };
        assert!(an_opponent_moved_to_another_battlefield(
            &ctx,
            &to(fixtures::BF2, THEIR_RAIDER),
            source
        ));
        assert!(
            !an_opponent_moved_to_another_battlefield(
                &ctx,
                &to(fixtures::BF1, THEIR_RAIDER),
                source
            ),
            "his own battlefield"
        );
        assert!(
            !an_opponent_moved_to_another_battlefield(
                &ctx,
                &to(fixtures::BF2, fixtures::VI),
                source
            ),
            "a friendly mover"
        );
        assert!(
            !an_opponent_moved_to_another_battlefield(
                &ctx,
                &Event::Moved {
                    card: THEIR_RAIDER,
                    from: Some(Location::Battlefield(fixtures::BF2)),
                    to: Location::Base(1),
                    cause: MoveCause::Standard,
                    by: None,
                },
                source
            ),
            "bases are not battlefields"
        );
    }

    #[test]
    fn his_own_march_and_a_friendly_move_to_a_battlefield_draw_nothing() {
        let mut fixture = storm(fixtures::BASE);
        let ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.location(VOLIBEAR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.seat(0).draws, 0);
        drop(ctx);
        let mut fixture = storm(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        effect_move(
            &mut ctx,
            0,
            fixtures::VI,
            Location::Battlefield(fixtures::BF2),
        );
        assert!(ctx.blob.chain.is_empty(), "Vi is friendly");
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn an_enemy_move_to_his_own_battlefield_or_to_a_base_draws_nothing() {
        let mut fixture = storm(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        effect_move(
            &mut ctx,
            1,
            THEIR_RAIDER,
            Location::Battlefield(fixtures::BF1),
        );
        assert!(
            !ctx.blob
                .chain
                .iter()
                .any(|item| item.kind.source() == VOLIBEAR),
            "a move to his battlefield"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        let mut fixture = storm(fixtures::BF1);
        let mut ctx = fixture.ctx();
        effect_move(&mut ctx, 1, fixtures::SPRITE, Location::Base(1));
        assert!(ctx.blob.chain.is_empty(), "bases are not battlefields");
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn an_opponent_moving_to_another_battlefield_draws_him_one_when_the_trigger_resolves() {
        let mut fixture = storm(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        effect_move(
            &mut ctx,
            1,
            THEIR_RAIDER,
            Location::Battlefield(fixtures::BF2),
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VOLIBEAR
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0, "his controller draws");
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.blob.seat(1).draws, 0);
    }
}

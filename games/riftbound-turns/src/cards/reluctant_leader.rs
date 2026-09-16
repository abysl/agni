use super::prelude::{done, might_this_turn, on_you_play_card, unit, when};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub const MIGHT: i16 = 2;

pub fn another_unit(_: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played { card, kind, .. } if *card != source.card && kind == KIND_UNIT
    )
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Reluctant Leader",
    &[],
    &[when(on_you_play_card(&[], rally), another_unit)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, KIND_GEAR};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const LEADER: u32 = 90;
    const THEIR_RECRUIT: u32 = 91;

    fn leader(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(0),
            domain: vec!["Order".into()],
            ..fixtures::unit(LEADER, zone, 0, "Reluctant Leader", 3)
        }
    }

    fn camp() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(leader(fixtures::BASE));
        let mut recruit = fixtures::unit(THEIR_RECRUIT, fixtures::HAND, 1, "Recruit", 1);
        recruit.energy = Some(0);
        recruit.domain = vec!["Mind".into()];
        fixture.table.cards.push(recruit);
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(0);
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().energy = Some(0);
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_one_you_play_card_trigger_filtered_to_another_unit() {
        assert!(std::ptr::eq(script_of("Reluctant Leader").unwrap(), &CARD));
        assert_eq!(CARD.name, "Reluctant Leader");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(!ability.optional);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(MIGHT, 2);
        let mut fixture = camp();
        let ctx = fixture.ctx();
        let source = Source {
            card: LEADER,
            ability: 0,
        };
        let played = |card: u32, kind: &str| Event::Played {
            card,
            controller: 0,
            kind: kind.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert!(another_unit(
            &ctx,
            &played(fixtures::HAND_UNIT, KIND_UNIT),
            source
        ));
        assert!(
            !another_unit(&ctx, &played(LEADER, KIND_UNIT), source),
            "his own play is not another unit"
        );
        assert!(!another_unit(
            &ctx,
            &played(fixtures::HAND_GEAR, KIND_GEAR),
            source
        ));
    }

    #[test]
    fn playing_another_unit_gives_him_two_might_for_the_turn() {
        let mut fixture = camp();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEADER
        ));
        assert_eq!(
            ctx.current_might(LEADER),
            3,
            "the buff waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(LEADER), 5);
        assert_eq!(
            ctx.current_might(fixtures::HAND_UNIT),
            2,
            "the leader, not the recruit"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {LEADER}}} gets +2 Might this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(LEADER), 3, "it ends with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_of_yours_and_a_unit_of_theirs_leave_him_at_three() {
        let mut fixture = camp();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a gear is not a unit");
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.current_might(LEADER), 3);
        ctx.raise(Event::Played {
            card: THEIR_RECRUIT,
            controller: 1,
            kind: KIND_UNIT.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        });
        assert_eq!(
            crate::engine::triggers::collect(&mut ctx),
            0,
            "the opponent's unit is not one you play"
        );
        assert_eq!(ctx.current_might(LEADER), 3);
    }

    #[test]
    fn his_own_play_does_not_rally_him() {
        let mut fixture = camp();
        fixture.table.card_mut(LEADER).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(LEADER).unwrap().energy = Some(0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LEADER).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.current_might(LEADER), 3);
    }
}

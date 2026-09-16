use super::prelude::{done, draw, on_friendly_unit_dies, once_each_turn, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

pub fn echo(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Wraith of Echoes",
    &[],
    &[once_each_turn(on_friendly_unit_dies(&[], echo))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Once, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const WRAITH: u32 = 90;
    const ALLY: u32 = 91;
    const TRINKET: u32 = 92;

    fn wraith(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(WRAITH, zone, seat, "Wraith of Echoes", 5);
        card.energy = Some(6);
        card.power = Some(1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn haunted() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wraith(fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_the_pool_name_with_a_once_each_turn_friendly_death_trigger() {
        assert!(std::ptr::eq(script_of("Wraith of Echoes").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Friendly));
        assert_eq!(ability.once, Once::PerTurn);
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(DRAWS, 1);
        let fixture = haunted();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WRAITH).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_effect_draws_one_for_the_controller_of_the_wraith() {
        let mut fixture = haunted();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: WRAITH,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(echo(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
    }

    #[test]
    fn an_enemy_death_and_a_gear_death_queue_nothing_for_the_wraith() {
        let mut fixture = haunted();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "an enemy unit is not friendly");
        assert_eq!(ctx.kill(TRINKET, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Died { card, unit: false, .. } if *card == TRINKET
        )));
        assert!(ctx.blob.chain.is_empty(), "gear is not a unit");
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn the_first_friendly_death_each_turn_draws_one_and_the_second_draws_nothing() {
        let mut fixture = haunted();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(ALLY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "once each turn");
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }
}

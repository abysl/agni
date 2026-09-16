use super::prelude::{
    channel_exhausted, done, on_enemy_unit_dies, once_each_turn, unit, when, Location,
};
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;

pub fn died_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    match event {
        Event::Died {
            unit: true, noted, ..
        } => ctx.location(source.card) == Some(Location::Battlefield(noted.zone)),
        _ => false,
    }
}

fn hoard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, RUNES);
    done()
}

pub static CARD: Card = unit(
    "Nasus, Guardian of Knowledge",
    &[],
    &[once_each_turn(when(
        on_enemy_unit_dies(&[], hoard),
        died_here,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Once, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ItemKind, Noted, FLAG_ONCE_USED};
    use agni_plugin_sdk::table::CardInfo;

    const NASUS: u32 = 90;
    const PREY: u32 = 91;
    const MORE_PREY: u32 = 92;
    const FAR_PREY: u32 = 93;
    const ALLY: u32 = 94;

    fn nasus(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(NASUS, zone, 0, "Nasus, Guardian of Knowledge", 6)
        }
    }

    fn library(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(nasus(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(PREY, fixtures::BF1, 1, "Jinx", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(MORE_PREY, fixtures::BF1, 1, "Jinx", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(FAR_PREY, fixtures::BF2, 1, "Jinx", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(NASUS).unwrap(), &CARD));
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn dies(ctx: &mut Ctx, card: u32) -> usize {
        assert_eq!(ctx.kill(card, Cause::Item(9)), Killed::Yes);
        settle(ctx).unwrap();
        let queued = ctx
            .blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == NASUS))
            .count();
        resolve_all(ctx);
        queued
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> (usize, usize) {
        let pool: Vec<bool> = ctx
            .table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| rune.exhausted)
            .collect();
        (pool.len(), pool.iter().filter(|held| **held).count())
    }

    #[test]
    fn the_script_is_one_once_a_turn_enemy_death_trigger_conditioned_on_here() {
        assert!(std::ptr::eq(
            script_of("Nasus, Guardian of Knowledge").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Enemy));
        assert_eq!(ability.once, Once::PerTurn);
        assert!(ability.condition.is_some(), "here");
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert_eq!(RUNES, 1);
    }

    #[test]
    fn the_condition_reads_the_dead_units_noted_zone_against_his_battlefield() {
        let mut fixture = library(fixtures::BF1);
        let ctx = fixture.ctx();
        let source = Source {
            card: NASUS,
            ability: 0,
        };
        let died = |zone: u16, unit: bool| Event::Died {
            card: PREY,
            controller: 1,
            unit,
            noted: Noted {
                zone,
                might: 1,
                controller: 1,
                alone: false,
                buffed: false,
            },
        };
        assert!(died_here(&ctx, &died(fixtures::BF1, true), source));
        assert!(!died_here(&ctx, &died(fixtures::BF2, true), source));
        assert!(
            !died_here(&ctx, &died(fixtures::BF1, false), source),
            "a gear is not a unit"
        );
        assert!(!died_here(&ctx, &Event::Attacks { card: PREY }, source));
        drop(ctx);
        let mut home = library(fixtures::BASE);
        let ctx = home.ctx();
        assert!(
            !died_here(&ctx, &died(fixtures::BASE, true), source),
            "an enemy never dies in your base · his base is not a battlefield"
        );
    }

    #[test]
    fn the_first_enemy_death_here_each_turn_channels_one_rune_exhausted() {
        let mut fixture = library(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let (runes, exhausted) = pool_of(&ctx, 0);
        assert_eq!(dies(&mut ctx, PREY), 1);
        assert_eq!(
            pool_of(&ctx, 0),
            (runes + 1, exhausted + 1),
            "one more rune, exhausted"
        );
        assert!(ctx.has_flag(NASUS, FLAG_ONCE_USED));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(dies(&mut ctx, MORE_PREY), 0, "once each turn");
        assert_eq!(pool_of(&ctx, 0).0, runes + 1);
        assert_eq!(pool_of(&ctx, 1).0, 2, "the opponent channels nothing");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_death_elsewhere_or_a_friendly_death_here_leaves_the_once_mark_unspent() {
        let mut fixture = library(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let runes = pool_of(&ctx, 0).0;
        assert_eq!(
            dies(&mut ctx, FAR_PREY),
            0,
            "another battlefield is not here"
        );
        assert_eq!(
            dies(&mut ctx, ALLY),
            0,
            "an enemy unit, never a friendly one"
        );
        assert!(!ctx.has_flag(NASUS, FLAG_ONCE_USED));
        assert_eq!(pool_of(&ctx, 0).0, runes);
        assert_eq!(dies(&mut ctx, PREY), 1, "the mark was never spent");
        assert_eq!(pool_of(&ctx, 0).0, runes + 1);
        drop(ctx);
        let mut home = library(fixtures::BASE);
        let mut ctx = home.ctx();
        assert_eq!(dies(&mut ctx, PREY), 0, "from his base nothing is here");
        assert_eq!(pool_of(&ctx, 0).0, runes);
    }
}

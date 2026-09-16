use super::prelude::{a_unit, card_target, deal, done, play, unit};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const VICTIM: TargetSpec = a_unit("a unit to deal its marked damage to");

pub fn marked_on(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.damage_on(unit).max(0)).unwrap_or(u8::MAX)
}

fn retribution(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let marked = marked_on(ctx, unit);
    if marked == 0 {
        ctx.narrate(format!("{{card {unit}}} has no damage marked on it"));
        return done();
    }
    if deal(ctx, item, unit, marked) {
        ctx.narrate(format!(
            "{{card {}}} deals {marked} to {{card {unit}}} · its marked damage",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Morgana, Vindictive",
    &[Keyword::Ambush],
    &[play(&[VICTIM], retribution)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Event, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const MORGANA: u32 = 90;
    const WOUNDED: u32 = 91;
    const ALLY: u32 = 92;

    fn morgana(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(0),
            domain: vec!["Fury".into()],
            ..fixtures::unit(MORGANA, zone, 0, "Morgana, Vindictive", 5)
        }
    }

    fn court(wounded_might: u8, marked: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(morgana(fixtures::HAND));
        fixture.table.cards.push(fixtures::unit(
            WOUNDED,
            fixtures::BF1,
            1,
            "Brute",
            wounded_might,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(WOUNDED),
            counter: COUNTER_DAMAGE,
            value: marked,
        });
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx, to: Location) -> u16 {
        play_engine::begin(ctx, 0, MORGANA, Origin::Hand, Some(to)).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for a unit, not {other:?}"),
        }
    }

    #[test]
    fn the_script_prints_ambush_and_one_targeted_play_trigger() {
        assert!(std::ptr::eq(
            script_of("Morgana, Vindictive").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Morgana, Vindictive");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[VICTIM]);
        assert_eq!((VICTIM.min, VICTIM.max), (1, 1));
        assert_eq!(VICTIM.kind, TargetKind::Card);
        assert_eq!(VICTIM.filter, Filter::Unit);
        let mut fixture = court(5, 2);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(MORGANA, Keyword::Ambush));
        assert_eq!(
            ctx.ambush_locations(0, MORGANA),
            [Location::Battlefield(fixtures::BF1)],
            "she ambushes where you have units"
        );
        assert_eq!(marked_on(&ctx, WOUNDED), 2);
        assert_eq!(marked_on(&ctx, ALLY), 0);
        assert_eq!(marked_on(&ctx, 999), 0);
    }

    #[test]
    fn ambushed_in_she_deals_a_wounded_unit_its_own_marked_damage() {
        let mut fixture = court(5, 2);
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {WOUNDED}}}"),
                format!("{{card {ALLY}}}"),
                format!("{{card {MORGANA}}}"),
            ],
            "any unit, wounded or not"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {WOUNDED}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MORGANA
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(WOUNDED)]);
        let trigger = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.damage_on(WOUNDED),
            2,
            "the damage waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(WOUNDED), 4, "two marked, two more dealt");
        assert!(ctx.on_board(WOUNDED), "four of five is not lethal");
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: WOUNDED,
            n: 2,
            source: Cause::Item(trigger)
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MORGANA}}} deals 2 to {{card {WOUNDED}}} · its marked damage"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn doubling_the_marked_damage_past_its_might_kills_it() {
        let mut fixture = court(3, 2);
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        fixtures::choose(&mut ctx, 0, &format!("{{card {WOUNDED}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(WOUNDED), "four marked on a 3 Might unit");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == WOUNDED)));
        assert!(ctx.on_board(MORGANA));
        assert!(ctx.on_board(ALLY));
    }

    #[test]
    fn an_unwounded_pick_takes_nothing() {
        let mut fixture = court(5, 2);
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Base(0));
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(ALLY), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { card, .. } if *card == ALLY)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} has no damage marked on it")));
        assert_eq!(ctx.damage_on(WOUNDED), 2, "only the pick");
    }
}

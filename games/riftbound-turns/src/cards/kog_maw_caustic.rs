use super::prelude::{deal, deathknell, done, noted_of, unit, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;

fn battlefield_died_at(ctx: &Ctx, item: &Item) -> Option<u16> {
    let zone = noted_of(item)?.zone;
    ctx.zones.battlefields.contains(&zone).then_some(zone)
}

fn caustic_burst(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(zone) = battlefield_died_at(ctx, item) else {
        ctx.narrate(format!(
            "{{card {me}}} did not die at a battlefield · nothing to burst"
        ));
        return done();
    };
    let units = ctx.units_at(Location::Battlefield(zone));
    for unit in units {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Kog'Maw - Caustic",
    &[Keyword::Deathknell],
    &[deathknell(&[], caustic_burst)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::{ItemKind, Noted};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const KOGMAW: u32 = 90;
    const ALLY: u32 = 91;
    const RAIDER: u32 = 92;
    const BYSTANDER: u32 = 93;

    fn kogmaw(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(KOGMAW, zone, 0, "Kog'Maw - Caustic", 1);
        card.energy = Some(3);
        card.power = Some(1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn crowded(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kogmaw(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, zone, 0, "Ally", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, zone, 1, "Raider", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(BYSTANDER, fixtures::BF3, 1, "Bystander", 2));
        if zone == fixtures::BF1 {
            fixture.blob.set_holder(fixtures::BF1, Some(0));
            fixture.blob.set_contested(fixtures::BF1, Some(1));
        }
        fixture.resolve();
        fixture
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_untargeted_death_ability() {
        assert!(std::ptr::eq(script_of("Kog'Maw - Caustic").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Death);
        assert!(CARD.abilities[0].targets.is_empty(), "all units, no choice");
        assert!(!CARD.abilities[0].optional);
        assert_eq!(DAMAGE, 4);
        let fixture = crowded(fixtures::BF1);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KOGMAW).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn dying_at_a_battlefield_deals_four_to_every_unit_there_on_both_sides_off_the_noted_zone() {
        let mut fixture = crowded(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(KOGMAW, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KOGMAW
        ));
        assert_eq!(
            ctx.blob.chain[0].noted,
            Some(Noted {
                zone: fixtures::BF1,
                might: 1,
                controller: 0,
                alone: false,
                buffed: false
            })
        );
        assert_eq!(damage_of(&ctx, ALLY), 0, "the burst waits for the chain");
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            damage_of(&ctx, ALLY),
            i32::from(DAMAGE),
            "friendly units too"
        );
        assert_eq!(
            ctx.card(RAIDER).unwrap().zone,
            Some(fixtures::TRASH),
            "3 Might under 4 damage dies in the cleanup after the ability"
        );
        assert_eq!(damage_of(&ctx, BYSTANDER), 0, "another battlefield");
        assert_eq!(
            damage_of(&ctx, fixtures::VI),
            0,
            "the base is not a battlefield"
        );
        assert_eq!(damage_of(&ctx, fixtures::SPRITE), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n, .. } if *card == RAIDER && *n == DAMAGE
        )));
        assert!(ctx.blob.log.contains(&format!("{{card {RAIDER}}} takes 4")));
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} takes 4")));
        assert!(ctx.blob.log.contains(&format!("{{card {RAIDER}}} dies")));
        assert!(cleanup::dying(&ctx).is_empty(), "5 Might stands");
        assert!(ctx.on_board(ALLY));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn lethal_damage_in_combat_walks_the_same_burst_from_the_noted_battlefield() {
        let mut fixture = crowded(fixtures::BF1);
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(KOGMAW),
            counter: COUNTER_DAMAGE,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::dying(&ctx), [KOGMAW]);
        assert_eq!(cleanup::lethal_kills(&mut ctx), [KOGMAW]);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.zone),
            Some(fixtures::BF1)
        );
        resolve(&mut ctx);
        assert_eq!(damage_of(&ctx, ALLY), i32::from(DAMAGE));
        assert_eq!(ctx.card(RAIDER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.location(KOGMAW), None);
    }

    #[test]
    fn dying_in_the_base_bursts_nothing_because_a_base_is_not_a_battlefield() {
        let mut fixture = crowded(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(KOGMAW, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Deathknell still triggers");
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.zone),
            Some(fixtures::BASE)
        );
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [ALLY, RAIDER, BYSTANDER, fixtures::VI] {
            assert_eq!(damage_of(&ctx, unit), 0, "{{card {unit}}}");
        }
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KOGMAW}}} did not die at a battlefield · nothing to burst"
        )));
        assert!(ctx.fault.is_none());
    }
}

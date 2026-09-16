use super::prelude::{
    deal, done, enemy_units, equip, gear, location_of, on_attack, on_defend, while_attached,
    with_statics, RAINBOW,
};
use super::{Ability, Card, Cost, Flow, Grant, Item, Keyword, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = RAINBOW;

pub const MIGHT_BONUS: i16 = 3;
pub const DAMAGE: u8 = 2;
pub const ON_ATTACK: u8 = GRANTED;
pub const ON_DEFEND: u8 = GRANTED + 1;

pub fn enemies_with(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let Some(here) = location_of(ctx, item.kind.source()) else {
        return Vec::new();
    };
    enemy_units(ctx, item.controller)
        .into_iter()
        .filter(|unit| location_of(ctx, *unit) == Some(here))
        .collect()
}

fn blaze(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let enemies = enemies_with(ctx, item);
    if enemies.is_empty() {
        ctx.narrate(format!("{{card {me}}} finds no enemy unit here"));
        return done();
    }
    for unit in enemies {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static WEARER_TEXT: [Ability; 2] = [on_attack(&[], blaze), on_defend(&[], blaze)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Forgefire Cape", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, queue_granted, GEAR};
    use crate::cards::prelude::{attach_gear, detach_gear};
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::TargetRef;

    const BRUTE: u32 = 91;
    const AWAY: u32 = 92;
    const ALLY: u32 = 93;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut cape = equipment(GEAR, 0, "Forgefire Cape", 4, "Calm");
        cape.domain = vec!["Calm".into(), "Mind".into()];
        cape.power = Some(2);
        fixture.table.cards.push(cape);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Jinx", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_rainbow_equipment_with_three_might_and_untargeted_attack_and_defend_listeners(
    ) {
        assert!(std::ptr::eq(script_of("Forgefire Cape").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(RAINBOW)]);
        assert_eq!(CARD.abilities.len(), 1);
        let attack = &WEARER_TEXT[0];
        let defend = &WEARER_TEXT[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(defend.trigger, Trigger::Defends(Who::Me));
        assert!(attack.targets.is_empty() && defend.targets.is_empty());
        assert!(attack.condition.is_none() && defend.condition.is_none());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(3), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_attack_deals_two_to_every_enemy_unit_here_and_nothing_elsewhere() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_ATTACK,
            TargetRef::Card(fixtures::VI),
        );
        assert!(ctx.blob.prompt.is_none(), "no target to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "two damage kills the 2-Might enemy"
        );
        assert_eq!(ctx.damage_on(BRUTE), i32::from(DAMAGE));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(AWAY), 0, "an enemy in its base is not here");
        assert_eq!(ctx.damage_on(ALLY), 0, "friendly units are spared");
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wearers_defense_blazes_too_and_a_wearer_off_the_board_finds_nobody() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_DEFEND,
            TargetRef::Card(fixtures::VI),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), i32::from(DAMAGE));
        assert!(detach_gear(&mut ctx, GEAR));
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_DEFEND,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(BRUTE),
            2 * i32::from(DAMAGE),
            "383.3 · the wearer's trigger resolves after the cape comes loose"
        );
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_DEFEND,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(BRUTE),
            2 * i32::from(DAMAGE),
            "a dead wearer has no location"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} finds no enemy unit here".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(Event::Defends { card: fixtures::VI });
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the wearer's defense reaches its granted listener"
        );
    }

    #[test]
    fn an_attack_by_the_wearer_blazes_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(Event::Attacks { card: fixtures::VI });
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), i32::from(DAMAGE));
    }
}

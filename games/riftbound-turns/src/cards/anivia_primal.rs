use super::prelude::{deal, done, on_attack, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;

pub fn enemies_here(ctx: &Ctx, me: u32, seat: u8) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn glacial_storm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    let enemies = enemies_here(ctx, me, item.controller);
    if enemies.is_empty() {
        ctx.narrate(format!("{{card {me}}}: no enemy units here"));
        return done();
    }
    for unit in &enemies {
        deal(ctx, item, *unit, DAMAGE);
    }
    ctx.narrate(format!(
        "{{card {me}}} deals {DAMAGE} to each of {} enemy units here",
        enemies.len()
    ));
    done()
}

pub static CARD: Card = unit("Anivia - Primal", &[], &[on_attack(&[], glacial_storm)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::ItemKind;

    const ANIVIA: u32 = 90;
    const SECOND: u32 = 91;
    const FRIEND: u32 = 92;
    const AWAY: u32 = 93;

    fn tundra() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut anivia = fixtures::unit(ANIVIA, fixtures::BF1, 0, "Anivia - Primal", 8);
        anivia.domain = vec!["Body".into()];
        anivia.energy = Some(7);
        anivia.power = Some(2);
        fixture.table.cards.push(anivia);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(FRIEND, fixtures::BF1, 0, "Friend", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Away", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ANIVIA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: ANIVIA });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "all enemies here: nothing to choose"
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == ANIVIA
        ));
    }

    #[test]
    fn the_script_is_a_unit_with_one_untargeted_attack_trigger() {
        assert!(std::ptr::eq(script_of("Anivia - Primal").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn attacking_deals_three_to_every_enemy_here_and_nothing_to_friends_or_units_elsewhere() {
        let mut fixture = tundra();
        let mut ctx = fixture.ctx();
        assert_eq!(
            enemies_here(&ctx, ANIVIA, 0),
            [fixtures::THEIR_UNIT, SECOND]
        );
        attacks(&mut ctx);
        assert_eq!(ctx.damage_on(SECOND), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "3 on 2 Might is lethal at the cleanup after the trigger"
        );
        assert!(ctx.on_board(SECOND), "3 on 5 Might is not");
        assert_eq!(ctx.damage_on(SECOND), 3);
        assert_eq!(ctx.damage_on(FRIEND), 0);
        assert_eq!(ctx.damage_on(AWAY), 0);
        assert_eq!(ctx.damage_on(ANIVIA), 0);
        for unit in [fixtures::THEIR_UNIT, SECOND] {
            assert!(ctx.events.contains(&Event::DamageDealt {
                card: unit,
                n: DAMAGE,
                source: Cause::Item(1)
            }));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} deals 3 to each of 2 enemy units here".to_string()));
        assert!(ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_trigger_resolves_quietly_and_defending_never_fires_it() {
        let mut fixture = tundra();
        for unit in [fixtures::THEIR_UNIT, SECOND] {
            fixture.table.card_mut(unit).unwrap().zone = Some(fixtures::BASE);
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enemies_here(&ctx, ANIVIA, 0).is_empty());
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90}: no enemy units here".to_string()));
        ctx.raise(Event::Defends { card: ANIVIA });
        assert_eq!(triggers::collect(&mut ctx), 0);
    }
}

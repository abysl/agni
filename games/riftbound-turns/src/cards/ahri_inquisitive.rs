use super::prelude::{
    a_card, card_target, done, might_this_turn, on_attack, on_defend, unit, ENEMY_UNIT_HERE,
};
use super::{Ability, Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = -2;
pub const MINIMUM: i32 = 1;
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to give -2 Might");

fn charm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, Some(MINIMUM));
        ctx.narrate(format!(
            "{{card {unit}}} gets {MIGHT} might this turn · to a minimum of {MINIMUM}"
        ));
    }
    done()
}

const ON_ATTACK: Ability = on_attack(&[TARGET], charm);
const ON_DEFEND: Ability = on_defend(&[TARGET], charm);

pub static CARD: Card = unit("Ahri - Inquisitive", &[], &[ON_ATTACK, ON_DEFEND]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, phases, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::Target;

    const AHRI: u32 = 90;
    const RUNT: u32 = 91;

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn shrine() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut ahri = fixtures::unit(AHRI, fixtures::BF1, 0, "Ahri - Inquisitive", 3);
        ahri.domain = vec!["Mind".into()];
        ahri.energy = Some(3);
        ahri.power = Some(1);
        fixture.table.cards.push(ahri);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BF1, 1, "Runt", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AHRI).unwrap(), &CARD));
        fixture
    }

    fn fires(ctx: &mut Ctx, event: Event, index: u8) {
        ctx.raise(event);
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {RUNT}}}")
            ]
        );
        fixtures::choose(ctx, 0, &format!("{{card {RUNT}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: held } if source == AHRI && held == index
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(RUNT)]);
    }

    #[test]
    fn the_script_is_a_unit_with_an_attack_and_a_defend_trigger_each_aimed_at_an_enemy_here() {
        assert!(std::ptr::eq(
            script_of("Ahri - Inquisitive").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Attacks(Who::Me));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Defends(Who::Me));
        for ability in CARD.abilities {
            assert_eq!(ability.targets, &[TARGET]);
            assert!(!ability.optional);
            assert!(ability.condition.is_none());
        }
    }

    #[test]
    fn attacking_charms_the_chosen_enemy_down_to_the_minimum_of_one_for_the_turn() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        fires(&mut ctx, Event::Attacks { card: AHRI }, 0);
        assert_eq!(ctx.current_might(RUNT), 2, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RUNT), 1, "454.3.b · -2 on 2 stops at 1");
        assert_eq!(might_counter(&ctx, RUNT), -1);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 91} gets -2 might this turn · to a minimum of 1".to_string()));
        let table = ctx.table.clone();
        let mut blob = ctx.blob.clone();
        drop(ctx);
        blob.prompt = None;
        blob.why = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(RUNT), 2, "the charm is for the turn");
        assert_eq!(might_counter(&ctx, RUNT), 0);
    }

    #[test]
    fn defending_fires_the_second_ability_and_a_big_enemy_loses_the_full_two() {
        let mut fixture = shrine();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Defends { card: AHRI });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == AHRI
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 3);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -2);
        assert_eq!(ctx.current_might(RUNT), 2);
    }

    #[test]
    fn with_no_enemy_here_both_triggers_fizzle_and_a_target_that_left_gets_nothing() {
        let mut fixture = shrine();
        for unit in [fixtures::THEIR_UNIT, RUNT] {
            fixture.table.card_mut(unit).unwrap().zone = Some(fixtures::BASE);
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        for event in [Event::Attacks { card: AHRI }, Event::Defends { card: AHRI }] {
            ctx.raise(event);
            assert_eq!(triggers::collect(&mut ctx), 1);
            chain::proceed(&mut ctx);
            assert!(ctx.blob.prompt.is_none());
            assert!(ctx.blob.chain.is_empty());
        }
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{card 90} trigger fizzles · no legal target")
                .count(),
            2
        );
        drop(ctx);

        let mut fled = shrine();
        let mut ctx = fled.ctx();
        fires(&mut ctx, Event::Attacks { card: AHRI }, 0);
        ctx.table.card_mut(RUNT).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(RUNT),
            2,
            "gone from here, the charm misses"
        );
        assert_eq!(might_counter(&ctx, RUNT), 0);
    }
}

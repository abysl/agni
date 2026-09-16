use super::prelude::{a_card, card_target, deal, done, on_attack, unit, ENEMY_UNIT_HERE};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 1;
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal 1 to");

fn crackshot(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        deal(ctx, item, unit, DAMAGE);
    }
    done()
}

pub static CARD: Card = unit("Crackshot Corsair", &[], &[on_attack(&[TARGET], crackshot)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, cleanup, play, priority, showdown, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const CORSAIR: u32 = 90;
    const AWAY: u32 = 91;

    fn boarding() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut corsair = fixtures::unit(CORSAIR, fixtures::BF1, 0, "Crackshot Corsair", 3);
        corsair.domain = vec!["Body".into()];
        corsair.energy = Some(3);
        fixture.table.cards.push(corsair);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CORSAIR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: CORSAIR });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    #[test]
    fn the_script_is_a_unit_with_one_attack_trigger_aimed_at_an_enemy_here() {
        assert!(std::ptr::eq(script_of("Crackshot Corsair").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!((TARGET.min, TARGET.max), (1, 1));
        assert!(!ability.optional);
    }

    #[test]
    fn attacking_asks_for_an_enemy_here_and_deals_one_to_it_when_the_trigger_resolves() {
        let mut fixture = boarding();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "only the enemy at the corsair's battlefield is offered"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[AWAY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy in its base is not here"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CORSAIR
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 1);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(AWAY), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_trigger_fizzles_and_a_defender_never_fires_it() {
        let mut fixture = boarding();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        ctx.raise(Event::Defends { card: CORSAIR });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "the corsair shoots only on the attack"
        );
    }

    #[test]
    fn in_a_real_combat_the_shot_lands_before_the_damage_step_and_finishes_the_defender() {
        let mut fixture = boarding();
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(CORSAIR));
        assert!(
            ctx.blob.prompt.is_none(),
            "the lone defender is the only legal target, so settle picks it"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 1);
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            if ctx.blob.prompt.is_some() {
                break;
            }
            if !ctx.blob.chain.is_empty() {
                let Some(holder) = priority::holder(&ctx) else {
                    break;
                };
                priority::pass(&mut ctx, holder).unwrap();
                continue;
            }
            showdown::pass(&mut ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none(), "the combat closed");
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "one from the shot and three in combat kill the 2-Might defender"
        );
        assert!(ctx.on_board(CORSAIR));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.fault.is_none());
    }
}

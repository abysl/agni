use super::prelude::{a_card, card_target, deal, done, on_attack, unit, ENEMY_UNIT_HERE};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal my Might to");

fn steel_tempest(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    let might = u8::try_from(ctx.current_might(me).max(0)).unwrap_or(u8::MAX);
    if deal(ctx, item, unit, might) {
        ctx.narrate(format!("{{card {me}}} deals {might} to {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Yasuo - Remorseful",
    &[],
    &[on_attack(&[TARGET], steel_tempest)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const YASUO: u32 = 90;
    const BRUTE: u32 = 91;

    fn wind() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut yasuo = fixtures::unit(YASUO, fixtures::BF1, 0, "Yasuo - Remorseful", 6);
        yasuo.domain = vec!["Calm".into()];
        yasuo.energy = Some(6);
        yasuo.power = Some(2);
        fixture.table.cards.push(yasuo);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 8));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(YASUO).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: YASUO });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
    }

    #[test]
    fn the_script_is_a_unit_with_one_attack_trigger_aimed_at_an_enemy_here() {
        assert!(std::ptr::eq(
            script_of("Yasuo - Remorseful").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(ability.targets, &[TARGET]);
        assert!(!ability.optional);
    }

    #[test]
    fn the_damage_is_his_might_as_the_trigger_resolves_not_as_it_triggers() {
        let mut fixture = wind();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}")
            ],
            "both enemies here are offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == YASUO
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        let pump = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &pump, YASUO, 2, None);
        assert_eq!(ctx.current_might(YASUO), 8);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 8,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {YASUO}}} deals 8 to {{card {BRUTE}}}")));
        assert!(
            !ctx.on_board(BRUTE),
            "8 with the pump, not the printed 6: lethal on 8 Might at the cleanup after the trigger"
        );
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_leaves_or_a_yasuo_that_leaves_deals_nothing() {
        let mut fixture = wind();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(BRUTE),
            0,
            "an enemy that fled the battlefield is no longer here"
        );
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut gone = wind();
        let mut ctx = gone.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table.card_mut(YASUO).unwrap().zone = Some(fixtures::TRASH);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), 0, "no Yasuo, no Might to deal");
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
    }
}

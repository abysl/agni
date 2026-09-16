use super::prelude::{a_card, card_target, deal, done, on_attack, unit, ENEMY_UNIT_HERE};
use super::{Card, Flow, Grant, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const ASSAULT: u8 = 1;
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal my Assault to");

pub fn assault_of(ctx: &Ctx, card: u32) -> u8 {
    let pick = |keyword: Keyword| match keyword {
        Keyword::Assault(n) => Some(n),
        _ => None,
    };
    let printed = ctx
        .script(card)
        .map(|script| {
            script
                .keywords
                .iter()
                .filter_map(|keyword| pick(*keyword))
                .fold(0u8, u8::saturating_add)
        })
        .unwrap_or(0);
    let granted = ctx
        .state_of(card)
        .map(|row| {
            row.granted
                .iter()
                .filter_map(|(keyword, _)| pick(*keyword))
                .fold(0u8, u8::saturating_add)
        })
        .unwrap_or(0);
    let projected = statics::grants_on(ctx, card)
        .into_iter()
        .filter_map(|grant| match grant {
            Grant::Keyword(keyword) => pick(keyword),
            _ => None,
        })
        .fold(0u8, u8::saturating_add);
    printed.saturating_add(granted).saturating_add(projected)
}

fn piercing_light(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    let amount = assault_of(ctx, me);
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {me}}} deals {amount} to {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Lucian - Gunslinger",
    &[Keyword::Assault(ASSAULT)],
    &[on_attack(&[TARGET], piercing_light)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, play, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const LUCIAN: u32 = 90;
    const AWAY: u32 = 91;

    fn sentinel() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut lucian = fixtures::unit(LUCIAN, fixtures::BF1, 0, "Lucian - Gunslinger", 2);
        lucian.domain = vec!["Fury".into()];
        lucian.energy = Some(3);
        fixture.table.cards.push(lucian);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(3);
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Away", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LUCIAN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.mark_attacker(LUCIAN);
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    #[test]
    fn the_script_prints_assault_one_and_one_attack_trigger_aimed_at_an_enemy_here() {
        assert!(std::ptr::eq(
            script_of("Lucian - Gunslinger").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Assault(ASSAULT)]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(ability.targets, &[TARGET]);
        assert!(!ability.optional);
    }

    #[test]
    fn his_assault_is_the_printed_one_plus_whatever_is_granted() {
        let mut fixture = sentinel();
        let mut ctx = fixture.ctx();
        assert_eq!(assault_of(&ctx, LUCIAN), 1);
        assert_eq!(assault_of(&ctx, fixtures::VI), 0);
        let until = this_turn(&ctx);
        assert!(ctx.grant(LUCIAN, Keyword::Assault(2), until));
        assert_eq!(assault_of(&ctx, LUCIAN), 3);
        ctx.expire(until);
        assert_eq!(assault_of(&ctx, LUCIAN), 1);
    }

    #[test]
    fn attacking_asks_for_an_enemy_here_and_deals_his_assault_as_it_stands_when_the_trigger_resolves(
    ) {
        let mut fixture = sentinel();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "only the enemy at his battlefield"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[AWAY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LUCIAN
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        let until = this_turn(&ctx);
        assert!(ctx.grant(LUCIAN, Keyword::Assault(2), until));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "3 from the granted Assault, not the printed 1: lethal on 3 Might"
        );
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: 3,
            source: Cause::Item(1)
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LUCIAN}}} deals 3 to {{card {}}}",
            fixtures::THEIR_UNIT
        )));
        assert_eq!(ctx.damage_on(AWAY), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_trigger_fizzles_and_defending_never_fires_it() {
        let mut fixture = sentinel();
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
        ctx.raise(Event::Defends { card: LUCIAN });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "he shoots only on the attack"
        );
    }
}

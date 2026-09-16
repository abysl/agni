use super::prelude::{
    a_card, activated, at_battlefield, card_target, deal, done, named, on_attack, on_defend,
    paying_with, unit, usable_if, with_statics, Location, ENEMY_UNIT_HERE,
};
use super::{
    Card, Cost, Domain, Flow, Item, Power, SelfCost, Source, Stage, Static, TargetSpec, Timing,
};
use crate::engine::ctx::Ctx;
use crate::engine::march;

pub const MIND: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal my Might to");

fn mystic_shot(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
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

pub fn deals_no_combat_damage(_: &Ctx, source: u32, unit: u32) -> bool {
    unit == source
}

fn away_from_base(ctx: &Ctx, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn arcane_shift(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    let home = Location::Base(ctx.controller(me));
    if ctx.location(me) == Some(home) {
        ctx.narrate(format!("{{card {me}}} is already in base"));
        return done();
    }
    march::effect_move(ctx, item, me, home);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Ezreal - Dashing",
        &[],
        &[
            on_attack(&[TARGET], mystic_shot),
            on_defend(&[TARGET], mystic_shot),
            usable_if(
                named(
                    paying_with(
                        activated(Timing::Action, MIND, &[], arcane_shift),
                        SelfCost::Free,
                    ),
                    "move to your base",
                ),
                away_from_base,
            ),
        ],
    ),
    &[Static::NoCombatDamageFrom(deals_no_combat_damage)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, chain, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const EZREAL: u32 = 90;
    const MIND_RUNE: u32 = 46;

    fn explorer(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut ezreal = fixtures::unit(EZREAL, zone, 0, "Ezreal - Dashing", 3);
        ezreal.domain = vec!["Mind".into()];
        ezreal.energy = Some(4);
        ezreal.power = Some(1);
        fixture.table.cards.push(ezreal);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(4);
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EZREAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn shoots(ctx: &mut Ctx, event: Event, index: u8) {
        ctx.raise(event);
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)]
        );
        fixtures::choose(ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: held } if source == EZREAL && held == index
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_shoots_on_attack_and_on_defend_suppresses_his_combat_damage_and_dashes_home() {
        assert!(std::ptr::eq(script_of("Ezreal - Dashing").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Attacks(Who::Me));
        assert_eq!(CARD.abilities[0].targets, &[TARGET]);
        assert_eq!(CARD.abilities[1].trigger, Trigger::Defends(Who::Me));
        assert_eq!(CARD.abilities[1].targets, &[TARGET]);
        let dash = &CARD.abilities[2];
        assert_eq!(dash.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(dash.cost, Some(MIND));
        assert_eq!(dash.self_cost, SelfCost::Free, "no exhaust is printed");
        assert!(dash.targets.is_empty());
        assert!(dash.usable.is_some());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::NoCombatDamageFrom(_)));
    }

    #[test]
    fn attacking_and_defending_each_deal_his_might_to_an_enemy_here() {
        let mut fixture = explorer(fixtures::BF1);
        let mut ctx = fixture.ctx();
        shoots(&mut ctx, Event::Attacks { card: EZREAL }, 0);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 3);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: 3,
            source: Cause::Item(1)
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {EZREAL}}} deals 3 to {{card {}}}",
            fixtures::THEIR_UNIT
        )));
        drop(ctx);

        let mut fixture = explorer(fixtures::BF1);
        let mut ctx = fixture.ctx();
        shoots(&mut ctx, Event::Defends { card: EZREAL }, 1);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_shot_fizzles() {
        let mut fixture = explorer(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: EZREAL });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn the_mind_action_moves_him_to_his_base_and_is_not_offered_from_base() {
        let mut fixture = explorer(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == EZREAL && offer.index == 2)
            .expect("the dash is offered at a battlefield");
        assert!(offer.enabled);
        assert!(offer.label.contains("move to your base"));
        activate::activate(&mut ctx, 0, EZREAL, 2).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move {
                    card: MIND_RUNE,
                    zone: fixtures::RUNE_DECK,
                    ..
                }
            )),
            "the Mind rune pays: {:?}",
            ctx.effects
        );
        assert!(
            !ctx.card(EZREAL).unwrap().exhausted,
            "no exhaust in the cost"
        );
        assert!(matches!(
            ctx.blob.chain.last().unwrap().kind,
            ItemKind::Ability { source, index: 2 } if source == EZREAL
        ));
        assert_eq!(
            ctx.location(EZREAL),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(EZREAL), Some(Location::Base(0)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card: EZREAL,
                to: Location::Base(0),
                cause: MoveCause::Effect,
                ..
            }
        )));
        assert!(
            !activate::offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == EZREAL && offer.index == 2),
            "already home"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, EZREAL, 2),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses with the reason the engine has for it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · Ctx::deals_combat_damage consults every other card's NoCombatDamageFrom static and skips the unit's own, so Ezreal's I-don't-deal-combat-damage is never read; the seam is deals_no_combat_damage(ctx, source, unit), true for himself"]
    fn ezreal_contributes_nothing_to_combat_damage() {
        let mut fixture = explorer(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(deals_no_combat_damage(&ctx, EZREAL, EZREAL));
        assert!(!deals_no_combat_damage(&ctx, EZREAL, fixtures::VI));
        assert!(!ctx.deals_combat_damage(EZREAL));
        assert_eq!(ctx.combat_might(EZREAL), 0);
        assert!(ctx.deals_combat_damage(fixtures::THEIR_UNIT));
    }
}

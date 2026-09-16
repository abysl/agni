use super::prelude::{
    a_card, card_target, done, on_defend, optional, paying_with, unit, Location, ATTACKING_UNIT,
};
use super::{Card, Flow, Item, SelfCost, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march;

pub const AN_ATTACKER: TargetSpec = a_card(ATTACKING_UNIT, "an attacking unit to send to its base");

fn tackle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(attacker) = card_target(ctx, item, 0) else {
        return done();
    };
    let home = Location::Base(ctx.controller(attacker));
    march::effect_move(ctx, item, attacker, home);
    done()
}

pub static CARD: Card = unit(
    "Overzealous Fan",
    &[],
    &[optional(paying_with(
        on_defend(&[AN_ATTACKER], tackle),
        SelfCost::KillSelf,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{cleanup, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const FAN: u32 = 90;
    const SECOND_ATTACKER: u32 = 91;

    fn stands() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fan = fixtures::unit(FAN, fixtures::BF1, 0, "Overzealous Fan", 2);
        fan.domain = vec!["Chaos".into()];
        fan.energy = Some(2);
        fixture.table.cards.push(fan);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FAN).unwrap(), &CARD));
        fixture
    }

    fn they_attack(ctx: &mut Ctx, attackers: &[u32]) {
        ctx.actor = 1;
        for attacker in attackers {
            ctx.move_unit(
                *attacker,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect,
            );
        }
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_defender(FAN));
    }

    #[test]
    fn the_script_is_a_unit_whose_optional_defend_trigger_costs_its_life_and_names_an_attacker() {
        assert!(std::ptr::eq(script_of("Overzealous Fan").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Defends(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.self_cost, SelfCost::KillSelf);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets, &[AN_ATTACKER]);
        assert_eq!((AN_ATTACKER.min, AN_ATTACKER.max), (1, 1));
    }

    #[test]
    fn defending_picks_the_attacker_asks_to_kill_the_fan_and_the_attacker_walks_home() {
        let mut fixture = stands();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx, &[fixtures::THEIR_UNIT]);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            }),
            "the lone attacker is the only legal target, so the may is the kill confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "kill {card 90} for the {card 90} trigger?"
        );
        assert!(ctx.on_board(FAN));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            !ctx.on_board(FAN),
            "204.3.a · the kill is the base cost, paid to finalize"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died {
                card: FAN,
                controller: 0,
                unit: true,
                ..
            }
        )));
        let trigger = ctx
            .blob
            .chain
            .iter()
            .find(
                |held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == FAN),
            )
            .expect("the trigger is on the chain");
        assert_eq!(trigger.targets, [TargetRef::Card(fixtures::THEIR_UNIT)]);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing moves before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card: fixtures::THEIR_UNIT,
                to: Location::Base(1),
                cause: MoveCause::Effect,
                ..
            }
        )));
        assert!(
            !ctx.is_attacker(fixtures::THEIR_UNIT),
            "gone from the battlefield, no longer an attacker"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn two_attackers_are_offered_and_only_the_pick_goes_home() {
        let mut fixture = stands();
        fixture.table.cards.push(fixtures::unit(
            SECOND_ATTACKER,
            fixtures::BASE,
            1,
            "Brute",
            4,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx, &[fixtures::THEIR_UNIT, SECOND_ATTACKER]);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {SECOND_ATTACKER}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND_ATTACKER}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(SECOND_ATTACKER), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!ctx.on_board(FAN));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_keeps_the_fan_alive_and_moves_nothing_and_a_fan_already_dead_cannot_pay() {
        let mut fixture = stands();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx, &[fixtures::THEIR_UNIT]);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.on_board(FAN));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|held| matches!(held.kind, ItemKind::Trigger { source, .. } if source == FAN)));
        drop(ctx);

        let mut fixture = stands();
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(&mut ctx, None);
        ctx.kill(FAN, Cause::Item(0));
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no kill to confirm for a dead fan"
        );
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|held| matches!(held.kind, ItemKind::Trigger { source, .. } if source == FAN)));
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}

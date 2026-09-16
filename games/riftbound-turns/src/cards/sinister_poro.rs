use super::prelude::{
    a_card, card_target, done, move_destinations, move_unit, on_attack, optional, unit, with_cost,
    Location, ENEMY_UNIT_HERE, ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to move to its base");

fn shoo(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(enemy) = card_target(ctx, item, 0) else {
        return done();
    };
    let home = Location::Base(ctx.controller(enemy));
    if move_destinations(ctx, enemy).contains(&home) {
        move_unit(ctx, item, enemy, home);
    } else {
        ctx.narrate(format!("{{card {enemy}}} can't move to its base"));
    }
    done()
}

pub static CARD: Card = unit(
    "Sinister Poro",
    &[],
    &[optional(with_cost(on_attack(&[TARGET], shoo), ONE_ENERGY))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::equipment;
    use crate::cards::prelude::{attach_gear, battlefield, with_statics};
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, prompts, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const PORO: u32 = 90;
    const BRUTE: u32 = 91;
    const AWAY: u32 = 92;

    static NO_RETREAT: Card = with_statics(
        battlefield("Proving Grounds", &[], &[]),
        &[Static::NoMoveToBase],
    );

    fn isles() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut poro = fixtures::unit(PORO, fixtures::BF1, 0, "Sinister Poro", 1);
        poro.domain = vec!["Chaos".into()];
        poro.power = Some(1);
        fixture.table.cards.push(poro);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BF2, 1, "Away", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: PORO });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    #[test]
    fn the_script_is_a_unit_with_one_optional_energy_costed_attack_trigger_aimed_at_an_enemy_here()
    {
        assert!(std::ptr::eq(script_of("Sinister Poro").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!(TARGET.filter, ENEMY_UNIT_HERE);
    }

    #[test]
    fn attacking_picks_an_enemy_here_then_asks_for_the_energy_and_paying_it_sends_the_enemy_home() {
        let mut fixture = isles();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}")],
            "the enemy at the other battlefield and the Poro itself are not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "pay 1 energy for the {card 90} trigger?"
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1, "one rune pays");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PORO
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(BRUTE), Some(Location::Base(1)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Base(1), cause: MoveCause::Effect, .. } if *card == BRUTE
        )));
        assert_eq!(
            ctx.location(AWAY),
            Some(Location::Battlefield(fixtures::BF2)),
            "only the chosen enemy moves"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_or_lacking_it_removes_the_trigger_and_no_enemy_here_fizzles_it() {
        let mut fixture = isles();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        drop(ctx);

        let mut poor = isles();
        for rune in [41, 42, 43] {
            poor.table.card_mut(rune).unwrap().exhausted = true;
        }
        poor.resolve();
        let mut ctx = poor.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        drop(ctx);

        let mut alone = isles();
        alone.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        alone.resolve();
        let mut ctx = alone.ctx();
        attacks(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
    }

    #[test]
    fn an_enemy_that_cannot_move_stays_and_the_energy_is_spent_all_the_same() {
        let mut fixture = isles();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_RETREAT);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} can't move to its base")));
    }

    #[test]
    fn a_jagged_cutlass_wearer_is_offered_but_the_shoo_can_not_move_it() {
        const CUTLASS: u32 = 95;
        let mut fixture = isles();
        fixture
            .table
            .cards
            .push(equipment(CUTLASS, 1, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, CUTLASS, BRUTE);
        attacks(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}")],
            "the wearer is a legal choice, just not a movable one"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "the energy is spent"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1)),
            "an enemy ability can't move the wearer"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BRUTE}}} can't be moved by {{card {PORO}}}"
        )));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert!(ctx.fault.is_none());
    }
}

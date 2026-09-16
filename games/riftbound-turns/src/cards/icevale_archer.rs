use super::prelude::{
    a_card, card_target, done, might_this_turn, on_attack, optional, unit, with_cost, ONE_ENERGY,
    UNIT_HERE,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const PENALTY: i16 = -1;
pub const TARGET: TargetSpec = a_card(UNIT_HERE, "a unit here to give -1 Might this turn");

fn frost_arrow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    might_this_turn(ctx, item, unit, PENALTY, None);
    ctx.narrate(format!("{{card {unit}}} gets {PENALTY} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Icevale Archer",
    &[],
    &[optional(with_cost(
        on_attack(&[TARGET], frost_arrow),
        ONE_ENERGY,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, prompts, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const ARCHER: u32 = 90;
    const BRUTE: u32 = 91;

    fn ridge() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut archer = fixtures::unit(ARCHER, fixtures::BF1, 0, "Icevale Archer", 2);
        archer.domain = vec!["Mind".into()];
        fixture.table.cards.push(archer);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ARCHER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: ARCHER });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    #[test]
    fn the_script_is_a_unit_with_one_optional_energy_costed_attack_trigger_aimed_at_a_unit_here() {
        assert!(std::ptr::eq(script_of("Icevale Archer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!(TARGET.filter, UNIT_HERE);
        assert_eq!(PENALTY, -1);
    }

    #[test]
    fn attacking_picks_a_unit_here_then_asks_for_the_energy_and_paying_it_shrinks_the_unit_for_the_turn(
    ) {
        let mut fixture = ridge();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [format!("{{card {ARCHER}}}"), format!("{{card {BRUTE}}}")],
            "every unit here, herself included; Vi in the base is not here"
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
            ItemKind::Trigger { source, index: 0 } if source == ARCHER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(ctx.current_might(BRUTE), 4, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert_eq!(ctx.current_might(ARCHER), 2);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} gets -1 Might this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(
            ctx.current_might(BRUTE),
            4,
            "the penalty ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_or_lacking_it_removes_the_trigger_and_defending_never_fires_it() {
        let mut fixture = ridge();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "nothing is paid");
        assert_eq!(ctx.current_might(BRUTE), 4);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        drop(ctx);

        let mut poor = ridge();
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
        assert_eq!(ctx.current_might(BRUTE), 4);
        ctx.raise(Event::Defends { card: ARCHER });
        assert_eq!(triggers::collect(&mut ctx), 0, "defending is not attacking");
    }

    #[test]
    fn a_unit_that_left_the_battlefield_before_the_trigger_resolves_is_untouched() {
        let mut fixture = ridge();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        ctx.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(BRUTE),
            4,
            "an enemy that fled is no longer here"
        );
        assert!(!ctx.blob.log.iter().any(|line| line.contains("gets -1")));
    }
}

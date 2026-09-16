use super::prelude::{
    a_card, card_target, done, grant_this_turn, might_this_turn, on_attack, on_defend, unit,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 3;
pub const ANOTHER_FRIENDLY_UNIT_HERE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Here,
    Filter::NotSelf,
]);
pub const TARGET: TargetSpec = a_card(
    ANOTHER_FRIENDLY_UNIT_HERE,
    "one of your other units here to give +3 Might and Tank this turn",
);

fn zoomies(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    might_this_turn(ctx, item, unit, BONUS, None);
    grant_this_turn(ctx, unit, Keyword::Tank);
    ctx.narrate(format!(
        "{{card {unit}}} gets +{BONUS} Might and Tank this turn"
    ));
    done()
}

pub static CARD: Card = unit(
    "Yuumi - Magical Cat",
    &[],
    &[on_attack(&[TARGET], zoomies), on_defend(&[TARGET], zoomies)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, combat, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const YUUMI: u32 = 90;
    const BOOK: u32 = 91;
    const BRUTE: u32 = 92;

    fn lap() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut yuumi = fixtures::unit(YUUMI, fixtures::BF1, 0, "Yuumi - Magical Cat", 1);
        yuumi.domain = vec!["Calm".into()];
        yuumi.energy = Some(3);
        yuumi.power = Some(1);
        fixture.table.cards.push(yuumi);
        fixture
            .table
            .cards
            .push(fixtures::unit(BOOK, fixtures::BF1, 0, "Book", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(YUUMI).unwrap(), &CARD));
        fixture
    }

    fn fires(ctx: &mut Ctx, event: Event) {
        ctx.raise(event);
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(ctx),
            [format!("{{card {BOOK}}}")],
            "her other unit here alone · not herself, not the enemy, not Vi in the base"
        );
    }

    #[test]
    fn the_script_is_a_unit_with_an_attack_and_a_defend_trigger_aimed_at_another_friendly_unit_here(
    ) {
        assert!(std::ptr::eq(
            script_of("Yuumi - Magical Cat").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Attacks(Who::Me));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Defends(Who::Me));
        for ability in CARD.abilities {
            assert_eq!(ability.targets, &[TARGET]);
            assert!(!ability.optional);
            assert!(ability.cost.is_none());
        }
        assert_eq!(TARGET.filter, ANOTHER_FRIENDLY_UNIT_HERE);
        assert_eq!(BONUS, 3);
    }

    #[test]
    fn attacking_gives_the_chosen_ally_three_might_and_tank_until_the_turn_ends() {
        let mut fixture = lap();
        let mut ctx = fixture.ctx();
        fires(&mut ctx, Event::Attacks { card: YUUMI });
        fixtures::choose(&mut ctx, 0, &format!("{{card {BOOK}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == YUUMI
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BOOK)]);
        assert_eq!(ctx.current_might(BOOK), 2, "nothing until it resolves");
        assert!(!ctx.has_keyword(BOOK, Keyword::Tank));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BOOK), 5);
        assert!(ctx.has_keyword(BOOK, Keyword::Tank));
        assert_eq!(
            combat::ordered(&ctx, &[YUUMI, BOOK], false),
            [BOOK],
            "815 · the Tank is assigned combat damage first"
        );
        assert_eq!(ctx.current_might(YUUMI), 1, "she gives, she does not take");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BOOK}}} gets +3 Might and Tank this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(BOOK), 2);
        assert!(
            !ctx.has_keyword(BOOK, Keyword::Tank),
            "Tank ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn defending_is_the_same_gift_through_its_own_trigger() {
        let mut fixture = lap();
        let mut ctx = fixture.ctx();
        fires(&mut ctx, Event::Defends { card: YUUMI });
        fixtures::choose(&mut ctx, 0, &format!("{{card {BOOK}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == YUUMI
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(BOOK), 5);
        assert!(ctx.has_keyword(BOOK, Keyword::Tank));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn alone_among_enemies_the_trigger_fizzles_and_an_ally_that_left_gets_nothing() {
        let mut fixture = lap();
        fixture.table.card_mut(BOOK).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: YUUMI });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        drop(ctx);

        let mut fled = lap();
        let mut ctx = fled.ctx();
        fires(&mut ctx, Event::Attacks { card: YUUMI });
        fixtures::choose(&mut ctx, 0, &format!("{{card {BOOK}}}")).unwrap();
        ctx.table.card_mut(BOOK).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BOOK), 2, "no longer here");
        assert!(!ctx.has_keyword(BOOK, Keyword::Tank));
    }
}

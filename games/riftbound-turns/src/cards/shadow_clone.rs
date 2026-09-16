use super::prelude::{done, grant_this_turn, on_attack, optional, paying_with, unit};
use super::{
    Card, Filter, Flow, Item, Keyword, SelfCost, Stage, TargetKind, TargetSpec, KIND_UNIT,
};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 4;

pub const UNIT_IN_YOUR_TRASH: TargetSpec = TargetSpec {
    filter: Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Friendly]),
    min: 1,
    max: 1,
    kind: TargetKind::Card,
    label: "a unit in your trash to banish",
    min_at_level: None,
};

fn shadow_strike(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) && grant_this_turn(ctx, me, Keyword::Assault(ASSAULT)) {
        ctx.narrate(format!("{{card {me}}} gets [Assault {ASSAULT}] this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Shadow Clone",
    &[],
    &[optional(paying_with(
        on_attack(&[UNIT_IN_YOUR_TRASH], shadow_strike),
        SelfCost::BanishTarget,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, triggers};
    use crate::state::{PromptWhy, TargetRef};

    const CLONE: u32 = 90;
    const FALLEN: u32 = 91;
    const THEIR_FALLEN: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(agni_plugin_sdk::table::CardInfo {
            might: Some(0),
            ..fixtures::card(CLONE, fixtures::BF1, 0, "Shadow Clone", "Unit")
        });
        fixture.table.tokens.push(CLONE);
        fixture.table.tokens.sort_unstable();
        fixture
            .table
            .cards
            .push(fixtures::unit(FALLEN, fixtures::TRASH, 0, "Jinx", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_FALLEN, fixtures::TRASH, 1, "Jinx", 2));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_attack_trigger_asks_with_a_unit_in_the_trash_and_grants_assault_on_yes() {
        assert!(std::ptr::eq(script_of("Shadow Clone").unwrap(), &CARD));
        assert!(crate::cards::is_token_name(CARD.name));
        let ability = &CARD.abilities[0];
        assert!(ability.optional);
        assert_eq!(ability.self_cost, SelfCost::BanishTarget);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: CLONE });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}"],
            "only a friendly unit in the trash is offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(FALLEN)]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.has_keyword(CLONE, Keyword::Assault(ASSAULT)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} gets [Assault 4] this turn".to_string()));
    }

    #[test]
    fn no_removes_the_trigger_and_an_empty_trash_never_asks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: CLONE });
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_keyword(CLONE, Keyword::Assault(ASSAULT)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        let mut empty = armed();
        empty.table.cards.retain(|card| card.id != FALLEN);
        empty.resolve();
        let mut ctx = empty.ctx();
        ctx.raise(Event::Attacks { card: CLONE });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
    }
}

use super::prelude::{
    a_card, card_target, done, on_move_to_battlefield, optional, target, unit, with_cost,
    zone_target, Location, CHAOS, UNIT_AT_ANOTHER_LOCATION,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march;

const THAT_BATTLEFIELD: usize = 0;
const PASSENGER: usize = 1;

pub const HERE: TargetSpec = target(Filter::Here, 1, 1, TargetKind::Zone, "that battlefield");
pub const A_UNIT_YOU_CONTROL: TargetSpec = a_card(
    UNIT_AT_ANOTHER_LOCATION,
    "a unit you control to move to the same battlefield",
);

fn ferry(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(zone) = zone_target(item, THAT_BATTLEFIELD) else {
        return done();
    };
    if !ctx.zones.is_battlefield(zone) {
        return done();
    }
    let Some(passenger) = card_target(ctx, item, PASSENGER) else {
        return done();
    };
    march::effect_move(ctx, item, passenger, Location::Battlefield(zone));
    done()
}

pub static CARD: Card = unit(
    "Fae Porter",
    &[],
    &[optional(with_cost(
        on_move_to_battlefield(&[HERE, A_UNIT_YOU_CONTROL], ferry),
        CHAOS,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{march, play, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const PORTER: u32 = 90;
    const FRIEND: u32 = 91;
    const CHAOS_RUNE: u32 = 46;

    fn glade() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut porter = fixtures::unit(PORTER, fixtures::BASE, 0, "Fae Porter", 4);
        porter.domain = vec!["Chaos".into()];
        porter.energy = Some(4);
        fixture.table.cards.push(porter);
        fixture
            .table
            .cards
            .push(fixtures::unit(FRIEND, fixtures::BF2, 0, "Friend", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PORTER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march(fixture: &mut Fixture) -> Ctx<'_> {
        let action = fixtures::move_action(PORTER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            PORTER,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        ctx
    }

    fn paid_with_the_chaos_rune(ctx: &Ctx) -> bool {
        ctx.effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Move {
                    card: CHAOS_RUNE,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    ..
                }
            )
        })
    }

    #[test]
    fn the_script_is_a_unit_with_an_optional_chaos_costed_move_trigger_locking_the_battlefield() {
        assert!(std::ptr::eq(script_of("Fae Porter").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(CHAOS));
        assert_eq!(ability.targets, &[HERE, A_UNIT_YOU_CONTROL]);
        assert_eq!(HERE.kind, TargetKind::Zone);
    }

    #[test]
    fn arriving_at_a_battlefield_locks_it_asks_for_a_unit_elsewhere_then_the_chaos_and_ferries_it()
    {
        let mut fixture = glade();
        let mut ctx = march(&mut fixture);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 1 }),
            "the battlefield has one candidate and fills itself"
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {FRIEND}}}")
            ],
            "friendly units anywhere but here"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 1, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit you control"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {FRIEND}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "pay 1 Chaos power for the {card 90} trigger?"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(paid_with_the_chaos_rune(&ctx), "{:?}", ctx.effects);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PORTER
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Zone(fixtures::BF1), TargetRef::Card(FRIEND)]
        );
        assert_eq!(
            ctx.location(FRIEND),
            Some(Location::Battlefield(fixtures::BF2)),
            "nothing moves before the trigger resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(FRIEND),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card: FRIEND,
                to: Location::Battlefield(fixtures::BF1),
                cause: MoveCause::Effect,
                ..
            }
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_chaos_moves_nothing_and_without_a_chaos_rune_nothing_is_asked() {
        let mut fixture = glade();
        let mut ctx = march(&mut fixture);
        fixtures::choose(&mut ctx, 0, &format!("{{card {FRIEND}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        assert_eq!(
            ctx.location(FRIEND),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!paid_with_the_chaos_rune(&ctx));
        drop(ctx);

        let mut poor = glade();
        poor.table.cards.retain(|card| card.id != CHAOS_RUNE);
        poor.resolve();
        let mut ctx = march(&mut poor);
        fixtures::choose(&mut ctx, 0, &format!("{{card {FRIEND}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_every_friendly_unit_already_here_the_trigger_fizzles_and_a_walk_home_fires_nothing() {
        let mut fixture = glade();
        fixture.table.card_mut(FRIEND).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = march(&mut fixture);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        drop(ctx);

        let mut fixture = glade();
        fixture.table.card_mut(PORTER).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(FRIEND).unwrap().exhausted = true;
        fixture.resolve();
        let action = fixtures::move_action(PORTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            PORTER,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}

use super::prelude::{ask_discard, done, draw, on_move_to_battlefield, triggered, unit};
use super::{Card, Flow, Item, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
const STAGE_DISCARDED: u8 = 1;

fn shake_down(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == STAGE_DISCARDED {
        return done();
    }
    match ask_discard(ctx, item, STAGE_DISCARDED) {
        Some(ask) => Flow::Ask(ask),
        None => done(),
    }
}

fn collect(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Corrupt Enforcer",
    &[],
    &[
        on_move_to_battlefield(&[], shake_down),
        triggered(Trigger::CombatWon(Who::Me), &[], collect),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Where};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const ENFORCER: u32 = 90;

    fn precinct(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut enforcer = fixtures::unit(ENFORCER, zone, 0, "Corrupt Enforcer", 4);
        enforcer.domain = vec!["Chaos".into()];
        enforcer.energy = Some(3);
        enforcer.power = Some(1);
        fixture.table.cards.push(enforcer);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ENFORCER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) -> Ctx<'_> {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(ENFORCER, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, ENFORCER, from, to);
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn the_script_is_a_unit_with_a_move_to_battlefield_discard_and_a_combat_won_draw() {
        assert!(std::ptr::eq(script_of("Corrupt Enforcer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(
            CARD.abilities[0].trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert_eq!(CARD.abilities[1].trigger, Trigger::CombatWon(Who::Me));
        assert!(CARD.abilities.iter().all(|ability| !ability.optional));
        assert!(CARD
            .abilities
            .iter()
            .all(|ability| ability.targets.is_empty()));
    }

    #[test]
    fn moving_to_a_battlefield_asks_its_controller_for_one_discard_as_the_trigger_resolves() {
        let mut fixture = precinct(fixtures::BASE);
        let mut ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        let hand = ctx.hand_of(0).len();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ENFORCER
        ));
        assert!(ctx.blob.prompt.is_none(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DISCARDED
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        assert_eq!(fixtures::labels(&ctx).len(), hand, "every card in hand");
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the opponent cannot discard for me"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_the_move_trigger_just_finishes_and_a_walk_home_never_fires_it() {
        let mut fixture = precinct(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::HAND) && card.seat == 0));
        fixture.resolve();
        let mut ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "409.4 · nothing to discard");
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = precinct(fixtures::BF1);
        let ctx = march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn winning_a_combat_where_he_stands_draws_one_and_another_seats_win_or_another_battlefield_does_not(
    ) {
        let mut fixture = precinct(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == ENFORCER
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the trigger");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 1,
        });
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the other seat's win is not his");
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF2,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "466.3.c · he inherits only the result where he stands"
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.fault.is_none());
    }
}

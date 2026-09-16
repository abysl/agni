use super::prelude::{
    card_target, done, on_move_to_battlefield, optional, target, unit, zone_target, Location,
    MOVABLE_ENEMY_UNIT,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march;

const THAT_BATTLEFIELD: usize = 0;
const ENEMY: usize = 1;

const HERE: TargetSpec = target(Filter::Here, 1, 1, TargetKind::Zone, "that battlefield");
const AN_ENEMY_UNIT: TargetSpec = target(
    MOVABLE_ENEMY_UNIT,
    0,
    1,
    TargetKind::Card,
    "an enemy unit to move there",
);

fn pull_an_enemy_unit_here(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(zone) = zone_target(item, THAT_BATTLEFIELD) else {
        return done();
    };
    if !ctx.zones.is_battlefield(zone) {
        return done();
    }
    let Some(enemy) = card_target(ctx, item, ENEMY) else {
        return done();
    };
    march::effect_move(ctx, item, enemy, Location::Battlefield(zone));
    done()
}

pub static CARD: Card = unit(
    "Irresistible Faefolk",
    &[],
    &[optional(on_move_to_battlefield(
        &[HERE, AN_ENEMY_UNIT],
        pull_an_enemy_unit_here,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, prompts, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const FAEFOLK: u32 = 90;

    fn glade() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            FAEFOLK,
            fixtures::BASE,
            0,
            "Irresistible Faefolk",
            1,
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FAEFOLK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march_to(fixture: &mut Fixture, zone: u16) -> Ctx<'_> {
        let action = fixtures::move_action(FAEFOLK, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            FAEFOLK,
            Location::Base(0),
            Location::Battlefield(zone),
        );
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn the_faefolk_has_one_may_move_trigger_naming_the_battlefield_and_an_enemy_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Irresistible Faefolk").unwrap(),
            &CARD
        ));
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
        assert_eq!(ability.targets, [HERE, AN_ENEMY_UNIT]);
        assert_eq!((AN_ENEMY_UNIT.min, AN_ENEMY_UNIT.max), (0, 1));
    }

    #[test]
    fn moving_to_a_battlefield_may_pull_an_enemy_unit_there_and_stages_the_combat() {
        let mut fixture = glade();
        let mut ctx = march_to(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 1 }),
            "the battlefield is the trigger's own location and needs no click"
        );
        assert_eq!(
            ctx.blob.pending(1).unwrap().item.targets,
            [TargetRef::Zone(fixtures::BF1)]
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}", "skip"]);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            format!("{{card {FAEFOLK}}}: choose an enemy unit to move there (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Base(1)),
            "nothing moves until the trigger resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::BF1,
            seat: 0,
            index: TOP
        }));
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(0),
            "the Faefolk's own arrival marked the contest, so its seat attacks"
        );
        let showdown = ctx.blob.showdown.clone().expect("the combat opens");
        assert_eq!(
            (
                showdown.zone,
                showdown.attacker,
                showdown.defender,
                showdown.combat
            ),
            (fixtures::BF1, 0, 1, true)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_pulls_nobody_and_a_move_to_base_raises_no_trigger() {
        let mut fixture = glade();
        let mut ctx = march_to(&mut fixture, fixtures::BF1);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        let mut home = glade();
        home.table.card_mut(FAEFOLK).unwrap().zone = Some(fixtures::BF1);
        home.blob.set_holder(fixtures::BF1, Some(0));
        home.resolve();
        let action = fixtures::move_action(FAEFOLK, fixtures::BASE, 0);
        let mut ctx = home.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            FAEFOLK,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
    }
}

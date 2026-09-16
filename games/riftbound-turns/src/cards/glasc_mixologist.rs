use super::prelude::{asking, deathknell, optional, target, unit, with_candidates, Price};
use super::the_harrowing::{recruit, recruit_candidates};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "a unit in your trash costing 3 or less to play from the mixologist";
pub const MAX_ENERGY: u8 = 3;
pub const MAX_POWER: u8 = 1;

pub const CHEAP_UNIT_IN_TRASH: TargetSpec = target(
    Filter::And(&[
        Filter::Kind(KIND_UNIT),
        Filter::InTrash,
        Filter::Friendly,
        Filter::EnergyAtMost(MAX_ENERGY),
        Filter::PowerAtMost(MAX_POWER),
    ]),
    0,
    1,
    TargetKind::Card,
    "a unit in your trash costing 3 or less",
);

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    recruit_candidates(ctx, item, stage, &CHEAP_UNIT_IN_TRASH, Price::Free)
}

fn last_brew(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    recruit(ctx, item, stage, &CHEAP_UNIT_IN_TRASH, Price::Free, 0)
}

pub static CARD: Card = unit(
    "Glasc Mixologist",
    &[Keyword::Deathknell],
    &[asking(
        with_candidates(optional(deathknell(&[], last_brew)), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::script_of;
    use crate::cards::the_harrowing::tests::{trashed, DRAWS};
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, Leave, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const MIXOLOGIST: u32 = 90;
    const CRAB: u32 = 91;
    const STEEP: u32 = 92;
    const HEAVY: u32 = 93;
    const THEIRS: u32 = 94;

    fn mixologist(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(MIXOLOGIST, zone, 0, "Glasc Mixologist", 5)
        }
    }

    fn laboratory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mixologist(fixtures::BF1));
        fixture.table.cards.push(trashed(CRAB, 0, "Crab", 3, 1));
        fixture
            .table
            .cards
            .push(trashed(STEEP, 0, "Big Lizard", 4, 0));
        fixture.table.cards.push(trashed(HEAVY, 0, "Titan", 2, 2));
        fixture.table.cards.push(trashed(THEIRS, 1, "Jinx", 1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(CRAB, &DRAWS);
        fixture
    }

    fn her_trigger_is_queued(ctx: &Ctx) -> bool {
        ctx.blob.chain.iter().any(|item| {
            matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == MIXOLOGIST)
        })
    }

    #[test]
    fn the_script_is_an_optional_deathknell_over_cheap_trash_units() {
        assert!(std::ptr::eq(script_of("Glasc Mixologist").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Death);
        assert!(ability.optional);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((CHEAP_UNIT_IN_TRASH.min, CHEAP_UNIT_IN_TRASH.max), (0, 1));
        assert_eq!((MAX_ENERGY, MAX_POWER), (3, 1));
    }

    #[test]
    fn dying_offers_a_unit_within_three_energy_and_one_power_and_plays_it_free_from_the_trash() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(MIXOLOGIST, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(her_trigger_is_queued(&ctx));
        assert!(ctx.in_trash(MIXOLOGIST));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CRAB}}}"), "skip".to_string()],
            "she costs 5, the Lizard 4, the Titan 2 power, Jinx is theirs"
        );
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CRAB}}}")).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })),
            "the base and the held battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(CRAB),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.card(CRAB).unwrap().exhausted);
        assert_eq!(ctx.runes_of(0).len(), runes);
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "ignoring its cost");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CRAB
        )));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Crab's play trigger fires again"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.trash_of(0), vec![STEEP, HEAVY, MIXOLOGIST]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_plays_nothing_and_with_nothing_cheap_of_yours_she_asks_nothing() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(MIXOLOGIST, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), vec![CRAB, STEEP, HEAVY, MIXOLOGIST]);
        drop(ctx);

        let mut fixture = laboratory();
        fixture.table.cards.retain(|card| card.id != CRAB);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(MIXOLOGIST, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing of mine qualifies: {:?}",
            fixtures::labels(&ctx)
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.in_trash(THEIRS),
            "the opponent's trash is never offered"
        );
        assert!(ctx.fault.is_none());
    }
}

use super::prelude::{asking, optional, play, target, unit, with_candidates, Price};
use super::the_harrowing::{recruit, recruit_candidates};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "a unit in your trash costing 3 or less to play";
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

fn mother(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    recruit(ctx, item, stage, &CHEAP_UNIT_IN_TRASH, Price::Free, 0)
}

pub static CARD: Card = unit(
    "Spectral Matron",
    &[],
    &[asking(
        with_candidates(optional(play(&[], mother)), candidates),
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
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::{ItemKind, Leave, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const MATRON: u32 = 90;
    const CRAB: u32 = 91;
    const STEEP: u32 = 92;
    const HEAVY: u32 = 93;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Order".into()],
            ..fixtures::unit(MATRON, fixtures::HAND, 0, "Spectral Matron", 4)
        });
        fixture.table.cards.push(trashed(CRAB, 0, "Crab", 3, 1));
        fixture
            .table
            .cards
            .push(trashed(STEEP, 0, "Big Lizard", 4, 0));
        fixture.table.cards.push(trashed(HEAVY, 0, "Titan", 2, 2));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(CRAB, &DRAWS);
        fixture
    }

    #[test]
    fn the_script_is_an_optional_play_trigger_over_cheap_trash_units() {
        assert!(std::ptr::eq(script_of("Spectral Matron").unwrap(), &CARD));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((CHEAP_UNIT_IN_TRASH.min, CHEAP_UNIT_IN_TRASH.max), (0, 1));
    }

    #[test]
    fn only_a_unit_within_three_energy_and_one_power_is_offered_and_it_is_played_free() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MATRON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CRAB}}}"), "skip".to_string()],
            "a 4-energy unit and a 2-power unit are out of reach"
        );
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CRAB}}}")).unwrap();
        assert_eq!(ctx.location(CRAB), Some(Location::Base(0)));
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
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CRAB
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.trash_of(0), vec![STEEP, HEAVY]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_plays_nothing_and_the_opponents_trash_is_never_offered() {
        let mut fixture = armed();
        fixture.table.card_mut(CRAB).unwrap().owner = 1;
        fixture.table.card_mut(CRAB).unwrap().seat = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MATRON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing of mine qualifies: {:?}",
            fixtures::labels(&ctx)
        );
        assert!(ctx.blob.chain.is_empty());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MATRON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), vec![CRAB, STEEP, HEAVY]);
        assert!(ctx.fault.is_none());
    }
}

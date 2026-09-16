use super::prelude::{asking, optional, play, unit, with_candidates, Price};
use super::the_harrowing::{recruit, recruit_candidates, UNIT_IN_TRASH};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "a unit in your trash to play for its Power cost";

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    recruit_candidates(ctx, item, stage, &UNIT_IN_TRASH, Price::PowerOnly)
}

fn gorge(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    recruit(ctx, item, stage, &UNIT_IN_TRASH, Price::PowerOnly, 0)
}

pub static CARD: Card = unit(
    "Soulgorger",
    &[],
    &[asking(
        with_candidates(optional(play(&[], gorge)), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::script_of;
    use crate::cards::the_harrowing::tests::{trashed, CHEAP, DEAR, THEIRS};
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::{Leave, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const SOULGORGER: u32 = 90;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(SOULGORGER, fixtures::HAND, 0, "Soulgorger", 5)
        });
        fixture
            .table
            .cards
            .push(trashed(CHEAP, 0, "Big Lizard", 6, 1));
        fixture.table.cards.push(trashed(DEAR, 0, "Titan", 8, 5));
        fixture.table.cards.push(trashed(THEIRS, 1, "Jinx", 1, 0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_an_optional_play_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(script_of("Soulgorger").unwrap(), &CARD));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn playing_it_offers_the_affordable_trash_unit_with_a_skip_and_plays_it_for_power() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SOULGORGER).unwrap();
        assert_eq!(ctx.location(SOULGORGER), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the play trigger waits on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}"), "skip".to_string()]
        );
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert_eq!(ctx.location(CHEAP), Some(Location::Base(0)));
        assert_eq!(ctx.runes_of(0).len(), runes - 1, "the Power was paid");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CHEAP
        )));
        assert_eq!(ctx.trash_of(0), vec![DEAR]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_leaves_the_trash_alone_and_an_empty_trash_asks_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SOULGORGER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), vec![CHEAP, DEAR]);
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != CHEAP && card.id != DEAR);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SOULGORGER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SOULGORGER}}} finds no unit to play")));
        assert!(ctx.fault.is_none());
    }
}

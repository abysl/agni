use super::prelude::{done, on_you_play_card, ready, unit, when};
use super::{Card, Flow, Item, Source, Stage, KIND_GEAR};
use crate::engine::ctx::{Ctx, Event};

fn a_gear(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { kind, .. } if kind == KIND_GEAR)
}

fn scramble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Pit Crew",
    &[],
    &[when(on_you_play_card(&[], scramble), a_gear)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{ItemKind, Origin};

    const CREW: u32 = 90;
    const THEIR_GEAR: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut crew = fixtures::unit(CREW, fixtures::BASE, 0, "Pit Crew", 3);
        crew.exhausted = true;
        fixture.table.cards.push(crew);
        fixture.table.cards.push(fixtures::gear(
            THEIR_GEAR,
            fixtures::HAND,
            1,
            "Plain Gear",
            2,
        ));
        fixture.resolve();
        fixture
    }

    #[test]
    fn a_gear_you_play_readies_the_crew_once_it_is_on_the_board() {
        assert!(std::ptr::eq(script_of("Pit Crew").unwrap(), &CARD));
        assert_eq!(CARD.abilities[0].trigger, Trigger::YouPlayCard);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(
            matches!(
                ctx.blob.chain.last().map(|top| top.kind),
                Some(ItemKind::Trigger { source, index: 0 }) if source == CREW
            ),
            "a permanent's finalization is its play: the trigger waits on the chain"
        );
        assert!(ctx.card(CREW).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(CREW).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == CREW
        )));
        assert!(ctx.blob.log.iter().any(|line| line == "{card 90} readies"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_of_yours_a_unit_of_yours_and_an_opponents_gear_ready_nothing() {
        let mut spell = armed();
        let mut ctx = spell.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(CREW).unwrap().exhausted, "a spell is not a gear");
        let mut unit = armed();
        let mut ctx = unit.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.card(CREW).unwrap().exhausted, "a unit is not a gear");
        let mut theirs = armed();
        let mut ctx = theirs.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_GEAR, chain, 0), 1)
            .unwrap();
        play_engine::begin(&mut ctx, 1, THEIR_GEAR, Origin::Hand, None).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(THEIR_GEAR), "the opponent's gear was played");
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(
            ctx.card(CREW).unwrap().exhausted,
            "an opponent's gear is not one you play"
        );
    }
}

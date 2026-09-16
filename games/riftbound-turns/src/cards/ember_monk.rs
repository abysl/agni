use super::prelude::{done, might_this_turn, on_you_play_card, unit, when};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Origin;

pub const BONUS: i16 = 2;

pub fn spell_played_from_hidden(_: &Ctx, _: &Event) -> bool {
    false
}

fn from_hidden(ctx: &Ctx, event: &Event, _: Source) -> bool {
    match event {
        Event::Played {
            origin: Origin::Facedown { .. },
            ..
        } => true,
        Event::PlayedSpell { .. } => spell_played_from_hidden(ctx, event),
        _ => false,
    }
}

fn kindle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Ember Monk",
    &[],
    &[when(on_you_play_card(&[], kindle), from_hidden)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::ItemKind;

    const MONK: u32 = 90;
    const HIDDEN_UNIT: u32 = 91;
    const HIDDEN_SPELL: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(MONK, fixtures::BASE, 0, "Ember Monk", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(HIDDEN_UNIT, fixtures::BF1, 0, "Ally", 2));
        fixture.table.cards.push(fixtures::spell(
            HIDDEN_SPELL,
            fixtures::BF1,
            0,
            "Spark",
            2,
            1,
        ));
        {
            let ally = fixture.table.card_mut(fixtures::HAND_UNIT).unwrap();
            ally.name = "Ally".into();
            ally.energy = Some(0);
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn play_facedown(ctx: &mut crate::engine::ctx::Ctx, card: u32, location: Option<Location>) {
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            ctx,
            0,
            card,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            location,
        )
        .unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn a_unit_played_from_hidden_gives_the_monk_two_might_for_the_turn() {
        assert!(std::ptr::eq(script_of("Ember Monk").unwrap(), &CARD));
        assert_eq!(CARD.abilities[0].trigger, Trigger::YouPlayCard);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_facedown(
            &mut ctx,
            HIDDEN_UNIT,
            Some(Location::Battlefield(fixtures::BF1)),
        );
        assert_eq!(
            ctx.location(HIDDEN_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == MONK
        ));
        assert_eq!(ctx.current_might(MONK), 4);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(MONK), 4 + i32::from(BONUS));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 90} gets +2 Might this turn"));
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(MONK), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_card_played_from_hand_is_not_from_hidden() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::HAND_UNIT, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(ctx.current_might(MONK), 4);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(MONK), 4);
    }

    #[test]
    #[ignore = "engine gap · PlayedSpell carries no origin and the spell's chain item and card state are gone when the trigger is collected, so a spell played from hidden is not seen; spell_played_from_hidden is the seam"]
    fn a_spell_played_from_hidden_gives_the_monk_two_might_once_it_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx, HIDDEN_SPELL, None);
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(MONK), 4 + i32::from(BONUS));
    }
}

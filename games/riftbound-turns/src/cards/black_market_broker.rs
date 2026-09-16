use super::ember_monk::spell_played_from_hidden;
use super::prelude::{done, on_you_play_card, unit, when};
use super::trove_golem::play_golds;
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Origin;

pub const GOLDS: usize = 1;

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

fn fence(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_golds(ctx, item.controller, GOLDS);
    done()
}

pub static CARD: Card = unit(
    "Black Market Broker",
    &[],
    &[when(on_you_play_card(&[], fence), from_hidden)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::trove_golem::tests::golds_of;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, settle};
    use crate::state::ItemKind;

    const BROKER: u32 = 90;
    const HIDDEN_UNIT: u32 = 91;
    const HIDDEN_SPELL: u32 = 92;
    const THEIR_HIDDEN_UNIT: u32 = 93;

    fn market() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            BROKER,
            fixtures::BASE,
            0,
            "Black Market Broker",
            3,
        ));
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
        fixture.table.cards.push(fixtures::unit(
            THEIR_HIDDEN_UNIT,
            fixtures::BF2,
            1,
            "Foe",
            2,
        ));
        {
            let ally = fixture.table.card_mut(fixtures::HAND_UNIT).unwrap();
            ally.name = "Ally".into();
            ally.energy = Some(0);
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BROKER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_facedown(ctx: &mut Ctx, seat: u8, card: u32, zone: u16, location: Option<Location>) {
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, Origin::Facedown { zone }, location).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_watches_your_plays_for_one_from_face_down() {
        assert!(std::ptr::eq(
            script_of("Black Market Broker").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(GOLDS, 1);
    }

    #[test]
    fn a_unit_you_play_from_face_down_pays_an_exhausted_gold_once_the_trigger_resolves() {
        let mut fixture = market();
        let mut ctx = fixture.ctx();
        play_facedown(
            &mut ctx,
            0,
            HIDDEN_UNIT,
            fixtures::BF1,
            Some(Location::Battlefield(fixtures::BF1)),
        );
        assert_eq!(
            ctx.location(HIDDEN_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == BROKER
        ));
        assert!(golds_of(&ctx, 0).is_empty(), "the Gold waits for the chain");
        let next = ctx.table.next_id;
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 0), [next]);
        assert!(ctx.card(next).unwrap().exhausted);
        assert_eq!(ctx.location(next), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_card_played_from_hand_and_an_opponents_hidden_play_pay_nothing() {
        let mut fixture = market();
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
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "from hand is not from face down"
        );
        play_facedown(
            &mut ctx,
            1,
            THEIR_HIDDEN_UNIT,
            fixtures::BF2,
            Some(Location::Battlefield(fixtures::BF2)),
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(golds_of(&ctx, 0).is_empty(), "their play is not yours");
        assert!(golds_of(&ctx, 1).is_empty());
    }

    #[test]
    #[ignore = "engine gap · PlayedSpell carries no origin and the spell's chain item and card state are gone when the trigger is collected, so a spell played from hidden is not seen; ember_monk::spell_played_from_hidden is the seam"]
    fn a_spell_played_from_face_down_pays_a_gold_once_it_resolves() {
        let mut fixture = market();
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx, 0, HIDDEN_SPELL, fixtures::BF1, None);
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 0).len(), GOLDS);
    }
}

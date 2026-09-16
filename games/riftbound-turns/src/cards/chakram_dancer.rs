use super::lord_broadmane::rally;
use super::prelude::{play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const GRANTED: Keyword = Keyword::Shield(1);

fn dance(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    rally(ctx, item, GRANTED, "Shield")
}

pub static CARD: Card = unit("Chakram Dancer", &[Keyword::Ambush], &[play(&[], dance)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::lord_broadmane::other_friendly_units_here;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, play as play_engine, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const DANCER: u32 = 90;
    const ALLY: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn dancer(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Mind".into()],
            ..fixtures::unit(DANCER, zone, seat, "Chakram Dancer", 3)
        }
    }

    fn deck() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dancer(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn has_shield(ctx: &Ctx, unit: u32) -> bool {
        ctx.has_keyword(unit, GRANTED)
    }

    #[test]
    fn the_script_prints_ambush_and_one_untargeted_play_trigger_that_grants_shield() {
        assert!(std::ptr::eq(script_of("Chakram Dancer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Chakram Dancer");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert_eq!(GRANTED, Keyword::Shield(1));
        let mut fixture = deck();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, DANCER),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            other_friendly_units_here(&ctx, 0, DANCER),
            [],
            "not on the board yet"
        );
    }

    #[test]
    fn played_to_a_battlefield_the_other_friendly_units_there_gain_shield_for_the_turn() {
        let mut fixture = deck();
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            DANCER,
            Origin::Hand,
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DANCER
        ));
        assert!(!has_shield(&ctx, ALLY), "the grant waits for the trigger");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(has_shield(&ctx, ALLY));
        assert!(!has_shield(&ctx, THEIR_BRUTE), "an enemy here gets nothing");
        assert!(
            !has_shield(&ctx, fixtures::VI),
            "a friendly unit elsewhere gets nothing"
        );
        assert!(!has_shield(&ctx, DANCER), "other units, not herself");
        assert!(
            !ctx.has_keyword(ALLY, Keyword::Assault(1)),
            "Shield, not Assault"
        );
        assert_eq!(
            ctx.state_of(ALLY).unwrap().granted,
            [(GRANTED, this_turn(&ctx))]
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DANCER}}} gives 1 other friendly unit here [Shield] this turn"
        )));
        phases::end_turn(&mut ctx).unwrap();
        assert!(!has_shield(&ctx, ALLY), "the grant lapses with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_an_empty_base_she_has_nobody_to_rally() {
        let mut fixture = deck();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, DANCER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DANCER}}} has no other friendly unit here"
        )));
        assert!(!has_shield(&ctx, ALLY));
    }
}

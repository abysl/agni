use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub fn other(_: &Ctx, source: u32, unit: u32) -> bool {
    unit != source
}

pub static CARD: Card = with_statics(
    unit("Gemcraft Seer", &[Keyword::Vision], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: other,
        grants: &[Grant::Keyword(Keyword::Vision)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::state::PromptWhy;
    use agni_plugin_sdk::decide::Effect;

    const SEER: u32 = 90;
    const SECOND_SEER: u32 = 91;

    fn seeing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(SEER, fixtures::BASE, 0, "Gemcraft Seer", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SEER).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_vision_and_its_aura_gives_vision_to_other_friendly_units() {
        assert!(std::ptr::eq(script_of("Gemcraft Seer").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.keywords, [Keyword::Vision]);
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Vision)],
                ..
            }]
        ));
    }

    #[test]
    fn every_other_friendly_unit_anywhere_has_vision_and_no_enemy_or_hand_card_does() {
        let mut fixture = seeing();
        fixture.table.cards.push(fixtures::unit(
            SECOND_SEER,
            fixtures::BF1,
            0,
            "Gemcraft Seer",
            3,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            ctx.has_keyword(fixtures::VI, Keyword::Vision),
            "at a battlefield"
        );
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [
                Grant::Keyword(Keyword::Vision),
                Grant::Keyword(Keyword::Vision)
            ]
        ));
        assert!(ctx.has_keyword(SEER, Keyword::Vision), "printed");
        assert!(matches!(
            statics::grants_on(&ctx, SEER).as_slice(),
            [Grant::Keyword(Keyword::Vision)]
        ));
        assert!(
            !ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Vision),
            "friendly"
        );
        assert!(!ctx.has_keyword(fixtures::SPRITE, Keyword::Vision));
        assert!(
            !ctx.has_keyword(fixtures::HAND_UNIT, Keyword::Vision),
            "a card in hand is not a unit on the board"
        );
        assert!(
            !ctx.has_keyword(fixtures::LEGEND_CARD, Keyword::Vision),
            "the legend is not a unit"
        );
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, SEER));
        assert!(
            ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Vision),
            "friendly follows the controller"
        );
        ctx.recall(SEER, false);
        ctx.recall(SECOND_SEER, false);
        assert!(
            ctx.has_keyword(fixtures::VI, Keyword::Vision),
            "recalled, not gone"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_played_under_the_aura_looks_at_the_top_card_of_its_owners_deck() {
        let mut fixture = seeing();
        let mut ctx = fixture.ctx();
        let deck = ctx.zones.main_deck.unwrap();
        let top = ctx.top_of(deck, 0, 1)[0];
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(&mut ctx, 0, "your base").unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.effects.contains(&Effect::Peek { card: top, seat: 0 }));
    }
}

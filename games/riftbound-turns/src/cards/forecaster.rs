use super::prelude::{unit, with_statics};
use super::rumble_mechanized_menace::your_mech;
use super::{Card, Grant, Keyword, Scope, Static};

pub static CARD: Card = with_statics(
    unit("Forecaster", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: your_mech,
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

    const FORECASTER: u32 = 90;
    const MEGA: u32 = 91;
    const THEIR_MECH: u32 = 92;

    fn weather() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            FORECASTER,
            fixtures::BASE,
            0,
            "Forecaster",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(MEGA, fixtures::BF1, 0, "Mega-Mech", 8));
        fixture.table.cards.push(fixtures::unit(
            THEIR_MECH,
            fixtures::BASE,
            1,
            "Adaptatron",
            3,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FORECASTER).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_prints_nothing_and_its_aura_gives_vision_to_your_mechs() {
        assert!(std::ptr::eq(script_of("Forecaster").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.keywords.is_empty(),
            "Vision is the aura's, not printed"
        );
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
    fn every_friendly_mech_including_the_forecaster_has_vision_and_nothing_else_does() {
        let mut fixture = weather();
        let mut ctx = fixture.ctx();
        assert!(
            ctx.has_keyword(FORECASTER, Keyword::Vision),
            "a Mech herself"
        );
        assert!(ctx.has_keyword(MEGA, Keyword::Vision), "at a battlefield");
        assert!(matches!(
            statics::grants_on(&ctx, MEGA).as_slice(),
            [Grant::Keyword(Keyword::Vision)]
        ));
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Vision),
            "Vi is no Mech"
        );
        assert!(!ctx.has_keyword(THEIR_MECH, Keyword::Vision), "not yours");
        assert!(!ctx.has_keyword(fixtures::SPRITE, Keyword::Vision));
        assert!(ctx.set_controller(THEIR_MECH, 0, FORECASTER));
        assert!(
            ctx.has_keyword(THEIR_MECH, Keyword::Vision),
            "yours follows the controller"
        );
        ctx.kill(FORECASTER, crate::engine::ctx::Cause::Rule);
        assert!(
            !ctx.has_keyword(MEGA, Keyword::Vision),
            "365.1 · a dead Forecaster forecasts nothing"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_needs_the_forecaster_on_the_board_and_a_recall_keeps_it() {
        let mut fixture = weather();
        fixture.table.card_mut(FORECASTER).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !ctx.has_keyword(MEGA, Keyword::Vision),
            "a card in hand projects nothing"
        );
        drop(ctx);
        let mut fixture = weather();
        let mut ctx = fixture.ctx();
        ctx.recall(FORECASTER, false);
        assert!(ctx.has_keyword(MEGA, Keyword::Vision), "recalled, not gone");
    }

    #[test]
    fn a_mech_played_under_the_aura_looks_at_the_top_card_of_its_owners_deck() {
        let mut fixture = weather();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().name = "Mega-Mech".into();
        fixture.resolve();
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

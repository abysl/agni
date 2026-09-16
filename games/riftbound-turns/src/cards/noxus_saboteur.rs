use super::prelude::{unit, Location};
use super::Card;
use crate::engine::ctx::Ctx;
use crate::engine::{hide, statics};

pub fn blocks_reveal(ctx: &Ctx, source: u32, card: u32) -> bool {
    let Some(zone) = hide::zone_of(ctx, card) else {
        return false;
    };
    statics::in_play(ctx, source)
        && ctx.location(source) == Some(Location::Battlefield(zone))
        && ctx.controller(card) != ctx.controller(source)
}

pub static CARD: Card = unit("Noxus Saboteur", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};

    const SABOTEUR: u32 = 90;
    const THEIRS: u32 = 91;
    const MINE: u32 = 92;

    fn contested() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            SABOTEUR,
            fixtures::BF2,
            0,
            "Noxus Saboteur",
            3,
        ));
        fixture
            .table
            .cards
            .push(fixtures::hidden(THEIRS, fixtures::BF2, 1));
        fixture
            .table
            .cards
            .push(fixtures::hidden(MINE, fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.blob.card_state_mut(THEIRS).hidden_at = Some(fixtures::BF2);
        fixture.blob.card_state_mut(MINE).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SABOTEUR).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_hidden_rule_waits_on_the_engine() {
        assert!(std::ptr::eq(script_of("Noxus Saboteur").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.keywords.is_empty(),
            "the Hidden it mentions is the opponents', not its own"
        );
        assert!(CARD.statics.is_empty());
    }

    #[test]
    fn the_seam_names_an_opponents_facedown_card_at_the_saboteurs_battlefield() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        assert!(ctx.is_facedown(THEIRS));
        assert!(blocks_reveal(&ctx, SABOTEUR, THEIRS));
        assert!(
            !blocks_reveal(&ctx, SABOTEUR, MINE),
            "your opponents' · not yours"
        );
        assert!(
            !blocks_reveal(&ctx, SABOTEUR, fixtures::THEIR_UNIT),
            "a faceup card is not revealed"
        );
        ctx.recall(SABOTEUR, false);
        assert!(
            !blocks_reveal(&ctx, SABOTEUR, THEIRS),
            "here · he went home"
        );
        drop(ctx);
        let mut fixture = contested();
        fixture.table.card_mut(MINE).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.card_state_mut(MINE).hidden_at = Some(fixtures::BF2);
        let ctx = fixture.ctx();
        assert!(!blocks_reveal(&ctx, SABOTEUR, MINE));
        assert!(blocks_reveal(&ctx, SABOTEUR, THEIRS));
    }

    #[test]
    #[ignore = "engine gap · blocks_reveal is not consulted, the engine owes hide::play_legal a check of opponents' units here that forbid the reveal"]
    fn the_opponent_cannot_play_their_hidden_card_where_the_saboteur_stands() {
        let mut fixture = contested();
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        {
            let theirs = fixture.table.card_mut(THEIRS).unwrap();
            theirs.name = "Hidden Blade".into();
            theirs.kind = Some("Spell".into());
        }
        fixture.blob.card_state_mut(THEIRS).hidden_since = 0;
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(hide::playable(&ctx, THEIRS));
        assert!(hide::play_legal(&ctx, 1, THEIRS).is_err());
    }
}

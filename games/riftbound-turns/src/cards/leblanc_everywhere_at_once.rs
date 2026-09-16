use super::prelude::{unit, Location};
use super::{Card, Keyword};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub fn is_leblanc(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn vetoes_temporary_at(ctx: &Ctx, leblanc: u32, unit: u32) -> bool {
    is_leblanc(ctx, leblanc)
        && statics::in_play(ctx, leblanc)
        && ctx.controller(leblanc) == ctx.controller(unit)
        && matches!(ctx.location(leblanc), Some(here @ Location::Battlefield(_)) if ctx.location(unit) == Some(here))
}

pub fn temporary_is_vetoed(ctx: &Ctx, unit: u32) -> bool {
    ctx.faces_on_board()
        .map(|held| held.id)
        .any(|leblanc| vetoes_temporary_at(ctx, leblanc, unit))
}

pub static CARD: Card = unit("LeBlanc - Everywhere At Once", &[Keyword::Backline], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Token};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use agni_plugin_sdk::table::CardInfo;

    const LEBLANC: u32 = 90;

    fn leblanc(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(LEBLANC, zone, seat, "LeBlanc - Everywhere At Once", 4);
        card.domain = vec!["Mind".into()];
        card.energy = Some(4);
        card
    }

    fn rose(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(leblanc(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LEBLANC).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    #[test]
    fn the_script_is_a_backline_unit_whose_veto_waits_on_the_engine() {
        assert!(std::ptr::eq(
            script_of("LeBlanc - Everywhere At Once").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Backline]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn the_seam_names_your_temporary_units_at_her_battlefield_and_no_others() {
        let mut fixture = rose(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let here = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        let in_base = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        let theirs = spawn(
            &mut ctx,
            1,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert!(is_leblanc(&ctx, LEBLANC));
        assert!(!is_leblanc(&ctx, fixtures::VI));
        assert!(vetoes_temporary_at(&ctx, LEBLANC, here));
        assert!(temporary_is_vetoed(&ctx, here));
        assert!(
            !temporary_is_vetoed(&ctx, in_base),
            "the base is not her battlefield"
        );
        assert!(
            !temporary_is_vetoed(&ctx, theirs),
            "an opponent's Temporary effect is not yours"
        );
        assert!(
            !temporary_is_vetoed(&ctx, fixtures::SPRITE),
            "a Temporary unit at another battlefield"
        );
        assert!(!vetoes_temporary_at(&ctx, fixtures::VI, here));
    }

    #[test]
    fn in_the_base_or_in_hand_she_vetoes_nothing() {
        let mut fixture = rose(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let in_base = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert!(
            !temporary_is_vetoed(&ctx, in_base),
            "her text reads at my battlefield, and the base is none"
        );
        drop(ctx);
        let mut fixture = rose(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let here = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert!(!temporary_is_vetoed(&ctx, here));
    }

    #[test]
    #[ignore = "engine gap · the implicit Temporary trigger in triggers::find_among consults no card: with the Beginning-phase arm skipping a source for which leblanc_everywhere_at_once::temporary_is_vetoed reads true, a Sprite of yours at her battlefield survives your Beginning Phase while one in your base still dies"]
    fn your_sprite_at_her_battlefield_survives_your_beginning_phase() {
        let mut fixture = rose(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let here = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        let in_base = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        phases::start_turn(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(ctx.on_board(here), "her veto keeps it");
        assert!(!ctx.on_board(in_base), "the base is not her battlefield");
    }
}

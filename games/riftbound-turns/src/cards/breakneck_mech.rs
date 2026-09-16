use super::prelude::{unit, with_statics};
use super::rumble_mechanized_menace::{friendly_mechs, your_mech};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const DEFLECT: u8 = 1;

pub fn controls_another_mech(ctx: &Ctx, me: u32) -> bool {
    friendly_mechs(ctx, ctx.controller(me))
        .into_iter()
        .any(|mech| mech != me)
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    controls_another_mech(ctx, me)
}

pub static CARD: Card = with_statics(
    unit("Breakneck Mech", &[], &[]),
    &[
        Static::Aura {
            scope: Scope::FriendlyUnits,
            when: your_mech,
            grants: &[
                Grant::Keyword(Keyword::Deflect(DEFLECT)),
                Grant::Keyword(Keyword::Ganking),
            ],
        },
        Static::EntersReady(enters_ready),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, play as play_engine, settle, statics};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BREAKNECK: u32 = 90;
    const BOT: u32 = 91;
    const THEIR_MECH: u32 = 92;

    fn breakneck(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(BREAKNECK, zone, 0, "Breakneck Mech", 7);
        card.domain = vec!["Mind".into()];
        card.energy = Some(1);
        card
    }

    fn garage(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(breakneck(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(BOT, fixtures::BF1, 0, "Bubble Bot", 3));
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
            fixture.scripts.of_card(BREAKNECK).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_an_aura_of_deflect_and_ganking_over_your_mechs_and_names_the_entry_seam() {
        assert!(std::ptr::eq(script_of("Breakneck Mech").unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(matches!(
            CARD.statics,
            [
                Static::Aura {
                    scope: Scope::FriendlyUnits,
                    grants: [
                        Grant::Keyword(Keyword::Deflect(1)),
                        Grant::Keyword(Keyword::Ganking)
                    ],
                    ..
                },
                Static::EntersReady(_)
            ]
        ));
        let mut fixture = garage(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, BREAKNECK), "Bubble Bot is another Mech");
        assert!(!enters_ready(&ctx, BOT), "the bot has only itself");
        assert!(!enters_ready(&ctx, THEIR_MECH), "our Mechs are not theirs");
    }

    #[test]
    fn your_mechs_including_the_breakneck_deflect_for_one_and_gank_while_nothing_else_does() {
        let mut fixture = garage(fixtures::BASE);
        let mut ctx = fixture.ctx();
        for mech in [BREAKNECK, BOT] {
            assert_eq!(ctx.deflect_of(mech), 1, "{{card {mech}}}");
            assert!(ctx.has_keyword(mech, Keyword::Ganking));
        }
        assert!(matches!(
            statics::grants_on(&ctx, BOT).as_slice(),
            [
                Grant::Keyword(Keyword::Deflect(1)),
                Grant::Keyword(Keyword::Ganking)
            ]
        ));
        assert_eq!(ctx.deflect_of(fixtures::VI), 0, "Vi is no Mech");
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(ctx.deflect_of(THEIR_MECH), 0, "not yours");
        assert!(!ctx.has_keyword(THEIR_MECH, Keyword::Ganking));
        let across = (
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        );
        assert_eq!(
            march::legal_destination(&ctx, BOT, across.0, across.1),
            Ok(()),
            "Ganking lets the bot march battlefield to battlefield"
        );
        ctx.kill(BREAKNECK, crate::engine::ctx::Cause::Rule);
        assert_eq!(
            ctx.deflect_of(BOT),
            0,
            "365.1 · the aura dies with its source"
        );
        assert!(!ctx.has_keyword(BOT, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(&ctx, BOT, across.0, across.1),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_beside_no_other_mech_it_enters_exhausted_like_any_unit() {
        let mut fixture = garage(fixtures::HAND);
        fixture.table.cards.retain(|card| card.id != BOT);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!enters_ready(&ctx, BREAKNECK));
        play_engine::begin(
            &mut ctx,
            0,
            BREAKNECK,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(BREAKNECK));
        assert!(ctx.card(BREAKNECK).unwrap().exhausted);
    }

    #[test]
    fn played_while_you_control_another_mech_it_enters_ready() {
        let mut fixture = garage(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, BREAKNECK));
        play_engine::begin(
            &mut ctx,
            0,
            BREAKNECK,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(BREAKNECK));
        assert!(
            !ctx.card(BREAKNECK).unwrap().exhausted,
            "I enter ready if you control another Mech"
        );
    }
}

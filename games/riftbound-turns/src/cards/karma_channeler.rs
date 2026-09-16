use super::prelude::{a_friendly_unit, buff, unit};
use super::{Card, Keyword, Source, TargetSpec};
use crate::engine::ctx::Ctx;

pub const BUFF_TARGET: TargetSpec = a_friendly_unit("a friendly unit to buff");

pub fn you_recycled_cards_until_a_recycled_event_exists(
    ctx: &Ctx,
    by: u8,
    recycled: &[u32],
    source: Source,
) -> bool {
    ctx.on_board(source.card)
        && by == ctx.controller(source.card)
        && recycled.iter().any(|card| !ctx.is_rune(*card))
}

pub fn attune(ctx: &mut Ctx, unit: u32) -> bool {
    if !ctx.is_unit(unit) || !ctx.on_board(unit) {
        return false;
    }
    let buffed = buff(ctx, unit);
    if buffed {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {unit}}} already has a buff"));
    }
    buffed
}

pub static CARD: Card = unit("Karma - Channeler", &[Keyword::Vision], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Filter, TargetKind};
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const KARMA: u32 = 90;

    fn karma(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(KARMA, zone, 0, "Karma - Channeler", 6);
        card.energy = Some(6);
        card.power = Some(1);
        card.domain = vec!["Order".into()];
        card
    }

    fn channeling(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(karma(zone));
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: KARMA,
            ability: 0,
        }
    }

    #[test]
    fn the_stub_prints_vision_and_both_the_vision_play_and_the_recycle_trigger_are_engine_seams() {
        assert!(std::ptr::eq(script_of("Karma - Channeler").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Vision]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(BUFF_TARGET.min, 1);
        assert_eq!(BUFF_TARGET.max, 1);
        assert_eq!(BUFF_TARGET.kind, TargetKind::Card);
        assert!(matches!(
            BUFF_TARGET.filter,
            Filter::And(&[Filter::Unit, Filter::Friendly])
        ));
        let fixture = channeling(fixtures::BASE);
        assert!(std::ptr::eq(fixture.scripts.of_card(KARMA).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_reads_her_controller_recycling_at_least_one_card_that_is_not_a_rune() {
        let mut fixture = channeling(fixtures::BASE);
        let ctx = fixture.ctx();
        let seam = you_recycled_cards_until_a_recycled_event_exists;
        assert!(seam(&ctx, 0, &[fixtures::HAND_SPELL], source()));
        assert!(
            seam(&ctx, 0, &[fixtures::RUNE_A, fixtures::HAND_UNIT], source()),
            "one card among the runes is enough"
        );
        assert!(
            !seam(&ctx, 0, &[fixtures::RUNE_A, 41], source()),
            "runes aren't cards"
        );
        assert!(!seam(&ctx, 0, &[], source()));
        assert!(
            !seam(&ctx, 1, &[fixtures::THEIR_HAND_CARD], source()),
            "an opponent's recycle is not yours"
        );
        drop(ctx);
        let mut fixture = channeling(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            !seam(&ctx, 0, &[fixtures::HAND_SPELL], source()),
            "in hand she watches nothing"
        );
    }

    #[test]
    fn the_effect_buffs_a_friendly_unit_once_and_refuses_what_is_not_a_unit_on_the_board() {
        let mut fixture = channeling(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(!ctx.is_buffed(fixtures::VI));
        assert!(attune(&mut ctx, fixtures::VI));
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4, "+1 from the buff");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        assert!(!attune(&mut ctx, fixtures::VI), "a unit holds one buff");
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} already has a buff", fixtures::VI)));
        assert!(!attune(&mut ctx, fixtures::HAND_UNIT), "in hand");
        assert!(!attune(&mut ctx, fixtures::HAND_GEAR), "not a unit");
        assert!(attune(&mut ctx, KARMA), "she can buff herself");
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · no Recycled event reaches triggers::matches and Vision is not yet an implicit play trigger; with them the script is triggered(Recycled(Who::You), &[BUFF_TARGET], ..) and her own Vision recycle buffs a friendly unit"]
    fn a_vision_recycle_on_play_triggers_her_own_buff() {
        let mut fixture = channeling(fixtures::HAND);
        fixture.table.card_mut(KARMA).unwrap().energy = Some(2);
        fixture.table.card_mut(KARMA).unwrap().power = Some(0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KARMA).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(KARMA));
        fixtures::choose(&mut ctx, 0, "recycle it").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the recycle trigger");
        fixtures::choose(&mut ctx, 0, &format!("{{card {KARMA}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(KARMA));
    }
}

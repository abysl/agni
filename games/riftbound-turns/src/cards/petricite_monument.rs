use super::prelude::{gear, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const DEFLECT: u8 = 1;

pub fn every_friendly_unit(_: &Ctx, _: u32, _: u32) -> bool {
    true
}

pub static CARD: Card = with_statics(
    gear("Petricite Monument", &[Keyword::Temporary], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: every_friendly_unit,
        grants: &[Grant::Keyword(Keyword::Deflect(DEFLECT))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::IMPLICIT_TEMPORARY;
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, priority};
    use crate::state::{ChainItem, ItemKind, Origin, Phase, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const MONUMENT: u32 = 90;
    const SECOND_MONUMENT: u32 = 91;
    const VEX: u32 = 92;

    fn monument(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::gear(id, zone, seat, "Petricite Monument", 2)
        }
    }

    fn garden(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(monument(MONUMENT, zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(VEX, fixtures::BF1, 0, "Vex - Apathetic", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MONUMENT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn deflect_tax(ctx: &Ctx, unit: u32) -> usize {
        let mut item = ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        item.targets.push(TargetRef::Card(unit));
        cost::of_item(ctx, &item, None)
            .power
            .iter()
            .filter(|need| **need == Need::Rainbow)
            .count()
    }

    #[test]
    fn the_script_is_temporary_gear_projecting_deflect_onto_every_friendly_unit() {
        assert!(std::ptr::eq(
            script_of("Petricite Monument").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Temporary]);
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Deflect(DEFLECT))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
        assert_eq!(DEFLECT, 1, "809.1.b.3 · X omitted is 1");
    }

    #[test]
    fn on_the_board_every_friendly_unit_reads_deflect_and_enemy_spells_choosing_them_pay_one_more()
    {
        let mut fixture = garden(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(ctx.is_temporary(MONUMENT));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deflect(1)));
        assert_eq!(ctx.deflect_of(fixtures::VI), 1, "a unit in the base");
        assert_eq!(deflect_tax(&ctx, fixtures::VI), 1);
        assert_eq!(
            ctx.deflect_of(VEX),
            2,
            "809.2 · Vex's printed Deflect and the Monument's are summed"
        );
        assert_eq!(deflect_tax(&ctx, VEX), 2);
        assert!(
            !ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Deflect(1)),
            "an enemy unit is not friendly"
        );
        assert_eq!(ctx.deflect_of(fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.deflect_of(fixtures::SPRITE), 0);
        assert!(
            !ctx.has_keyword(MONUMENT, Keyword::Deflect(1)),
            "the Monument is gear, not a unit"
        );
    }

    #[test]
    fn played_from_hand_the_aura_is_live_at_once_and_two_monuments_sum_to_two() {
        let mut fixture = garden(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(fixtures::VI), 0, "nothing from the hand");
        fixtures::play_from_hand(&mut ctx, 0, MONUMENT).unwrap();
        assert!(ctx.on_board(MONUMENT));
        assert!(
            ctx.blob.chain.is_empty(),
            "gear resolves at once and the Monument has no play trigger"
        );
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        assert_eq!(deflect_tax(&ctx, fixtures::VI), 1);
        drop(ctx);
        let mut fixture = garden(fixtures::BASE);
        fixture
            .table
            .cards
            .push(monument(SECOND_MONUMENT, fixtures::BASE, 0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            2,
            "809.2 · each Monument is an additional source"
        );
        assert_eq!(deflect_tax(&ctx, fixtures::VI), 2);
        drop(ctx);
        let mut fixture = garden(fixtures::BASE);
        fixture.table.card_mut(MONUMENT).unwrap().seat = 1;
        fixture.table.card_mut(MONUMENT).unwrap().owner = 1;
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            0,
            "an opponent's Monument shelters their units"
        );
        assert_eq!(ctx.deflect_of(fixtures::THEIR_UNIT), 1);
    }

    #[test]
    fn the_beginning_phase_kills_the_monument_and_the_deflect_leaves_with_it() {
        let mut fixture = garden(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == MONUMENT && index == IMPLICIT_TEMPORARY
        ));
        assert!(ctx.on_board(MONUMENT), "742.1.b · killed on resolution");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(!ctx.on_board(MONUMENT));
        assert_eq!(ctx.card(MONUMENT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MONUMENT}}} is Temporary and dies")));
        assert_eq!(ctx.deflect_of(fixtures::VI), 0, "the aura is gone");
        assert_eq!(deflect_tax(&ctx, fixtures::VI), 0);
        assert_eq!(ctx.deflect_of(VEX), 1, "Vex keeps her printed Deflect");
        assert!(ctx.fault.is_none());
    }
}

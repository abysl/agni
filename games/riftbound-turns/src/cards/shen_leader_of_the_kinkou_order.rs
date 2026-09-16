use super::prelude::{done, on_hold_me, score_point, unit, when};
use super::shen_scourge_of_shadows::exactly_one_other_unit_you_control_here;
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const SHIELD: u8 = 1;

fn lead(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Shen, Leader of the Kinkou Order",
    &[Keyword::Shield(SHIELD)],
    &[when(
        on_hold_me(&[], lead),
        exactly_one_other_unit_you_control_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::shen_scourge_of_shadows::other_units_you_control_here;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};
    use crate::state::{ItemKind, FLAG_DEFENDER};
    use agni_plugin_sdk::table::CardInfo;

    const SHEN: u32 = 90;
    const ALLY: u32 = 91;
    const SECOND_ALLY: u32 = 92;

    fn shen(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(2),
            domain: vec!["Order".into()],
            ..fixtures::unit(SHEN, zone, seat, "Shen, Leader of the Kinkou Order", 7)
        }
    }

    fn kinkou(company: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shen(fixtures::BF1, 0));
        for id in company {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BF1, 0, "Acolyte", 2));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SHEN).unwrap(), &CARD));
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn shen_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == SHEN))
            .count()
    }

    #[test]
    fn the_script_prints_shield_one_and_a_hold_trigger_sharing_the_scourges_condition() {
        assert!(std::ptr::eq(
            script_of("Shen, Leader of the Kinkou Order").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Shield(SHIELD)]);
        assert_eq!(SHIELD, 1);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::Me));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
    }

    #[test]
    fn shield_counts_only_while_he_defends() {
        let mut fixture = kinkou(&[]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(SHEN), 7);
        ctx.set_flag(SHEN, FLAG_DEFENDER, true);
        assert_eq!(
            ctx.current_might(SHEN),
            8,
            "Shield 1 while he is a defender"
        );
        ctx.set_flag(SHEN, FLAG_DEFENDER, false);
        assert_eq!(ctx.current_might(SHEN), 7);
    }

    #[test]
    fn holding_with_exactly_one_ally_scores_a_second_point_when_the_trigger_resolves() {
        let mut fixture = kinkou(&[ALLY]);
        let mut ctx = fixture.ctx();
        assert_eq!(other_units_you_control_here(&ctx, SHEN), 1);
        hold(&mut ctx);
        assert_eq!(ctx.points(0), 1, "the hold's own point");
        assert_eq!(shen_items(&ctx), 1, "the trigger waits on the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2, "an unrestricted point on top");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn alone_or_with_two_allies_the_hold_scores_its_own_point_only() {
        for company in [&[][..], &[ALLY, SECOND_ALLY][..]] {
            let mut fixture = kinkou(company);
            let mut ctx = fixture.ctx();
            hold(&mut ctx);
            assert_eq!(
                shen_items(&ctx),
                0,
                "383.2.a.1 · exactly one other unit you control is the condition: {company:?}"
            );
            fixtures::pass_until_open(&mut ctx);
            assert_eq!(ctx.points(0), 1);
            assert!(ctx.fault.is_none());
        }
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_an_opponents_hold_is_not_his() {
        let mut fixture = kinkou(&[ALLY]);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            shen_items(&ctx),
            0,
            "383.4.c · a conquer is its own trigger family"
        );
        drop(ctx);

        let mut fixture = kinkou(&[ALLY]);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert_eq!(shen_items(&ctx), 0);
        assert_eq!(ctx.points(1), 1);
    }
}

use super::prelude::{at_battlefield, unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub fn other_friendly_at_her_battlefield(ctx: &Ctx, me: u32, unit: u32) -> bool {
    unit != me
        && at_battlefield(ctx, me)
        && ctx.location(unit) == ctx.location(me)
        && ctx.controller(unit) == ctx.controller(me)
}

pub static CARD: Card = with_statics(
    unit("Allay, Eager Admirer", &[Keyword::Deflect(1)], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: other_friendly_at_her_battlefield,
        grants: &[Grant::Keyword(Keyword::Deflect(1))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, MOVABLE_UNIT};
    use crate::cards::{script_of, Flow};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{statics, targets};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const ALLAY: u32 = 90;
    const FAN: u32 = 91;
    const ZAP: u32 = 92;

    static SPARK: Card = spell(
        "Spark",
        &[],
        &[play(&[a_card(MOVABLE_UNIT, "a unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn admired(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLAY, at, 0, "Allay, Eager Admirer", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(FAN, at, 0, "Fan", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(at);
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(ZAP, fixtures::HAND, 1, "Spark", 1, 1)
        });
        fixture.table.cards.retain(|card| card.id != 45);
        if at == fixtures::BF1 {
            fixture.blob.set_holder(fixtures::BF1, Some(0));
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &SPARK);
        assert!(std::ptr::eq(fixture.scripts.of_card(ALLAY).unwrap(), &CARD));
        fixture
    }

    fn theirs() -> ChainItem {
        ChainItem::new(7, ItemKind::Spell { card: ZAP }, 1, Origin::Hand)
    }

    #[test]
    fn the_script_prints_deflect_and_its_aura_lends_deflect_to_other_friendly_units_here() {
        assert!(std::ptr::eq(
            script_of("Allay, Eager Admirer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Keyword(Keyword::Deflect(1))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
    }

    #[test]
    fn at_a_battlefield_the_fan_beside_her_deflects_and_she_keeps_one_deflect_of_her_own() {
        let mut fixture = admired(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(other_friendly_at_her_battlefield(&ctx, ALLAY, FAN));
        assert!(!other_friendly_at_her_battlefield(&ctx, ALLAY, ALLAY));
        assert!(!other_friendly_at_her_battlefield(
            &ctx,
            ALLAY,
            fixtures::THEIR_UNIT
        ));
        assert!(matches!(
            statics::grants_on(&ctx, FAN).as_slice(),
            [Grant::Keyword(Keyword::Deflect(1))]
        ));
        assert!(ctx.has_keyword(FAN, Keyword::Deflect(1)));
        assert_eq!(ctx.deflect_of(FAN), 1);
        assert_eq!(
            ctx.deflect_of(ALLAY),
            1,
            "her own Deflect is printed, not projected"
        );
        assert!(statics::grants_on(&ctx, ALLAY).is_empty());
        assert_eq!(
            ctx.deflect_of(fixtures::THEIR_UNIT),
            0,
            "the enemy here admires nobody"
        );
        let spec = a_card(MOVABLE_UNIT, "a unit");
        let zap = theirs();
        let listed = targets::candidates(&ctx, &zap, &spec);
        assert!(
            !listed.contains(&TargetRef::Card(FAN)),
            "735 · the one Mind rune pays the spell, nothing is left for the rainbow: {listed:?}"
        );
        assert!(!listed.contains(&TargetRef::Card(ALLAY)));
        assert!(listed.contains(&TargetRef::Card(fixtures::VI)));
        assert!(!targets::deflect_affordable(
            &ctx,
            &zap,
            TargetRef::Card(FAN)
        ));
        assert!(
            !targets::deflect_affordable(&ctx, &zap, TargetRef::Card(ALLAY)),
            "and she costs the same"
        );
        assert!(targets::deflect_affordable(
            &ctx,
            &zap,
            TargetRef::Card(fixtures::VI)
        ));
        let mine = ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(
            targets::deflect_affordable(&ctx, &mine, TargetRef::Card(FAN)),
            "735.1 · Deflect taxes opponents only"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_base_she_lends_nothing_and_a_fan_at_another_battlefield_gets_nothing() {
        let mut fixture = admired(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            !other_friendly_at_her_battlefield(&ctx, ALLAY, FAN),
            "while I'm at a battlefield"
        );
        assert!(statics::grants_on(&ctx, FAN).is_empty());
        assert_eq!(ctx.deflect_of(FAN), 0);
        assert_eq!(ctx.deflect_of(ALLAY), 1);
        assert!(targets::deflect_affordable(
            &ctx,
            &theirs(),
            TargetRef::Card(FAN)
        ));
        drop(ctx);
        let mut fixture = admired(fixtures::BF1);
        fixture.table.card_mut(FAN).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &SPARK);
        let ctx = fixture.ctx();
        assert!(!other_friendly_at_her_battlefield(&ctx, ALLAY, FAN));
        assert_eq!(ctx.deflect_of(FAN), 0, "here means her battlefield");
        assert!(targets::deflect_affordable(
            &ctx,
            &theirs(),
            TargetRef::Card(FAN)
        ));
    }

    #[test]
    fn a_stunned_allay_still_lends_and_one_in_the_trash_or_taken_by_the_enemy_lends_to_theirs() {
        let mut fixture = admired(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(ALLAY));
        assert_eq!(ctx.deflect_of(FAN), 1, "a stun is no exile");
        assert!(ctx.set_controller(ALLAY, 1, fixtures::THEIR_UNIT));
        assert!(
            !other_friendly_at_her_battlefield(&ctx, ALLAY, FAN),
            "she went to their base and the fan is no longer hers"
        );
        assert_eq!(ctx.deflect_of(FAN), 0);
        drop(ctx);
        let mut fixture = admired(fixtures::BF1);
        fixture.table.card_mut(ALLAY).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!statics::in_play(&ctx, ALLAY));
        assert_eq!(ctx.deflect_of(FAN), 0);
    }
}

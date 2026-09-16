use super::prelude::{battlefield, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, TargetRef};

pub static CARD: Card = with_statics(
    battlefield("Heisho, Shell of the World", &[], &[]),
    &[Static::DeflectIgnoredHere(chooses_something_here)],
);

fn here(ctx: &Ctx, heisho: u32) -> Option<Location> {
    match ctx.location(heisho) {
        at @ Some(Location::Battlefield(_)) => at,
        _ => None,
    }
}

pub fn chooses_something_here(
    ctx: &Ctx,
    item: &ChainItem,
    extra: Option<TargetRef>,
    heisho: u32,
) -> bool {
    let Some(at) = here(ctx, heisho) else {
        return false;
    };
    item.targets
        .iter()
        .copied()
        .chain(extra)
        .any(|target| match target {
            TargetRef::Card(card) => ctx.location(card) == Some(at),
            TargetRef::Zone(zone) => Location::Battlefield(zone) == at,
            _ => false,
        })
}

pub fn deflect_ignored_here(ctx: &Ctx, item: &ChainItem, extra: Option<TargetRef>) -> bool {
    ctx.deflect_ignored_here(item, extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const HEISHO: u32 = fixtures::GROUNDS;
    const DEFLECTOR: u32 = 90;
    const DEFLECTOR_ELSEWHERE: u32 = 91;
    const THEIR_SPELL: u32 = 92;

    fn deflector(id: u32, zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, 0, "Jhin - Murderous Artist", 4)
        }
    }

    fn shell() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(HEISHO).unwrap().name = "Heisho, Shell of the World".into();
        fixture
            .table
            .cards
            .push(deflector(DEFLECTOR, fixtures::BF1));
        fixture
            .table
            .cards
            .push(deflector(DEFLECTOR_ELSEWHERE, fixtures::BF2));
        let mut theirs = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Their Spark", 2, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HEISHO).unwrap(),
            &CARD
        ));
        fixture
    }

    fn their_spell_choosing(target: u32) -> ChainItem {
        let mut item = ChainItem::new(1, ItemKind::Spell { card: THEIR_SPELL }, 1, Origin::Hand);
        item.targets = vec![TargetRef::Card(target)];
        item
    }

    #[test]
    fn the_script_is_the_pool_name_and_the_shell_carries_the_table_wide_deflect_static() {
        assert!(std::ptr::eq(
            script_of("Heisho, Shell of the World").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(
            matches!(CARD.statics, [Static::DeflectIgnoredHere(_)]),
            "Static::IgnoresDeflect is read off the paying item only, so the shell carries the table-wide rule"
        );
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn a_spell_choosing_a_unit_here_is_read_as_choosing_something_here_and_one_elsewhere_is_not() {
        let mut fixture = shell();
        let ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(DEFLECTOR), 1);
        let here = their_spell_choosing(DEFLECTOR);
        assert!(chooses_something_here(&ctx, &here, None, HEISHO));
        assert!(deflect_ignored_here(&ctx, &here, None));
        let elsewhere = their_spell_choosing(DEFLECTOR_ELSEWHERE);
        assert!(!chooses_something_here(&ctx, &elsewhere, None, HEISHO));
        assert!(!deflect_ignored_here(&ctx, &elsewhere, None));
        let unchosen = their_spell_choosing(fixtures::THEIR_UNIT);
        assert!(!deflect_ignored_here(&ctx, &unchosen, None));
        let mut none = their_spell_choosing(DEFLECTOR_ELSEWHERE);
        none.targets.clear();
        assert!(
            deflect_ignored_here(&ctx, &none, Some(TargetRef::Card(DEFLECTOR))),
            "a target still being picked counts"
        );
        assert!(
            deflect_ignored_here(&ctx, &none, Some(TargetRef::Zone(fixtures::BF1))),
            "choosing the battlefield itself is choosing something here"
        );
        assert!(!deflect_ignored_here(
            &ctx,
            &none,
            Some(TargetRef::Zone(fixtures::BF2))
        ));
    }

    #[test]
    fn the_shell_reads_nothing_while_it_is_not_at_a_battlefield_or_is_another_card() {
        let mut fixture = shell();
        fixture.table.card_mut(HEISHO).unwrap().name = "Proving Grounds".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!deflect_ignored_here(
            &ctx,
            &their_spell_choosing(DEFLECTOR),
            None
        ));
        drop(ctx);
        let mut fixture = shell();
        fixture.table.card_mut(HEISHO).unwrap().zone = Some(fixtures::SIDEBOARD);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(here(&ctx, HEISHO).is_none());
        assert!(!chooses_something_here(
            &ctx,
            &their_spell_choosing(DEFLECTOR),
            None,
            HEISHO
        ));
    }

    #[test]
    fn another_battlefield_under_the_same_name_still_charges_the_deflect() {
        let mut fixture = shell();
        fixture.table.card_mut(HEISHO).unwrap().name = "Proving Grounds".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        let here = their_spell_choosing(DEFLECTOR);
        assert!(!cost::ignores_deflect(&ctx, &here));
        assert!(!deflect_ignored_here(&ctx, &here, None));
        assert_eq!(cost::deflect(&ctx, &here, None), 1);
        let priced = cost::of_item(&ctx, &here, None);
        assert_eq!(priced.energy, 2);
        assert_eq!(
            priced.power,
            [Need::Domain(crate::cards::Domain::Mind), Need::Rainbow],
            "809.1.c · the rainbow only the shell waives"
        );
    }

    #[test]
    fn a_spell_choosing_a_deflect_unit_at_the_shell_pays_no_rainbow() {
        let mut fixture = shell();
        let ctx = fixture.ctx();
        let here = their_spell_choosing(DEFLECTOR);
        assert!(!cost::ignores_deflect(&ctx, &here));
        assert!(deflect_ignored_here(&ctx, &here, None));
        assert_eq!(cost::deflect(&ctx, &here, None), 0);
        let priced = cost::of_item(&ctx, &here, None);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(crate::cards::Domain::Mind)]);
        let elsewhere = their_spell_choosing(DEFLECTOR_ELSEWHERE);
        assert_eq!(
            cost::deflect(&ctx, &elsewhere, None),
            1,
            "the other battlefield is not the shell"
        );
    }
}

use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

fn other_friendly(ctx: &Ctx, me: u32, unit: u32) -> bool {
    unit != me && ctx.controller(unit) == ctx.controller(me)
}

pub fn outmuscled_by_a_wildclaw_here(ctx: &Ctx, unit: u32) -> bool {
    let Some(here) = ctx.location(unit) else {
        return false;
    };
    let mut wildclaws: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
        .collect();
    wildclaws.sort_unstable();
    wildclaws.into_iter().any(|wildclaw| {
        wildclaw != unit
            && statics::in_play(ctx, wildclaw)
            && ctx.location(wildclaw) == Some(here)
            && ctx.controller(wildclaw) == ctx.controller(unit)
            && ctx.current_might(unit) < ctx.current_might(wildclaw)
    })
}

pub static CARD: Card = with_statics(
    unit("Alpha Wildclaw", &[Keyword::Tank], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: other_friendly,
        grants: &[Grant::Static(Static::Untargetable(
            outmuscled_by_a_wildclaw_here,
        ))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, this_turn};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::targets;
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};

    const WILDCLAW: u32 = 90;
    const CUB: u32 = 91;
    const PEER: u32 = 92;
    const MIGHT: u8 = 7;

    fn pride(at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(WILDCLAW, at, 0, "Alpha Wildclaw", MIGHT));
        fixture
            .table
            .cards
            .push(fixtures::unit(CUB, at, 0, "Cub", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(PEER, at, 0, "Peer", MIGHT));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(at);
        if at == fixtures::BF1 {
            fixture.blob.set_holder(fixtures::BF1, Some(0));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WILDCLAW).unwrap(),
            &CARD
        ));
        fixture
    }

    fn theirs() -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        )
    }

    fn mine() -> ChainItem {
        ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        )
    }

    fn offered(ctx: &Ctx, item: &ChainItem) -> Vec<u32> {
        targets::candidates(ctx, item, &a_unit("a unit"))
            .into_iter()
            .filter_map(|target| match target {
                TargetRef::Card(card) => Some(card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_prints_tank_and_its_aura_projects_untargetable_over_the_smaller_units_here() {
        assert!(std::ptr::eq(script_of("Alpha Wildclaw").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Static(Static::Untargetable(_))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
    }

    #[test]
    fn an_enemy_spell_cannot_choose_the_cub_beside_him_but_lists_his_peer_and_the_alpha_himself() {
        let mut fixture = pride(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(outmuscled_by_a_wildclaw_here(&ctx, CUB));
        assert!(
            !outmuscled_by_a_wildclaw_here(&ctx, PEER),
            "less, not equal"
        );
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, WILDCLAW));
        assert!(
            !outmuscled_by_a_wildclaw_here(&ctx, fixtures::THEIR_UNIT),
            "your units only"
        );
        assert!(matches!(
            ctx.projected_statics(CUB).as_slice(),
            [Static::Untargetable(_)]
        ));
        assert!(ctx.projected_statics(WILDCLAW).is_empty());
        let theirs = theirs();
        let listed = offered(&ctx, &theirs);
        assert!(!listed.contains(&CUB), "{listed:?}");
        assert!(listed.contains(&PEER));
        assert!(listed.contains(&WILDCLAW), "Tank is his only guard");
        assert!(targets::untargetable(&ctx, &theirs, TargetRef::Card(CUB)));
        assert!(!targets::untargetable(&ctx, &theirs, TargetRef::Card(PEER)));
        let mine = offered(&ctx, &mine());
        assert!(
            mine.contains(&CUB),
            "a friendly spell or ability chooses freely: {mine:?}"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_guard_follows_might_a_shrunken_alpha_or_a_grown_cub_lifts_it() {
        let mut fixture = pride(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let until = this_turn(&ctx);
        ctx.might(CUB, 4, until, None, 0);
        assert_eq!(ctx.current_might(CUB), 7);
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, CUB));
        assert!(offered(&ctx, &theirs()).contains(&CUB));
        ctx.might(WILDCLAW, -1, until, None, 0);
        assert_eq!(ctx.current_might(WILDCLAW), 6);
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, PEER));
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, CUB));
        ctx.might(WILDCLAW, 3, until, None, 0);
        assert!(outmuscled_by_a_wildclaw_here(&ctx, PEER), "9 over 7");
        assert!(!offered(&ctx, &theirs()).contains(&PEER));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_cub_elsewhere_an_alpha_in_base_lending_to_base_and_an_alpha_in_the_trash_read_by_the_rule()
    {
        let mut fixture = pride(fixtures::BF1);
        fixture.table.card_mut(CUB).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, CUB), "here only");
        assert!(ctx.projected_statics(CUB).is_empty());
        assert!(offered(&ctx, &theirs()).contains(&CUB));
        drop(ctx);
        let mut fixture = pride(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            outmuscled_by_a_wildclaw_here(&ctx, CUB),
            "here is wherever he stands, the base included"
        );
        assert!(!offered(&ctx, &theirs()).contains(&CUB));
        drop(ctx);
        let mut fixture = pride(fixtures::BF1);
        fixture.table.card_mut(WILDCLAW).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!outmuscled_by_a_wildclaw_here(&ctx, CUB));
        assert!(offered(&ctx, &theirs()).contains(&CUB));
    }
}

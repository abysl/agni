use super::prelude::{unit, with_statics};
use super::{Card, Keyword, Static};

pub static CARD: Card = with_statics(
    unit("Rengar - Trophy Hunter", &[Keyword::Ambush], &[]),
    &[Static::AmbushIntoEnemies],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{act, settle};
    use crate::state::{Origin, Phase};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::view::LegalKind;

    const RENGAR: u32 = 90;

    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        let mut rengar = fixtures::unit(RENGAR, fixtures::HAND, 0, "Rengar - Trophy Hunter", 6);
        rengar.energy = Some(0);
        rengar.domain = vec!["Body".into()];
        fixture.table.cards.push(rengar);
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: RENGAR,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn mid_chain(fixture: &mut Fixture) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        ctx
    }

    #[test]
    fn the_script_prints_ambush_into_enemies() {
        assert!(std::ptr::eq(
            script_of("Rengar - Trophy Hunter").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Ambush));
        assert!(CARD.has_static(Static::AmbushIntoEnemies));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, RENGAR),
            [Location::Battlefield(fixtures::BF2)],
            "the Sprite's battlefield is an ambush location, the empty one is not"
        );
    }

    #[test]
    fn rengar_ambushes_a_battlefield_held_by_the_enemy_while_a_spell_is_on_the_chain() {
        let mut fixture = armed();
        let mut ctx = mid_chain(&mut fixture);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Ok(Intent::Play {
                card: RENGAR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "the base keeps Rengar's own timing"
        );
        let rows = legal::highlights(&ctx, 0);
        let row = rows.iter().find(|row| row.card == RENGAR).expect("a row");
        assert_eq!(row.kinds, [LegalKind::React]);
        assert_eq!(
            row.zones,
            [fixtures::BF2, fixtures::CHAIN],
            "the drag to the chain is allowed so the location prompt can name the ambush"
        );
        ctx.table
            .apply_entry(&fixtures::move_action(RENGAR, fixtures::BF2, 0), 0)
            .unwrap();
        act(
            &mut ctx,
            0,
            Intent::Play {
                card: RENGAR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(RENGAR),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.card(RENGAR).unwrap().exhausted);
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(ctx.blob.staged.len(), 1);
        assert!(ctx.blob.staged[0].combat);
        assert_eq!(ctx.blob.staged[0].contester, 0);
        assert!(
            ctx.blob.showdown.is_none(),
            "nothing opens while the chain is closed"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the combat opens after the chain");
        assert_eq!(showdown.zone, fixtures::BF2);
        assert_eq!(showdown.attacker, 0);
        assert!(showdown.combat);
    }

    #[test]
    fn a_plain_ambush_unit_is_refused_where_only_enemies_stand_and_rockfall_path_is_never_listed() {
        let mut fixture = armed();
        fixture.table.card_mut(RENGAR).unwrap().name = "Kha'Zix - Mutating Horror".into();
        fixture.resolve();
        let ctx = mid_chain(&mut fixture);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
        let mut rockfall = armed();
        rockfall.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF2);
        rockfall.resolve();
        let ctx = mid_chain(&mut rockfall);
        assert!(ctx.ambush_locations(0, RENGAR).is_empty());
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NoUnitsPlayedHere))
        );
    }

    #[test]
    fn the_ambush_is_taken_back_at_payment_when_its_battlefield_emptied_meanwhile() {
        let mut fixture = armed();
        fixture.table.card_mut(RENGAR).unwrap().name = "Kha'Zix - Mutating Horror".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = mid_chain(&mut fixture);
        ctx.table
            .apply_entry(&fixtures::move_action(RENGAR, fixtures::BF1, 0), 0)
            .unwrap();
        crate::engine::play::begin(
            &mut ctx,
            0,
            RENGAR,
            Origin::Hand,
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(RENGAR), "with Vi there the ambush lands");
        let mut emptied = armed();
        emptied.table.card_mut(RENGAR).unwrap().name = "Kha'Zix - Mutating Horror".into();
        emptied.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        emptied.resolve();
        let mut ctx = mid_chain(&mut emptied);
        ctx.table
            .apply_entry(&fixtures::move_action(RENGAR, fixtures::BF1, 0), 0)
            .unwrap();
        let id = ctx.blob.next_item_id();
        let mut item = crate::state::ChainItem::new(
            id,
            crate::state::ItemKind::Permanent { card: RENGAR },
            0,
            Origin::Hand,
        );
        item.targets
            .push(crate::state::TargetRef::Zone(fixtures::BF1));
        item.stage = crate::engine::play::STAGE_PAY;
        ctx.blob.queue.push(crate::state::Pending {
            item,
            needs: crate::state::Needs::Choices,
        });
        ctx.bounce(fixtures::VI);
        crate::engine::play::advance(&mut ctx, id).unwrap();
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.card(RENGAR).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("can no longer be played there")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
    }
}

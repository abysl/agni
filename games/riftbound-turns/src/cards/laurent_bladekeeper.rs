use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Laurent Bladekeeper", &[Keyword::Ganking], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, legal, march, settle};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BLADEKEEPER: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn bladekeeper(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Body".into()],
            ..fixtures::unit(BLADEKEEPER, zone, seat, "Laurent Bladekeeper", MIGHT)
        }
    }

    fn afield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bladekeeper(fixtures::BF1, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BLADEKEEPER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drag(fixture: &mut Fixture, unit: u32, to: u16) -> Result<Ctx<'_>, Refusal> {
        let action = fixtures::move_action(unit, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry)?;
        act(&mut ctx, 0, intent)?;
        settle(&mut ctx)?;
        Ok(ctx)
    }

    #[test]
    fn the_script_is_a_ganking_unit_with_no_abilities() {
        assert!(std::ptr::eq(
            script_of("Laurent Bladekeeper").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Laurent Bladekeeper");
        assert_eq!(CARD.keywords, &[Keyword::Ganking]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = afield();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(BLADEKEEPER, Keyword::Ganking));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
    }

    #[test]
    fn it_marches_from_one_battlefield_to_another_and_contests_the_enemy_one() {
        let mut fixture = afield();
        let ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                BLADEKEEPER,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "143.4 · Ganking opens the battlefield-to-battlefield route"
        );
        drop(ctx);
        let ctx = drag(&mut fixture, BLADEKEEPER, fixtures::BF2).unwrap();
        assert_eq!(
            ctx.location(BLADEKEEPER),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "seat 1 holds it, so the march is an attack"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_unit_without_ganking_is_refused_the_same_march() {
        let mut fixture = afield();
        let ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        drop(ctx);
        let refused = drag(&mut fixture, fixtures::VI, fixtures::BF2).err();
        assert_eq!(refused, Some(Refusal::Illegal(Reason::NeedsGanking)));
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "Vi stays put"
        );
    }
}

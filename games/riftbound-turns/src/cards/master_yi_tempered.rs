use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};

pub const HUNT: u8 = 2;
pub const LEVEL: u8 = 6;

pub static LEVELED: &[Grant] = &[
    Grant::Keyword(Keyword::Deflect(1)),
    Grant::Keyword(Keyword::Ganking),
];

pub static CARD: Card = with_statics(
    unit("Master Yi - Tempered", &[Keyword::Hunt(HUNT)], &[]),
    &[Static::Level(LEVEL, LEVELED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, ENEMY_UNIT};
    use crate::cards::{Flow, IMPLICIT_HUNT};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, priority, statics, targets, triggers};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const YI: u32 = 90;
    const HEX: u32 = 91;

    static HEX_CARD: Card = spell(
        "Hex",
        &[],
        &[play(&[a_card(ENEMY_UNIT, "an enemy unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn yi(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            might: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::card(YI, zone, seat, "Master Yi - Tempered", "Unit")
        }
    }

    fn training(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(yi(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(YI).unwrap(), &CARD));
        fixture
    }

    fn hex_item() -> ChainItem {
        let mut item = ChainItem::new(7, ItemKind::Spell { card: HEX }, 1, Origin::Hand);
        item.stage = crate::engine::play::STAGE_TARGET;
        item
    }

    #[test]
    fn the_script_prints_hunt_two_and_a_level_six_that_grants_deflect_and_ganking() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Master Yi - Tempered").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Hunt(2)));
        assert_eq!(CARD.hunt(), 2);
        assert!(
            !CARD.has_keyword(Keyword::Deflect(1)),
            "Deflect is the Level's, not printed"
        );
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(matches!(
            CARD.statics,
            [Static::Level(
                6,
                [
                    Grant::Keyword(Keyword::Deflect(1)),
                    Grant::Keyword(Keyword::Ganking)
                ]
            )]
        ));
    }

    #[test]
    fn at_six_xp_he_has_deflect_and_ganking_and_at_five_he_has_neither() {
        let mut fixture = training(5);
        let ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(YI), 0);
        assert!(!ctx.has_keyword(YI, Keyword::Ganking));
        assert!(statics::grants_on(&ctx, YI).is_empty());
        assert_eq!(
            march::legal_destination(
                &ctx,
                YI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "at 5 XP a battlefield-to-battlefield move is refused"
        );
        drop(ctx);

        let mut fixture = training(6);
        let ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(YI), 1, "824.1.c · Level 6 at 6 XP");
        assert!(ctx.has_keyword(YI, Keyword::Ganking));
        assert_eq!(statics::grants_on(&ctx, YI).len(), 2);
        assert_eq!(
            march::legal_destination(
                &ctx,
                YI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "736 · the granted Ganking lets him march battlefield to battlefield"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_spell_choosing_him_costs_one_more_rainbow_only_at_level_six() {
        let mut fixture = training(6);
        fixture
            .table
            .cards
            .push(fixtures::spell(HEX, fixtures::HAND, 1, "Hex", 1, 0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HEX, &HEX_CARD);
        let mut ctx = fixture.ctx();
        let item = hex_item();
        let with = cost::of_item(&ctx, &item, Some(TargetRef::Card(YI)));
        assert_eq!(with.energy, 1);
        assert_eq!(
            with.power,
            [Need::Rainbow],
            "the enemy Charm-shaped spell pays a rainbow for the projected Deflect"
        );
        assert!(!targets::untargetable(&ctx, &item, TargetRef::Card(YI)));
        assert!(ctx.spend_xp(0, 2));
        let item = hex_item();
        let without = cost::of_item(&ctx, &item, Some(TargetRef::Card(YI)));
        assert!(
            without.power.is_empty(),
            "the projection has nothing cached: the Deflect is gone with the XP"
        );
        let mut own = ChainItem::new(8, ItemKind::Spell { card: HEX }, 0, Origin::Hand);
        own.stage = crate::engine::play::STAGE_TARGET;
        ctx.score_xp(0, 2);
        let friendly = cost::of_item(&ctx, &own, Some(TargetRef::Card(YI)));
        assert!(friendly.power.is_empty(), "Deflect binds only opponents");
    }

    #[test]
    fn conquering_gains_two_xp_after_the_chain_and_holding_gains_two_more() {
        let mut fixture = training(4);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(YI), 2);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![YI],
        });
        triggers::collect(&mut ctx);
        assert_eq!(ctx.blob.queue.len(), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == YI && index == IMPLICIT_HUNT
        ));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "383.4.c.2.a · the hunt waits on the chain"
        );
        assert_eq!(ctx.xp(0), 4);
        assert!(!ctx.has_keyword(YI, Keyword::Ganking));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 6);
        assert!(
            ctx.has_keyword(YI, Keyword::Ganking),
            "the sixth XP lands and the next check reads Level 6"
        );
        assert_eq!(ctx.deflect_of(YI), 1);
        ctx.raise(Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![YI],
        });
        triggers::collect(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 8);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_xp_does_not_level_him_and_a_conquer_by_another_unit_hunts_nothing() {
        let mut fixture = training(0);
        fixture.set_xp(1, 6);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.xp(1), 6);
        assert!(
            !ctx.has_keyword(YI, Keyword::Ganking),
            "824.1.c · Level reads his controller's XP"
        );
        assert_eq!(ctx.deflect_of(YI), 0);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        });
        triggers::collect(&mut ctx);
        assert!(
            ctx.blob.queue.is_empty(),
            "a battlefield he did not conquer this scoring gains nothing"
        );
        assert_eq!(
            march::legal_destination(
                &ctx,
                YI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
    }
}

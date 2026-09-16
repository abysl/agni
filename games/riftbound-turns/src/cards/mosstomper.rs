use super::prelude::{unit, with_statics};
use super::{Card, Grant, Keyword, Static};

pub const HUNT: u8 = 2;
pub const LEVEL: u8 = 3;
pub const BONUS: i16 = 1;

pub static LEVELED: &[Grant] = &[Grant::Might(BONUS), Grant::Keyword(Keyword::Deflect(1))];

pub static CARD: Card = with_statics(
    unit("Mosstomper", &[Keyword::Hunt(HUNT)], &[]),
    &[Static::Level(LEVEL, LEVELED)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, ENEMY_UNIT};
    use crate::cards::{script_of, Flow, IMPLICIT_HUNT};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, statics, targets, triggers};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const STOMPER: u32 = 90;
    const HEX: u32 = 91;

    static HEX_CARD: Card = spell(
        "Hex",
        &[],
        &[play(&[a_card(ENEMY_UNIT, "an enemy unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn stomper(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(STOMPER, zone, seat, "Mosstomper", 3)
        }
    }

    fn moss(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stomper(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(HEX, fixtures::HAND, 1, "Hex", 1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HEX, &HEX_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(STOMPER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn hex_item(controller: u8) -> ChainItem {
        let mut item = ChainItem::new(7, ItemKind::Spell { card: HEX }, controller, Origin::Hand);
        item.stage = crate::engine::play::STAGE_TARGET;
        item
    }

    #[test]
    fn the_script_prints_hunt_two_and_a_level_three_of_plus_one_and_deflect() {
        assert!(std::ptr::eq(script_of("Mosstomper").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(2)]);
        assert_eq!(CARD.hunt(), 2);
        assert!(
            !CARD.has_keyword(Keyword::Deflect(1)),
            "Deflect is the Level's, not printed"
        );
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(matches!(
            CARD.statics,
            [Static::Level(
                3,
                [Grant::Might(1), Grant::Keyword(Keyword::Deflect(1))]
            )]
        ));
    }

    #[test]
    fn at_three_xp_it_is_a_four_with_deflect_and_at_two_a_plain_three() {
        let mut fixture = moss(2);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(STOMPER), 3);
        assert_eq!(ctx.deflect_of(STOMPER), 0);
        assert!(statics::grants_on(&ctx, STOMPER).is_empty());
        let item = hex_item(1);
        let plain = cost::of_item(&ctx, &item, Some(TargetRef::Card(STOMPER)));
        assert!(plain.power.is_empty(), "no Deflect to pay for at 2 XP");
        drop(ctx);

        let mut fixture = moss(3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(STOMPER), 4, "824.1.c · Level 3 at 3 XP");
        assert_eq!(ctx.deflect_of(STOMPER), 1);
        assert_eq!(statics::grants_on(&ctx, STOMPER).len(), 2);
        let item = hex_item(1);
        let with = cost::of_item(&ctx, &item, Some(TargetRef::Card(STOMPER)));
        assert_eq!(with.energy, 1);
        assert_eq!(
            with.power,
            [Need::Rainbow],
            "the enemy spell pays a rainbow for the projected Deflect"
        );
        assert!(!targets::untargetable(
            &ctx,
            &item,
            TargetRef::Card(STOMPER)
        ));
        let own = hex_item(0);
        let friendly = cost::of_item(&ctx, &own, Some(TargetRef::Card(STOMPER)));
        assert!(friendly.power.is_empty(), "Deflect binds only opponents");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_xp_does_not_level_it() {
        let mut fixture = moss(0);
        fixture.set_xp(1, 3);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(STOMPER), 3);
        assert_eq!(
            ctx.deflect_of(STOMPER),
            0,
            "Level reads its controller's XP"
        );
    }

    #[test]
    fn conquering_hunts_two_xp_after_the_chain_and_levels_it() {
        let mut fixture = moss(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(STOMPER), 2);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![STOMPER],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == STOMPER && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.deflect_of(STOMPER), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 3);
        assert_eq!(ctx.current_might(STOMPER), 4);
        assert_eq!(ctx.deflect_of(STOMPER), 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}

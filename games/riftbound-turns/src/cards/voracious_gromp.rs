use super::prelude::unit;
use super::{Card, Keyword};

pub const HUNT: u8 = 3;

pub static CARD: Card = unit("Voracious Gromp", &[Keyword::Hunt(HUNT)], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, IMPLICIT_HUNT};
    use crate::engine::ctx::{Ctx, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, targets, triggers};
    use crate::state::{ItemKind, TargetRef};

    const GROMP: u32 = 90;

    fn afield(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(GROMP, zone, 0, "Voracious Gromp", 5));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GROMP).unwrap(), &CARD));
        fixture
    }

    fn resolves(ctx: &mut Ctx, event: Event) -> usize {
        ctx.raise(event);
        let found = triggers::collect(ctx);
        settle(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        found
    }

    #[test]
    fn the_script_is_a_hunt_three_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Voracious Gromp").unwrap(), &CARD));
        assert_eq!(CARD.name, "Voracious Gromp");
        assert_eq!(CARD.keywords, &[Keyword::Hunt(3)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = afield(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(GROMP, Keyword::Hunt(3)));
        assert_eq!(ctx.hunt_value(GROMP), HUNT);
    }

    #[test]
    fn a_conquer_and_a_hold_each_queue_the_implicit_hunt_and_pay_three_xp_when_it_resolves() {
        let mut fixture = afield(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![GROMP],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "823 · the hunt waits on the chain");
        let item = &ctx.blob.chain[0];
        assert!(matches!(
            item.kind,
            ItemKind::Trigger { source, index } if source == GROMP && index == IMPLICIT_HUNT
        ));
        assert_eq!(item.subject, Some(TargetRef::Zone(fixtures::BF1)));
        assert!(std::ptr::eq(
            targets::ability_of(&ctx, item).unwrap(),
            &targets::HUNT_ABILITY
        ));
        assert_eq!(ctx.xp(0), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(HUNT));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {GROMP}}} hunts · {{seat 0}} gains {HUNT} XP"
        )));
        assert_eq!(
            resolves(
                &mut ctx,
                Event::Held {
                    zone: fixtures::BF1,
                    seat: 0,
                    units: vec![GROMP],
                }
            ),
            1
        );
        assert_eq!(ctx.xp(0), 2 * i32::from(HUNT), "a hold hunts as well");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_conquer_by_another_unit_or_one_it_did_not_take_part_in_gives_nothing() {
        let mut fixture = afield(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            resolves(
                &mut ctx,
                Event::Conquered {
                    zone: fixtures::BF2,
                    seat: 0,
                    units: vec![fixtures::VI],
                }
            ),
            0
        );
        assert_eq!(ctx.xp(0), 0);
        assert_eq!(
            resolves(
                &mut ctx,
                Event::Held {
                    zone: fixtures::BF2,
                    seat: 1,
                    units: vec![fixtures::SPRITE],
                }
            ),
            0
        );
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.chain.is_empty());
    }
}

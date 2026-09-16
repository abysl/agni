use super::prelude::{done, equip, gear, on_hold_me, score_point, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 2;
pub const ON_HOLD: u8 = GRANTED;

fn score(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_hold_me(&[], score)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Trinity Force", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, held, queue_granted, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::{ItemKind, TargetRef};

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Trinity Force", 4, "Body"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_body_equipment_with_two_might_and_a_wearer_hold_listener() {
        assert!(std::ptr::eq(script_of("Trinity Force").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].label, Some("equip"));
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::Hold(Who::Me));
        assert!(listener.condition.is_none());
        assert!(listener.targets.is_empty());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(2), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_hold_scores_one_point_and_a_conquer_is_not_a_hold() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        let found = triggers::find(&ctx, &held(&[fixtures::VI]));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].kind(),
            ItemKind::Granted {
                holder: fixtures::VI,
                lender: GEAR,
                index: ON_HOLD
            }
        );
        assert!(triggers::find(&ctx, &held(&[fixtures::SPRITE])).is_empty());
        assert_eq!(
            triggers::matches(
                &ctx,
                WEARER_TEXT[0].trigger,
                &conquered(&[fixtures::VI]),
                fixtures::VI
            ),
            None,
            "a Hold trigger ignores a conquer"
        );
        assert_eq!(ctx.points(0), 0);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_HOLD,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.points(1), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_loose_trinity_force_hears_nothing_and_an_attached_one_hears_the_wearer() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(triggers::find(&ctx, &held(&[fixtures::VI])).is_empty());
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 0, "no wearer, no hold of mine");
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_hold_by_the_wearer_scores_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.points(0), 1);
    }
}

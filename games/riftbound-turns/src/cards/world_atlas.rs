use super::prelude::{done, equip, gear, on_hold_me, spawn_gold, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};

pub const MIGHT_BONUS: i16 = 2;
pub const GOLDS: usize = 2;
pub const ON_HOLD: u8 = GRANTED;

fn chart(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for _ in 0..GOLDS {
        spawn_gold(ctx, item.controller, false);
    }
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_hold_me(&[], chart)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("World Atlas", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, golds_of, held, queue_granted, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Static, Trigger, Who, TOKEN_GOLD};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::{ItemKind, TargetRef};

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "World Atlas", 3, "Mind"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_mind_equipment_with_two_might_and_a_wearer_hold_listener() {
        assert!(std::ptr::eq(script_of("World Atlas").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
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
    fn the_wearers_hold_plays_two_golds_exhausted() {
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
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_HOLD,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds_of(&ctx, 0), GOLDS);
        assert_eq!(golds_of(&ctx, 1), 0);
        assert!(ctx
            .table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD)
            .all(|card| card.exhausted));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{seat 0} gains a Gold")
                .count(),
            GOLDS
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn another_units_hold_and_a_loose_atlas_give_nothing_and_the_engine_hears_the_wearer() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(triggers::find(&ctx, &held(&[fixtures::VI])).is_empty());
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(triggers::find(&ctx, &held(&[fixtures::SPRITE])).is_empty());
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
        assert_eq!(golds_of(&ctx, 0), 0, "queued, not yet resolved");
    }

    #[test]
    fn a_hold_by_the_wearer_plays_the_golds_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 0), GOLDS);
    }
}

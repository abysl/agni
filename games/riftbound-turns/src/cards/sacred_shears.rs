use super::prelude::{deathknell, done, draw, equip, gear, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const DRAWS: usize = 1;
pub const DEATHKNELL: u8 = GRANTED;

pub fn the_wearers_deathknell_draws(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [deathknell(&[], the_wearers_deathknell_draws)];

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Deathknell),
    Grant::Might(MIGHT_BONUS),
    Grant::Ability(&WEARER_TEXT),
];

pub static CARD: Card = with_statics(
    gear("Sacred Shears", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Event, Static, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::{ChainItem, ItemKind, Noted, Origin, TargetRef};

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut shears = equipment(GEAR, 0, "Sacred Shears", 2, "Order");
        shears.power = Some(1);
        fixture.table.cards.push(shears);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn noted() -> Noted {
        Noted {
            zone: fixtures::BF1,
            might: 3,
            controller: 0,
            alone: true,
            buffed: false,
        }
    }

    fn died(card: u32) -> Event {
        Event::Died {
            card,
            controller: 0,
            unit: true,
            noted: noted(),
        }
    }

    fn the_trigger(dead: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Granted {
                holder: dead,
                lender: GEAR,
                index: DEATHKNELL,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(TargetRef::Card(dead));
        item
    }

    #[test]
    fn the_script_grants_deathknell_one_might_and_the_knell_while_attached_and_carries_only_its_equip(
    ) {
        assert!(std::ptr::eq(script_of("Sacred Shears").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert!(
            !CARD.has_keyword(Keyword::Deathknell),
            "the Deathknell is the wearer's, not the shears'"
        );
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].label, Some("equip"));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [
                Grant::Keyword(Keyword::Deathknell),
                Grant::Might(1),
                Grant::Ability(_)
            ]
        ));
        let knell = &WEARER_TEXT[0];
        assert_eq!(knell.trigger, Trigger::Death, "the wearer's own Deathknell");
        assert!(knell.condition.is_none(), "136.2.c · I am the wearer");
        assert!(knell.targets.is_empty());
    }

    #[test]
    fn the_wearer_reads_deathknell_and_the_knell_is_its_death_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(triggers::deathknells(&ctx, fixtures::VI, noted()).is_empty());
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deathknell));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        let knells = triggers::deathknells(&ctx, fixtures::VI, noted());
        assert_eq!(knells.len(), 1);
        assert_eq!(
            knells[0].kind(),
            ItemKind::Granted {
                holder: fixtures::VI,
                lender: GEAR,
                index: DEATHKNELL
            }
        );
        assert_eq!(knells[0].subject, Some(TargetRef::Card(fixtures::VI)));
        assert!(triggers::deathknells(&ctx, fixtures::THEIR_UNIT, noted()).is_empty());
        assert!(triggers::find(&ctx, &died(fixtures::THEIR_UNIT)).is_empty());
        assert!(triggers::find(&ctx, &Event::Attacks { card: fixtures::VI }).is_empty());
        ctx.detach(GEAR);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Deathknell));
        assert!(triggers::deathknells(&ctx, fixtures::VI, noted()).is_empty());
    }

    #[test]
    fn the_run_draws_one_for_the_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        assert_eq!(
            the_wearers_deathknell_draws(&mut ctx, &the_trigger(fixtures::VI), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), theirs);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wearers_death_draws_one_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }
}

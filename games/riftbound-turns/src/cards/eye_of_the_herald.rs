use super::faithful_manufactor::play_recruits;
use super::prelude::{done, equip, gear, location_of, triggered, while_attached, with_statics};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, Trigger, Where, Who,
    GRANTED,
};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};

pub const MIGHT_BONUS: i16 = 0;
pub const RECRUITS: usize = 1;
pub const ON_MOVE: u8 = GRANTED;

fn herald(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} is off the board · no Recruit"));
        return done();
    };
    play_recruits(ctx, item.controller, here, RECRUITS);
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [triggered(
    Trigger::Move {
        of: Who::Me,
        to: Where::Any,
    },
    &[],
    herald,
)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear(
        "Eye of the Herald",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, queue_granted, GEAR};
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::prelude::{attach_gear, battlefield, with_statics, Location, MoveCause};
    use crate::cards::{script_of, Static};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::TargetRef;

    const BARE_GROUND: u32 = 92;

    static NO_RECRUITS: Card = with_statics(
        battlefield("Sealed Gate", &[], &[]),
        &[Static::NoUnitsPlayedHere],
    );

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Eye of the Herald", 1, "Order"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn moved(card: u32, to: Location) -> Event {
        Event::Moved {
            card,
            from: Some(Location::Base(0)),
            to,
            cause: MoveCause::Standard,
            by: None,
        }
    }

    #[test]
    fn the_script_is_an_order_equipment_with_no_might_bonus_and_a_wearer_move_listener() {
        assert!(std::ptr::eq(script_of("Eye of the Herald").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
        let listener = &WEARER_TEXT[0];
        assert_eq!(
            listener.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(listener.condition.is_none());
        assert!(listener.targets.is_empty());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(0), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_move_plays_a_recruit_where_it_arrived() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(
            triggers::find(
                &ctx,
                &moved(fixtures::VI, Location::Battlefield(fixtures::BF1))
            )
            .len(),
            1
        );
        assert!(triggers::find(
            &ctx,
            &moved(fixtures::THEIR_UNIT, Location::Battlefield(fixtures::BF1))
        )
        .is_empty());
        assert_eq!(
            ctx.location(GEAR),
            Some(Location::Battlefield(fixtures::BF1)),
            "421.4 · the eye stands where the wearer stands"
        );
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_MOVE,
            TargetRef::Card(fixtures::VI),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits.len(), RECRUITS);
        let recruit = recruits[0];
        assert_eq!(
            ctx.location(recruit),
            Some(Location::Battlefield(fixtures::BF1)),
            "here is where the wearer stands after the move"
        );
        assert_eq!(ctx.current_might(recruit), i32::from(RECRUIT_MIGHT));
        assert!(ctx.card(recruit).unwrap().exhausted, "played, so exhausted");
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn no_recruit_where_units_cannot_be_played_nor_for_a_dead_wearer_and_the_engine_hears_it() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Sealed Gate".into();
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_RECRUITS);
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_MOVE,
            TargetRef::Card(fixtures::VI),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Recruit · units can't be played at")));
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_MOVE,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is off the board · no Recruit".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(moved(fixtures::VI, Location::Battlefield(fixtures::BF2)));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_move_by_the_wearer_plays_a_recruit_through_the_engine() {
        let mut fixture = armed();
        fixture.table.cards.push(fixtures::card(
            BARE_GROUND,
            fixtures::BF3,
            0,
            "Bare Ground",
            "Battlefield",
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF3),
            MoveCause::Standard,
        );
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(recruits_of(&ctx, 0).len(), 1);
    }
}

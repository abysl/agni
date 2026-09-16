use super::prelude::{done, equip, gear, might_this_turn, triggered, while_attached, with_statics};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, Trigger, Where, Who,
    GRANTED,
};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const SWING: i16 = 2;
pub const ON_MOVE: u8 = GRANTED;

fn swing(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} is off the board · no swing"));
        return done();
    }
    might_this_turn(ctx, item, me, SWING, None);
    ctx.narrate(format!("{{card {me}}} swings for +{SWING} Might this turn"));
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [triggered(
    Trigger::Move {
        of: Who::Me,
        to: Where::Battlefield,
    },
    &[],
    swing,
)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Pendulum Blade", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, queue_granted, GEAR};
    use crate::cards::prelude::{attach_gear, attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing};
    use crate::engine::ctx::{Cause, Event, Killed, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle, triggers};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;

    const EQUIP_INDEX: u8 = 0;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Pendulum Blade", 3, "Fury"));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GEAR).unwrap(), &CARD));
        fixture
    }

    fn moved(card: u32, from: Location, to: Location) -> Event {
        Event::Moved {
            card,
            from: Some(from),
            to,
            cause: MoveCause::Standard,
            by: None,
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_fury_equipment_with_plus_one_and_a_wearer_move_to_battlefield_listener() {
        assert!(std::ptr::eq(script_of("Pendulum Blade").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        let listener = &WEARER_TEXT[0];
        assert_eq!(
            listener.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(listener.condition.is_none());
        assert!(listener.targets.is_empty());
        assert!(!listener.optional);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::Ability(_)]
        ));
        assert_eq!(MIGHT_BONUS, 1);
        assert_eq!(SWING, 2);
    }

    #[test]
    fn equipping_costs_a_fury_rune_and_the_wearer_reads_plus_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one Fury rune recycled for the power"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +1 Might while {card 90} is attached".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wearers_move_to_a_battlefield_swings_for_two_this_turn_and_the_turn_takes_it_back() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Standard,
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the move queues the wearer's swing"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3 + 1 + 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} swings for +2 Might this turn".to_string()));
        crate::engine::expiry::at_expiration(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the swing lapses with the turn, the +1 stays"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_listener_reads_the_wearer_alone_and_a_move_home_or_another_units_move_is_not_a_swing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let to_bf1 = Location::Battlefield(fixtures::BF1);
        assert_eq!(
            triggers::find(&ctx, &moved(fixtures::VI, Location::Base(0), to_bf1)).len(),
            1
        );
        assert!(triggers::find(
            &ctx,
            &moved(fixtures::THEIR_UNIT, Location::Base(1), to_bf1)
        )
        .is_empty());
        let listener = WEARER_TEXT[0].trigger;
        assert_eq!(
            triggers::matches(
                &ctx,
                listener,
                &moved(fixtures::VI, Location::Base(0), to_bf1),
                fixtures::VI
            ),
            Some(0)
        );
        assert_eq!(
            triggers::matches(
                &ctx,
                listener,
                &moved(fixtures::VI, to_bf1, Location::Base(0)),
                fixtures::VI
            ),
            None,
            "a move to the base is not a move to a battlefield"
        );
        ctx.detach(GEAR);
        assert!(triggers::find(&ctx, &moved(fixtures::VI, Location::Base(0), to_bf1)).is_empty());
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_MOVE,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is off the board · no swing".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_a_missing_fury_rune_and_an_enemy_wearer_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, GEAR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        for id in [40, 41, 43] {
            let held = broke.table.card_mut(id).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(moved(
            fixtures::VI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        ));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_move_by_the_wearer_swings_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Standard,
        );
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
    }
}

use super::prelude::{done, might_this_turn, triggered, unit, when};
use super::{Card, Event, Flow, Item, Source, Stage, Trigger, Where, Who};
use crate::engine::ctx::{Ctx, Location};

pub const MIGHT: i16 = 2;
pub const I_MOVE_FROM_A_LOCATION: Trigger = Trigger::Move {
    of: Who::Me,
    to: Where::FromLocation,
};

fn from_a_battlefield(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(
        event,
        Event::Moved {
            from: Some(Location::Battlefield(_)),
            ..
        }
    )
}

fn harpoon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Harpoon Squad",
    &[],
    &[when(
        triggered(I_MOVE_FROM_A_LOCATION, &[], harpoon),
        from_a_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, settle};
    use crate::state::ItemKind;

    const SQUAD: u32 = 90;

    fn deck(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut squad = fixtures::unit(SQUAD, zone, 0, "Harpoon Squad", 4);
        squad.domain = vec!["Chaos".into()];
        squad.energy = Some(4);
        fixture.table.cards.push(squad);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SQUAD).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_one_conditional_self_move_trigger() {
        assert!(std::ptr::eq(script_of("Harpoon Squad").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, I_MOVE_FROM_A_LOCATION);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) -> Ctx<'_> {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(SQUAD, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, SQUAD, from, to);
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn leaving_a_battlefield_for_base_gives_two_might_this_turn_when_the_trigger_resolves() {
        let mut fixture = deck(fixtures::BF1);
        let mut ctx = march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert_eq!(ctx.location(SQUAD), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SQUAD
        ));
        assert_eq!(ctx.current_might(SQUAD), 4, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(SQUAD), 6);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SQUAD}}} gets +2 Might this turn")));
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(SQUAD), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_effect_that_moves_it_off_a_battlefield_fires_too() {
        let mut fixture = deck(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        assert_eq!(
            ctx.move_unit(SQUAD, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].controller, 0, "the squad's own trigger");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(SQUAD), 6);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_move_out_of_base_or_a_recall_fires_nothing() {
        let mut fixture = deck(fixtures::BASE);
        let ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.location(SQUAD),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "from base is not from a battlefield"
        );
        assert_eq!(ctx.current_might(SQUAD), 4);
        drop(ctx);

        let mut fixture = deck(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(SQUAD, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(SQUAD), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
        assert_eq!(ctx.current_might(SQUAD), 4);
        assert!(ctx.fault.is_none());
    }
}

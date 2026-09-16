use super::prelude::{
    a_card, card_target, done, might_this_turn, on_move_to_battlefield, unit,
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const TARGET: TargetSpec = a_card(
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
    "another friendly unit to give +1 Might this turn",
);

fn ribbon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Ribbon Dancer",
    &[],
    &[on_move_to_battlefield(&[TARGET], ribbon)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, play, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const DANCER: u32 = 90;
    const PARTNER: u32 = 91;

    fn stage(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut dancer = fixtures::unit(DANCER, zone, 0, "Ribbon Dancer", 3);
        dancer.domain = vec!["Calm".into()];
        dancer.energy = Some(3);
        fixture.table.cards.push(dancer);
        fixture
            .table
            .cards
            .push(fixtures::unit(PARTNER, fixtures::BF1, 0, "Partner", 2));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DANCER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) -> Ctx<'_> {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(DANCER, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, DANCER, from, to);
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn the_script_is_a_unit_whose_move_to_a_battlefield_pumps_another_friendly_unit() {
        assert!(std::ptr::eq(script_of("Ribbon Dancer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert_eq!(ability.targets, &[TARGET]);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
    }

    #[test]
    fn moving_to_a_battlefield_offers_the_other_friendly_units_and_the_pick_gets_one_this_turn() {
        let mut fixture = stage(fixtures::BASE);
        let mut ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.location(DANCER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {PARTNER}}}")
            ],
            "friendly units anywhere but the dancer herself"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[DANCER]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "another friendly unit, not her"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "not an enemy"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {PARTNER}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DANCER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(PARTNER)]);
        assert_eq!(ctx.current_might(PARTNER), 2, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(PARTNER), 3);
        assert_eq!(ctx.current_might(DANCER), 3);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PARTNER}}} gets +1 Might this turn")));
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(PARTNER), 2, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn walking_home_fires_nothing_and_alone_on_the_board_the_trigger_fizzles() {
        let mut fixture = stage(fixtures::BF1);
        fixture.table.card_mut(PARTNER).unwrap().exhausted = true;
        fixture.resolve();
        let ctx = march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert_eq!(ctx.location(DANCER), Some(Location::Base(0)));
        assert!(
            ctx.blob.chain.is_empty(),
            "Where::Battlefield · not a walk home"
        );
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);

        let mut lonely = stage(fixtures::BASE);
        lonely
            .table
            .cards
            .retain(|card| ![PARTNER, fixtures::VI].contains(&card.id));
        lonely.resolve();
        let ctx = march(
            &mut lonely,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        assert!(ctx.fault.is_none());
    }
}

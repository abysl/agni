use super::prelude::{friendly_units, open_battlefields, unit, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit("Miss Fortune - Buccaneer", &[], &[]),
    &[Static::PlayLocations(open_play_locations)],
);

fn is_her(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn grants_open_battlefields(ctx: &Ctx, seat: u8) -> bool {
    friendly_units(ctx, seat)
        .into_iter()
        .any(|card| is_her(ctx, card))
}

pub fn open_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    if ctx.is_unit(card) && (is_her(ctx, card) || grants_open_battlefields(ctx, seat)) {
        open_battlefields(ctx)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const FORTUNE: u32 = 90;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fortune = fixtures::unit(FORTUNE, zone, 0, "Miss Fortune - Buccaneer", 4);
        fortune.energy = Some(0);
        fortune.domain = vec!["Chaos".into()];
        fixture.table.cards.push(fortune);
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, card: u32, to: u16) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn her_own_grant_is_hers_from_the_hand_and_the_friendly_grant_needs_her_on_the_board() {
        assert!(std::ptr::eq(
            script_of("Miss Fortune - Buccaneer").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        assert!(CARD.grants_play_locations());
        let mut hand = armed(fixtures::HAND);
        let ctx = hand.ctx();
        assert!(
            !grants_open_battlefields(&ctx, 0),
            "in hand she grants nothing"
        );
        assert_eq!(
            open_play_locations(&ctx, 0, FORTUNE),
            [Location::Battlefield(fixtures::BF1)],
            "she herself may go to the open battlefield"
        );
        assert!(open_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty());
        let mut board = armed(fixtures::BASE);
        let ctx = board.ctx();
        assert!(grants_open_battlefields(&ctx, 0));
        assert!(
            !grants_open_battlefields(&ctx, 1),
            "not for the enemy's units"
        );
        assert_eq!(
            open_play_locations(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert!(
            open_play_locations(&ctx, 0, fixtures::HAND_GEAR).is_empty(),
            "gear is not a unit"
        );
        assert!(open_play_locations(&ctx, 1, fixtures::THEIR_HAND_CARD).is_empty());
    }

    #[test]
    fn on_the_board_she_lists_the_open_battlefields_for_friendly_units_alone() {
        let mut board = armed(fixtures::BASE);
        let ctx = board.ctx();
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::HAND_UNIT, fixtures::BASE)),
            Ok(Intent::Play {
                card: fixtures::HAND_UNIT,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
        assert!(
            ctx.granted_play_locations(0, fixtures::HAND_GEAR)
                .is_empty(),
            "gear is not a unit"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::HAND_GEAR, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::GearToBase))
        );
        assert!(
            ctx.granted_play_locations(1, fixtures::THEIR_HAND_CARD)
                .is_empty(),
            "not for the enemy's units"
        );
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(fixtures::unit(
            FORTUNE,
            fixtures::BASE,
            1,
            "Miss Fortune - Buccaneer",
            4,
        ));
        theirs.resolve();
        let ctx = theirs.ctx();
        assert!(
            ctx.granted_play_locations(0, fixtures::HAND_UNIT)
                .is_empty(),
            "the enemy's Miss Fortune grants your units nothing"
        );
    }

    #[test]
    fn with_her_on_the_board_a_friendly_unit_may_be_played_to_an_open_battlefield() {
        let mut board = armed(fixtures::BASE);
        let ctx = board.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::HAND_UNIT, fixtures::BF1)),
            Ok(Intent::Play {
                card: fixtures::HAND_UNIT,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        let mut hand = armed(fixtures::HAND);
        let ctx = hand.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, FORTUNE, fixtures::BF1)),
            Ok(Intent::Play {
                card: FORTUNE,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::HAND_UNIT, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "the friendly grant waits for her to be on the board"
        );
    }
}

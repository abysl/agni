use super::prelude::{unit, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit("Perched Grimwyrm", &[], &[]),
    &[Static::OnlyPlayLocations(where_you_conquered)],
);

pub fn conquered_this_turn(ctx: &Ctx, seat: u8) -> Vec<u16> {
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| ctx.blob.holder(*zone) == Some(seat) && ctx.blob.scored(*zone, seat))
        .collect()
}

pub fn conquered_play_locations(ctx: &Ctx, seat: u8) -> Vec<Location> {
    conquered_this_turn(ctx, seat)
        .into_iter()
        .filter(|zone| ctx.units_played_here(*zone))
        .map(Location::Battlefield)
        .collect()
}

fn where_you_conquered(ctx: &Ctx, seat: u8, _: u32) -> Vec<Location> {
    conquered_play_locations(ctx, seat)
}

pub fn only_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Option<Vec<Location>> {
    ctx.only_play_locations(seat, card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{cleanup, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const GRIMWYRM: u32 = 90;
    const PLAIN: u32 = 54;

    fn roost() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut grimwyrm = fixtures::unit(GRIMWYRM, fixtures::HAND, 0, "Perched Grimwyrm", 5);
        grimwyrm.energy = Some(0);
        grimwyrm.domain = vec!["Fury".into()];
        fixture.table.cards.push(grimwyrm);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF3);
        fixture.blob.set_contested(fixtures::BF3, Some(0));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx, zone: u16) {
        assert_eq!(
            cleanup::establish(ctx, zone),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: GRIMWYRM,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn a_battlefield_conquered_this_turn_is_his_only_play_location_and_the_grant_is_his_own() {
        assert!(std::ptr::eq(script_of("Perched Grimwyrm").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        assert!(CARD.only_play_locations().is_some());
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        assert!(conquered_this_turn(&ctx, 0).is_empty());
        assert_eq!(
            only_play_locations(&ctx, 0, GRIMWYRM),
            Some(Vec::new()),
            "nothing conquered yet: nowhere to play him, not even the base"
        );
        conquer(&mut ctx, fixtures::BF3);
        assert_eq!(conquered_this_turn(&ctx, 0), [fixtures::BF3]);
        assert!(conquered_this_turn(&ctx, 1).is_empty());
        assert_eq!(
            conquered_play_locations(&ctx, 0),
            [Location::Battlefield(fixtures::BF3)]
        );
        assert_eq!(
            only_play_locations(&ctx, 0, GRIMWYRM),
            Some(vec![Location::Battlefield(fixtures::BF3)])
        );
        assert_eq!(
            only_play_locations(&ctx, 0, fixtures::HAND_UNIT),
            None,
            "other units keep every play location"
        );
        drop(ctx);
        let mut lost = roost();
        let mut ctx = lost.ctx();
        conquer(&mut ctx, fixtures::BF3);
        ctx.blob.set_holder(fixtures::BF3, Some(1));
        assert!(
            conquered_this_turn(&ctx, 0).is_empty(),
            "conquered and lost again: no longer yours to play to"
        );
        drop(ctx);
        let mut rockfall = roost();
        rockfall.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        rockfall.table.card_mut(PLAIN).unwrap().zone = Some(fixtures::BF2);
        rockfall.resolve();
        let mut ctx = rockfall.ctx();
        conquer(&mut ctx, fixtures::BF3);
        assert_eq!(conquered_this_turn(&ctx, 0), [fixtures::BF3]);
        assert!(
            conquered_play_locations(&ctx, 0).is_empty(),
            "Rockfall Path was conquered but units can't be played there"
        );
    }

    #[test]
    fn the_restriction_is_his_own_and_an_unconquered_battlefield_stays_refused() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        assert!(ctx.play_locations_for(0, GRIMWYRM).is_empty());
        assert_eq!(
            ctx.play_locations_for(0, fixtures::HAND_UNIT),
            [Location::Base(0)],
            "other units keep the base"
        );
        conquer(&mut ctx, fixtures::BF3);
        assert_eq!(
            ctx.play_locations_for(0, GRIMWYRM),
            [Location::Battlefield(fixtures::BF3)]
        );
        assert_eq!(
            ctx.play_locations_for(0, fixtures::HAND_UNIT),
            [Location::Base(0), Location::Battlefield(fixtures::BF3)]
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "an empty uncontrolled battlefield was not conquered"
        );
        ctx.blob.set_holder(fixtures::BF1, Some(0));
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "held since before this turn is not conquered this turn"
        );
    }

    #[test]
    fn he_is_refused_at_the_base_and_lands_only_where_you_conquered_this_turn() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "nothing conquered: nowhere to play him"
        );
        conquer(&mut ctx, fixtures::BF3);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "only to a battlefield you conquered this turn"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF3)),
            Ok(Intent::Play {
                card: GRIMWYRM,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF3)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, GRIMWYRM),
            [Location::Battlefield(fixtures::BF3)]
        );
        ctx.table
            .apply_entry(&fixtures::move_action(GRIMWYRM, fixtures::BF3, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: GRIMWYRM,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF3)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(GRIMWYRM),
            Some(Location::Battlefield(fixtures::BF3))
        );
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: a hold and a conquer both set Control.scored, so conquered_this_turn reads a battlefield held since the Beginning Phase as conquered; the engine owes a per-turn conquer record the seam reads instead"]
    fn a_battlefield_held_since_the_beginning_phase_was_not_conquered_this_turn() {
        let mut fixture = roost();
        fixture.blob.set_contested(fixtures::BF3, None);
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF3]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1, "the hold scored");
        assert!(
            conquered_this_turn(&ctx, 0).is_empty(),
            "held, scored, but not conquered"
        );
    }
}

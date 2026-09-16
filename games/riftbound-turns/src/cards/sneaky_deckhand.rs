use super::prelude::{open_battlefields, unit, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit("Sneaky Deckhand", &[], &[]),
    &[Static::PlayLocations(open_play_locations)],
);

pub fn open_play_locations(ctx: &Ctx, _: u8, card: u32) -> Vec<Location> {
    if ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
    {
        open_battlefields(ctx)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::is_open_battlefield;
    use crate::cards::script_of;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const DECKHAND: u32 = 90;
    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut deckhand = fixtures::unit(DECKHAND, fixtures::HAND, 0, "Sneaky Deckhand", 2);
        deckhand.energy = Some(0);
        deckhand.domain = vec!["Chaos".into()];
        fixture.table.cards.push(deckhand);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: DECKHAND,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn an_open_battlefield_is_unoccupied_and_uncontrolled_and_rockfall_path_is_never_one() {
        assert!(std::ptr::eq(script_of("Sneaky Deckhand").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        assert!(CARD.grants_play_locations());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            open_battlefields(&ctx),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3)
            ],
            "170.11.c · the held battlefield with the Sprite is neither unoccupied nor uncontrolled"
        );
        assert_eq!(
            open_play_locations(&ctx, 0, DECKHAND),
            open_battlefields(&ctx)
        );
        assert!(
            open_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty(),
            "the grant is his own"
        );
        let mut occupied = armed();
        occupied.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        occupied.resolve();
        let ctx = occupied.ctx();
        assert_eq!(
            open_battlefields(&ctx),
            [Location::Battlefield(fixtures::BF3)],
            "a friendly unit occupies it too"
        );
        let mut held = armed();
        held.blob.set_holder(fixtures::BF1, Some(0));
        let ctx = held.ctx();
        assert_eq!(
            open_battlefields(&ctx),
            [Location::Battlefield(fixtures::BF3)],
            "a battlefield you hold is controlled, and already a play location"
        );
        let mut rockfall = armed();
        rockfall.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF1);
        rockfall.table.card_mut(fixtures::GROUNDS).unwrap().zone = Some(fixtures::BF2);
        rockfall.resolve();
        let ctx = rockfall.ctx();
        assert!(is_open_battlefield(&ctx, fixtures::BF1));
        assert_eq!(
            open_battlefields(&ctx),
            [Location::Battlefield(fixtures::BF3)],
            "Rockfall Path is open but units can't be played there"
        );
    }

    #[test]
    fn the_open_battlefields_join_his_play_locations_beside_the_base() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.granted_play_locations(0, DECKHAND),
            open_battlefields(&ctx)
        );
        assert!(ctx
            .granted_play_locations(0, fixtures::HAND_UNIT)
            .is_empty());
        assert_eq!(
            legal::locations_for(&ctx, 0, DECKHAND),
            [
                Location::Base(0),
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3)
            ]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0)],
            "the grant is his own"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: DECKHAND,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
    }

    #[test]
    fn he_may_be_played_to_an_open_battlefield_and_contests_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: DECKHAND,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "the enemy's held battlefield is not open"
        );
        ctx.table
            .apply_entry(&fixtures::move_action(DECKHAND, fixtures::BF1, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: DECKHAND,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(DECKHAND),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
    }
}

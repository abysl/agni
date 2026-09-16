use super::deadbloom_predator::occupied_enemy_battlefields;
use super::prelude::{unit, with_statics, Location};
use super::{Card, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit("Dauntless Vanguard", &[], &[]),
    &[Static::PlayLocations(enemy_play_locations)],
);

pub fn enemy_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    if ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
    {
        occupied_enemy_battlefields(ctx, seat)
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

    const VANGUARD: u32 = 90;
    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut vanguard = fixtures::unit(VANGUARD, fixtures::HAND, 0, "Dauntless Vanguard", 4);
        vanguard.energy = Some(0);
        vanguard.domain = vec!["Body".into()];
        fixture.table.cards.push(vanguard);
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: VANGUARD,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_occupied_enemy_battlefield_is_his_grant_and_his_alone() {
        assert!(std::ptr::eq(
            script_of("Dauntless Vanguard").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        assert!(CARD.grants_play_locations());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            enemy_play_locations(&ctx, 0, VANGUARD),
            [Location::Battlefield(fixtures::BF2)],
            "the Sprite stands on the battlefield seat 1 holds"
        );
        assert!(
            enemy_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty(),
            "the grant is his own"
        );
        assert!(
            enemy_play_locations(&ctx, 1, VANGUARD).is_empty(),
            "to its holder it is not an enemy battlefield"
        );
        drop(ctx);
        let mut empty = armed();
        empty.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        empty.table.card_mut(fixtures::SPRITE).unwrap().seat = 1;
        empty.resolve();
        let ctx = empty.ctx();
        assert!(
            enemy_play_locations(&ctx, 0, VANGUARD).is_empty(),
            "170.11.a · held but unoccupied"
        );
        drop(ctx);
        let mut rockfall = armed();
        rockfall.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF3);
        rockfall.blob.set_holder(fixtures::BF3, Some(1));
        rockfall.blob.set_holder(fixtures::BF2, None);
        rockfall.resolve();
        let ctx = rockfall.ctx();
        assert!(
            enemy_play_locations(&ctx, 0, VANGUARD).is_empty(),
            "units can't be played at Rockfall Path"
        );
    }

    #[test]
    fn the_occupied_enemy_battlefield_joins_his_play_locations_beside_the_base() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.granted_play_locations(0, VANGUARD),
            [Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, VANGUARD),
            [Location::Base(0), Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0)],
            "the grant is his own"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "an empty uncontrolled battlefield is not his either"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: VANGUARD,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
    }

    #[test]
    fn he_may_be_played_to_the_occupied_enemy_battlefield_at_sorcery_speed_and_combat_follows() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Ok(Intent::Play {
                card: VANGUARD,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
        ctx.table
            .apply_entry(&fixtures::move_action(VANGUARD, fixtures::BF2, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: VANGUARD,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            },
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(VANGUARD),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        let showdown = ctx.blob.showdown.clone().expect("combat opens");
        assert!(showdown.combat);
        assert_eq!(showdown.zone, fixtures::BF2);
    }
}

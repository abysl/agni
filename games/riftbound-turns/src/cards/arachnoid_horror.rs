use super::prelude::{friendly_units, unit, with_statics, Location};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const HUNT: u8 = 2;

pub static CARD: Card = with_statics(
    unit("Arachnoid Horror", &[Keyword::Hunt(HUNT)], &[]),
    &[Static::PlayLocations(enemy_play_locations)],
);

fn is_horror(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn enemy_alone_at_zone(ctx: &Ctx, seat: u8, zone: u16) -> bool {
    let units = ctx.units_at(Location::Battlefield(zone));
    units.len() == 1 && ctx.controller(units[0]) != seat
}

pub fn battlefields_with_a_lone_enemy(ctx: &Ctx, seat: u8) -> Vec<Location> {
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| ctx.units_played_here(*zone))
        .filter(|zone| enemy_alone_at_zone(ctx, seat, *zone))
        .map(Location::Battlefield)
        .collect()
}

pub fn grants_lone_enemy_battlefields(ctx: &Ctx, seat: u8) -> bool {
    friendly_units(ctx, seat)
        .into_iter()
        .any(|card| is_horror(ctx, card))
}

pub fn enemy_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    if ctx.is_unit(card) && (is_horror(ctx, card) || grants_lone_enemy_battlefields(ctx, seat)) {
        battlefields_with_a_lone_enemy(ctx, seat)
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
    use agni_plugin_sdk::table::CardInfo;

    const HORROR: u32 = 90;
    const SECOND_ENEMY: u32 = 91;
    const PLAIN: u32 = 54;

    fn horror(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(HORROR, zone, seat, "Arachnoid Horror", 6)
        }
    }

    fn web(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(horror(zone, 0));
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
    fn the_script_prints_hunt_two_and_nothing_else() {
        assert!(std::ptr::eq(script_of("Arachnoid Horror").unwrap(), &CARD));
        assert_eq!(CARD.name, "Arachnoid Horror");
        assert_eq!(CARD.keywords, [Keyword::Hunt(2)]);
        assert_eq!(CARD.hunt(), HUNT);
        assert!(CARD.abilities.is_empty(), "Hunt is implicit");
        assert!(CARD.grants_play_locations());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = web(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(HORROR), 2);
    }

    #[test]
    fn a_battlefield_with_a_lone_enemy_has_exactly_one_unit_and_it_is_not_yours() {
        let mut fixture = web(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(
            enemy_alone_at_zone(&ctx, 0, fixtures::BF2),
            "the Sprite alone"
        );
        assert!(
            !enemy_alone_at_zone(&ctx, 1, fixtures::BF2),
            "to its controller the Sprite is not an enemy"
        );
        assert!(!enemy_alone_at_zone(&ctx, 0, fixtures::BF1), "nobody there");
        assert_eq!(
            battlefields_with_a_lone_enemy(&ctx, 0),
            [Location::Battlefield(fixtures::BF2)]
        );
        assert!(battlefields_with_a_lone_enemy(&ctx, 1).is_empty());
        drop(ctx);

        let mut crowded = web(fixtures::HAND);
        crowded
            .table
            .cards
            .push(fixtures::unit(SECOND_ENEMY, fixtures::BF2, 1, "Second", 2));
        crowded.resolve();
        let ctx = crowded.ctx();
        assert!(
            battlefields_with_a_lone_enemy(&ctx, 0).is_empty(),
            "two enemies are not one alone"
        );
        drop(ctx);

        let mut company = web(fixtures::HAND);
        company.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        company.resolve();
        let ctx = company.ctx();
        assert!(
            battlefields_with_a_lone_enemy(&ctx, 0).is_empty(),
            "an enemy beside Vi is not alone"
        );
        drop(ctx);

        let mut unheld = web(fixtures::HAND);
        unheld.blob.set_holder(fixtures::BF2, None);
        let ctx = unheld.ctx();
        assert_eq!(
            battlefields_with_a_lone_enemy(&ctx, 0),
            [Location::Battlefield(fixtures::BF2)],
            "occupied is about units, not who holds it"
        );
        drop(ctx);

        let mut rockfall = web(fixtures::HAND);
        rockfall.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF3);
        rockfall.resolve();
        let ctx = rockfall.ctx();
        assert!(
            battlefields_with_a_lone_enemy(&ctx, 0).is_empty(),
            "units can't be played at Rockfall Path"
        );
    }

    #[test]
    fn his_own_grant_is_his_from_the_hand_and_the_friendly_grant_needs_him_on_the_board() {
        let mut hand = web(fixtures::HAND);
        let ctx = hand.ctx();
        assert!(
            !grants_lone_enemy_battlefields(&ctx, 0),
            "in hand he grants nothing"
        );
        assert_eq!(
            enemy_play_locations(&ctx, 0, HORROR),
            [Location::Battlefield(fixtures::BF2)],
            "he himself may go where an enemy is alone"
        );
        assert!(enemy_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty());
        assert!(enemy_play_locations(&ctx, 0, fixtures::HAND_GEAR).is_empty());
        drop(ctx);

        let mut board = web(fixtures::BASE);
        let ctx = board.ctx();
        assert!(grants_lone_enemy_battlefields(&ctx, 0));
        assert!(
            !grants_lone_enemy_battlefields(&ctx, 1),
            "the grant is for his controller's units"
        );
        assert_eq!(
            enemy_play_locations(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Battlefield(fixtures::BF2)],
            "friendly units share the grant"
        );
        assert!(
            enemy_play_locations(&ctx, 0, fixtures::HAND_GEAR).is_empty(),
            "gear is not a unit"
        );
        assert!(
            enemy_play_locations(&ctx, 1, fixtures::THEIR_HAND_CARD).is_empty(),
            "the opponent's cards read nothing from him"
        );
    }

    #[test]
    fn the_lone_enemys_battlefield_joins_his_play_locations_and_his_friends_once_he_is_on_the_board(
    ) {
        let mut fixture = web(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.granted_play_locations(0, HORROR),
            [Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, HORROR),
            [Location::Base(0), Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0)],
            "the friendly grant waits for him on the board"
        );
        drop(ctx);
        let mut board = web(fixtures::BASE);
        let ctx = board.ctx();
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0), Location::Battlefield(fixtures::BF2)]
        );
        assert!(
            ctx.granted_play_locations(1, fixtures::THEIR_HAND_CARD)
                .is_empty(),
            "the opponent's cards read nothing from him"
        );
        drop(ctx);
        let mut fixture = web(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, HORROR, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "an empty uncontrolled battlefield is not his either"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, HORROR, fixtures::BASE)),
            Ok(Intent::Play {
                card: HORROR,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
    }

    #[test]
    fn he_and_a_friendly_unit_may_be_played_where_an_enemy_is_alone_and_combat_follows() {
        let mut fixture = web(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, HORROR, fixtures::BF2)),
            Ok(Intent::Play {
                card: HORROR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::HAND_UNIT, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "the friendly grant waits for him on the board"
        );
        ctx.table
            .apply_entry(&fixtures::move_action(HORROR, fixtures::BF2, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: HORROR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF2)),
                on_chain: false,
            },
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(HORROR),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        let showdown = ctx.blob.showdown.clone().expect("combat opens");
        assert!(showdown.combat);
        assert_eq!(showdown.zone, fixtures::BF2);
    }
}

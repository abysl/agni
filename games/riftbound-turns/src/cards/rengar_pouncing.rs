use super::prelude::{unit, with_statics, Location};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit(
        "Rengar - Pouncing",
        &[Keyword::Reaction, Keyword::Assault(2)],
        &[],
    ),
    &[Static::PlayLocations(attacking_play_locations)],
);

pub fn battlefields_you_are_attacking(ctx: &Ctx, seat: u8) -> Vec<Location> {
    ctx.blob
        .showdown
        .as_ref()
        .filter(|showdown| showdown.combat && showdown.attacker == seat)
        .map(|showdown| showdown.zone)
        .filter(|zone| ctx.units_played_here(*zone))
        .map(Location::Battlefield)
        .into_iter()
        .collect()
}

pub fn attacking_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    if ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
    {
        battlefields_you_are_attacking(ctx, seat)
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
    use crate::engine::{cleanup, showdown};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const RENGAR: u32 = 90;
    const PLAIN: u32 = 54;

    fn hunting() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut rengar = fixtures::unit(RENGAR, fixtures::HAND, 0, "Rengar - Pouncing", 3);
        rengar.energy = Some(0);
        rengar.domain = vec!["Fury".into()];
        fixture.table.cards.push(rengar);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn open_combat(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        assert_eq!(open.zone, fixtures::BF1);
        assert_eq!(open.attacker, 0);
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: RENGAR,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_prints_reaction_and_assault_two_and_the_attacked_battlefield_is_his_grant() {
        assert!(std::ptr::eq(script_of("Rengar - Pouncing").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction, Keyword::Assault(2)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.grants_play_locations());
        let mut fixture = hunting();
        let mut ctx = fixture.ctx();
        assert!(
            battlefields_you_are_attacking(&ctx, 0).is_empty(),
            "contested, but the combat has not opened"
        );
        open_combat(&mut ctx);
        assert_eq!(
            battlefields_you_are_attacking(&ctx, 0),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert!(
            battlefields_you_are_attacking(&ctx, 1).is_empty(),
            "the defender is not attacking"
        );
        assert_eq!(
            attacking_play_locations(&ctx, 0, RENGAR),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert!(
            attacking_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty(),
            "the grant is his own"
        );
        drop(ctx);
        let mut rockfall = hunting();
        rockfall.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF1);
        rockfall.table.card_mut(fixtures::GROUNDS).unwrap().zone = Some(fixtures::BF2);
        rockfall.resolve();
        let mut ctx = rockfall.ctx();
        open_combat(&mut ctx);
        assert!(
            battlefields_you_are_attacking(&ctx, 0).is_empty(),
            "units can't be played at Rockfall Path"
        );
    }

    #[test]
    fn the_attacked_battlefield_joins_his_play_locations_once_the_combat_is_open() {
        let mut fixture = hunting();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "contested, but the combat has not opened"
        );
        open_combat(&mut ctx);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: RENGAR,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            }),
            "a Reaction unit plays during the showdown with focus"
        );
        assert_eq!(
            ctx.granted_play_locations(0, RENGAR),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, RENGAR),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)]
        );
        assert!(
            ctx.granted_play_locations(0, fixtures::HAND_UNIT)
                .is_empty(),
            "the grant is his own"
        );
        assert!(
            ctx.granted_play_locations(1, fixtures::THEIR_HAND_CARD)
                .is_empty(),
            "the defender is not attacking"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF3)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "an empty battlefield is neither held nor attacked"
        );
    }

    #[test]
    fn he_pounces_onto_the_battlefield_you_are_attacking_and_joins_the_attack() {
        let mut fixture = hunting();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: RENGAR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF3)),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
        ctx.table
            .apply_entry(&fixtures::move_action(RENGAR, fixtures::BF1, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: RENGAR,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(RENGAR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.is_attacker(RENGAR),
            "464.2.c.3.a · he joins as an attacker"
        );
        assert_eq!(
            ctx.current_might(RENGAR),
            5,
            "Assault 2 while he is an attacker"
        );
        let open = ctx.blob.showdown.clone().expect("the combat stays open");
        assert_eq!(open.focus(), 1, "347.1.b · his play hands focus on");
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(
            ctx.blob
                .log
                .iter()
                .any(|line| line == "attackers 8 might vs defenders 2 might"),
            "his 5 joins Vi's 3 against Jinx"
        );
    }
}

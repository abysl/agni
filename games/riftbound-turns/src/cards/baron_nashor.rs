use super::prelude::{battlefield, unit, with_statics, Location};
use super::{Card, Grant, Scope, Static, TOKEN_BARON_PIT};
use crate::engine::ctx::{Ctx, Token};

pub const BONUS: i16 = 2;

fn other_friendly_unit(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source && ctx.controller(unit) == ctx.controller(source)
}

pub fn baron_pit(ctx: &Ctx) -> Option<u16> {
    ctx.zones.battlefields.iter().copied().find(|zone| {
        ctx.table
            .in_zone(*zone)
            .any(|card| ctx.is_battlefield_card(card.id) && card.name == TOKEN_BARON_PIT)
    })
}

pub fn add_the_baron_pit(ctx: &mut Ctx, seat: u8) -> Option<u16> {
    if let Some(zone) = baron_pit(ctx) {
        ctx.narrate(format!(
            "the Baron Pit is already on the board at {{zone {zone}}}"
        ));
        return None;
    }
    let added = ctx.add_battlefield_token(seat, Token::BaronPit);
    if added.is_none() {
        ctx.narrate(format!(
            "{{seat {seat}}} would add the Baron Pit to the board · the table has no zone for it"
        ));
    }
    added
}

pub fn enters_at(added: Option<u16>, chosen: Location) -> Location {
    added.map(Location::Battlefield).unwrap_or(chosen)
}

fn enter_at_the_pit(ctx: &mut Ctx, card: u32, chosen: Location) -> Location {
    let seat = ctx.controller(card);
    enters_at(add_the_baron_pit(ctx, seat), chosen)
}

pub static BARON_PIT_TOKEN: Card = with_statics(
    battlefield(TOKEN_BARON_PIT, &[], &[]),
    &[Static::MoveHereFromAnywhere],
);

pub static CARD: Card = with_statics(
    unit("Baron Nashor", &[], &[]),
    &[
        Static::Untargetable(|_, _| true),
        Static::Aura {
            scope: Scope::FriendlyUnits,
            when: other_friendly_unit,
            grants: &[Grant::Might(BONUS)],
        },
        Static::EntersAt(enter_at_the_pit),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::a_unit;
    use crate::cards::{resolve, script_of};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::targets;
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const BARON: u32 = 90;
    const ALLY: u32 = 91;
    const PIT: u32 = 54;
    const THIRD: u32 = 55;
    const MIGHT: u8 = 12;

    fn baron(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(10),
            power: Some(3),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(BARON, zone, seat, "Baron Nashor", MIGHT)
        }
    }

    const CHAOS_RUNES: std::ops::Range<u32> = 100..110;

    fn river(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(baron(zone, 0));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn with_pit() -> Fixture {
        let mut fixture = river(fixtures::BASE);
        fixture.table.cards.push(fixtures::card(
            PIT,
            fixtures::BF3,
            0,
            TOKEN_BARON_PIT,
            "Battlefield",
        ));
        fixture.table.tokens.push(PIT);
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_untargetable_by_enemies_with_a_plus_two_aura_for_other_friendly_units() {
        assert!(std::ptr::eq(script_of("Baron Nashor").unwrap(), &CARD));
        assert_eq!(CARD.name, "Baron Nashor");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), 3);
        assert!(CARD.has_static(Static::Untargetable(|_, _| true)));
        assert!(CARD.has_static(Static::EntersAt(enter_at_the_pit)));
        assert_eq!(BARON_PIT_TOKEN.name, TOKEN_BARON_PIT);
        assert!(BARON_PIT_TOKEN.has_static(Static::MoveHereFromAnywhere));
        assert!(BARON_PIT_TOKEN.abilities.is_empty());
        assert!(matches!(
            CARD.statics[1],
            Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(2)],
                ..
            }
        ));
        assert!(CARD.has_aura());
        assert_eq!(BONUS, 2);
        assert_eq!(TOKEN_BARON_PIT, "Baron Pit");
        let ultimate = CardInfo {
            name: "Baron Nashor (Ultimate)".into(),
            ..baron(fixtures::HAND, 0)
        };
        assert!(
            std::ptr::eq(resolve(&ultimate).unwrap(), &CARD),
            "the Ultimate print is the same card"
        );
    }

    #[test]
    fn other_friendly_units_anywhere_read_plus_two_and_enemies_and_the_baron_do_not() {
        let mut fixture = river(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(ctx.current_might(BARON), i32::from(MIGHT), "never himself");
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3 + i32::from(BONUS),
            "Vi in the base"
        );
        assert_eq!(
            ctx.current_might(ALLY),
            2 + i32::from(BONUS),
            "the ally at a battlefield · the aura reaches every friendly unit"
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2, "never an enemy");
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        drop(fixture);

        let mut in_hand = river(fixtures::HAND);
        let ctx = in_hand.ctx();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "in hand he grants nothing"
        );
        assert_eq!(ctx.current_might(ALLY), 2);
    }

    #[test]
    fn he_is_never_a_candidate_for_an_enemy_item_but_is_for_a_friendly_one() {
        let mut fixture = river(fixtures::BASE);
        let ctx = fixture.ctx();
        let spec = a_unit("a unit");
        let theirs = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        let offered = targets::candidates(&ctx, &theirs, &spec);
        assert!(
            !offered.contains(&TargetRef::Card(BARON)),
            "an enemy spell cannot choose him"
        );
        assert!(
            offered.contains(&TargetRef::Card(ALLY)),
            "the aura shields nobody else"
        );
        let mine = ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(targets::candidates(&ctx, &mine, &spec).contains(&TargetRef::Card(BARON)));
    }

    #[test]
    fn the_pit_is_found_by_name_among_the_battlefields_and_is_not_added_twice() {
        let mut fixture = river(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(baron_pit(&ctx), None);
        assert_eq!(
            add_the_baron_pit(&mut ctx, 0),
            Some(fixtures::BF3),
            "the spare shared zone takes the pit"
        );
        let pit = baron_pit(&ctx).expect("the pit stands at the new zone");
        assert_eq!(pit, fixtures::BF3);
        assert!(ctx.zones.is_battlefield(fixtures::BF3));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} adds {{card 200}} to the board at {{zone {}}}",
            fixtures::BF3
        )));
        assert!(ctx.is_token(200));
        assert!(ctx.is_battlefield_card(200));
        assert!(ctx.moves_here_from_anywhere(fixtures::BF3));
        assert!(!ctx.moves_here_from_anywhere(fixtures::BF1));
        assert_eq!(
            add_the_baron_pit(&mut ctx, 0),
            None,
            "if it's not there already"
        );
        assert!(ctx.blob.log.contains(&format!(
            "the Baron Pit is already on the board at {{zone {}}}",
            fixtures::BF3
        )));
        assert_eq!(
            enters_at(None, Location::Base(0)),
            Location::Base(0),
            "without the pit he enters where he was played"
        );
        assert_eq!(
            enters_at(Some(fixtures::BF3), Location::Base(0)),
            Location::Battlefield(fixtures::BF3)
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut pitted = with_pit();
        let mut ctx = pitted.ctx();
        assert_eq!(baron_pit(&ctx), Some(fixtures::BF3));
        assert_eq!(add_the_baron_pit(&mut ctx, 0), None);
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut full = river(fixtures::HAND);
        full.table.zones.retain(|zone| zone.id != fixtures::BF3);
        let mut ctx = full.ctx();
        assert_eq!(
            add_the_baron_pit(&mut ctx, 0),
            None,
            "the table has no zone to add"
        );
        assert!(ctx.blob.log.contains(
            &"{seat 0} would add the Baron Pit to the board · the table has no zone for it"
                .to_string()
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_named_like_the_pit_that_is_not_a_battlefield_is_not_the_pit() {
        let mut fixture = river(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(PIT, fixtures::BF1, 1, TOKEN_BARON_PIT, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(baron_pit(&ctx), None);
    }

    fn full_table() -> Fixture {
        let mut fixture = river(fixtures::HAND);
        fixture.table.cards.push(fixtures::card(
            THIRD,
            fixtures::BF3,
            1,
            "Fallen Watchtower",
            "Battlefield",
        ));
        fixture.resolve();
        fixture
    }

    #[test]
    fn on_a_table_using_every_declared_battlefield_zone_he_enters_where_he_was_played() {
        let mut fixture = full_table();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.zones.battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3],
            "three players, 2v2 or the battlefields = 3 option fill every zone the host declares"
        );
        assert_eq!(ctx.zones.spare_battlefield(&ctx.table), None);
        fixtures::play_from_hand(&mut ctx, 0, BARON).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(baron_pit(&ctx), None, "no zone, no pit");
        assert_eq!(
            ctx.zones.battlefields,
            [fixtures::BF1, fixtures::BF2, fixtures::BF3]
        );
        assert_eq!(ctx.location(BARON), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(
            &"{seat 0} would add the Baron Pit to the board · the table has no zone for it"
                .to_string()
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn playing_him_adds_the_baron_pit_and_he_enters_there() {
        let mut fixture = river(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let battlefields = ctx.zones.battlefields.len();
        fixtures::play_from_hand(&mut ctx, 0, BARON).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let pit = baron_pit(&ctx).expect("the pit was added");
        assert_eq!(ctx.zones.battlefields.len(), battlefields + 1);
        assert_eq!(ctx.location(BARON), Some(Location::Battlefield(pit)));
        assert!(ctx.fault.is_none());
    }
}

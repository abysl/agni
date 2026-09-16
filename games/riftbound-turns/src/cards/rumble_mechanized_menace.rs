use super::experimental_hexplate::is_mech_by_hexplate;
use super::prelude::{friendly_units, legend, with_statics};
use super::{base_name, Card, Filter, Grant, Keyword, Scope, Static};
use crate::engine::ctx::Ctx;

pub const SHIELD: Keyword = Keyword::Shield(1);
pub const HEXPLATE: &str = "Experimental Hexplate";
pub const MECH_TOKEN: &str = "Mech";

pub const MECHS: [&str; 17] = [
    "Adaptatron",
    "Blitzcrank - Impassive",
    "Breakneck Mech",
    "Bubble Bot",
    "Carrion Dredger",
    "Dangerous Duo",
    "Ferrous Forerunner",
    "Forecaster",
    "Gem Jammer",
    MECH_TOKEN,
    "Mega-Mech",
    "Patched Porobot",
    "Plaza Guardian",
    "Prize of Progress",
    "Rumble - Hotheaded",
    "Rumble - Scrapper",
    "Scrapyard Champion",
];

pub const MECH: Filter = Filter::Or(&[
    Filter::Named(MECHS[0]),
    Filter::Named(MECHS[1]),
    Filter::Named(MECHS[2]),
    Filter::Named(MECHS[3]),
    Filter::Named(MECHS[4]),
    Filter::Named(MECHS[5]),
    Filter::Named(MECHS[6]),
    Filter::Named(MECHS[7]),
    Filter::Named(MECHS[8]),
    Filter::Named(MECHS[9]),
    Filter::Named(MECHS[10]),
    Filter::Named(MECHS[11]),
    Filter::Named(MECHS[12]),
    Filter::Named(MECHS[13]),
    Filter::Named(MECHS[14]),
    Filter::Named(MECHS[15]),
    Filter::Named(MECHS[16]),
]);

pub fn is_mech(ctx: &Ctx, card: u32) -> bool {
    let printed = ctx
        .card(card)
        .is_some_and(|held| MECHS.contains(&base_name(&held.name)));
    printed || is_mech_by_hexplate(ctx, card)
}

pub fn your_mech(ctx: &Ctx, source: u32, unit: u32) -> bool {
    ctx.controller(unit) == ctx.controller(source) && is_mech(ctx, unit)
}

pub fn friendly_mechs(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_mech(ctx, *unit))
        .collect()
}

fn a_mech(ctx: &Ctx, _: u32, unit: u32) -> bool {
    is_mech(ctx, unit)
}

pub static CARD: Card = with_statics(
    legend("Rumble - Mechanized Menace", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: a_mech,
        grants: &[Grant::Keyword(SHIELD)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{attach, statics};
    use crate::state::{FLAG_ATTACKER, FLAG_DEFENDER};

    const RUMBLE: u32 = fixtures::LEGEND_CARD;
    const MECH: u32 = 90;
    const THEIR_MECH: u32 = 91;
    const HEXPLATE_ID: u32 = 92;

    fn garage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(RUMBLE).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(MECH, fixtures::BF1, 0, "Mega-Mech", 7));
        fixture.table.cards.push(fixtures::unit(
            THEIR_MECH,
            fixtures::BF1,
            1,
            "Mega-Mech (Alternate Art)",
            7,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RUMBLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn defending(fixture: &mut Fixture, unit: u32) {
        fixture.blob.card_state_mut(unit).set(FLAG_DEFENDER, true);
    }

    #[test]
    fn the_legend_is_an_aura_over_friendly_mechs_granting_shield_one() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.abilities.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Shield(1))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
        let mut sorted = MECHS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, MECHS, "the list is sorted and has no repeats");
        assert!(MECHS.contains(&MECH_TOKEN));
        let Filter::Or(named) = super::MECH else {
            panic!("the Mech filter is a list of printed names");
        };
        assert_eq!(named.len(), MECHS.len());
        for (filter, name) in named.iter().zip(MECHS) {
            assert_eq!(*filter, Filter::Named(name));
        }
    }

    #[test]
    fn a_mech_is_read_from_the_printed_name_or_a_hexplate_it_wears() {
        let mut fixture = garage();
        fixture.table.cards.push(fixtures::gear(
            HEXPLATE_ID,
            fixtures::BASE,
            0,
            "Experimental Hexplate",
            3,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(is_mech(&ctx, MECH));
        assert!(
            is_mech(&ctx, THEIR_MECH),
            "an alternate art print is the same card"
        );
        assert!(!is_mech(&ctx, fixtures::VI));
        assert!(!is_mech(&ctx, fixtures::SPRITE));
        assert!(!is_mech(&ctx, RUMBLE), "the legend is no Mech himself");
        assert!(
            !is_mech(&ctx, HEXPLATE_ID),
            "the gear is not a Mech, its wearer is"
        );
        assert_eq!(
            attach::attach(&mut ctx, HEXPLATE_ID, fixtures::VI),
            attach::Attached::Yes
        );
        assert!(
            is_mech(&ctx, fixtures::VI),
            "the Hexplate makes its wearer a Mech"
        );
        assert!(attach::detach(&mut ctx, HEXPLATE_ID));
        assert!(!is_mech(&ctx, fixtures::VI));
    }

    #[test]
    fn friendly_mechs_have_shield_and_read_one_more_might_while_defending() {
        let mut fixture = garage();
        defending(&mut fixture, MECH);
        defending(&mut fixture, THEIR_MECH);
        defending(&mut fixture, fixtures::VI);
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(MECH, Keyword::Shield(1)));
        assert!(matches!(
            statics::grants_on(&ctx, MECH).as_slice(),
            [Grant::Keyword(Keyword::Shield(1))]
        ));
        assert_eq!(
            ctx.current_might(MECH),
            8,
            "7 printed, +1 Shield as a defender"
        );
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Shield(1)),
            "Vi is no Mech"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(
            !ctx.has_keyword(THEIR_MECH, Keyword::Shield(1)),
            "the opponent's Mech is not his"
        );
        assert_eq!(ctx.current_might(THEIR_MECH), 7);
        assert!(ctx.set_controller(THEIR_MECH, 0, MECH));
        assert!(
            ctx.has_keyword(THEIR_MECH, Keyword::Shield(1)),
            "friendly follows the controller"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn shield_counts_only_for_a_defender_and_a_second_shield_source_stacks() {
        let mut fixture = garage();
        fixture.blob.card_state_mut(MECH).set(FLAG_ATTACKER, true);
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(MECH, Keyword::Shield(1)));
        assert_eq!(
            ctx.current_might(MECH),
            7,
            "Shield reads nothing for an attacker"
        );
        ctx.set_flag(MECH, FLAG_ATTACKER, false);
        ctx.set_flag(MECH, FLAG_DEFENDER, true);
        assert_eq!(ctx.current_might(MECH), 8);
        let until = crate::cards::prelude::this_turn(&ctx);
        assert!(ctx.grant(MECH, Keyword::Shield(1), until));
        assert_eq!(
            ctx.current_might(MECH),
            9,
            "a granted Shield adds to the aura's"
        );
    }

    #[test]
    fn an_exhausted_or_opposing_rumble_changes_nothing_for_the_wrong_seat() {
        let mut fixture = garage();
        fixture.table.card_mut(RUMBLE).unwrap().exhausted = true;
        defending(&mut fixture, MECH);
        let ctx = fixture.ctx();
        assert!(
            ctx.has_keyword(MECH, Keyword::Shield(1)),
            "a static reads nothing from his exhaustion"
        );
        assert_eq!(ctx.current_might(MECH), 8);
        drop(ctx);
        let mut theirs = Fixture::enforced();
        theirs.table.card_mut(RUMBLE).unwrap().name = CARD.name.into();
        theirs.table.card_mut(RUMBLE).unwrap().owner = 1;
        theirs.table.card_mut(RUMBLE).unwrap().seat = 1;
        theirs
            .table
            .cards
            .push(fixtures::unit(MECH, fixtures::BF1, 0, "Mega-Mech", 7));
        theirs.resolve();
        theirs.blob.card_state_mut(MECH).set(FLAG_DEFENDER, true);
        let ctx = theirs.ctx();
        assert_eq!(ctx.controller(RUMBLE), 1);
        assert!(
            !ctx.has_keyword(MECH, Keyword::Shield(1)),
            "seat 1's Rumble shields seat 1's Mechs alone"
        );
        assert_eq!(ctx.current_might(MECH), 7);
    }
}

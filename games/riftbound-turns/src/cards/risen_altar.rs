use super::irelia_graceful::chosen_reduction_until_the_pay_stage_asks_energy_or_power;
use super::prelude::{battlefield, with_statics, Location};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::engine::targets;
use crate::state::{ChainItem, ItemKind};

pub const EMPOWER_LABEL: &str = "empower";

pub fn is_empower_activation(ctx: &Ctx, item: &ChainItem) -> bool {
    let source = match item.kind {
        ItemKind::Ability { source, .. } | ItemKind::Lent { holder: source, .. } => source,
        _ => return false,
    };
    ctx.script(source)
        .is_some_and(|script| script.has_keyword(Keyword::Empower(Cost::FREE)))
        && targets::ability_of(ctx, item)
            .is_some_and(|ability| ability.label == Some(EMPOWER_LABEL))
}

pub fn holds_the_altar(ctx: &Ctx, altar: u32, seat: u8) -> bool {
    ctx.card(altar)
        .and_then(|held| held.zone)
        .filter(|zone| ctx.zones.is_battlefield(*zone))
        .is_some_and(|zone| ctx.blob.holder(zone) == Some(seat))
}

pub fn an_empower_of_a_unit_here(ctx: &Ctx, item: &ChainItem, altar: u32) -> bool {
    let Some(at @ Location::Battlefield(_)) = ctx.location(altar) else {
        return false;
    };
    let unit = item.kind.source();
    is_empower_activation(ctx, item)
        && holds_the_altar(ctx, altar, item.controller)
        && ctx.is_unit(unit)
        && ctx.controller(unit) == item.controller
        && ctx.location(unit) == Some(at)
}

pub fn empower_discount(ctx: &Ctx, item: &ChainItem, altar: u32) -> Cost {
    if !an_empower_of_a_unit_here(ctx, item, altar) {
        return Cost::FREE;
    }
    chosen_reduction_until_the_pay_stage_asks_energy_or_power(ctx, item)
}

pub static CARD: Card = with_statics(
    battlefield("Risen Altar", &[], &[]),
    &[Static::AbilityDiscount(empower_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{ONE_ENERGY, RAINBOW};
    use crate::cards::{script_of, Domain, GRANTED};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::Origin;
    use agni_plugin_sdk::table::CardInfo;

    const ALTAR: u32 = fixtures::GROUNDS;
    const MATRIARCH: u32 = 90;
    const MATRIARCH_AT_HOME: u32 = 91;
    const THEIR_MATRIARCH: u32 = 92;

    fn matriarch(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Tail-Cloaked Matriarch", 4)
        }
    }

    fn altar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ALTAR).unwrap().name = "Risen Altar".into();
        fixture
            .table
            .cards
            .push(matriarch(MATRIARCH, fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(matriarch(MATRIARCH_AT_HOME, fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(matriarch(THEIR_MATRIARCH, fixtures::BF1, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ALTAR).unwrap(), &CARD));
        fixture
    }

    fn empower_of(ctx: &Ctx, unit: u32) -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Ability {
                source: unit,
                index: 0,
            },
            ctx.controller(unit),
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_the_pool_name_with_one_ability_discount_and_no_empower_of_its_own() {
        assert!(std::ptr::eq(script_of("Risen Altar").unwrap(), &CARD));
        assert!(
            CARD.keywords.is_empty(),
            "the keyword the text names is not the altar's"
        );
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::AbilityDiscount(empower_discount)));
        assert!(CARD.replacement.is_none());
        assert_eq!(EMPOWER_LABEL, "empower");
    }

    #[test]
    fn the_holders_empower_of_a_unit_here_is_a_rainbow_cheaper_and_one_at_home_or_of_the_other_player_is_not(
    ) {
        let mut fixture = altar();
        let ctx = fixture.ctx();
        assert!(holds_the_altar(&ctx, ALTAR, 0));
        assert!(!holds_the_altar(&ctx, ALTAR, 1));
        let here = empower_of(&ctx, MATRIARCH);
        assert!(is_empower_activation(&ctx, &here));
        assert!(an_empower_of_a_unit_here(&ctx, &here, ALTAR));
        assert_eq!(
            empower_discount(&ctx, &here, ALTAR),
            RAINBOW,
            "the Chaos power comes off until the pay stage asks"
        );
        let home = empower_of(&ctx, MATRIARCH_AT_HOME);
        assert!(is_empower_activation(&ctx, &home));
        assert!(!an_empower_of_a_unit_here(&ctx, &home, ALTAR));
        assert_eq!(empower_discount(&ctx, &home, ALTAR), Cost::FREE);
        let theirs = empower_of(&ctx, THEIR_MATRIARCH);
        assert!(is_empower_activation(&ctx, &theirs));
        assert!(
            !an_empower_of_a_unit_here(&ctx, &theirs, ALTAR),
            "190.6.d: the altar's you is its holder, not every player standing here"
        );
        assert_eq!(empower_discount(&ctx, &theirs, ALTAR), Cost::FREE);
    }

    #[test]
    fn the_altar_discount_applies_to_a_copied_empower_lent_by_attached_gear() {
        const GEAR: u32 = 93;
        let mut fixture = altar();
        fixture
            .table
            .cards
            .push(fixtures::gear(GEAR, fixtures::BF1, 0, "Svellsongur", 3));
        fixture.resolve();
        fixture.blob.card_state_mut(GEAR).attached_to = Some(MATRIARCH);
        let ctx = fixture.ctx();
        let lent = ChainItem::new(
            2,
            ItemKind::Lent {
                holder: MATRIARCH,
                lender: GEAR,
                index: GRANTED,
            },
            0,
            Origin::Board,
        );
        assert!(is_empower_activation(&ctx, &lent));
        let discount = cost::ability_discounts(&ctx, &lent);
        assert_eq!(discount.energy, 0);
        assert_eq!(discount.power, [Need::Rainbow]);
        assert_eq!(empower_discount(&ctx, &lent, ALTAR), RAINBOW);
    }

    #[test]
    fn an_unheld_altar_discounts_nobody() {
        let mut fixture = altar();
        fixture.blob.set_holder(fixtures::BF1, None);
        let ctx = fixture.ctx();
        assert!(!holds_the_altar(&ctx, ALTAR, 0));
        let here = empower_of(&ctx, MATRIARCH);
        assert!(!an_empower_of_a_unit_here(&ctx, &here, ALTAR));
        assert_eq!(empower_discount(&ctx, &here, ALTAR), Cost::FREE);
        let priced = cost::of_activation(&ctx, MATRIARCH, 0);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(Domain::Chaos)]);
    }

    #[test]
    fn an_empower_without_a_power_cost_takes_the_energy_and_other_abilities_take_nothing() {
        let mut fixture = altar();
        fixture.table.card_mut(MATRIARCH).unwrap().name = "Nasus, Ascended".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        let printed = ctx.script(MATRIARCH).unwrap().empower_cost().unwrap();
        assert_eq!((printed.energy, printed.power), (8, &[][..]));
        let item = empower_of(&ctx, MATRIARCH);
        assert!(is_empower_activation(&ctx, &item));
        assert_eq!(
            empower_discount(&ctx, &item, ALTAR),
            ONE_ENERGY,
            "no power to strike: the energy comes off"
        );
        let other = ChainItem::new(
            2,
            ItemKind::Ability {
                source: MATRIARCH,
                index: 1,
            },
            0,
            Origin::Board,
        );
        assert!(
            !is_empower_activation(&ctx, &other),
            "the Matriarch's second ability is not her Empower"
        );
        assert_eq!(empower_discount(&ctx, &other, ALTAR), Cost::FREE);
        let spell = ChainItem::new(
            3,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(!is_empower_activation(&ctx, &spell));
        assert_eq!(empower_discount(&ctx, &spell, ALTAR), Cost::FREE);
        let vi = ChainItem::new(
            4,
            ItemKind::Ability {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert!(!is_empower_activation(&ctx, &vi), "Vi prints no Empower");
    }

    #[test]
    fn elsewhere_the_engine_prices_the_empower_at_its_printed_cost() {
        let mut fixture = altar();
        fixture.table.card_mut(MATRIARCH).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let ctx = fixture.ctx();
        let priced = cost::of_activation(&ctx, MATRIARCH, 0);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(Domain::Chaos)]);
        assert_eq!(
            empower_discount(&ctx, &empower_of(&ctx, MATRIARCH), ALTAR),
            Cost::FREE,
            "no altar stands at that battlefield"
        );
    }

    #[test]
    fn the_empower_of_a_unit_here_is_priced_a_rainbow_lower() {
        let mut fixture = altar();
        let ctx = fixture.ctx();
        let priced = cost::of_activation(&ctx, MATRIARCH, 0);
        assert_eq!(priced.energy, 2);
        assert!(priced.power.is_empty(), "the Chaos need is struck");
        let home = cost::of_activation(&ctx, MATRIARCH_AT_HOME, 0);
        assert_eq!(home.power, [Need::Domain(Domain::Chaos)]);
        let theirs = cost::of_activation(&ctx, THEIR_MATRIARCH, 0);
        assert_eq!(theirs.energy, 2);
        assert_eq!(
            theirs.power,
            [Need::Domain(Domain::Chaos)],
            "190.6.d: the altar's you is its holder"
        );
    }
}

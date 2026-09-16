use super::prelude::{unit, with_statics};
use super::{Card, Cost, Domain, Grant, Power, Static};
use crate::engine::ctx::Ctx;

pub const UNCHOSEN_LEVEL: u8 = 16;

pub const TIERS: &[(u8, Cost)] = &[
    (
        3,
        Cost {
            energy: 2,
            power: &[Power::Domain(Domain::Calm)],
        },
    ),
    (
        6,
        Cost {
            energy: 4,
            power: &[Power::Domain(Domain::Calm), Power::Domain(Domain::Calm)],
        },
    ),
    (
        11,
        Cost {
            energy: 6,
            power: &[
                Power::Domain(Domain::Calm),
                Power::Domain(Domain::Calm),
                Power::Domain(Domain::Calm),
            ],
        },
    ),
];

pub fn discount_at(xp: i32) -> Cost {
    TIERS
        .iter()
        .rev()
        .find(|(level, _)| xp >= i32::from(*level))
        .map(|(_, discount)| *discount)
        .unwrap_or(Cost::FREE)
}

fn level_discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    discount_at(ctx.xp(seat))
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

pub static UNCHOSEN: &[Grant] = &[Grant::Static(Static::Untargetable(always))];

pub static CARD: Card = with_statics(
    unit("Master Yi - Unstoppable", &[], &[]),
    &[
        Static::SelfDiscount(level_discount),
        Static::Level(UNCHOSEN_LEVEL, UNCHOSEN),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, ENEMY_UNIT};
    use crate::cards::{script_of, Flow};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::targets;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const YI: u32 = 90;
    const HEX: u32 = 91;
    const PRINTED_ENERGY: u8 = 12;

    static HEX_CARD: Card = spell(
        "Hex",
        &[],
        &[play(&[a_card(ENEMY_UNIT, "an enemy unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn yi(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(PRINTED_ENERGY),
            power: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(YI, zone, seat, "Master Yi - Unstoppable", 12)
        }
    }

    fn training(zone: u16, xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(yi(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(HEX, fixtures::HAND, 1, "Hex", 1, 0));
        for rune in [100, 101, 102] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HEX, &HEX_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(YI).unwrap(), &CARD));
        fixture
    }

    fn hex_item(controller: u8) -> ChainItem {
        let mut item = ChainItem::new(7, ItemKind::Spell { card: HEX }, controller, Origin::Hand);
        item.stage = crate::engine::play::STAGE_TARGET;
        item
    }

    fn calm(count: usize) -> Vec<Need> {
        vec![Need::Domain(Domain::Calm); count]
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_a_level_ladder_discount_and_a_level_sixteen_untargetable(
    ) {
        assert!(std::ptr::eq(
            script_of("Master Yi - Unstoppable").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 2);
        assert!(CARD.has_static(Static::SelfDiscount(level_discount)));
        assert!(matches!(
            CARD.statics[1],
            Static::Level(16, [Grant::Static(Static::Untargetable(_))])
        ));
        assert_eq!(TIERS.len(), 3);
        assert_eq!(TIERS[0].0, 3);
        assert_eq!(TIERS[1].0, 6);
        assert_eq!(TIERS[2].0, 11);
    }

    #[test]
    fn the_ladder_reads_the_highest_level_reached_and_each_tier_replaces_the_last() {
        assert_eq!(discount_at(0), Cost::FREE);
        assert_eq!(discount_at(2), Cost::FREE);
        assert_eq!(discount_at(3).energy, 2);
        assert_eq!(discount_at(3).power.len(), 1);
        assert_eq!(discount_at(5).energy, 2);
        assert_eq!(discount_at(6).energy, 4);
        assert_eq!(discount_at(6).power.len(), 2);
        assert_eq!(discount_at(10).energy, 4);
        assert_eq!(discount_at(11).energy, 6);
        assert_eq!(discount_at(11).power.len(), 3);
        assert_eq!(
            discount_at(16).energy,
            6,
            "'instead' · Level 16 adds no fourth discount"
        );
        assert!(discount_at(16)
            .power
            .iter()
            .all(|power| *power == Power::Domain(Domain::Calm)));
    }

    #[test]
    fn from_hand_he_costs_twelve_and_three_calm_then_ten_and_two_then_eight_and_one_then_six() {
        for (xp, energy, power) in [(0, 12, 3), (2, 12, 3), (3, 10, 2), (6, 8, 1), (11, 6, 0)] {
            let mut fixture = training(fixtures::HAND, xp);
            let ctx = fixture.ctx();
            let total = cost::total(&ctx, YI, false);
            assert_eq!(total.energy, energy, "at {xp} XP");
            assert_eq!(total.power, calm(power), "at {xp} XP");
            let item = ChainItem::new(1, ItemKind::Permanent { card: YI }, 0, Origin::Hand);
            let priced = cost::of_item(&ctx, &item, None);
            assert_eq!((priced.energy, priced.power), (energy, calm(power)));
        }
    }

    #[test]
    fn the_discount_reads_the_playing_seats_xp_not_the_opponents() {
        let mut fixture = training(fixtures::HAND, 0);
        fixture.set_xp(1, 11);
        let ctx = fixture.ctx();
        let total = cost::total(&ctx, YI, false);
        assert_eq!(total.energy, 12);
        assert_eq!(total.power, calm(3));
    }

    #[test]
    fn at_eleven_xp_six_ready_runes_play_him_and_at_none_the_same_runes_are_refused() {
        let mut fixture = training(fixtures::HAND, 11);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 6);
        fixtures::play_from_hand(&mut ctx, 0, YI).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(YI));
        assert_eq!(ctx.location(YI), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "six energy, no power");
        assert_eq!(ctx.current_might(YI), 12);
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = training(fixtures::HAND, 0);
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, YI),
            Err(Refusal::NotEnoughRunes {
                needed: 12,
                ready: 6
            })
        );
        assert!(!ctx.on_board(YI));
    }

    #[test]
    fn at_sixteen_xp_enemy_spells_cannot_choose_him_and_his_own_still_can() {
        let mut fixture = training(fixtures::BF1, 15);
        let ctx = fixture.ctx();
        assert!(
            !targets::untargetable(&ctx, &hex_item(1), TargetRef::Card(YI)),
            "at 15 XP the enemy Hex may choose him"
        );
        assert!(ctx.projected_statics(YI).is_empty());
        drop(ctx);

        let mut fixture = training(fixtures::BF1, 16);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.projected_statics(YI).len(), 1);
        assert!(
            targets::untargetable(&ctx, &hex_item(1), TargetRef::Card(YI)),
            "824.1.c · Level 16 at 16 XP"
        );
        assert!(
            !targets::untargetable(&ctx, &hex_item(0), TargetRef::Card(YI)),
            "his controller's spells and abilities still choose him"
        );
        assert!(
            !targets::untargetable(&ctx, &hex_item(1), TargetRef::Card(fixtures::VI)),
            "the Level is his alone"
        );
        assert!(ctx.spend_xp(0, 1));
        assert!(
            !targets::untargetable(&ctx, &hex_item(1), TargetRef::Card(YI)),
            "the protection leaves with the sixteenth XP"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_sixteen_does_not_shield_him() {
        let mut fixture = training(fixtures::BF1, 0);
        fixture.set_xp(1, 16);
        let ctx = fixture.ctx();
        assert!(!targets::untargetable(
            &ctx,
            &hex_item(1),
            TargetRef::Card(YI)
        ));
    }
}

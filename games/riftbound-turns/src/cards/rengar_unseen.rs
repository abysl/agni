use super::prelude::unit;
use super::{Card, Keyword};

pub const ASSAULT: u8 = 2;

pub static CARD: Card = unit(
    "Rengar - Unseen",
    &[
        Keyword::Accelerate,
        Keyword::Assault(ASSAULT),
        Keyword::Deflect(1),
        Keyword::Ganking,
    ],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, move_destinations, play, spell, Location, MOVABLE_UNIT};
    use crate::cards::{script_of, Flow};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cost, march, targets};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RENGAR: u32 = 90;
    const ZAP: u32 = 92;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;
    const EXTRA_RUNES: [u32; 2] = [100, 101];

    static SPARK: Card = spell(
        "Spark",
        &[],
        &[play(&[a_card(MOVABLE_UNIT, "a unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn rengar(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            ..fixtures::unit(RENGAR, zone, seat, "Rengar - Unseen", MIGHT)
        }
    }

    fn stalking(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rengar(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENGAR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn across(fixture: &mut Fixture, unit: u32) -> Result<(), Refusal> {
        let ctx = fixture.ctx();
        march::legal_destination(
            &ctx,
            unit,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_prints_accelerate_assault_two_deflect_and_ganking_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Rengar - Unseen").unwrap(), &CARD));
        assert_eq!(CARD.name, "Rengar - Unseen");
        assert_eq!(
            CARD.keywords,
            &[
                Keyword::Accelerate,
                Keyword::Assault(2),
                Keyword::Deflect(1),
                Keyword::Ganking
            ]
        );
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = stalking(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(cost::can_accelerate(&ctx, RENGAR), "731 · six runes ready");
        assert_eq!(ctx.deflect_of(RENGAR), 1);
        assert!(ctx.has_keyword(RENGAR, Keyword::Ganking));
    }

    #[test]
    fn ganking_lets_him_walk_battlefield_to_battlefield_where_vi_is_refused() {
        let mut fixture = stalking(fixtures::BF1);
        assert_eq!(across(&mut fixture, RENGAR), Ok(()));
        assert_eq!(
            across(&mut fixture, fixtures::VI),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "736 · no Ganking, no battlefield-to-battlefield move"
        );
        let ctx = fixture.ctx();
        assert_eq!(
            move_destinations(&ctx, RENGAR),
            [Location::Base(0), Location::Battlefield(fixtures::BF2)]
        );
        assert_eq!(
            move_destinations(&ctx, fixtures::VI),
            move_destinations(&ctx, RENGAR),
            "an effect moves anyone anywhere, Ganking is for the standard move"
        );
    }

    #[test]
    fn he_attacks_at_six_and_defends_at_four() {
        let mut fixture = stalking(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(RENGAR), i32::from(MIGHT));
        assert!(ctx.mark_attacker(RENGAR));
        assert_eq!(
            ctx.current_might(RENGAR),
            i32::from(MIGHT + ASSAULT),
            "732 · +2 while an attacker"
        );
        ctx.clear_designation(RENGAR);
        assert!(ctx.mark_defender(RENGAR));
        assert_eq!(ctx.current_might(RENGAR), i32::from(MIGHT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_spell_pays_the_rainbow_to_choose_him_and_his_own_side_pays_nothing() {
        let mut fixture = stalking(fixtures::BF1);
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(ZAP, fixtures::HAND, 1, "Spark", 1, 1)
        });
        fixture.table.cards.retain(|card| card.id != 45);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &SPARK);
        let spec = a_card(MOVABLE_UNIT, "a unit");
        let theirs = ChainItem::new(7, ItemKind::Spell { card: ZAP }, 1, Origin::Hand);
        let ctx = fixture.ctx();
        let listed = targets::candidates(&ctx, &theirs, &spec);
        assert!(
            !listed.contains(&TargetRef::Card(RENGAR)),
            "735 · the one Mind rune pays the spell, nothing is left for the rainbow: {listed:?}"
        );
        assert!(listed.contains(&TargetRef::Card(fixtures::VI)));
        assert!(!targets::deflect_affordable(
            &ctx,
            &theirs,
            TargetRef::Card(RENGAR)
        ));
        assert!(targets::deflect_affordable(
            &ctx,
            &theirs,
            TargetRef::Card(fixtures::VI)
        ));
        drop(ctx);
        fixture
            .table
            .cards
            .push(fixtures::rune(45, 1, "Mind", true));
        let ctx = fixture.ctx();
        assert!(
            targets::deflect_affordable(&ctx, &theirs, TargetRef::Card(RENGAR)),
            "a second rune, exhausted or not, recycles for the deflect"
        );
        assert!(targets::candidates(&ctx, &theirs, &spec).contains(&TargetRef::Card(RENGAR)));
        let mine = ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(
            targets::deflect_affordable(&ctx, &mine, TargetRef::Card(RENGAR)),
            "735.1 · Deflect taxes opponents only"
        );
    }
}

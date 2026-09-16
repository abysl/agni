use super::prelude::unit;
use super::{Card, Keyword};

pub const ASSAULT: u8 = 2;

pub static CARD: Card = unit(
    "Inferna",
    &[Keyword::Ambush, Keyword::Assault(ASSAULT)],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{act, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const INFERNA: u32 = 90;
    const PLAIN: u32 = 54;
    const ENERGY: u8 = 2;
    const MIGHT: u8 = 1;
    const EXTRA_RUNES: [u32; 2] = [100, 101];

    fn inferna(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            ..fixtures::unit(INFERNA, zone, seat, "Inferna", MIGHT)
        }
    }

    fn lurking() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.cards.push(inferna(fixtures::HAND, 0));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(INFERNA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: INFERNA,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn mid_chain(fixture: &mut Fixture) -> Ctx<'_> {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx
    }

    #[test]
    fn the_script_is_an_ambush_assault_two_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Inferna").unwrap(), &CARD));
        assert_eq!(CARD.name, "Inferna");
        assert_eq!(CARD.keywords, &[Keyword::Ambush, Keyword::Assault(2)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = lurking();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(INFERNA, Keyword::Ambush));
        assert!(ctx.has_keyword(INFERNA, Keyword::Assault(2)));
        assert_eq!(
            ctx.ambush_locations(0, INFERNA),
            [Location::Battlefield(fixtures::BF1)],
            "737 · only the battlefield where she has units"
        );
    }

    #[test]
    fn she_ambushes_a_battlefield_with_her_units_while_a_spell_is_on_the_chain_and_attacks_at_three(
    ) {
        let mut fixture = lurking();
        let mut ctx = mid_chain(&mut fixture);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: INFERNA,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        ctx.table
            .apply_entry(&fixtures::move_action(INFERNA, fixtures::BF1, 0), 0)
            .unwrap();
        act(
            &mut ctx,
            0,
            Intent::Play {
                card: INFERNA,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(INFERNA),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.card(INFERNA).unwrap().exhausted);
        assert_eq!(ctx.current_might(INFERNA), i32::from(MIGHT));
        assert!(ctx.mark_attacker(INFERNA));
        assert_eq!(
            ctx.current_might(INFERNA),
            i32::from(MIGHT + ASSAULT),
            "732 · +2 while an attacker"
        );
        ctx.clear_designation(INFERNA);
        assert!(ctx.mark_defender(INFERNA));
        assert_eq!(
            ctx.current_might(INFERNA),
            i32::from(MIGHT),
            "Assault is nothing to a defender"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_base_keeps_her_own_timing_and_a_battlefield_without_her_units_refuses_the_ambush() {
        let mut fixture = lurking();
        let ctx = mid_chain(&mut fixture);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "the Sprite's battlefield holds enemies only"
        );
        drop(ctx);
        let mut short = lurking();
        short
            .table
            .cards
            .retain(|card| !EXTRA_RUNES.contains(&card.id));
        short.resolve();
        let ctx = mid_chain(&mut short);
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 1
            }),
            "the spell took two of the three ready runes"
        );
    }
}

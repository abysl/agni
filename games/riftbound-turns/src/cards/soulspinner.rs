use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Soulspinner", &[Keyword::Ambush], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::priority;
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SPINNER: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn spinner(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Order".into()],
            ..fixtures::unit(SPINNER, zone, seat, "Soulspinner", MIGHT)
        }
    }

    fn web(vi_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spinner(fixtures::HAND, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(vi_at);
        for rune in [46, 47] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPINNER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drop_to(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: SPINNER,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn close_the_chain(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(priority::holder(ctx), Some(0));
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "Spark took two of five");
    }

    #[test]
    fn the_script_is_a_vanilla_ambush_and_nothing_else() {
        assert!(std::ptr::eq(script_of("Soulspinner").unwrap(), &CARD));
        assert_eq!(CARD.name, "Soulspinner");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = web(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SPINNER, Keyword::Ambush));
        assert!(!ctx.has_keyword(SPINNER, Keyword::Reaction));
    }

    #[test]
    fn the_ambush_battlefields_are_the_ones_where_you_have_units() {
        let mut fixture = web(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::ambush_locations(&ctx, 0, SPINNER),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, SPINNER),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)],
            "on your own open turn the ambush adds nothing to the plain list"
        );
        assert!(
            legal::ambush_locations(&ctx, 1, SPINNER).is_empty(),
            "the opponent has no units at a battlefield"
        );
        drop(ctx);
        let mut fixture = web(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(
            legal::ambush_locations(&ctx, 0, SPINNER).is_empty(),
            "a held battlefield without your units is no ambush"
        );
        assert_eq!(
            legal::ambush_locations(&ctx, 1, fixtures::THEIR_UNIT),
            Vec::<Location>::new(),
            "Jinx has no Ambush at all"
        );
    }

    #[test]
    fn on_a_closed_chain_it_is_played_to_the_battlefield_with_your_units_and_refused_to_your_base()
    {
        let mut fixture = web(fixtures::BF1);
        let mut ctx = fixture.ctx();
        close_the_chain(&mut ctx);
        assert_eq!(
            legal::timing_at(&ctx, 0, SPINNER, Some(Location::Base(0))),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "808 · a unit keeps its Sorcery timing in its own base"
        );
        assert_eq!(
            legal::timing_at(&ctx, 0, SPINNER, Some(Location::Battlefield(fixtures::BF1))),
            Ok(()),
            "808.1.b · played as a Reaction where you have units"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drop_to(&ctx, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drop_to(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: SPINNER,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, SPINNER),
            [Location::Battlefield(fixtures::BF1)]
        );
        drop(ctx);
        let mut fixture = web(fixtures::BASE);
        let mut ctx = fixture.ctx();
        close_the_chain(&mut ctx);
        assert_eq!(
            legal::classify(&ctx, 0, &drop_to(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "with nobody of yours there a held battlefield is a Sorcery play"
        );
        assert!(legal::locations_for(&ctx, 0, SPINNER).is_empty());
    }
}
